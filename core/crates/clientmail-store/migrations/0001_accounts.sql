CREATE TABLE accounts (
        id TEXT PRIMARY KEY NOT NULL,
        address TEXT NOT NULL UNIQUE COLLATE NOCASE,
        display_name TEXT,
        provider TEXT NOT NULL
    ) STRICT;
