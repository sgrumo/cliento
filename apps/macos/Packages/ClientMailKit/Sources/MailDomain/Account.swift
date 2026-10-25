/// A mail account configured in the app.
public struct Account: Identifiable, Hashable, Sendable {
    /// Stable identifier assigned by the core.
    public let id: String
    /// Normalised email address.
    public let address: String
    /// Sender name, when the user set one.
    public let displayName: String?
    /// Service that syncs the account.
    public let provider: MailProvider

    /// Creates an account value.
    public init(id: String, address: String, displayName: String?, provider: MailProvider) {
        self.id = id
        self.address = address
        self.displayName = displayName
        self.provider = provider
    }

    /// The name to show for the account: the display name, or the address when there is none.
    public var title: String {
        displayName ?? address
    }
}
