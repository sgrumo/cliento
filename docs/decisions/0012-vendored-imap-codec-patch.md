# 0012: Gmail's fetch items in a patched copy of `imap-codec`

## Status

Accepted (step 1.7).

## Context

Decision record 0011 chose `imap-next` and `imap-codec` and named the cost: `imap-types` models
neither Gmail's `X-GM-MSGID`, `X-GM-THRID` and `X-GM-LABELS` in a `FETCH` command nor in its
response. `imap-next` sends typed commands only, so without them a `FETCH` cannot even ask for
Gmail's ids, and an answer carrying them is discarded as unparsable. Gmail sync needs all three
(decision record 0003, architecture's identity model).

## Decision

- `core/vendor/` holds the published source of `imap-types` 2.0.0-alpha.7 and `imap-codec`
  2.0.0-alpha.9, used under their MIT licence, with `[patch.crates-io]` pointing the dependency
  graph there.
- The patch adds `MessageDataItemName::{XGmMsgId, XGmThrId, XGmLabels}`,
  `MessageDataItem::{XGmMsgId(u64), XGmThrId(u64), XGmLabels(Vec<GmLabel>)}` and
  `GmLabel::{System(Atom), Name(AString)}`, with a decoder (`msg_att_gmail`, accepted anywhere in
  a `FETCH` item list) and encoders. Every changed line is marked `// clientmail:`.
- `vendor/` is excluded from the workspace: our formatting, lints and tests do not run on it. The
  patch is tested from `clientmail-imap`, through the public codecs (`tests/gmail_codec.rs`) and
  by the Gmail transcripts.
- `core/Cargo.toml` pins both crates with `=`, so `cargo update` cannot bypass the patch.
- `vendor/VENDOR.md` lists each copy, its source, version, licence and reason.

## Alternatives

| | Patched copy in the repository (chosen) | A fork on GitHub as a git dependency | Parsing Gmail's `FETCH` lines ourselves |
| --- | --- | --- | --- |
| In the repository | About 28 000 lines of third-party source | One line | Our own `FETCH` parser |
| Outward-facing | No | Yes: a public fork under your account | No |
| Complete | Yes | Yes | No: the command still cannot ask for the items |

## Consequences

- Upgrading either crate means re-applying a small, marked patch; the transcripts catch a
  mistake.
- The patch can be offered upstream, which is outward-facing and waits for your go-ahead. When a
  release contains it, the copy and the `[patch]` entry go.
- The vendored copies keep their upstream tests. They run with a wrapper workspace (see
  `vendor/VENDOR.md`), which is how an upgrade is checked; day to day, the codec tests and the
  transcripts guard the patch.
