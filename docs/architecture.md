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
│ clientmail-<proto>    IMAP / Gmail / Graph / SMTP adapters     │
│ clientmail-domain     types, validation, port traits          │
└───────────────────────────────────────────────────────────────┘
```

Dependencies point down. `clientmail-domain` depends on nothing in the repo. The engine knows only
the port traits; storage and protocol crates implement them. The FFI crate is the only place that
picks concrete implementations. On the Swift side, features see only `MailDomain` protocols, so
they are tested with fakes and never touch Rust directly.

## Data flow

**Commands** go down: a view model calls a `MailDomain` protocol, `CoreAdapter` calls the
generated Swift, which calls the Rust `CoreClient`, which runs an engine use case.

**Changes** come up. Today the view model refreshes itself after each call. From milestone 2,
the core pushes change events through a UniFFI callback interface, so sync results reach the UI
without polling.

## Threading

- Rust calls block. `CoreAdapter` methods are `@concurrent`, so they never run on the main actor.
- View models are `@MainActor`.
- The store serialises access to one SQLite connection with a `Mutex`. When sync arrives, the
  store gets a pool: one writer, several readers (SQLite WAL mode is already on).
- Network work runs on a Tokio runtime owned by `clientmail-ffi`.

## Storage

- One SQLite database per installation, in the app's sandbox container under
  `Application Support/ClientMail/mail.sqlite`.
- `STRICT` tables. Schema changes are append-only migrations tracked by `PRAGMA user_version`.
- Message bodies and attachments live as files next to the database, addressed by content hash,
  so the database stays small and fast.
- Full-text search uses SQLite FTS5 over subject, participants and plain-text bodies.
- Credentials and OAuth tokens live in the macOS Keychain, never in the database.

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
| `MailDomain` | unit | pure values |
| `CoreAdapter` | integration | real Rust core through the XCFramework |
| `<Name>Feature` | unit | fake services |

No test ever talks to a real mail server. Protocol crates are tested against recorded server
transcripts.
