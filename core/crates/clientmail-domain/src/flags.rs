use std::collections::BTreeSet;
use std::fmt;

use crate::DomainError;

/// A flag every mail client understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Flag {
    /// The message was opened.
    Seen,
    /// The user starred or flagged the message.
    Flagged,
    /// The user replied.
    Answered,
    /// The message is a draft.
    Draft,
    /// The user forwarded the message (the `$Forwarded` keyword on the wire).
    Forwarded,
    /// The message is marked for removal but not expunged yet (`\Deleted`). Hidden from lists.
    Deleted,
}

impl Flag {
    /// Returns the bit that stores this flag inside [`MessageFlags`].
    const fn bit(self) -> u8 {
        match self {
            Self::Seen => 1,
            Self::Flagged => 1 << 1,
            Self::Answered => 1 << 2,
            Self::Draft => 1 << 3,
            Self::Forwarded => 1 << 4,
            Self::Deleted => 1 << 5,
        }
    }
}

/// Keywords that have a [`Flag`] of their own. Accepting them as keywords would let the same
/// state exist in two forms.
const RESERVED_KEYWORDS: [&str; 1] = ["$Forwarded"];

/// A custom IMAP keyword, such as `$Junk` or a `ClientMail` marker.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Keyword(String);

impl Keyword {
    /// Validates a keyword against the IMAP `atom` rules.
    ///
    /// # Arguments
    ///
    /// * `text` - The keyword, for example `$Junk`.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidKeyword`] when the text is empty, is not ASCII, or contains
    /// spaces, control characters or any of `( ) { % * " \ ]`. System flags such as `\Seen` are
    /// rejected because they start with `\`, and `$Forwarded` (any case) is rejected because it is
    /// [`Flag::Forwarded`]; use [`Flag`] for both.
    pub fn parse(text: &str) -> Result<Self, DomainError> {
        let is_atom_char = |character: char| {
            character.is_ascii_graphic()
                && !matches!(character, '(' | ')' | '{' | '%' | '*' | '"' | '\\' | ']')
        };
        let is_reserved = RESERVED_KEYWORDS
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(text));
        if text.is_empty() || is_reserved || !text.chars().all(is_atom_char) {
            return Err(DomainError::InvalidKeyword);
        }
        Ok(Self(text.to_owned()))
    }

    /// Returns the keyword text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Keyword {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The flags and keywords set on a message.
///
/// The [`Flag`] values are packed into one byte. Keywords are kept sorted and unique, so two
/// flag sets with the same content compare equal whatever order they were built in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct MessageFlags {
    flags: u8,
    keywords: BTreeSet<Keyword>,
}

impl MessageFlags {
    /// Returns true when `flag` is set.
    #[must_use]
    pub const fn contains(&self, flag: Flag) -> bool {
        self.flags & flag.bit() != 0
    }

    /// Sets `flag`.
    pub const fn insert(&mut self, flag: Flag) {
        self.flags |= flag.bit();
    }

    /// Clears `flag`.
    pub const fn remove(&mut self, flag: Flag) {
        self.flags &= !flag.bit();
    }

    /// Adds a keyword. Adding one that is already present does nothing.
    pub fn insert_keyword(&mut self, keyword: Keyword) {
        self.keywords.insert(keyword);
    }

    /// Returns the keywords in sorted order.
    pub fn keywords(&self) -> impl Iterator<Item = &Keyword> {
        self.keywords.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_can_be_set_and_cleared_independently() {
        let mut flags = MessageFlags::default();

        flags.insert(Flag::Seen);
        flags.insert(Flag::Forwarded);
        flags.remove(Flag::Seen);

        assert!(!flags.contains(Flag::Seen), "seen cleared");
        assert!(flags.contains(Flag::Forwarded), "forwarded kept");
        assert!(!flags.contains(Flag::Flagged), "flagged never set");
    }

    #[test]
    fn every_flag_has_its_own_bit() {
        let all = [
            Flag::Seen,
            Flag::Flagged,
            Flag::Answered,
            Flag::Draft,
            Flag::Forwarded,
            Flag::Deleted,
        ];

        for flag in all {
            let mut flags = MessageFlags::default();
            flags.insert(flag);

            let set: Vec<Flag> = all
                .into_iter()
                .filter(|other| flags.contains(*other))
                .collect();

            assert_eq!(set, [flag], "{flag:?} must not overlap another flag");
        }
    }

    #[test]
    fn keyword_accepts_atoms() {
        for text in ["$Junk", "$ClientMail.Snoozed", "NonJunk"] {
            assert!(Keyword::parse(text).is_ok(), "{text} must be accepted");
        }
    }

    #[test]
    fn keyword_rejects_flags_that_have_their_own_variant() {
        for text in ["$Forwarded", "$forwarded", "$FORWARDED"] {
            assert_eq!(
                Keyword::parse(text),
                Err(DomainError::InvalidKeyword),
                "{text} is Flag::Forwarded"
            );
        }
    }

    #[test]
    fn keyword_rejects_non_atoms() {
        for text in [
            "",
            "has space",
            "\\Seen",
            "a]b",
            "a(b",
            "a)b",
            "a{b",
            "a%b",
            "a*b",
            "a\"b",
            "caffè",
            "tab\there",
        ] {
            assert_eq!(
                Keyword::parse(text),
                Err(DomainError::InvalidKeyword),
                "{text:?} must be rejected"
            );
        }
    }

    #[test]
    fn keyword_order_does_not_affect_equality() {
        let junk = Keyword::parse("$Junk").unwrap();
        let work = Keyword::parse("Work").unwrap();
        let mut first = MessageFlags::default();
        let mut second = MessageFlags::default();

        first.insert_keyword(junk.clone());
        first.insert_keyword(work.clone());
        second.insert_keyword(work);
        second.insert_keyword(junk.clone());
        second.insert_keyword(junk);

        assert_eq!(
            first, second,
            "same keywords, different order and duplicates"
        );
        assert_eq!(
            first.keywords().map(Keyword::as_str).collect::<Vec<_>>(),
            ["$Junk", "Work"],
            "sorted and unique"
        );
    }
}
