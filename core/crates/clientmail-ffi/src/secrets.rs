use std::sync::Arc;

use clientmail_domain::ports::SecretStore;
use clientmail_domain::{ExposeSecret, SecretKey, SecretString};

/// Secret storage the app implements with the platform's credential store (the Keychain on
/// macOS) and hands to [`crate::CoreClient`].
///
/// The core calls it from its own threads, so implementations must be thread-safe. Secrets cross
/// the boundary as plain strings; never log them.
#[uniffi::export(with_foreign)]
pub trait SecretStoreBridge: Send + Sync {
    /// Returns the secret stored under `label`, or `None`.
    ///
    /// # Errors
    ///
    /// Returns [`SecretBridgeError`] when the platform store fails.
    fn read(&self, label: String) -> Result<Option<String>, SecretBridgeError>;

    /// Stores `secret` under `label`, replacing any previous value.
    ///
    /// # Errors
    ///
    /// Returns [`SecretBridgeError`] when the platform store fails.
    fn write(&self, label: String, secret: String) -> Result<(), SecretBridgeError>;

    /// Deletes the secret under `label`. Returns `false` when there was none.
    ///
    /// # Errors
    ///
    /// Returns [`SecretBridgeError`] when the platform store fails.
    fn delete(&self, label: String) -> Result<bool, SecretBridgeError>;
}

/// Failures a [`SecretStoreBridge`] reports. They carry no secret and no personal data.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum SecretBridgeError {
    /// The store cannot be used right now, for example because the keychain is locked.
    #[error("secret storage unavailable")]
    Unavailable,
    /// The platform store failed with a status code (an `OSStatus` on macOS).
    #[error("secret storage failed with status {code}")]
    Failure {
        /// The platform status code.
        code: i32,
    },
    /// The foreign implementation threw an error the bridge does not declare.
    #[error("secret storage failed unexpectedly")]
    Unexpected,
}

/// `UniFFI` requires this conversion for errors of foreign traits: it is used when the foreign
/// code fails in a way the interface does not declare.
impl From<uniffi::UnexpectedUniFFICallbackError> for SecretBridgeError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Unexpected
    }
}

/// Adapts a [`SecretStoreBridge`] to the domain's [`SecretStore`] port.
pub(crate) struct ForeignSecretStore {
    bridge: Arc<dyn SecretStoreBridge>,
}

impl ForeignSecretStore {
    pub(crate) fn new(bridge: Arc<dyn SecretStoreBridge>) -> Self {
        Self { bridge }
    }
}

impl std::fmt::Debug for ForeignSecretStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ForeignSecretStore")
    }
}

impl SecretStore for ForeignSecretStore {
    type Error = SecretBridgeError;

    fn read(&self, key: &SecretKey) -> Result<Option<SecretString>, SecretBridgeError> {
        // The `String` from the bridge moves into a `SecretString`, which zeroes this copy when
        // dropped. Copies UniFFI and Swift made on the way are freed without zeroing.
        Ok(self.bridge.read(key.label())?.map(SecretString::from))
    }

    fn write(&self, key: &SecretKey, secret: &SecretString) -> Result<(), SecretBridgeError> {
        self.bridge
            .write(key.label(), secret.expose_secret().to_owned())
    }

    fn delete(&self, key: &SecretKey) -> Result<bool, SecretBridgeError> {
        self.bridge.delete(key.label())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use clientmail_domain::{AccountId, SecretKind};

    use super::*;

    /// Rust implementation of the bridge, as the Swift one would be.
    #[derive(Default)]
    struct MemoryBridge {
        items: Mutex<HashMap<String, String>>,
    }

    impl SecretStoreBridge for MemoryBridge {
        fn read(&self, label: String) -> Result<Option<String>, SecretBridgeError> {
            Ok(self.items.lock().unwrap().get(&label).cloned())
        }

        fn write(&self, label: String, secret: String) -> Result<(), SecretBridgeError> {
            self.items.lock().unwrap().insert(label, secret);
            Ok(())
        }

        fn delete(&self, label: String) -> Result<bool, SecretBridgeError> {
            Ok(self.items.lock().unwrap().remove(&label).is_some())
        }
    }

    struct LockedBridge;

    impl SecretStoreBridge for LockedBridge {
        fn read(&self, _label: String) -> Result<Option<String>, SecretBridgeError> {
            Err(SecretBridgeError::Unavailable)
        }

        fn write(&self, _label: String, _secret: String) -> Result<(), SecretBridgeError> {
            Err(SecretBridgeError::Unavailable)
        }

        fn delete(&self, _label: String) -> Result<bool, SecretBridgeError> {
            Err(SecretBridgeError::Failure { code: -25_308 })
        }
    }

    fn key() -> SecretKey {
        SecretKey::new(AccountId::generate(), SecretKind::OAuthRefreshToken)
    }

    #[test]
    fn round_trips_through_the_bridge_under_the_key_label() {
        let bridge = Arc::new(MemoryBridge::default());
        let store = ForeignSecretStore::new(Arc::<MemoryBridge>::clone(&bridge));
        let key = key();

        store.write(&key, &SecretString::from("token")).unwrap();
        let read = store.read(&key).unwrap();
        let stored_label = bridge.items.lock().unwrap().keys().next().cloned();
        let deleted = store.delete(&key).unwrap();
        let after_delete = store.read(&key).unwrap();

        assert_eq!(
            read.as_ref().map(ExposeSecret::expose_secret),
            Some("token"),
            "same secret back"
        );
        assert_eq!(stored_label, Some(key.label()), "stored under the label");
        assert!(deleted, "deleted");
        assert!(after_delete.is_none(), "gone");
    }

    #[test]
    fn bridge_errors_pass_through() {
        let store = ForeignSecretStore::new(Arc::new(LockedBridge));

        let read = store.read(&key());
        let delete = store.delete(&key());

        assert_eq!(read.err(), Some(SecretBridgeError::Unavailable), "locked");
        assert_eq!(
            delete.err(),
            Some(SecretBridgeError::Failure { code: -25_308 }),
            "status code kept"
        );
    }

    #[test]
    fn debug_output_never_shows_the_secret() {
        let secret = SecretString::from("super-secret-token");
        let store = ForeignSecretStore::new(Arc::new(MemoryBridge::default()));

        let outputs = [format!("{secret:?}"), format!("{store:?}")];

        for output in outputs {
            assert!(!output.contains("super-secret"), "leaked: {output}");
        }
    }
}
