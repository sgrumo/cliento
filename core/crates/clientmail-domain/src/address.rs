use std::fmt;
use std::str::FromStr;

use crate::DomainError;

/// A validated email address.
///
/// The domain part is stored in lowercase, because domains are case-insensitive. The local part
/// keeps its case, because RFC 5321 lets servers treat it as case-sensitive.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EmailAddress {
    value: String,
    at_index: usize,
}

impl EmailAddress {
    /// Parses and normalises an email address.
    ///
    /// # Arguments
    ///
    /// * `input` - The address as typed by the user. Surrounding whitespace is ignored.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidEmailAddress`] when the text is not an RFC-compliant address,
    /// or when it has no dot in the domain (addresses like `user@localhost` cannot receive mail
    /// from the internet).
    ///
    /// # Examples
    ///
    /// ```
    /// use clientmail_domain::EmailAddress;
    ///
    /// let address = EmailAddress::parse("  Ada@Example.COM ")?;
    /// assert_eq!(address.as_str(), "Ada@example.com");
    /// # Ok::<(), clientmail_domain::DomainError>(())
    /// ```
    pub fn parse(input: &str) -> Result<Self, DomainError> {
        let trimmed = input.trim();
        let parsed = email_address::EmailAddress::from_str(trimmed).map_err(|error| {
            DomainError::InvalidEmailAddress {
                reason: error.to_string(),
            }
        })?;
        let domain = parsed.domain().to_lowercase();
        if !domain.contains('.') {
            return Err(DomainError::InvalidEmailAddress {
                reason: "the domain has no dot".to_owned(),
            });
        }
        let local = parsed.local_part();
        Ok(Self {
            value: format!("{local}@{domain}"),
            at_index: local.len(),
        })
    }

    /// Returns the full normalised address.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Returns the part before the `@`.
    #[must_use]
    pub fn local_part(&self) -> &str {
        // `split_at` cannot panic: `at_index` was computed from this same string.
        self.value.split_at(self.at_index).0
    }

    /// Returns the lowercase part after the `@`.
    #[must_use]
    pub fn domain(&self) -> &str {
        // Skips the `@` itself, which is one byte long.
        self.value.get(self.at_index + 1..).unwrap_or_default()
    }
}

impl fmt::Display for EmailAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.value)
    }
}

impl FromStr for EmailAddress {
    type Err = DomainError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_trims_and_lowercases_the_domain() {
        let address = EmailAddress::parse("  Ada.Lovelace@Example.COM\n").unwrap();

        assert_eq!(address.as_str(), "Ada.Lovelace@example.com");
        assert_eq!(address.local_part(), "Ada.Lovelace");
        assert_eq!(address.domain(), "example.com");
    }

    #[test]
    fn parse_rejects_text_without_at_sign() {
        let result = EmailAddress::parse("not-an-address");

        assert!(
            matches!(result, Err(DomainError::InvalidEmailAddress { .. })),
            "expected an invalid address error, got {result:?}"
        );
    }

    #[test]
    fn parse_rejects_domain_without_dot() {
        let result = EmailAddress::parse("root@localhost");

        assert!(
            matches!(result, Err(DomainError::InvalidEmailAddress { .. })),
            "expected an invalid address error, got {result:?}"
        );
    }

    #[test]
    fn parse_rejects_empty_input() {
        let result = EmailAddress::parse("   ");

        assert!(result.is_err(), "empty input must be rejected");
    }

    #[test]
    fn parse_keeps_multibyte_local_part() {
        let address = EmailAddress::parse("josé@example.com").unwrap();

        assert_eq!(address.local_part(), "josé");
        assert_eq!(address.domain(), "example.com");
    }

    #[test]
    fn display_and_from_str_round_trip() {
        let address: EmailAddress = "ada@example.com".parse().unwrap();

        assert_eq!(address.to_string(), "ada@example.com");
    }
}
