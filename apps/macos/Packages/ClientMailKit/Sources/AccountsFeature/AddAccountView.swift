import SwiftUI

/// Sheet that collects an address and an optional name for a new account.
struct AddAccountView: View {
    @Bindable var viewModel: AccountsViewModel

    var body: some View {
        Form {
            TextField(Strings.emailAddressField, text: $viewModel.draftAddress)
                .textContentType(.emailAddress)
            TextField(Strings.displayNameField, text: $viewModel.draftDisplayName)
                .textContentType(.name)
        }
        .formStyle(.grouped)
        .navigationTitle(Strings.addAccount)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button(Strings.cancel, action: viewModel.cancelAddingAccount)
            }
            ToolbarItem(placement: .confirmationAction) {
                Button(Strings.add) {
                    Task { await viewModel.submitDraft() }
                }
                .disabled(!viewModel.canSubmitDraft)
            }
        }
    }
}
