# Steps

Every milestone from `docs/roadmap.md`, broken into steps small enough to plan, review and commit
on their own. Each step follows the workflow in the root `AGENTS.md`: plan, implement, adversarial
review, user check, deslop, commit.

Status: `todo`, `planning`, `in progress`, `in review`, `done`.

## M0: Foundations

| Step | Goal | Depends on | Status |
| --- | --- | --- | --- |
| 0.1 | Scaffold: crates, Swift package, app, bridge, rules, docs | none | done |

## M1: Sign-in and first sync (Gmail)

| Step | Goal | Depends on | Status |
| --- | --- | --- | --- |
| 1.1 | [Domain types for mailboxes and message headers](1.1-mail-domain-types.md) | 0.1 | planning |
| 1.2 | Store: mailboxes and messages tables, body file store | 1.1 | todo |
| 1.3 | Secret storage: Keychain access and where it lives (Swift or Rust) | 0.1 | todo |
| 1.4 | Google OAuth sign-in with PKCE, tokens saved and refreshed | 1.3 | todo |
| 1.5 | Async core: Tokio runtime in the FFI crate, async UniFFI API | 0.1 | todo |
| 1.6 | IMAP crate: TLS connect, XOAUTH2, list folders, transcript tests | 1.1, 1.5 | todo |
| 1.7 | IMAP header fetch for the last 30 days, Gmail labels and thread ids | 1.6 | todo |
| 1.8 | Engine: initial sync use case writing to the store | 1.2, 1.7 | todo |
| 1.9 | Change events from core to Swift through a callback interface | 1.5, 1.8 | todo |
| 1.10 | Window shell: three panes, sidebar of accounts and mailboxes | 1.9 | todo |
| 1.11 | Message list bound to synced headers | 1.10 | todo |

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
