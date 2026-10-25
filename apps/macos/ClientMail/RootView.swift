import AccountsFeature
import SwiftUI

/// Shows the app, or a startup failure when the database could not be opened.
struct RootView: View {
    let startup: Startup

    var body: some View {
        switch startup {
        case .ready(let accounts):
            NavigationStack {
                AccountsView(viewModel: accounts)
            }
        case .failed:
            ContentUnavailableView(
                "ClientMail could not open its data",
                systemImage: "exclamationmark.triangle",
                description: Text("Quit and reopen the app. If it keeps happening, report a bug.")
            )
        }
    }
}
