use std::fmt;
use std::str::FromStr;

use crate::DomainError;

/// Names a stored body or attachment by the SHA-256 of its content.
///
/// Equal content always gets the same id, so a blob is stored once however many messages share it.
/// `Display` writes 64 lowercase hex characters, and `FromStr` reads them back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlobId([u8; 32]);

impl BlobId {
    /// Wraps a SHA-256 digest.
    ///
    /// # Arguments
    ///
    /// * `digest` - The 32-byte hash of the content. Computing it is the storage layer's job.
    #[must_use]
    pub const fn from_bytes(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    /// Returns the digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for BlobId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0
            .iter()
            .try_for_each(|byte| write!(formatter, "{byte:02x}"))
    }
}

impl FromStr for BlobId {
    type Err = DomainError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let bytes = input.as_bytes();
        // `from_str_radix` alone would also accept a leading `+`, which does not round-trip.
        if bytes.len() != 64 || !bytes.iter().all(u8::is_ascii_hexdigit) {
            return Err(DomainError::InvalidBlobId);
        }
        let mut digest = [0_u8; 32];
        // `as_chunks` views the 64 bytes as 32 pairs of hex characters (the length was checked, so
        // nothing is left over); `zip` pairs each with its output byte.
        let (pairs, _) = bytes.as_chunks::<2>();
        for (byte, pair) in digest.iter_mut().zip(pairs) {
            let pair = std::str::from_utf8(pair).map_err(|_| DomainError::InvalidBlobId)?;
            *byte = u8::from_str_radix(pair, 16).map_err(|_| DomainError::InvalidBlobId)?;
        }
        Ok(Self(digest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        let id = BlobId::from_bytes([0xab; 32]);

        let parsed: BlobId = id.to_string().parse().unwrap();

        assert_eq!(parsed, id, "round trip");
        assert_eq!(id.to_string(), "ab".repeat(32), "lowercase hex");
    }

    #[test]
    fn parse_accepts_uppercase() {
        let parsed: BlobId = "AB".repeat(32).parse().unwrap();

        assert_eq!(parsed, BlobId::from_bytes([0xab; 32]), "uppercase hex");
    }

    #[test]
    fn parse_rejects_bad_input() {
        for input in [
            String::new(),
            "ab".to_owned(),
            "zz".repeat(32),
            "a".repeat(65),
            "é".repeat(32),
            "+f".repeat(32),
        ] {
            assert_eq!(
                input.parse::<BlobId>(),
                Err(DomainError::InvalidBlobId),
                "{input:?}"
            );
        }
    }
}
