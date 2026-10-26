-- Mailboxes, messages and their child rows. See docs/decisions/0006-message-storage.md.

CREATE TABLE mailboxes (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    delimiter TEXT CHECK (delimiter IS NULL OR length(delimiter) = 1),
    role TEXT NOT NULL CHECK (role IN ('inbox', 'all-mail', 'archive', 'drafts', 'sent',
                                       'trash', 'junk', 'starred', 'important', 'custom')),
    selectable INTEGER NOT NULL CHECK (selectable IN (0, 1)),
    uid_validity INTEGER,
    uid_next INTEGER,
    highest_modseq INTEGER,
    UNIQUE (account_id, path)
) STRICT;

CREATE TABLE messages (
    -- Stable integer key for FTS5 (`content_rowid`); `id` is the identity everything else uses.
    seq INTEGER PRIMARY KEY,
    id TEXT NOT NULL UNIQUE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    remote_kind TEXT NOT NULL CHECK (remote_kind IN ('imap', 'gmail')),
    -- Plain IMAP only: the folder that holds the message.
    mailbox_id TEXT REFERENCES mailboxes(id) ON DELETE CASCADE,
    -- Plain IMAP: values in `mailbox_id`. Gmail: values in [Gmail]/All Mail.
    uid_validity INTEGER NOT NULL,
    uid INTEGER NOT NULL,
    -- Gmail only. The bits of a u64, stored in a signed INTEGER.
    gmail_message_id INTEGER,
    gmail_thread_id INTEGER,
    subject TEXT,
    sent_at INTEGER,
    received_at INTEGER NOT NULL,
    header_message_id TEXT,
    in_reply_to TEXT,
    list_id TEXT,
    has_list_unsubscribe INTEGER NOT NULL CHECK (has_list_unsubscribe IN (0, 1)),
    size_bytes INTEGER NOT NULL,
    flags INTEGER NOT NULL,
    CHECK ((remote_kind = 'imap' AND mailbox_id IS NOT NULL
            AND gmail_message_id IS NULL AND gmail_thread_id IS NULL)
        OR (remote_kind = 'gmail' AND mailbox_id IS NULL
            AND gmail_message_id IS NOT NULL AND gmail_thread_id IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX messages_imap_key ON messages (mailbox_id, uid_validity, uid)
    WHERE remote_kind = 'imap';
CREATE UNIQUE INDEX messages_gmail_key ON messages (account_id, gmail_message_id)
    WHERE remote_kind = 'gmail';
CREATE INDEX messages_mailbox_list ON messages (mailbox_id, received_at DESC, id DESC);
CREATE INDEX messages_header_message_id ON messages (header_message_id);
CREATE INDEX messages_gmail_thread ON messages (account_id, gmail_thread_id);

CREATE TABLE message_participants (
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    field TEXT NOT NULL CHECK (field IN ('from', 'to', 'cc', 'bcc', 'reply-to')),
    position INTEGER NOT NULL,
    name TEXT,
    address TEXT NOT NULL,
    PRIMARY KEY (message_id, field, position)
) STRICT, WITHOUT ROWID;
CREATE INDEX message_participants_address ON message_participants (address COLLATE NOCASE);

CREATE TABLE message_references (
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    header_message_id TEXT NOT NULL,
    PRIMARY KEY (message_id, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE message_keywords (
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    keyword TEXT NOT NULL,
    PRIMARY KEY (message_id, keyword)
) STRICT, WITHOUT ROWID;

-- Gmail only. `received_at` copies the message's, so a label page reads in index order.
CREATE TABLE message_labels (
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    mailbox_id TEXT NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
    received_at INTEGER NOT NULL,
    PRIMARY KEY (message_id, mailbox_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX message_labels_list ON message_labels (mailbox_id, received_at DESC, message_id DESC);
