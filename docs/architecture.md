# Architecture

## Goals

1. **Light.** Starts in under a second, stays under 150 MB of memory with several accounts.
2. **Local-first.** Every action applies to the local database at once and syncs later. The app
   works offline.
3. **Private.** No server of ours ever holds mail or credentials. Sorting and search run on the Mac.
4. **Native.** Feels like a Mac app: keyboard-first, menus, standard controls, system sharing.

## Layers

```
┌───────────────────────────────────────────────────────────────┐
│ ClientMail.app        composition root (Startup, RootView)    │
├───────────────────────────────────────────────────────────────┤
│ <Name>Feature         SwiftUI views + @Observable view models │
│ MailDomain            Swift models + service protocols        │
│ CoreAdapter           implements MailDomain with CoreBridge   │
│ CoreBridge            UniFFI-generated Swift (never edited)   │
╞═══════════════════════ C ABI (static library) ════════════════╡
│ clientmail-ffi        UniFFI API, composes everything below   │
│ clientmail-engine     use cases: accounts, sync, actions      │
│ clientmail-store      SQLite: schema, migrations, queries     │
│ clientmail-oauth      Google OAuth: PKCE, token exchange      │
│ clientmail-imap       IMAP via imap-next: TLS, XOAUTH2 (0011) │
│ clientmail-mime       RFC 5322 headers (mail-parser)          │
│ clientmail-<proto>    IMAP / Gmail / Graph / SMTP adapters     │
│ clientmail-domain     types, validation, port traits          │
└───────────────────────────────────────────────────────────────┘
```

`core/vendor/` holds patched copies of third-party crates (Gmail's fetch items in `imap-codec`,
decision record 0012), outside the workspace.

Dependencies point down. `clientmail-domain` depends on nothing in the repo. The engine knows only
the port traits; storage and protocol crates implement them. The FFI crate is the only place that
picks concrete implementations. On the Swift side, features see only `MailDomain` protocols, so
they are tested with fakes and never touch Rust directly.

## Platform adapters

The core needs a few services only macOS offers. Each one is a port in `clientmail-domain`, a
UniFFI callback interface in `clientmail-ffi`, and a Swift implementation in `CoreAdapter` that
the app hands to the core when it opens it (decision record 0007). The adapters hold no rules.

| Service | Port | Swift adapter | Since |
| --- | --- | --- | --- |
| Secret storage | `SecretStore` / `SecretStoreBridge` | `KeychainSecretStore` over `SecItem` | 1.3 |

Keychain items are generic passwords with service `dev.samuelemedici.clientmail` and account
`<account id>:<secret kind>`, readable after the first unlock, bound to this Mac
(`...ThisDeviceOnly`: never synced through iCloud, never restored onto another Mac). They
live in the login keychain until the app is team-signed (7.2), then move to the data-protection
keychain.

## Data flow

**Commands** go down: a view model calls a `MailDomain` protocol, `CoreAdapter` awaits the
generated Swift, which calls the Rust `CoreClient`, which runs an engine use case on its runtime.

**Sign-in** (Google, from 1.4): the core builds the authorization URL with a PKCE verifier and a
random state; the Accounts sheet opens it in an ephemeral `WebAuthenticationSession` and passes
the callback URL back. The core checks the callback's redirect path and state, exchanges the
code over HTTPS (`clientmail-oauth`), refuses a grant without mail access, then stores the account
row and only then its refresh token. Signing in again to an existing account writes the new token
under the account's lock, only if the account still exists. Access tokens stay in memory. Google
OAuth setup: `docs/google-oauth.md`.

**Changes** come up. Today the view model refreshes itself after each call. From milestone 2,
the core pushes change events through a UniFFI callback interface, so sync results reach the UI
without polling.

## Threading

- The Rust API is `async`; `CoreAdapter` awaits it. Swift's executor polls each call's Rust
  future, which only converts arguments and hands the work to the core's threads; no Swift thread
  waits while that work runs. UniFFI's Swift bindings do not support cancellation: a cancelled task
  keeps waiting for the call to finish. Opening the database is the exception to all this:
  `CoreClient::open` is synchronous and runs at launch on the main actor.
- Each `CoreClient` owns one multi-thread Tokio runtime (`CoreRuntime` in `clientmail-ffi`,
  decision record 0008): two workers for async network work, at most eight blocking threads, all
  named `clientmail-core`.
- Nothing blocks a Tokio worker. SQLite calls and platform callbacks (the Keychain) run on the
  runtime's blocking pool, so platform callbacks stay synchronous.
- Async work (HTTPS now, IMAP later) runs on the workers through `CoreRuntime::spawn`. Async
  engine use cases reach the synchronous ports through the `Blocking` port, which the FFI crate
  implements with the blocking pool (decision record 0009).
- Writing an account's token and removing that account take the same per-account lock in the
  engine, so a refreshed token never outlives its account.
- View models are `@MainActor`.
- The store serialises access to one SQLite connection with a `Mutex`. When sync arrives, the
  store gets a pool: one writer, several readers (SQLite WAL mode is already on).

## Storage

- One SQLite database per installation, in the app's sandbox container under
  `Application Support/ClientMail/mail.sqlite`.
- `STRICT` tables. Schema changes are append-only migrations tracked by `PRAGMA user_version`,
  kept as `.sql` files in `clientmail-store/migrations/` and embedded at compile time. Each
  migration runs with foreign keys off and must pass `foreign_key_check`, so rebuilding a table
  never cascades into its child rows.
- Tables: `accounts`, `mailboxes` (with each mailbox's sync position), `messages`, and child tables
  for participants, references, keywords and Gmail labels. The layout and its trade-offs are in
  decision record 0006.
- Writes of many messages run in one transaction; the message list reads one page at a time with a
  `(received_at, id)` cursor.
- Message bodies and attachments live as files next to the database (`blobs/ab/cdef...`), named by
  the SHA-256 of their content, written atomically and checked against their hash on read, so the
  database stays small and fast.
- Full-text search uses SQLite FTS5 over subject, participants and plain-text bodies.
- Credentials and OAuth tokens live in the macOS Keychain, never in the database.

## Identity model

- Accounts, mailboxes and messages have local UUID v7 ids (`AccountId`, `MailboxId`,
  `MessageId`). They never change, so the store and the UI key on them.
- Each message also keeps a `RemoteMessageRef` that says where it lives on the server:
  - plain IMAP: mailbox, `UIDVALIDITY` and `UID`. A new `UIDVALIDITY` invalidates every `UID` in
    that mailbox, and the sync engine re-matches messages and upserts them with their old
    ids and new keys (the store matches on id first). This mailbox is the single source of
    truth for where a plain IMAP message is;
  - Gmail: `X-GM-MSGID` (same in every label, so a message with several labels is stored once),
    `X-GM-THRID` (the conversation), and the `UIDVALIDITY` and `UID` in `[Gmail]/All Mail`, which
    every write and expunge check uses. Labels are a separate many-to-many relation.
- Mailboxes are keyed on `MailboxId`, not on their name: folders get renamed.
- `MailboxRole` keeps `AllMail` (`\All`, usually virtual) apart from `Archive` (`\Archive`, where
  archived mail moves).
- The message list sorts by the time the server received a message, not by the sender's `Date`.
- Header participants keep addresses that are not RFC-valid as raw text instead of rejecting them,
  so malformed mail still shows. Participants with a blank address are dropped.
- Domain types that hold personal data print `<redacted>` in `Debug`, so logging them is safe.

## Sync model (planned)

- One sync task per account. It keeps an IMAP IDLE connection on the inbox and polls other folders
  on a schedule.
- Local actions (archive, read, move, snooze) go into an operation queue in the database first.
  The UI updates at once; the queue replays to the server and retries with backoff.
- Conflicts resolve in favour of the server for message state, and in favour of the user for
  actions still in the queue.
- Gmail uses IMAP with Gmail extensions (`X-GM-LABELS`, `X-GM-THRID`, `X-GM-RAW`).
  Microsoft accounts use Microsoft Graph. Everything else uses IMAP and SMTP.

## Rendering mail

HTML mail renders in a `WKWebView` with JavaScript disabled, remote content blocked until the user
allows it, and a strict content security policy. The core sanitises HTML and strips tracking
pixels before it reaches the view.

## Testing strategy

| Layer | Kind | Uses |
| --- | --- | --- |
| `clientmail-domain` | unit | pure values |
| `clientmail-store` | unit | in-memory SQLite |
| `clientmail-engine` | unit | fake repositories |
| `clientmail-ffi` | integration | real engine + in-memory or temp-file SQLite |
| `clientmail-imap` | integration | scripted server transcripts over an in-memory pipe |
| `MailDomain` | unit | pure values |
| `CoreAdapter` | integration | real Rust core through the XCFramework |
| `<Name>Feature` | unit | fake services |

No test in `make check` talks to a real mail server. Protocol crates are tested against server
transcripts: text files of `C:` and `S:` lines that a scripted server plays and checks over an
in-memory pipe (`clientmail-imap/tests/`). They are written from the RFCs and the provider's
documentation, not recorded, because a recording of a real account holds its folder names and
address. One opt-in test (`make imap-smoke`) checks them against a real Gmail account.
