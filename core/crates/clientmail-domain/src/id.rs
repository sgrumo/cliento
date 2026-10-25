/// Defines a local identifier backed by a UUID v7, with `generate`, `Display` and `FromStr`.
///
/// UUID v7 starts with a timestamp, so ids sort in creation order without a separate column.
///
/// `macro_rules!` works like a template expanded at compile time: each invocation writes out a
/// full type, so the three id types share one definition without sharing a runtime type.
macro_rules! uuid_id {
    ($(#[$meta:meta])* $name:ident, $invalid:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(::uuid::Uuid);

        impl $name {
            /// Creates a new, unique id.
            #[must_use]
            pub fn generate() -> Self {
                Self(::uuid::Uuid::now_v7())
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::DomainError;

            fn from_str(input: &str) -> Result<Self, Self::Err> {
                ::uuid::Uuid::parse_str(input)
                    .map(Self)
                    .map_err(|_| $invalid)
            }
        }
    };
}

pub(crate) use uuid_id;

#[cfg(test)]
mod tests {
    use crate::{AccountId, DomainError, MailboxId, MessageId};

    #[test]
    fn generated_ids_are_unique_and_ordered() {
        let first = MessageId::generate();
        let second = MessageId::generate();

        assert_ne!(first, second, "two generated ids must differ");
        assert!(first < second, "ids must sort in creation order");
    }

    #[test]
    fn ids_round_trip_through_text() {
        let id = MailboxId::generate();

        let parsed: MailboxId = id.to_string().parse().unwrap();

        assert_eq!(parsed, id, "parsed id must equal the original");
    }

    #[test]
    fn each_id_type_reports_its_own_error() {
        assert_eq!(
            "x".parse::<AccountId>(),
            Err(DomainError::InvalidAccountId),
            "account id"
        );
        assert_eq!(
            "x".parse::<MailboxId>(),
            Err(DomainError::InvalidMailboxId),
            "mailbox id"
        );
        assert_eq!(
            "x".parse::<MessageId>(),
            Err(DomainError::InvalidMessageId),
            "message id"
        );
    }
}
