/// The mail service behind an account.
public enum MailProvider: Hashable, Sendable, CaseIterable {
    /// Gmail and Google Workspace.
    case gmail
    /// Outlook.com and Microsoft 365.
    case outlook
    /// Fastmail.
    case fastmail
    /// iCloud Mail.
    case icloud
    /// Any other IMAP server.
    case genericImap
}
