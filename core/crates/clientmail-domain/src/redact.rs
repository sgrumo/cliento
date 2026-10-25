use std::fmt;

/// Stands in for personal data in `Debug` output.
///
/// Types that hold names, addresses, subjects or folder names implement `Debug` by hand and print
/// this instead of the value, so `tracing::debug!(?value)` can never leak them into logs.
pub(crate) struct Redacted;

impl fmt::Debug for Redacted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// Returns `Some(Redacted)` when `value` is set, so `Debug` still shows whether a field is empty.
pub(crate) const fn redact_option<T>(value: Option<&T>) -> Option<Redacted> {
    match value {
        Some(_) => Some(Redacted),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        EmailAddress, HeaderFields, MailboxPath, MessageHeader, NewAccount, Participant, UnixTime,
    };

    const SECRET: &str = "secret";

    #[test]
    fn debug_output_never_contains_personal_data() {
        let address = EmailAddress::parse("secret@secret.example").unwrap();
        let participant = Participant::new(Some(SECRET), "secret@secret.example").unwrap();
        let path = MailboxPath::new("secret/secret", Some('/')).unwrap();
        let account = NewAccount::parse("secret@secret.example", Some(SECRET))
            .unwrap()
            .into_account();
        let fields = HeaderFields {
            subject: Some(SECRET.to_owned()),
            from: vec![participant.clone()],
            to: vec![participant.clone()],
            cc: Vec::new(),
            bcc: Vec::new(),
            reply_to: Vec::new(),
            sent_at: None,
            received_at: UnixTime::from_seconds(0),
            message_id: Some("<secret@secret.example>".to_owned()),
            in_reply_to: None,
            references: vec!["<secret@secret.example>".to_owned()],
            list_id: Some("<secret.secret.example>".to_owned()),
            has_list_unsubscribe: true,
            size_bytes: 1,
        };
        let header = MessageHeader::from_fields(fields.clone());

        let outputs = [
            format!("{address:?}"),
            format!("{participant:?}"),
            format!("{path:?}"),
            format!("{account:?}"),
            format!("{fields:?}"),
            format!("{header:?}"),
        ];

        for output in outputs {
            assert!(!output.contains(SECRET), "personal data leaked: {output}");
            assert!(
                output.contains("<redacted>"),
                "placeholder missing: {output}"
            );
        }
    }
}
