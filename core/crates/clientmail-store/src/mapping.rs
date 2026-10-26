//! Conversions between domain values and their column encodings.
//!
//! The encodings here are part of the database format: changing one needs a migration.

use clientmail_domain::{Flag, MailboxRole, MessageFlags};

use crate::StoreError;

/// Returns the text stored in `mailboxes.role`.
pub(crate) const fn role_to_column(role: MailboxRole) -> &'static str {
    match role {
        MailboxRole::Inbox => "inbox",
        MailboxRole::AllMail => "all-mail",
        MailboxRole::Archive => "archive",
        MailboxRole::Drafts => "drafts",
        MailboxRole::Sent => "sent",
        MailboxRole::Trash => "trash",
        MailboxRole::Junk => "junk",
        MailboxRole::Starred => "starred",
        MailboxRole::Important => "important",
        MailboxRole::Custom => "custom",
    }
}

/// Parses `mailboxes.role`.
///
/// # Errors
///
/// Returns [`StoreError::CorruptRow`] for an unknown value.
pub(crate) fn role_from_column(text: &str) -> Result<MailboxRole, StoreError> {
    const ROLES: [MailboxRole; 10] = [
        MailboxRole::Inbox,
        MailboxRole::AllMail,
        MailboxRole::Archive,
        MailboxRole::Drafts,
        MailboxRole::Sent,
        MailboxRole::Trash,
        MailboxRole::Junk,
        MailboxRole::Starred,
        MailboxRole::Important,
        MailboxRole::Custom,
    ];
    ROLES
        .into_iter()
        .find(|role| role_to_column(*role) == text)
        .ok_or(StoreError::CorruptRow { column: "role" })
}

/// Returns the bit that stores `flag` in `messages.flags`.
///
/// Owned by the store, not taken from the domain's in-memory layout, so that layout can change
/// without a migration.
const fn flag_column_bit(flag: Flag) -> i64 {
    match flag {
        Flag::Seen => 1,
        Flag::Flagged => 1 << 1,
        Flag::Answered => 1 << 2,
        Flag::Draft => 1 << 3,
        Flag::Forwarded => 1 << 4,
        Flag::Deleted => 1 << 5,
    }
}

/// The bit of [`Flag::Deleted`] in `messages.flags`, for SQL filters.
pub(crate) const DELETED_COLUMN_BIT: i64 = flag_column_bit(Flag::Deleted);

/// Returns the value stored in `messages.flags`. Keywords are stored in their own table.
pub(crate) fn flags_to_column(flags: &MessageFlags) -> i64 {
    Flag::ALL
        .into_iter()
        .filter(|flag| flags.contains(*flag))
        .fold(0, |bits, flag| bits | flag_column_bit(flag))
}

/// Parses `messages.flags` into flags without keywords.
///
/// # Errors
///
/// Returns [`StoreError::CorruptRow`] when a bit is set that no flag uses.
pub(crate) fn flags_from_column(bits: i64) -> Result<MessageFlags, StoreError> {
    let known = Flag::ALL
        .into_iter()
        .fold(0, |mask, flag| mask | flag_column_bit(flag));
    if bits & !known != 0 {
        return Err(StoreError::CorruptRow { column: "flags" });
    }
    let mut flags = MessageFlags::default();
    for flag in Flag::ALL {
        if bits & flag_column_bit(flag) != 0 {
            flags.insert(flag);
        }
    }
    Ok(flags)
}

/// Returns a `u64` as the `i64` with the same bits. `SQLite` integers are signed.
pub(crate) const fn u64_to_column(value: u64) -> i64 {
    value.cast_signed()
}

/// Returns the `u64` whose bits an `i64` column holds.
pub(crate) const fn u64_from_column(value: i64) -> u64 {
    value.cast_unsigned()
}

/// Converts an `INTEGER` column holding a 32-bit unsigned value.
///
/// # Errors
///
/// Returns [`StoreError::CorruptRow`] naming `column` when the value is out of range.
pub(crate) fn u32_from_column(value: i64, column: &'static str) -> Result<u32, StoreError> {
    u32::try_from(value).map_err(|_| StoreError::CorruptRow { column })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_round_trips() {
        for role in [
            MailboxRole::Inbox,
            MailboxRole::AllMail,
            MailboxRole::Archive,
            MailboxRole::Custom,
        ] {
            assert_eq!(
                role_from_column(role_to_column(role)).unwrap(),
                role,
                "{role:?}"
            );
        }
        assert!(role_from_column("outbox").is_err(), "unknown role");
    }

    #[test]
    fn flags_round_trip_and_reject_unknown_bits() {
        let mut flags = MessageFlags::default();
        flags.insert(Flag::Seen);
        flags.insert(Flag::Deleted);

        let bits = flags_to_column(&flags);

        assert_eq!(bits, 0b10_0001, "storage bits");
        assert_eq!(flags_from_column(bits).unwrap(), flags, "round trip");
        assert!(flags_from_column(1 << 6).is_err(), "unknown bit");
    }

    #[test]
    fn u64_keeps_its_bits_across_the_signed_column() {
        for value in [0, 1, 1 << 63, u64::MAX] {
            assert_eq!(u64_from_column(u64_to_column(value)), value, "{value}");
        }
    }

    #[test]
    fn u32_rejects_out_of_range_values() {
        assert_eq!(u32_from_column(7, "uid").unwrap(), 7, "in range");
        assert!(u32_from_column(-1, "uid").is_err(), "negative");
        assert!(
            u32_from_column(i64::from(u32::MAX) + 1, "uid").is_err(),
            "too large"
        );
    }
}
