import CoreBridge
import MailDomain

extension Account {
    /// Maps a record from the Rust core.
    init(record: AccountRecord) {
        self.init(
            id: record.id,
            address: record.address,
            displayName: record.displayName,
            provider: MailProvider(record.provider)
        )
    }
}

extension MailProvider {
    /// Maps a provider kind from the Rust core.
    init(_ kind: ProviderKind) {
        switch kind {
        case .gmail: self = .gmail
        case .outlook: self = .outlook
        case .fastmail: self = .fastmail
        case .icloud: self = .icloud
        case .genericImap: self = .genericImap
        }
    }
}
