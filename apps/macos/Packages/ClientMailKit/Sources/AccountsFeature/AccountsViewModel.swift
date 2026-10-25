import MailDomain
import Observation

/// State and actions of the Accounts screen.
@MainActor
@Observable
public final class AccountsViewModel {
    /// Configured accounts, oldest first.
    public private(set) var accounts: [Account] = []
    /// True while the first load runs.
    public private(set) var isLoading = false
    /// The last failure, shown as an alert. The view clears it when the alert closes.
    public var error: AccountServiceError?
    /// True while the add-account sheet is open.
    public var isAddingAccount = false
    /// Address typed in the add-account sheet.
    public var draftAddress = ""
    /// Display name typed in the add-account sheet.
    public var draftDisplayName = ""

    private let service: any AccountService

    /// Creates the view model.
    ///
    /// - Parameter service: Where accounts are read and written.
    public init(service: any AccountService) {
        self.service = service
    }

    /// True when the draft has enough input to submit.
    public var canSubmitDraft: Bool {
        !draftAddress.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// Loads the accounts.
    public func load() async {
        isLoading = true
        defer { isLoading = false }
        do {
            accounts = try await service.accounts()
        } catch {
            self.error = error
        }
    }

    /// Opens the add-account sheet with an empty draft.
    public func startAddingAccount() {
        draftAddress = ""
        draftDisplayName = ""
        isAddingAccount = true
    }

    /// Closes the add-account sheet without saving.
    public func cancelAddingAccount() {
        isAddingAccount = false
    }

    /// Stores the draft as a new account and closes the sheet on success.
    public func submitDraft() async {
        guard canSubmitDraft else { return }
        let displayName = draftDisplayName.trimmingCharacters(in: .whitespacesAndNewlines)
        do {
            let account = try await service.addAccount(
                address: draftAddress,
                displayName: displayName.isEmpty ? nil : displayName
            )
            accounts.append(account)
            isAddingAccount = false
        } catch {
            self.error = error
        }
    }

    /// Removes an account.
    ///
    /// - Parameter account: The account to remove.
    public func remove(_ account: Account) async {
        do {
            try await service.removeAccount(id: account.id)
            accounts.removeAll { $0.id == account.id }
        } catch {
            self.error = error
        }
    }
}
