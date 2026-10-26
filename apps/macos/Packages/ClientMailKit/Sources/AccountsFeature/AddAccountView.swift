import AuthenticationServices
import SwiftUI

/// Sheet that adds an account: sign in with Google, or an address and an optional name.
struct AddAccountView: View {
    @Bindable var viewModel: AccountsViewModel
    @Environment(\.webAuthenticationSession) private var webAuthenticationSession

    var body: some View {
        Form {
            if viewModel.isGoogleSignInAvailable {
                Section {
                    Button(Strings.signInWithGoogle, systemImage: "person.badge.key", action: signIn)
                        .disabled(viewModel.isSigningIn)
                    if viewModel.isSigningIn {
                        ProgressView(Strings.signingIn)
                    }
                } footer: {
                    Text(Strings.signInWithGoogleFooter)
                }
            }
            Section {
                TextField(Strings.emailAddressField, text: $viewModel.draftAddress)
                    .textContentType(.emailAddress)
                TextField(Strings.displayNameField, text: $viewModel.draftDisplayName)
                    .textContentType(.name)
            }
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

    private func signIn() {
        Task {
            await viewModel.signInWithGoogle { url, callbackScheme in
                do {
                    // Ephemeral: no cookies shared with Safari or an earlier sign-in, so adding a
                    // second Google account does not reuse the first one's login.
                    return try await webAuthenticationSession.authenticate(
                        using: url,
                        callback: .customScheme(callbackScheme),
                        preferredBrowserSession: .ephemeral,
                        additionalHeaderFields: [:]
                    )
                } catch ASWebAuthenticationSessionError.canceledLogin {
                    throw CancellationError()
                }
            }
        }
    }
}
