use crate::AccountId;

/// What an account secret is for. Each kind is stored as its own item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretKind {
    /// The long-lived OAuth token that obtains new access tokens.
    OAuthRefreshToken,
}

impl SecretKind {
    /// Every kind, so deleting an account's secrets cannot miss one added later.
    pub const ALL: [Self; 1] = [Self::OAuthRefreshToken];

    /// Returns the stable name used in storage labels.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OAuthRefreshToken => "oauth-refresh-token",
        }
    }
}

/// Names one secret: an account and what the secret is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SecretKey {
    account: AccountId,
    kind: SecretKind,
}

impl SecretKey {
    /// Creates a key.
    ///
    /// # Arguments
    ///
    /// * `account` - The account the secret belongs to.
    /// * `kind` - What the secret is for.
    #[must_use]
    pub const fn new(account: AccountId, kind: SecretKind) -> Self {
        Self { account, kind }
    }

    /// Returns the account the secret belongs to.
    #[must_use]
    pub const fn account(&self) -> AccountId {
        self.account
    }

    /// Returns what the secret is for.
    #[must_use]
    pub const fn kind(&self) -> SecretKind {
        self.kind
    }

    /// Returns the label storage uses to find the secret: `<account id>:<kind>`.
    ///
    /// Built from the account id, never the address, so a listing of stored secrets reveals
    /// nothing personal.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{}:{}", self.account, self.kind.as_str())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::convert::Infallible;

    use secrecy::{ExposeSecret, SecretString};

    use super::*;
    use crate::ports::SecretStore;

    /// In-memory store keyed by label. `RefCell` allows mutation behind `&self`.
    #[derive(Default)]
    struct FakeSecrets {
        items: RefCell<HashMap<String, String>>,
    }

    impl SecretStore for FakeSecrets {
        type Error = Infallible;

        fn read(&self, key: &SecretKey) -> Result<Option<SecretString>, Infallible> {
            Ok(self
                .items
                .borrow()
                .get(&key.label())
                .map(|value| SecretString::from(value.clone())))
        }

        fn write(&self, key: &SecretKey, secret: &SecretString) -> Result<(), Infallible> {
            self.items
                .borrow_mut()
                .insert(key.label(), secret.expose_secret().to_owned());
            Ok(())
        }

        fn delete(&self, key: &SecretKey) -> Result<bool, Infallible> {
            Ok(self.items.borrow_mut().remove(&key.label()).is_some())
        }
    }

    #[test]
    fn delete_account_removes_every_kind_of_that_account_only() {
        let secrets = FakeSecrets::default();
        let removed = AccountId::generate();
        let kept = AccountId::generate();
        for account in [removed, kept] {
            for kind in SecretKind::ALL {
                secrets
                    .write(&SecretKey::new(account, kind), &SecretString::from("token"))
                    .unwrap();
            }
        }

        secrets.delete_account(removed).unwrap();

        for kind in SecretKind::ALL {
            assert!(
                secrets
                    .read(&SecretKey::new(removed, kind))
                    .unwrap()
                    .is_none(),
                "{kind:?} of the removed account is gone"
            );
            assert!(
                secrets.read(&SecretKey::new(kept, kind)).unwrap().is_some(),
                "{kind:?} of the other account is kept"
            );
        }
    }

    #[test]
    fn delete_account_without_secrets_succeeds() {
        let secrets = FakeSecrets::default();

        let result = secrets.delete_account(AccountId::generate());

        assert!(result.is_ok(), "nothing to delete is not an error");
    }

    #[test]
    fn label_holds_account_id_and_kind() {
        let account = AccountId::generate();

        let label = SecretKey::new(account, SecretKind::OAuthRefreshToken).label();

        assert_eq!(
            label,
            format!("{account}:oauth-refresh-token"),
            "account id, then kind"
        );
        assert!(!label.contains('@'), "no address in a label");
    }

    #[test]
    fn labels_of_different_accounts_differ() {
        let first = SecretKey::new(AccountId::generate(), SecretKind::OAuthRefreshToken);
        let second = SecretKey::new(AccountId::generate(), SecretKind::OAuthRefreshToken);

        assert_ne!(first.label(), second.label(), "one item per account");
    }
}
