use std::num::NonZeroU64;

use crate::{ImapUid, UidValidity};

/// The largest mod-sequence RFC 7162 allows: 2^63 - 1.
const MAX_MOD_SEQ: u64 = i64::MAX.cast_unsigned();

/// A CONDSTORE mod-sequence (`HIGHESTMODSEQ`, `MODSEQ`). Between 1 and 2^63 - 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModSeq(NonZeroU64);

impl ModSeq {
    /// Wraps a mod-sequence from the server.
    ///
    /// # Returns
    ///
    /// `None` when `value` is 0 or above 2^63 - 1, which RFC 7162 forbids.
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        if value > MAX_MOD_SEQ {
            return None;
        }
        // A `const fn` cannot call closures, so `Option::map(Self)` is spelled out as a `match`.
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the raw value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Where incremental sync of a mailbox resumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MailboxSyncState {
    uid_validity: UidValidity,
    uid_next: Option<ImapUid>,
    highest_modseq: Option<ModSeq>,
}

impl MailboxSyncState {
    /// Creates a sync state.
    ///
    /// # Arguments
    ///
    /// * `uid_validity` - The mailbox `UIDVALIDITY` the other values belong to.
    /// * `uid_next` - The next `UID` the server will assign, when it reported one.
    /// * `highest_modseq` - The mailbox `HIGHESTMODSEQ`, when the server supports CONDSTORE.
    #[must_use]
    pub const fn new(
        uid_validity: UidValidity,
        uid_next: Option<ImapUid>,
        highest_modseq: Option<ModSeq>,
    ) -> Self {
        Self {
            uid_validity,
            uid_next,
            highest_modseq,
        }
    }

    /// Returns the `UIDVALIDITY`.
    #[must_use]
    pub const fn uid_validity(&self) -> UidValidity {
        self.uid_validity
    }

    /// Returns the next `UID`, if known.
    #[must_use]
    pub const fn uid_next(&self) -> Option<ImapUid> {
        self.uid_next
    }

    /// Returns the `HIGHESTMODSEQ`, if known.
    #[must_use]
    pub const fn highest_modseq(&self) -> Option<ModSeq> {
        self.highest_modseq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_seq_accepts_the_rfc_range() {
        assert_eq!(ModSeq::new(1).map(ModSeq::get), Some(1), "lowest");
        assert_eq!(
            ModSeq::new(MAX_MOD_SEQ).map(ModSeq::get),
            Some(MAX_MOD_SEQ),
            "highest"
        );
    }

    #[test]
    fn mod_seq_rejects_zero_and_values_above_the_rfc_range() {
        assert_eq!(ModSeq::new(0), None, "zero");
        assert_eq!(ModSeq::new(MAX_MOD_SEQ + 1), None, "2^63");
    }

    #[test]
    fn sync_state_exposes_its_parts() {
        let uid_validity = UidValidity::new(3).unwrap();
        let uid_next = ImapUid::new(10);

        let state = MailboxSyncState::new(uid_validity, uid_next, None);

        assert_eq!(state.uid_validity(), uid_validity, "uidvalidity");
        assert_eq!(state.uid_next(), uid_next, "uidnext");
        assert_eq!(state.highest_modseq(), None, "no condstore");
    }
}
