import Foundation
import MailDomain
import Testing

@testable import AccountsFeature

@MainActor
struct AccountsViewModelTests {
    private let ada = Account(id: "1", address: "ada@example.com", displayName: "Ada", provider: .gmail)

    @Test func loadPublishesAccounts() async {
        let viewModel = AccountsViewModel(service: FakeAccountService(accounts: [ada]))

        await viewModel.load()

        #expect(viewModel.accounts == [ada])
        #expect(!viewModel.isLoading)
        #expect(viewModel.error == nil)
    }

    @Test func loadFailureSetsError() async {
        let service = FakeAccountService()
        await service.failNextCall(with: .storage)
        let viewModel = AccountsViewModel(service: service)

        await viewModel.load()

        #expect(viewModel.error == .storage)
        #expect(viewModel.accounts.isEmpty)
    }

    @Test func startAddingAccountClearsDraft() {
        let viewModel = AccountsViewModel(service: FakeAccountService())
        viewModel.draftAddress = "old@example.com"

        viewModel.startAddingAccount()

        #expect(viewModel.isAddingAccount)
        #expect(viewModel.draftAddress.isEmpty)
        #expect(!viewModel.canSubmitDraft)
    }

    @Test func submitDraftAddsAccountAndClosesSheet() async {
        let service = FakeAccountService()
        let viewModel = AccountsViewModel(service: service)
        viewModel.startAddingAccount()
        viewModel.draftAddress = "ada@example.com"
        viewModel.draftDisplayName = "  "

        await viewModel.submitDraft()

        #expect(!viewModel.isAddingAccount)
        #expect(viewModel.accounts.map(\.address) == ["ada@example.com"])
        #expect(await service.stored.first?.displayName == nil, "blank name is sent as nil")
    }

    @Test func submitDraftFailureKeepsSheetOpen() async {
        let service = FakeAccountService()
        await service.failNextCall(with: .duplicateAccount)
        let viewModel = AccountsViewModel(service: service)
        viewModel.startAddingAccount()
        viewModel.draftAddress = "ada@example.com"

        await viewModel.submitDraft()

        #expect(viewModel.isAddingAccount)
        #expect(viewModel.error == .duplicateAccount)
        #expect(viewModel.accounts.isEmpty)
    }

    @Test func submitDraftIgnoresBlankAddress() async {
        let service = FakeAccountService()
        let viewModel = AccountsViewModel(service: service)
        viewModel.startAddingAccount()
        viewModel.draftAddress = "   "

        await viewModel.submitDraft()

        #expect(await service.stored.isEmpty)
        #expect(viewModel.isAddingAccount)
    }

    @Test func removeDeletesAccount() async {
        let service = FakeAccountService(accounts: [ada])
        let viewModel = AccountsViewModel(service: service)
        await viewModel.load()

        await viewModel.remove(ada)

        #expect(viewModel.accounts.isEmpty)
        #expect(await service.stored.isEmpty)
    }

    @Test func loadReportsWhetherGoogleSignInIsAvailable() async {
        let service = FakeAccountService()
        await service.offerGoogleSignIn(FakeSignInSession(result: .success(ada)))
        let viewModel = AccountsViewModel(service: service)

        await viewModel.load()

        #expect(viewModel.isGoogleSignInAvailable)
    }

    @Test func googleSignInAddsTheAccountAndClosesTheSheet() async {
        let session = FakeSignInSession(result: .success(ada))
        let service = FakeAccountService()
        await service.offerGoogleSignIn(session)
        let viewModel = AccountsViewModel(service: service)
        viewModel.startAddingAccount()
        let callback = URL(string: "test:/done?code=1")!

        await viewModel.signInWithGoogle { url, scheme in
            #expect(url == session.authorizationURL)
            #expect(scheme == "test")
            return callback
        }

        #expect(viewModel.accounts == [ada])
        #expect(!viewModel.isAddingAccount)
        #expect(!viewModel.isSigningIn)
        #expect(session.callbackURL == callback)
    }

    @Test func signingInAgainReplacesTheListedAccount() async {
        let renamed = Account(id: ada.id, address: ada.address, displayName: "Ada L.", provider: .gmail)
        let service = FakeAccountService(accounts: [ada])
        await service.offerGoogleSignIn(FakeSignInSession(result: .success(renamed)))
        let viewModel = AccountsViewModel(service: service)
        await viewModel.load()

        await viewModel.signInWithGoogle { _, _ in URL(string: "test:/done")! }

        #expect(viewModel.accounts == [renamed])
    }

    @Test func signingInAgainKeepsTheAccountsPlace() async {
        let bob = Account(id: "2", address: "bob@example.com", displayName: nil, provider: .gmail)
        let renamed = Account(id: ada.id, address: ada.address, displayName: "Ada L.", provider: .gmail)
        let service = FakeAccountService(accounts: [ada, bob])
        await service.offerGoogleSignIn(FakeSignInSession(result: .success(renamed)))
        let viewModel = AccountsViewModel(service: service)
        await viewModel.load()

        await viewModel.signInWithGoogle { _, _ in URL(filePath: "/done") }

        #expect(viewModel.accounts == [renamed, bob])
    }

    @Test func closingTheBrowserCancelsQuietly() async {
        let session = FakeSignInSession(result: .success(ada))
        let service = FakeAccountService()
        await service.offerGoogleSignIn(session)
        let viewModel = AccountsViewModel(service: service)
        viewModel.startAddingAccount()

        await viewModel.signInWithGoogle { _, _ in throw CancellationError() }

        #expect(session.cancels == 1)
        #expect(session.callbackURL == nil, "never completed")
        #expect(viewModel.error == nil)
        #expect(viewModel.isAddingAccount, "sheet stays open")
    }

    @Test(arguments: [AccountServiceError.signInDenied, .signInCancelled])
    func deniedOrCancelledSignInShowsNoError(failure: AccountServiceError) async {
        let service = FakeAccountService()
        await service.offerGoogleSignIn(FakeSignInSession(result: .failure(failure)))
        let viewModel = AccountsViewModel(service: service)

        await viewModel.signInWithGoogle { _, _ in URL(string: "test:/done")! }

        #expect(viewModel.error == nil)
        #expect(viewModel.accounts.isEmpty)
    }

    @Test func otherSignInFailuresShowTheError() async {
        let service = FakeAccountService()
        await service.offerGoogleSignIn(FakeSignInSession(result: .failure(.network)))
        let viewModel = AccountsViewModel(service: service)

        await viewModel.signInWithGoogle { _, _ in URL(string: "test:/done")! }

        #expect(viewModel.error == .network)
    }

    @Test func otherBrowserErrorsShowSignInFailed() async {
        struct PresentationFailure: Error {}
        let session = FakeSignInSession(result: .success(ada))
        let service = FakeAccountService()
        await service.offerGoogleSignIn(session)
        let viewModel = AccountsViewModel(service: service)

        await viewModel.signInWithGoogle { _, _ in throw PresentationFailure() }

        #expect(viewModel.error == .signInFailed)
        #expect(session.cancels == 1)
    }

    @Test func aCancelledSignInReloadsTheList() async {
        let service = FakeAccountService()
        await service.offerGoogleSignIn(FakeSignInSession(result: .failure(.signInCancelled)))
        await service.store(ada)
        let viewModel = AccountsViewModel(service: service)

        await viewModel.signInWithGoogle { _, _ in URL(filePath: "/done") }

        #expect(viewModel.accounts == [ada], "stored before the cancel landed")
        #expect(viewModel.error == nil)
    }

    @Test func closingTheSheetBeforeTheBrowserOpensStopsTheSignIn() async {
        let session = FakeSignInSession(result: .success(ada))
        let service = FakeAccountService()
        await service.offerGoogleSignIn(session)
        let viewModel = AccountsViewModel(service: service)
        viewModel.startAddingAccount()
        await service.onBeginGoogleSignIn { await viewModel.cancelAddingAccount() }
        var browserOpened = false

        await viewModel.signInWithGoogle { _, _ in
            browserOpened = true
            return URL(filePath: "/done")
        }

        #expect(!browserOpened)
        #expect(session.cancels == 1)
        #expect(viewModel.accounts.isEmpty)
    }

    @Test func cancellingTheSheetCancelsARunningSignIn() async {
        let session = FakeSignInSession(result: .success(ada))
        let service = FakeAccountService()
        await service.offerGoogleSignIn(session)
        let viewModel = AccountsViewModel(service: service)
        viewModel.startAddingAccount()

        await viewModel.signInWithGoogle { _, _ in
            viewModel.cancelAddingAccount()
            throw CancellationError()
        }

        #expect(session.cancels == 2, "once by the sheet, once when the browser closed")
        #expect(!viewModel.isAddingAccount)
    }

    @Test func everyErrorHasAMessage() {
        let errors: [AccountServiceError] = [
            .invalidAddress, .duplicateAccount, .accountNotFound, .storage, .secretStorageUnavailable,
            .secretStorage, .signInUnavailable, .signInCancelled, .signInDenied, .signInFailed, .network,
            .mailAccessNotGranted,
        ]

        let messages = errors.map(Strings.message(for:))

        #expect(Set(messages).count == errors.count, "each error has its own message")
        #expect(messages.allSatisfy { !$0.isEmpty })
    }
}
