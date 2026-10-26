use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use clientmail_domain::BlobId;
use clientmail_domain::ports::BlobStore;
use sha2::{Digest, Sha256};

use crate::StoreError;

/// Numbers temporary files, so concurrent writes in one process never share a name.
static TEMPORARY_FILES: AtomicU64 = AtomicU64::new(0);

/// The suffix of temporary files. A crash can leave them behind; step 2.1's sweep removes them.
const TEMPORARY_SUFFIX: &str = ".tmp";

/// Stores blobs as files named by the SHA-256 of their content.
///
/// A blob with id `abcd...` lives at `<root>/ab/cd...`, so no folder holds more than a few
/// thousand files.
#[derive(Debug, Clone)]
pub struct FileBlobStore {
    root: PathBuf,
}

impl FileBlobStore {
    /// Creates a store rooted at `root`. Folders are created on the first write.
    ///
    /// # Arguments
    ///
    /// * `root` - The folder that holds the blobs, usually next to the database.
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Returns the folder and the file path of a blob.
    fn location(&self, id: BlobId) -> (PathBuf, PathBuf) {
        let hex = id.to_string();
        // `split_at(2)` cannot panic: hex is 64 ASCII characters.
        let (prefix, rest) = hex.split_at(2);
        let folder = self.root.join(prefix);
        let file = folder.join(rest);
        (folder, file)
    }
}

impl BlobStore for FileBlobStore {
    type Error = StoreError;

    /// Stores `bytes`. When a file with this id already exists and is intact, nothing is written;
    /// when it is corrupt, it is replaced.
    fn put(&self, bytes: &[u8]) -> Result<BlobId, StoreError> {
        let id = BlobId::from_bytes(sha256(bytes));
        let (folder, file) = self.location(id);
        if read_verified(&file, id)?.is_some_and(|stored| stored.is_ok()) {
            return Ok(id);
        }
        let folder_is_new = !folder.try_exists()?;
        fs::create_dir_all(&folder)?;
        let temporary = folder.join(temporary_name());
        // Writing to a temporary name and renaming is atomic: a crash leaves either no blob or a
        // complete one under the final name, never half a file.
        let written = write_durably(&temporary, bytes).and_then(|()| fs::rename(&temporary, &file));
        if let Err(error) = written {
            // Best effort: the original error matters more than a failed cleanup.
            fs::remove_file(&temporary).ok();
            return Err(error.into());
        }
        // A rename reaches the disk only when its folder is flushed; a new folder also needs its
        // parent flushed, or the folder itself can vanish on power loss.
        sync_folder(&folder)?;
        if folder_is_new {
            sync_folder(&self.root)?;
        }
        Ok(id)
    }

    fn get(&self, id: BlobId) -> Result<Option<Vec<u8>>, StoreError> {
        let (_, file) = self.location(id);
        read_verified(&file, id)?
            .map(|stored| stored.map_err(|()| StoreError::CorruptBlob))
            .transpose()
    }
}

/// Returns the SHA-256 of `bytes`.
fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Reads a blob file and checks it against its id.
///
/// # Returns
///
/// `None` when the file does not exist, `Some(Ok(bytes))` when it matches, `Some(Err(()))` when
/// its content no longer matches the id.
fn read_verified(file: &Path, id: BlobId) -> io::Result<Option<Result<Vec<u8>, ()>>> {
    let bytes = match fs::read(file) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok(Some(if sha256(&bytes) == *id.as_bytes() {
        Ok(bytes)
    } else {
        Err(())
    }))
}

/// Returns a temporary file name no other write uses: process id, launch-independent time and a
/// per-process counter, so names do not repeat across launches either.
fn temporary_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!(
        ".{}.{nanos}.{}{TEMPORARY_SUFFIX}",
        process::id(),
        TEMPORARY_FILES.fetch_add(1, Ordering::Relaxed)
    )
}

/// Writes `bytes` to a new file and waits until they reach the disk.
fn write_durably(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create_new(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Flushes a folder's entries (new and renamed files) to the disk.
fn sync_folder(folder: &Path) -> io::Result<()> {
    File::open(folder)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, FileBlobStore) {
        let folder = tempfile::tempdir().unwrap();
        let store = FileBlobStore::new(folder.path().to_path_buf());
        (folder, store)
    }

    #[test]
    fn put_then_get_round_trips() {
        let (_folder, store) = store();

        let id = store.put(b"hello").unwrap();
        let read = store.get(id).unwrap();

        assert_eq!(read.as_deref(), Some(&b"hello"[..]), "same bytes");
        assert_eq!(
            id.to_string(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            "SHA-256 of the content"
        );
    }

    #[test]
    fn same_content_is_stored_once() {
        let (_folder, store) = store();

        let first = store.put(b"hello").unwrap();
        let second = store.put(b"hello").unwrap();
        let (prefix_folder, _) = store.location(first);
        let files = fs::read_dir(prefix_folder).unwrap().count();

        assert_eq!(first, second, "same id");
        assert_eq!(files, 1, "one file, no leftover temporary file");
    }

    #[test]
    fn missing_blob_returns_none() {
        let (_folder, store) = store();

        let read = store.get(BlobId::from_bytes([7; 32])).unwrap();

        assert_eq!(read, None, "nothing stored");
    }

    #[test]
    fn tampered_blob_is_reported_as_corrupt() {
        let (_folder, store) = store();
        let id = store.put(b"hello").unwrap();
        let (_, file) = store.location(id);
        fs::write(file, b"tampered").unwrap();

        let result = store.get(id);

        assert!(
            matches!(result, Err(StoreError::CorruptBlob)),
            "hash mismatch must be reported, got {result:?}"
        );
    }

    #[test]
    fn put_repairs_a_corrupt_blob() {
        let (_folder, store) = store();
        let id = store.put(b"hello").unwrap();
        let (_, file) = store.location(id);
        fs::write(file, b"tampered").unwrap();

        let again = store.put(b"hello").unwrap();
        let read = store.get(id).unwrap();

        assert_eq!(again, id, "same id");
        assert_eq!(read.as_deref(), Some(&b"hello"[..]), "content restored");
    }

    #[test]
    fn temporary_names_never_repeat() {
        let first = temporary_name();
        let second = temporary_name();

        assert_ne!(first, second, "counter and time make names unique");
        assert!(first.ends_with(TEMPORARY_SUFFIX), "sweepable suffix");
    }
}
