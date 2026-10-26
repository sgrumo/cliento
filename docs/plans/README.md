# Steps

Every milestone from `docs/roadmap.md`, broken into steps small enough to plan, review and commit
on their own. Each step follows the workflow in the root `AGENTS.md`: plan, implement, adversarial
review, user check, deslop, commit.

Status: `todo`, `planning`, `in progress`, `in review`, `done`.

## M0: Foundations

| Step | Goal | Depends on | Status |
| --- | --- | --- | --- |
| 0.1 | Scaffold: crates, Swift package, app, bridge, rules, docs | none | done |

## Notes carried into later steps

Findings from reviews that belong to a step not planned yet. Each step's plan must address the
notes listed for it.

Notes already resolved move to the "Resolved" column with the step that resolved them.

| For step | Note | From | Resolved |
| --- | --- | --- | --- |
| 1.2 | Add a `Message` aggregate (id, account, `RemoteMessageRef`, header, flags) and a mailbox sync-state type (`UIDVALIDITY`, `UIDNEXT`, `HIGHESTMODSEQ`) to `clientmail-domain`, not to the store | 1.1 review P9 | 1.2 |
| 1.2 | For plain IMAP, `RemoteMessageRef::Imap.mailbox` is the source of truth; the message-mailbox relation holds Gmail labels only | 1.1 review P9 | 1.2 |
| 1.2 | Key mailboxes on `MailboxId`; `(account, path)` must not be their identity, so renames are not delete plus create | 1.1 review P10 | 1.2 |
| 1.2 | Store `GmailMessageId` and `GmailThreadId` as the bits of a `u64` in an `INTEGER` column (`as i64` round trip), not by numeric value | 1.1 review P12 | 1.2 |
| 1.4 | Store the refresh token through `SecretStore` (`SecretKind::OAuthRefreshToken`) and keep access tokens in memory only | 1.3 plan | 1.4 |
| 1.4 | Token writes and removal of the same account must never overlap (for example a lock per account in the engine), or a refreshed token can outlive its account | 1.3 review P1 | 1.4 |
| 1.12 | When the Keychain keeps failing on removal (denied prompt, no keychain), block only on `Unavailable`; otherwise remove the account, record the secret's label in a pending-deletion list in the database and retry it on later launches | 1.3 review P2, moved by the 1.4 plan | |
| 1.8 | Background Keychain reads must never show UI (`LAContext.interactionNotAllowed` via `kSecUseAuthenticationContext`); check what a locked login keychain really returns before relying on `Unavailable` | 1.3 review P3; 1.4 sets the context, unverified (1.4 review P2), moved by the 1.6 plan | |
| 1.4 | Insert the account row before writing its token, and delete the token if the insert is rolled back | 1.3 review P6 | 1.4 |
| 1.12 | Revoke the refresh token at Google (`oauth2.googleapis.com/revoke`) when an account is removed, and the old one when signing in again replaces it | 1.4 review P5 | |
| 1.12 | Sweep the Keychain: list items under the ClientMail service and delete those with no account (needs a bridge call that lists labels) | 1.3 review P6, moved by the 1.4 plan | |
| 1.4 | Decide how async engine use cases (token refresh first) reach the synchronous store and secret store without blocking a Tokio worker. Suggested: async ports in the engine, implemented by adapters in `clientmail-ffi` over `CoreRuntime::blocking`; then make decision record 0008 match | 1.5 review P2 | 1.4 |
| 1.4 | Add `CoreRuntime::spawn` so async network work runs on the workers (an exported method's body is polled on a Swift thread with no Tokio context), and make `blocking` reachable from spawned tasks | 1.5 review P3 | 1.4 |
| 1.4 | UniFFI's Swift bindings have no cancellation: closing sign-in mid-exchange keeps the Swift task waiting. Add a core-level cancel for the sign-in flow | 1.5 review P1 | 1.4 |
| 1.8 | Keychain calls share the blocking pool with `SQLite`: a token read can queue behind store work, and a Keychain call stuck on a prompt holds a thread. Watch for it; a separate pool or priority if it shows | 1.5 review P5, reopened by 1.4 review P2, moved by the 1.6 plan | |
| 1.5 | Call the synchronous secret store through `spawn_blocking` (or make the callback interface async) so a blocked Keychain call never stalls a Tokio worker | 1.3 review P5 | 1.5 |
| 1.8 | Check in the app, with a token read in the background: what a locked login keychain returns for a read with `interactionNotAllowed` (expected `errSecInteractionNotAllowed`, mapped to `Unavailable`), and whether the "allow access" dialog the login keychain shows after an ad-hoc rebuild still appears; `interactionNotAllowed` may not suppress it. If it does, the read blocks a core thread: fail fast instead (or move to the data-protection keychain early) | 1.3 review P3, 1.4 review P2, moved by the 1.6 plan | |
| 1.8 | `TokenService` checks expiry with the injected `Clock`, but the oauth crate stamps `expires_at` from the system clock; make the grant carry `expires_in` (or give the parser the clock) before tests mix a fake clock with the real parser | 1.4 review P8, moved by the 1.6 plan | |
| 1.8 | Wire `TokenService` into the FFI (with a system `Clock`) and authenticate IMAP with its access token; give `SignInRequired` its own Swift error case instead of mapping it to `signInFailed` | 1.4 implementation, moved by the 1.6 plan | |
| 1.6 | When `MailboxPath::new` rejects a server folder, skip that folder and keep syncing the rest | 1.1 review C6 | 1.6 |
| 1.8 | Add a `CoreError` case for invalid server data, so it is not reported as a storage failure | 1.1 review C7, moved by the 1.6 plan | |
| 1.7 | `imap-codec` cannot parse Gmail's `X-GM-MSGID`, `X-GM-THRID` and `X-GM-LABELS` fetch items; a `FETCH` with them would be discarded. Add them (upstream contribution, or a patched copy under `[patch.crates-io]` until it lands) before fetching Gmail headers | 1.6 review P1, decision record 0011 | |
| 1.7 | Keep the capabilities the server sends after sign-in (untagged `CAPABILITY` or the `[CAPABILITY]` code) on the session, so 1.7 can check `X-GM-EXT-1` (and `CONDSTORE` later) without asking again | 1.6 review P7 | |
| 1.7 | Every command runs through `ImapSession::send`, which keeps the session's busy/broken state; a `FETCH` reads until its own tagged response and counts unparsable lines like `LIST` does | 1.6 review P4 | |
| 1.7 | Fetch `Bcc` on sent mail, `List-Id` and `List-Unsubscribe` presence, `X-GM-THRID` and the All Mail `UID` | 1.1 review P1, P2, P6, P7 | |
| 1.7 | Add `MessageFlags` building from server flags (`FromIterator` or `extend`) when the adapter needs it | 1.1 review C10 | |
| 1.8 | A long sync cannot be cancelled from Swift (UniFFI has no cancellation); give sync its own cancel, and check that token reads do not queue behind long store writes on the blocking pool | 1.5 review P1, P5 | |
| 1.8 | Before mail is cached, match Google accounts on the ID token's `sub` (stored with the account) instead of the address, so a renamed Workspace address keeps its account and a recycled one cannot inherit another person's mail | 1.4 review P4 | |
| 1.8 | Connect over IPv6 and IPv4 at once, or with a timeout per address (`TcpStream::connect` tries them in turn, so a dead IPv6 route uses the whole 30 s) | 1.6 review C5 | |
| 1.8 | 1.8 now carries the sync use case, `TokenService` in the FFI, two error cases, the Keychain checks, the clock fix and sync cancel; decide when planning it whether "`TokenService` in the FFI" becomes its own step | 1.6 review P8 | |
| 1.8 | `MailErrorKind::Unavailable` (busy server) means retry later with backoff, never "sign in again"; only `AuthenticationFailed` refreshes the token and then asks the user | 1.6 review P2 | |
| 1.8 | Add the All Mail mailbox to every Gmail message's labels with `set_labels`, so the All Mail list is not empty (`X-GM-LABELS` never lists it) | 1.2 plan | |
| 1.8 | After a `UIDVALIDITY` change, re-match messages (for example by `Message-ID`) and upsert them with their old `MessageId` and new key; drop the ones that cannot be matched. A moved IMAP message is upserted the same way | 1.2 plan, 1.2 review P1 | |
| 1.8 | Add a connection pool (one writer, several readers) so a long sync write does not block list reads | 1.2 plan | |
| 1.8 | Apply folder renames for an account in one transaction through temporary paths, so swaps and cycles do not hit `UNIQUE (account_id, path)`; add a batch mailbox method then | 1.2 review P7 | |
| 1.8 | Save `set_sync_state` last, after messages and labels, so a crash in between makes the next sync fetch them again | 1.2 review P8 | |
| 1.11 | Measure page loads: each page loads child rows with one query per table per message; batch them if the list lags | 1.2 implementation | |
| 2.1 | Link body blobs to messages, and sweep blobs no message references plus leftover `.tmp` files in the blob folders | 1.2 plan, 1.2 review C5 | |
| 5.4 | Add `MessageFlags::remove_keyword` for snooze | 1.1 review C10 | |
| 6.1 | Decide how an account added by hand changes provider: signing in with Google to a hand-added Workspace address reports "already exists", while a hand-added `@gmail.com` one silently gets the token | 1.4 review P7 | |
| 6.4 | `TokenService` serves one OAuth provider; Microsoft needs dispatch by the account's provider | 1.4 review P8 | |
| 6.5 | Graph folders have opaque ids and `displayName`s that may contain `/`; Outlook categories contain spaces and do not fit `Keyword` | 1.1 review P10 | |
| 7.2 | With team signing, move Keychain items to the data-protection keychain (`kSecUseDataProtectionKeychain`, access group) and migrate items saved before | 1.3 plan | |

## M1: Sign-in and first sync (Gmail)

| Step | Goal | Depends on | Status |
| --- | --- | --- | --- |
| 1.1 | [Domain types for mailboxes and message headers](1.1-mail-domain-types.md) | 0.1 | done |
| 1.2 | [Store: mailboxes and messages tables, body file store](1.2-store-mailboxes-messages.md) | 1.1 | done |
| 1.3 | [Secret storage: Keychain access and where it lives (Swift or Rust)](1.3-secret-storage.md) | 0.1 | done |
| 1.4 | [Google OAuth sign-in with PKCE, tokens saved and refreshed](1.4-google-sign-in.md) | 1.3, 1.5 | done |
| 1.5 | [Async core: Tokio runtime in the FFI crate, async UniFFI API](1.5-async-core.md) | 0.1 | done |
| 1.6 | [IMAP crate: TLS connect, XOAUTH2, list folders, transcript tests](1.6-imap-connect-list.md) | 1.1, 1.5 | done |
| 1.7 | IMAP header fetch for the last 30 days, Gmail labels and thread ids | 1.6 | todo |
| 1.8 | Engine: initial sync use case writing to the store | 1.2, 1.7 | todo |
| 1.9 | Change events from core to Swift through a callback interface | 1.5, 1.8 | todo |
| 1.10 | Window shell: three panes, sidebar of accounts and mailboxes | 1.9 | todo |
| 1.11 | Message list bound to synced headers | 1.10 | todo |
| 1.12 | Secret clean-up: pending deletions and orphaned Keychain items | 1.4 | todo |

## M2: Reading and actions

| Step | Goal | Depends on | Status |
| --- | --- | --- | --- |
| 2.1 | Body fetch and MIME parsing into the body file store | 1.11 | todo |
| 2.2 | HTML sanitising and tracker stripping in the core | 2.1 | todo |
| 2.3 | Reader: locked-down `WKWebView`, remote content banner | 2.2 | todo |
| 2.4 | Threading: Gmail thread ids, JWZ for plain IMAP | 2.1 | todo |
| 2.5 | Operation queue: persisted local actions, replay, retry, undo | 1.8 | todo |
| 2.6 | Actions: archive, delete, read, star, move, with shortcuts | 2.5 | todo |
| 2.7 | IMAP IDLE on the inbox and incremental sync | 2.5 | todo |

## M3: Compose and send

| Step | Goal | Depends on | Status |
| --- | --- | --- | --- |
| 3.1 | Message building: MIME, threading headers, quoting | 2.1 | todo |
| 3.2 | SMTP sending with XOAUTH2 and a persisted outbox | 3.1, 2.5 | todo |
| 3.3 | Composer UI: new, reply, reply all, forward | 3.2 | todo |
| 3.4 | Drafts saved locally and to the server | 3.3 | todo |
| 3.5 | Attachments: add, send, open, save | 3.3 | todo |
| 3.6 | Signatures per account | 3.3 | todo |

## M4: Search and multiple accounts

| Step | Goal | Depends on | Status |
| --- | --- | --- | --- |
| 4.1 | FTS5 index kept in sync with the store | 2.1 | todo |
| 4.2 | Search query language: `from:`, `has:attachment`, dates | 4.1 | todo |
| 4.3 | Search UI with live results | 4.2 | todo |
| 4.4 | Several accounts syncing at once, unified inbox | 2.7 | todo |

## M5: Smart features

| Step | Goal | Depends on | Status |
| --- | --- | --- | --- |
| 5.1 | Smart Inbox classification rules in the core | 2.1 | todo |
| 5.2 | Smart Inbox UI with "why is this here" and one-click fixes | 5.1 | todo |
| 5.3 | Sender screening | 5.1 | todo |
| 5.4 | Snooze | 2.5 | todo |
| 5.5 | Send later | 3.2 | todo |
| 5.6 | Follow-up reminders | 2.5 | todo |
| 5.7 | Templates and quick replies | 3.3 | todo |
| 5.8 | Pins | 2.5 | todo |
| 5.9 | Command palette | 2.6 | todo |

## M6: More providers

| Step | Goal | Depends on | Status |
| --- | --- | --- | --- |
| 6.1 | Server discovery and password sign-in for generic IMAP | 1.6 | todo |
| 6.2 | Fastmail | 6.1 | todo |
| 6.3 | iCloud with app-specific passwords | 6.1 | todo |
| 6.4 | Microsoft sign-in | 1.4 | todo |
| 6.5 | Microsoft Graph sync | 6.4, 1.8 | todo |
| 6.6 | Microsoft Graph send | 6.5, 3.1 | todo |

## M7: Release

| Step | Goal | Depends on | Status |
| --- | --- | --- | --- |
| 7.1 | CI: `make check` on every push | none | todo |
| 7.2 | Developer ID signing and notarisation | 0.1 | todo |
| 7.3 | Sparkle updates | 7.2 | todo |
| 7.4 | Crash reporting without personal data | 7.2 | todo |
| 7.5 | Background notifications agent | 2.7 | todo |
| 7.6 | Performance budget for launch time and memory in CI | 7.1 | todo |
