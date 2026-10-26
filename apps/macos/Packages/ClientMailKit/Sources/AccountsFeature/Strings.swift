import Foundation
import MailDomain

/// Localised strings of this feature, read from the module's `Localizable.xcstrings`.
///
/// Swift packages cannot use Xcode's generated string symbols, so this enum plays that role.
enum Strings {
    static var accountsTitle: String {
        String(localized: "accountsTitle", defaultValue: "Accounts", bundle: .module)
    }
    static var addAccount: String {
        String(localized: "addAccount", defaultValue: "Add Account", bundle: .module)
    }
    static var add: String {
        String(localized: "add", defaultValue: "Add", bundle: .module)
    }
    static var cancel: String {
        String(localized: "cancel", defaultValue: "Cancel", bundle: .module)
    }
    static var removeAccount: String {
        String(localized: "removeAccount", defaultValue: "Remove Account", bundle: .module)
    }
    static var emailAddressField: String {
        String(localized: "emailAddressField", defaultValue: "Email address", bundle: .module)
    }
    static var displayNameField: String {
        String(localized: "displayNameField", defaultValue: "Name (optional)", bundle: .module)
    }
    static var noAccountsTitle: String {
        String(localized: "noAccountsTitle", defaultValue: "No Accounts", bundle: .module)
    }
    static var noAccountsDescription: String {
        String(
            localized: "noAccountsDescription",
            defaultValue: "Add an email account to get started.",
            bundle: .module
        )
    }
    static var errorTitle: String {
        String(localized: "errorTitle", defaultValue: "Something Went Wrong", bundle: .module)
    }
    static var ok: String {
        String(localized: "ok", defaultValue: "OK", bundle: .module)
    }

    /// Returns the message for an account error.
    static func message(for error: AccountServiceError) -> String {
        switch error {
        case .invalidAddress:
            String(
                localized: "errorInvalidAddress",
                defaultValue: "That email address is not valid.",
                bundle: .module
            )
        case .duplicateAccount:
            String(
                localized: "errorDuplicateAccount",
                defaultValue: "This account is already in ClientMail.",
                bundle: .module
            )
        case .accountNotFound:
            String(
                localized: "errorAccountNotFound",
                defaultValue: "This account no longer exists.",
                bundle: .module
            )
        case .storage:
            String(
                localized: "errorStorage",
                defaultValue: "ClientMail could not read or save its data.",
                bundle: .module
            )
        case .secretStorageUnavailable:
            String(
                localized: "errorSecretStorageUnavailable",
                defaultValue: "The Keychain is locked. Unlock your Mac and try again.",
                bundle: .module
            )
        case .secretStorage:
            String(
                localized: "errorSecretStorage",
                defaultValue: "ClientMail could not use the Keychain. Check Keychain Access for problems.",
                bundle: .module
            )
        }
    }

    /// Returns the name of a mail provider.
    static func name(of provider: MailProvider) -> String {
        switch provider {
        case .gmail: String(localized: "providerGmail", defaultValue: "Gmail", bundle: .module)
        case .outlook:
            String(localized: "providerOutlook", defaultValue: "Outlook", bundle: .module)
        case .fastmail:
            String(localized: "providerFastmail", defaultValue: "Fastmail", bundle: .module)
        case .icloud:
            String(localized: "providerIcloud", defaultValue: "iCloud Mail", bundle: .module)
        case .genericImap:
            String(localized: "providerGenericImap", defaultValue: "IMAP", bundle: .module)
        }
    }
}
