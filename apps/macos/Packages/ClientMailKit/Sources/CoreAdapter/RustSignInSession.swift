import CoreBridge
import Foundation
import MailDomain

/// Implements ``SignInSession`` with the core's Google sign-in session.
final class RustSignInSession: SignInSession {
    let authorizationURL: URL
    let callbackScheme: String
    private let session: GoogleSignInSession

    /// Wraps a session the core started.
    ///
    /// - Throws: ``AccountServiceError/signInFailed`` when the core's URL is not a valid URL.
    init(_ session: GoogleSignInSession) throws(AccountServiceError) {
        guard let url = URL(string: session.authorizationUrl()) else { throw .signInFailed }
        self.session = session
        authorizationURL = url
        callbackScheme = session.callbackScheme()
    }

    func complete(callbackURL: URL) async throws(AccountServiceError) -> Account {
        let record = try await CoreCall.run {
            try await session.complete(callbackUrl: callbackURL.absoluteString)
        }
        return Account(record: record)
    }

    func cancel() {
        session.cancel()
    }
}
