# Vendored crates

Third-party source kept here because ClientMail needs a change upstream does not have yet. Each
crate is the published crate's source, unchanged except where marked `// clientmail:`.

| Crate | Version | Source | Licence | Why |
| --- | --- | --- | --- | --- |
| `imap-types` | 2.0.0-alpha.7 | crates.io, <https://github.com/duesee/imap-codec> | MIT OR Apache-2.0; used under MIT (`LICENSE-MIT`, copied from the upstream repository) | Gmail's `X-GM-MSGID`, `X-GM-THRID`, `X-GM-LABELS` fetch items |
| `imap-codec` | 2.0.0-alpha.9 | crates.io, <https://github.com/duesee/imap-codec> | MIT OR Apache-2.0; used under MIT (`LICENSE-MIT`, copied from the upstream repository) | Parsing and encoding those items |

Rules:

- `core/Cargo.toml` excludes `vendor/` from the workspace, so `cargo fmt`, `clippy` and the
  workspace tests skip it; `[patch.crates-io]` points the dependency graph here.
- `core/Cargo.toml` pins both crates to these exact versions (`=`), so `cargo update` cannot
  pick a newer release from crates.io and silently bypass the patch; upgrading means copying the
  new release here, re-applying the patch, and changing the pins together.
- Every change carries a `// clientmail:` comment, so the patch can be re-applied on an upgrade
  and offered upstream. Drop a copy as soon as a release contains its change.
- To run a copy's upstream tests (on an upgrade), make a scratch workspace outside the repository
  whose members are copies of both crates, with `[patch.crates-io] imap-types = { path = ... }`,
  and run `cargo test --features nom/alloc`. In this repository they are covered by
  `clientmail-imap/tests/gmail_codec.rs` and the Gmail transcripts.
- Decision record 0012 explains the choice.
