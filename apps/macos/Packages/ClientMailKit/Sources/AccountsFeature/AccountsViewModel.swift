import Foundation
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
    /// True when this build can sign in with Google.
    public private(set) var isGoogleSignInAvailable = false
    /// True while a Google sign-in runs.
    public private(set) var isSigningIn = false

    private let service: any AccountService
    private var signInSession: (any SignInSession)?
    /// Set when the sheet closes during a sign-in, so a session that is still being started
    /// stops before the browser opens.
    private var isSignInAbandoned = false

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
        isGoogleSignInAvailable = await service.isGoogleSignInAvailable()
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

    /// Closes the add-account sheet without saving, and stops a running sign-in.
    public func cancelAddingAccount() {
        signInSession?.cancel()
        isSignInAbandoned = isSigningIn
        isAddingAccount = false
    }

    /// Signs in with Google and adds the account, or updates it when it is already there.
    ///
    /// Closing the browser or refusing access ends quietly; other failures show the error. A
    /// cancel can land after Google issued the tokens, when the account is already stored, so a
    /// cancelled sign-in reloads the list.
    ///
    /// - Parameter authenticate: Opens the URL in a browser sheet that waits for the scheme and
    ///   returns the URL it ended on, or throws `CancellationError` when the user closed it. The
    ///   view passes SwiftUI's `WebAuthenticationSession`.
    public func signInWithGoogle(
        authenticate: (_ url: URL, _ callbackScheme: String) async throws -> URL
    ) async {
        guard !isSigningIn else { return }
        isSigningIn = true
        isSignInAbandoned = false
        defer {
            isSigningIn = false
            signInSession = nil
        }
        let session: any SignInSession
        do {
            session = try await service.beginGoogleSignIn()
        } catch {
            self.error = error
            return
        }
        guard !isSignInAbandoned else {
            session.cancel()
            return
        }
        signInSession = session
        let callbackURL: URL
        do {
            callbackURL = try await authenticate(session.authorizationURL, session.callbackScheme)
        } catch {
            session.cancel()
            if !(error is CancellationError) {
                self.error = .signInFailed
            }
            return
        }
        do {
            let account = try await session.complete(callbackURL: callbackURL)
            if let index = accounts.firstIndex(where: { $0.id == account.id }) {
                accounts[index] = account
            } else {
                accounts.append(account)
            }
            isAddingAccount = false
        } catch {
            switch error {
            case .signInCancelled: await reloadAccounts()
            case .signInDenied: break
            default: self.error = error
            }
        }
    }

    private func reloadAccounts() async {
        do {
            accounts = try await service.accounts()
        } catch {
            self.error = error
        }
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
