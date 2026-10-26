use clientmail_domain::{MailboxPath, MailboxRole};
use imap_next::imap_types::flag::FlagNameAttribute;

/// Returns what a folder is for, from its name and its attributes (RFC 6154 and Gmail's
/// `\Important`).
pub(crate) fn role(path: &MailboxPath, attributes: &[FlagNameAttribute<'_>]) -> MailboxRole {
    // `MailboxPath` stores the inbox in any spelling as `INBOX`.
    if path.as_str() == "INBOX" {
        return MailboxRole::Inbox;
    }
    attributes
        .iter()
        .find_map(|attribute| role_of(&attribute.to_string()))
        .unwrap_or(MailboxRole::Custom)
}

/// Maps one attribute, written as the server sends it (`\All`), to a role.
fn role_of(attribute: &str) -> Option<MailboxRole> {
    const ROLES: [(&str, MailboxRole); 8] = [
        ("\\All", MailboxRole::AllMail),
        ("\\Archive", MailboxRole::Archive),
        ("\\Drafts", MailboxRole::Drafts),
        ("\\Sent", MailboxRole::Sent),
        ("\\Trash", MailboxRole::Trash),
        ("\\Junk", MailboxRole::Junk),
        ("\\Flagged", MailboxRole::Starred),
        ("\\Important", MailboxRole::Important),
    ];
    ROLES
        .iter()
        .find(|(name, _)| attribute.eq_ignore_ascii_case(name))
        .map(|(_, role)| *role)
}

/// Returns whether a folder can hold messages. `\Noselect` folders only group others;
/// `\NonExistent` ones (RFC 5258) are listed only because they have children.
pub(crate) fn is_selectable(attributes: &[FlagNameAttribute<'_>]) -> bool {
    !attributes.iter().any(|attribute| {
        matches!(attribute, FlagNameAttribute::Noselect)
            || attribute.to_string().eq_ignore_ascii_case("\\NonExistent")
    })
}

#[cfg(test)]
mod tests {
    use imap_next::imap_types::core::Atom;

    use super::*;

    fn path(name: &str) -> MailboxPath {
        MailboxPath::new(name, Some('/')).unwrap()
    }

    fn attribute(name: &'static str) -> FlagNameAttribute<'static> {
        FlagNameAttribute::from(Atom::try_from(name).unwrap())
    }

    #[test]
    fn special_use_attributes_give_roles() {
        let cases = [
            ("All", MailboxRole::AllMail),
            ("Archive", MailboxRole::Archive),
            ("Drafts", MailboxRole::Drafts),
            ("Sent", MailboxRole::Sent),
            ("Trash", MailboxRole::Trash),
            ("Junk", MailboxRole::Junk),
            ("Flagged", MailboxRole::Starred),
            ("important", MailboxRole::Important),
        ];

        for (name, expected) in cases {
            let attributes = [attribute("HasNoChildren"), attribute(name)];

            assert_eq!(role(&path("[Gmail]/X"), &attributes), expected, "{name}");
        }
    }

    #[test]
    fn attributes_that_only_start_like_a_role_are_custom() {
        let attributes = [attribute("AllMail"), attribute("Sentinel")];

        assert_eq!(
            role(&path("Work"), &attributes),
            MailboxRole::Custom,
            "no prefix match"
        );
    }

    #[test]
    fn the_inbox_is_found_by_name_in_any_case() {
        assert_eq!(role(&path("inbox"), &[]), MailboxRole::Inbox, "lower case");
        assert_eq!(
            role(&path("INBOX/Receipts"), &[]),
            MailboxRole::Custom,
            "a child of the inbox is not the inbox"
        );
    }

    #[test]
    fn noselect_and_nonexistent_folders_are_not_selectable() {
        assert!(!is_selectable(&[attribute("Noselect")]), "\\Noselect");
        assert!(!is_selectable(&[attribute("NonExistent")]), "\\NonExistent");
        assert!(
            is_selectable(&[attribute("HasNoChildren"), attribute("Sent")]),
            "a normal folder"
        );
    }
}
