use crate::EmailAddress;

/// The mail service behind an account. It decides which protocol adapter syncs the account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    /// Gmail and Google Workspace, synced over IMAP with Gmail extensions.
    Gmail,
    /// Outlook.com and Microsoft 365, synced over Microsoft Graph.
    Outlook,
    /// Fastmail.
    Fastmail,
    /// iCloud Mail.
    Icloud,
    /// Any other server that speaks IMAP and SMTP.
    GenericImap,
}

impl Provider {
    /// Every provider, in display order.
    pub const ALL: [Self; 5] = [
        Self::Gmail,
        Self::Outlook,
        Self::Fastmail,
        Self::Icloud,
        Self::GenericImap,
    ];

    /// Guesses the provider from the address domain.
    ///
    /// Only well-known consumer domains are recognised. Custom domains (for example a Google
    /// Workspace domain) return [`Provider::GenericImap`] until server discovery refines them.
    ///
    /// # Arguments
    ///
    /// * `address` - The account address.
    ///
    /// # Returns
    ///
    /// The detected provider, or [`Provider::GenericImap`] for unknown domains.
    #[must_use]
    pub fn detect(address: &EmailAddress) -> Self {
        match address.domain() {
            "gmail.com" | "googlemail.com" => Self::Gmail,
            "outlook.com" | "hotmail.com" | "live.com" | "msn.com" => Self::Outlook,
            "fastmail.com" | "fastmail.fm" => Self::Fastmail,
            "icloud.com" | "me.com" | "mac.com" => Self::Icloud,
            _ => Self::GenericImap,
        }
    }

    /// Returns the stable identifier used in storage and across the FFI boundary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gmail => "gmail",
            Self::Outlook => "outlook",
            Self::Fastmail => "fastmail",
            Self::Icloud => "icloud",
            Self::GenericImap => "generic-imap",
        }
    }

    /// Parses an identifier produced by [`Provider::as_str`].
    ///
    /// # Arguments
    ///
    /// * `identifier` - A stored provider identifier.
    ///
    /// # Returns
    ///
    /// The provider, or `None` when the identifier is unknown.
    #[must_use]
    pub fn from_identifier(identifier: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|provider| provider.as_str() == identifier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(input: &str) -> Provider {
        Provider::detect(&EmailAddress::parse(input).unwrap())
    }

    #[test]
    fn detect_recognises_consumer_domains() {
        assert_eq!(detect("a@gmail.com"), Provider::Gmail, "gmail.com");
        assert_eq!(
            detect("a@googlemail.com"),
            Provider::Gmail,
            "googlemail.com"
        );
        assert_eq!(detect("a@hotmail.com"), Provider::Outlook, "hotmail.com");
        assert_eq!(detect("a@fastmail.fm"), Provider::Fastmail, "fastmail.fm");
        assert_eq!(detect("a@me.com"), Provider::Icloud, "me.com");
    }

    #[test]
    fn detect_is_case_insensitive_on_domain() {
        assert_eq!(detect("a@GMail.Com"), Provider::Gmail, "mixed-case domain");
    }

    #[test]
    fn detect_falls_back_to_generic_imap() {
        assert_eq!(
            detect("a@example.org"),
            Provider::GenericImap,
            "unknown domain"
        );
    }

    #[test]
    fn identifier_round_trips_for_every_provider() {
        for provider in Provider::ALL {
            assert_eq!(
                Provider::from_identifier(provider.as_str()),
                Some(provider),
                "round trip for {provider:?}"
            );
        }
    }

    #[test]
    fn from_identifier_rejects_unknown_text() {
        assert_eq!(Provider::from_identifier("aol"), None, "unknown identifier");
    }
}
