use std::fmt;

use crate::EmailAddress;
use crate::redact::{Redacted, redact_option};

/// An address found in a message header.
///
/// Headers often hold text that is not a valid address (`undisclosed-recipients:;`, missing
/// domains). Such text is kept as [`ParticipantAddress::Unparsed`] so the message still shows.
/// `Debug` hides the address.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum ParticipantAddress {
    /// A valid, normalised address.
    Valid(EmailAddress),
    /// Text the header held where an address was expected, trimmed. Never empty.
    Unparsed(String),
}

impl ParticipantAddress {
    /// Returns the address text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Valid(address) => address.as_str(),
            Self::Unparsed(text) => text,
        }
    }
}

impl fmt::Debug for ParticipantAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Valid(address) => formatter.debug_tuple("Valid").field(address).finish(),
            Self::Unparsed(_) => formatter.debug_tuple("Unparsed").field(&Redacted).finish(),
        }
    }
}

/// A sender or recipient of a message. `Debug` hides the name and address.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Participant {
    name: Option<String>,
    address: ParticipantAddress,
}

impl Participant {
    /// Builds a participant from decoded header values.
    ///
    /// # Arguments
    ///
    /// * `name` - The display name, already decoded. Blank names, and names equal to the address,
    ///   are dropped.
    /// * `address` - The bare address text (`ada@example.com`, not `Ada <ada@example.com>`).
    ///   Invalid addresses are kept as unparsed text.
    ///
    /// # Returns
    ///
    /// `None` when the address is blank, as in group markers or an empty `From:`: there is no one
    /// to show or reply to.
    #[must_use]
    pub fn new(name: Option<&str>, address: &str) -> Option<Self> {
        let address = address.trim();
        if address.is_empty() {
            return None;
        }
        let address = EmailAddress::parse(address).map_or_else(
            |_| ParticipantAddress::Unparsed(address.to_owned()),
            ParticipantAddress::Valid,
        );
        let name = name
            .map(str::trim)
            .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case(address.as_str()))
            .map(ToOwned::to_owned);
        Some(Self { name, address })
    }

    /// Returns the display name, if the header had a useful one.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the address.
    #[must_use]
    pub const fn address(&self) -> &ParticipantAddress {
        &self.address
    }

    /// Returns the text to show: the name, or the address when there is no name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.name().unwrap_or_else(|| self.address.as_str())
    }
}

impl fmt::Debug for Participant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Participant")
            .field("name", &redact_option(self.name.as_ref()))
            .field("address", &self.address)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_address_is_parsed() {
        let participant = Participant::new(Some("Ada Lovelace"), "ada@Example.com").unwrap();

        assert!(
            matches!(participant.address(), ParticipantAddress::Valid(_)),
            "valid address"
        );
        assert_eq!(
            participant.address().as_str(),
            "ada@example.com",
            "normalised"
        );
        assert_eq!(participant.display_name(), "Ada Lovelace", "name shown");
    }

    #[test]
    fn invalid_address_is_kept_as_text() {
        let participant = Participant::new(None, " undisclosed-recipients:; ").unwrap();

        assert_eq!(
            participant.address(),
            &ParticipantAddress::Unparsed("undisclosed-recipients:;".to_owned()),
            "unparsed and trimmed"
        );
        assert_eq!(
            participant.display_name(),
            "undisclosed-recipients:;",
            "address shown without a name"
        );
    }

    #[test]
    fn display_name_form_is_kept_whole_instead_of_dropping_the_name() {
        let participant = Participant::new(None, "Mallory <ada@example.com>").unwrap();

        assert_eq!(
            participant.address(),
            &ParticipantAddress::Unparsed("Mallory <ada@example.com>".to_owned()),
            "the name stays visible"
        );
    }

    #[test]
    fn blank_address_yields_no_participant() {
        assert_eq!(Participant::new(Some("Ada"), "   "), None, "blank address");
        assert_eq!(Participant::new(None, ""), None, "empty address");
    }

    #[test]
    fn blank_name_is_dropped() {
        let participant = Participant::new(Some("   "), "ada@example.com").unwrap();

        assert_eq!(participant.name(), None, "blank name");
        assert_eq!(
            participant.display_name(),
            "ada@example.com",
            "falls back to address"
        );
    }

    #[test]
    fn name_equal_to_address_is_dropped() {
        let participant = Participant::new(Some("ADA@example.com"), "ada@example.com").unwrap();

        assert_eq!(participant.name(), None, "name repeats the address");
    }
}
