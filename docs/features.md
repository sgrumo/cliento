# Features

Inspired by what Spark does well, rebuilt local-first. Each feature names what we take, what we
change, and why.

## Smart Inbox

Groups the inbox into **People**, **Notifications** and **Newsletters**.

- **Change:** sorting runs on the Mac with explicit rules, not on a server.
  - Newsletter: has a `List-Unsubscribe` or `List-Id` header.
  - Notification: sender matches `no-reply`, `noreply`, `notifications@` or a known service domain.
  - People: everything else, and always anyone the user has written to.
- **Change:** every message shows *why* it landed in its group, and one click moves the sender for
  good. Rules are stored and synced as IMAP keywords where the server allows.

## Sender screening

New senders wait in a **Screener** until the user accepts or blocks them.

- **Change:** optional and off by default. Accepting a sender also accepts their whole domain on
  request.

## Snooze, send later, follow-up reminders

- **Snooze:** the message leaves the inbox and returns at a chosen time.
- **Send later:** the message sits in the outbox until its time.
- **Reminder:** the message comes back if nobody replies by a date.
- **Change:** state is stored on the server as IMAP keywords and a dedicated folder, so it survives
  reinstalls and is visible from other ClientMail installs. Timers run while the app is open; on
  launch, everything that came due is applied at once.

## Unified inbox, pins, templates, quick replies

Kept as in Spark.

- **Change:** every action has a keyboard shortcut and appears in a command palette
  (Command-K).

## Privacy

- Remote images and tracking pixels are blocked by default. The banner says which trackers were
  blocked and lets the user load images once or always for a sender.
- No mail, address or credential ever leaves the Mac except to the user's own mail servers.

## Notifications

- **Change from Spark:** no push server. Notifications arrive while ClientMail runs (IMAP IDLE).
  A login item keeps a light background agent alive for users who want notifications all the time.

## Out of scope for 1.0

- Team features: shared inboxes, shared drafts, comments, delegation. They require a server.
- Calendar.
- Mobile apps.
