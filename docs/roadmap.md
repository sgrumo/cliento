# Roadmap

Each milestone ends with `make check` green and a usable app. Acceptance criteria are what a
reviewer verifies by hand.

## M0: Foundations (done)

- Rust workspace with domain, store, engine and FFI crates; strict clippy; tests at every layer.
- UniFFI bridge built as an XCFramework; Swift package with domain, adapter and Accounts feature.
- XcodeGen app target, sandboxed, ad-hoc signed.
- **Accept:** add, list and remove accounts; they persist across launches.

## M1: Sign-in and first sync (Gmail)

- OAuth 2.0 with PKCE through `ASWebAuthenticationSession`; tokens in the Keychain.
- `clientmail-imap` crate: connect, authenticate with XOAUTH2, list folders, fetch the last 30 days
  of inbox headers.
- Store: `mailboxes` and `messages` tables; bodies on disk.
- Engine emits change events through a UniFFI callback interface.
- UI: three-pane window (sidebar, message list, reader).
- **Accept:** sign in to Gmail, see the inbox within 5 seconds, quit offline and still read it.

## M2: Reading and actions

- HTML sanitising and a locked-down `WKWebView` reader; remote content blocked.
- Threading (Gmail thread ids; JWZ algorithm for plain IMAP).
- Operation queue: archive, delete, mark read, star, move, all with undo.
- IMAP IDLE on the inbox.
- **Accept:** archive 20 messages offline, reconnect, and the server matches.

## M3: Compose and send

- Composer: reply, reply all, forward, new message; drafts saved locally and to the server.
- `clientmail-smtp` crate (or Gmail's SMTP with XOAUTH2); outbox with retry.
- Signatures per account.
- **Accept:** reply with an attachment while offline; it sends on reconnect.

## M4: Search and multiple accounts

- FTS5 index, search operators (`from:`, `has:attachment`, date ranges).
- Unified inbox across accounts.
- **Accept:** search 50,000 messages in under 100 ms.

## M5: Smart features

- Smart Inbox groups with explanations; sender screening.
- Snooze, send later, follow-up reminders.
- Templates, quick replies, pins, command palette.
- **Accept:** every feature in `docs/features.md` except notifications agent.

## M6: More providers

- Generic IMAP with server discovery (autoconfig, SRV records).
- Fastmail; iCloud (app-specific passwords).
- Microsoft 365 and Outlook.com through Microsoft Graph.
- **Accept:** one account of each kind syncs side by side.

## M7: Release

- Notarised Developer ID build, Sparkle updates, crash reporting that sends no personal data.
- Background notifications agent.
- Performance budget enforced in CI: launch time and memory.
