import Foundation

/// One sign-in with a provider in the browser, from the URL to open to the stored account.
///
/// Used once: open ``authorizationURL`` in a browser sheet that waits for ``callbackScheme``, then
/// pass the URL it returns to ``complete(callbackURL:)``.
public protocol SignInSession: Sendable {
    /// The page to open in the browser sheet.
    var authorizationURL: URL { get }

    /// The URL scheme that ends the browser sheet.
    var callbackScheme: String { get }

    /// Finishes the sign-in and stores the account and its credentials.
    ///
    /// - Parameter callbackURL: The URL the browser sheet returned.
    /// - Returns: The new account, or the existing one when the user signed in to it again.
    func complete(callbackURL: URL) async throws(AccountServiceError) -> Account

    /// Stops the sign-in. A waiting ``complete(callbackURL:)`` throws
    /// ``AccountServiceError/signInCancelled``.
    func cancel()
}
