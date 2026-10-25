import MailDomain
import SwiftUI

/// Lists the configured accounts and lets the user add or remove them.
public struct AccountsView: View {
    @Bindable private var viewModel: AccountsViewModel

    /// Creates the view.
    ///
    /// - Parameter viewModel: The screen state, owned by the caller.
    public init(viewModel: AccountsViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
        AccountList(viewModel: viewModel)
            .navigationTitle(Strings.accountsTitle)
            .toolbar {
                Button(Strings.addAccount, systemImage: "plus", action: viewModel.startAddingAccount)
                    .keyboardShortcut("n", modifiers: [.command, .shift])
            }
            .sheet(isPresented: $viewModel.isAddingAccount) {
                AddAccountView(viewModel: viewModel)
            }
            .alert(
                Strings.errorTitle,
                isPresented: Binding(
                    get: { viewModel.error != nil },
                    set: { isPresented in if !isPresented { viewModel.error = nil } }
                ),
                presenting: viewModel.error
            ) { _ in
                Button(Strings.ok, role: .cancel) {}
            } message: { error in
                Text(Strings.message(for: error))
            }
            .task { await viewModel.load() }
    }
}
