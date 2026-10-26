# 0011: IMAP with `imap-next` and `imap-codec`

## Status

Accepted (step 1.6), replacing a first choice of `async-imap` after the step's adversarial review.

## Context

Gmail and every other non-Microsoft provider sync over IMAP (decision record 0003). The core needs
TLS and XOAUTH2 now, then `SELECT`, `FETCH` with Gmail's extensions (1.7) and `IDLE` (2.7).

Step 1.6 first used `async-imap` 0.12 (parser `imap-proto` 0.17). Its adversarial review, checked
against the sources, found:

- Command handling that reports failures as success: `LIST`, `SELECT` and `FETCH` streams end
  quietly at end of file, `FETCH` and `LIST` treat a tagged `NO` as completion, `IDLE` waits
  forever after a `NO`, and up to 100 unread server notices are kept, later ones dropped silently.
- Parser bugs in the core grammar: quoted strings keep their escapes (`"a\"b"`), so names,
  subjects and labels with `"` or `\` are wrong; `\All` and `\Sent` match as prefixes, so a line
  with `\AllMail` or `\Sentinel` fails to parse and, through the stream, fails the whole command.

So the step compared both libraries on the same transcripts (a spike in the step's scratch space).

## Decision

- `clientmail-imap` uses `imap-next` 0.3 (a sans-I/O client) with `imap-codec` 2.0 and
  `imap-types`, and drives the bytes itself over any `AsyncRead + AsyncWrite` stream: our TLS
  (decision record 0010) in the app, an in-memory pipe in tests.
- Every command is sent and completed by the adapter, which accepts only its own tagged `OK`.
- A response `imap-codec` cannot parse is discarded and reported; during `LIST` the folder is
  skipped and counted, so one odd line never fails the listing.
- `imap-next` and `imap-codec` trace raw protocol fragments (the XOAUTH2 token included) through
  `tracing`. The app installs no subscriber; any added later caps `imap_next` and `imap_codec` at
  `warn`.

## Comparison

| | `async-imap` 0.12 / `imap-proto` 0.17 | `imap-next` 0.3 / `imap-codec` 2.0 (chosen) |
| --- | --- | --- |
| Same transcripts (Gmail sign-in and list, refused token, no XOAUTH2, names, refused LIST, dropped connection, silent server) | 7 of 7, after replacing its `LIST` with our own loop | 7 of 7 |
| Probe: `"a\"b\\c"`, a `"\\"` delimiter, a literal name, `\AllMail`, `\Sentinel`, a non-UTF-8 literal | The whole listing fails (reported as a network error) | All parsed correctly; the non-UTF-8 name is skipped and counted |
| Response codes | `[AUTHENTICATIONFAILED]` not parsed; refusals folded into strings | Every code, unknown ones as raw bytes |
| Gmail `X-GM-MSGID`, `X-GM-THRID`, `X-GM-LABELS` | Parsed (labels sent as literals lost; no thread id accessor) | **Not supported**: such a `FETCH` line would be discarded |
| Stream | Any stream | Sans-I/O: any stream through our own small driver |
| Maturity | Stable releases | Pre-releases (`2.0.0-alpha`); needs `nom`'s `alloc` feature added by hand |

The core grammar is used by every server and every command; Gmail's extensions are three fetch
items. A correct core with a contained gap beats a working extension on a broken core.

## Consequences

- **1.7 must add Gmail's fetch items** to `imap-codec` (an upstream contribution, or a small
  patched copy under `[patch.crates-io]` until it lands) before it can fetch labels and thread
  ids. This is the main cost of the choice.
- The pre-release versions are pinned by `Cargo.lock`; upgrades are deliberate and re-run the
  transcripts.
- `clientmail-imap` depends on `nom` directly only to switch on `alloc`, which
  `imap-codec` 2.0.0-alpha.9 uses without asking for.
- The engine sees only the domain's `MailConnector` and `MailSession` ports, so a later change of
  library touches only the adapter, and the transcripts carry over (only tags and SASL-IR differ).
