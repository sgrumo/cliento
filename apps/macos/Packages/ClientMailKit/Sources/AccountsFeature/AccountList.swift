import MailDomain
import SwiftUI

/// The account rows, or an empty state when there are none.
struct AccountList: View {
    let viewModel: AccountsViewModel

    var body: some View {
        if viewModel.accounts.isEmpty, !viewModel.isLoading {
            ContentUnavailableView(
                Strings.noAccountsTitle,
                systemImage: "tray",
                description: Text(Strings.noAccountsDescription)
            )
        } else {
            List(viewModel.accounts) { account in
                AccountRow(account: account)
                    .contextMenu {
                        Button(Strings.removeAccount, systemImage: "trash", role: .destructive) {
                            Task { await viewModel.remove(account) }
                        }
                    }
            }
        }
    }
}
