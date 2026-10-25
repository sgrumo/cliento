# 0002: Local-first, no server of ours

## Status

Accepted.

## Context

Spark sorts mail, sends push notifications and runs snooze timers on Readdle's servers, which
therefore hold access to users' mailboxes. Privacy is a main reason to choose this app.

## Decision

- No backend operated by us. The app talks only to the user's mail providers.
- Sorting, search, snooze and reminders run on the Mac.
- State that must survive reinstalls is stored on the mail server itself (IMAP keywords, folders).

## Consequences

- No notifications while the app is closed, unless the optional background agent runs.
- Snoozed mail returns only when a ClientMail instance is running.
- Team features (shared inboxes, comments) are out of scope until a separate, opt-in decision.
