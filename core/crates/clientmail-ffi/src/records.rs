use clientmail_domain::{Account, Provider};

/// The mail service behind an account, as seen by Swift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ProviderKind {
    /// Gmail and Google Workspace.
    Gmail,
    /// Outlook.com and Microsoft 365.
    Outlook,
    /// Fastmail.
    Fastmail,
    /// iCloud Mail.
    Icloud,
    /// Any other IMAP server.
    GenericImap,
}

impl From<Provider> for ProviderKind {
    fn from(provider: Provider) -> Self {
        match provider {
            Provider::Gmail => Self::Gmail,
            Provider::Outlook => Self::Outlook,
            Provider::Fastmail => Self::Fastmail,
            Provider::Icloud => Self::Icloud,
            Provider::GenericImap => Self::GenericImap,
        }
    }
}

/// An account, as seen by Swift. Plain fields, so `UniFFI` can copy it across the boundary.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AccountRecord {
    /// The account id, as text.
    pub id: String,
    /// The normalised email address.
    pub address: String,
    /// The sender name, if set.
    pub display_name: Option<String>,
    /// The provider that syncs the account.
    pub provider: ProviderKind,
}

impl From<&Account> for AccountRecord {
    fn from(account: &Account) -> Self {
        Self {
            id: account.id().to_string(),
            address: account.address().as_str().to_owned(),
            display_name: account.display_name().map(ToOwned::to_owned),
            provider: account.provider().into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use clientmail_domain::NewAccount;

    use super::*;

    #[test]
    fn account_record_copies_every_field() {
        let account = NewAccount::parse("ada@icloud.com", Some("Ada"))
            .unwrap()
            .into_account();

        let record = AccountRecord::from(&account);

        assert_eq!(record.id, account.id().to_string(), "id");
        assert_eq!(record.address, "ada@icloud.com", "address");
        assert_eq!(record.display_name.as_deref(), Some("Ada"), "display name");
        assert_eq!(record.provider, ProviderKind::Icloud, "provider");
    }
}
