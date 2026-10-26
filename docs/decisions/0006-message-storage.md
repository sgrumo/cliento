# 0006: Message storage layout

## Status

Accepted (step 1.2, revised after its adversarial review).

## Context

Messages come from two kinds of server identity (plain IMAP and Gmail), carry lists of
participants and references, and must answer three kinds of question fast: "list this mailbox,
newest first", "has the user written to this address?" (Smart Inbox, search) and "which message has
this `Message-ID`?" (threading). Local message ids must survive server-side changes.

## Decision

- **One `messages` table** for both kinds, with a `remote_kind` column and a `CHECK` that rejects
  half-filled rows. Both kinds share `uid_validity` and `uid` (for Gmail, the All Mail values).
  Partial unique indexes give each kind its own identity key.
- **Two keys per message.** `seq INTEGER PRIMARY KEY` is a stable integer for FTS5
  (`content_rowid`); `id TEXT UNIQUE` is the identity every other table and layer uses.
- **Upsert matches on `id` first, then on the server key.** A caller that re-matched a message
  (new `UIDVALIDITY`, move to another folder) upserts it with its old id and its new server key;
  a caller that has not seen a message gets the stored id back.
- **Child tables** for participants, references, keywords and Gmail labels, instead of JSON
  columns. Participants have an index on the address; references keep their order with a
  `position` column.
- **Plain IMAP folder in `messages.mailbox_id`**, Gmail labels in `message_labels`, which only
  accepts Gmail messages. `message_labels` copies `received_at` so a label page reads in index
  order.
- **Mailboxes keyed on their id.** The path is unique per account but updatable, so a rename is an
  update.
- **Flags as one integer** with bits owned by the store, keywords in their own table. Pages skip
  messages flagged `\Deleted`.
- **Gmail ids** stored as the bits of a `u64` in a signed `INTEGER`.
- **Keyset pagination** on `(received_at, id)`, with separate statements for the first page and
  later pages, so the cursor seeks in the index instead of filtering every row before it.
- **Migrations run with foreign keys off** and must pass `foreign_key_check` before they commit,
  so a future table rebuild cannot cascade-delete child rows.

## Alternatives

- **JSON columns for participants and references.** Fewer tables and one row per message, but no
  index on addresses, so Smart Inbox and search would scan every message.
- **One table per remote kind.** Cleaner columns, but every query that does not care about the
  kind (list, thread, search) would need a union.
- **Labels without `received_at`.** One column less, but every label page sorts the whole label:
  measured 258 ms for the first page of a 100,000-message label.

## Consequences

- An upsert touches five tables; it runs in one transaction with cached statements. Measured in a
  release build: 10,000 messages in about 0.4 s in memory and 0.5 s on a WAL database file.
- `received_at` lives in two places; the upsert keeps `message_labels` in step.
- Reading a page loads child rows with one query per table per message. Fine for pages of tens of
  messages; revisit if the list shows lag.
