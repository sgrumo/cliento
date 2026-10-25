# 0004: SQLite storage, bodies on disk

## Status

Accepted.

## Decision

- SQLite through `rusqlite` with the bundled library, `STRICT` tables and WAL mode.
- Migrations are an append-only list in `clientmail-store`, tracked by `PRAGMA user_version`.
- Message bodies and attachments are files addressed by content hash, outside the database.
- Search uses FTS5 in the same database.
- Secrets live in the Keychain, never in SQLite.

## Rationale

- One embedded database keeps the app light and needs no service.
- Bundling SQLite pins the version and its features (FTS5) regardless of the macOS release.
- Large blobs in files keep the database small, which keeps queries and backups fast.

## Consequences

- Deleting a message must delete its files; a periodic sweep removes orphans.
