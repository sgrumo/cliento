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

    @Test func everyErrorHasAMessage() {
        let errors: [AccountServiceError] = [
            .invalidAddress, .duplicateAccount, .accountNotFound, .storage, .secretStorageUnavailable,
            .secretStorage,
        ]

        let messages = errors.map(Strings.message(for:))

        #expect(Set(messages).count == errors.count, "each error has its own message")
        #expect(messages.allSatisfy { !$0.isEmpty })
    }
}
