# 0010: HTTP and TLS in Rust with `reqwest` and rustls

## Status

Accepted (step 1.4).

## Context

Google sign-in needs HTTPS to Google's token endpoint. IMAP (1.6) and SMTP (3.2) need TLS too, and
Microsoft Graph (6.5) is HTTPS. All rules and sync live in the Rust core (decision record 0001),
so network work does too.

## Decision

- `clientmail-oauth` uses `reqwest` 0.13 with the features `rustls`, `form` and `system-proxy`,
  and no default features.
- `rustls` uses the aws-lc-rs crypto provider and checks server certificates with the platform
  verifier, so macOS trust settings (including roots a user or an MDM profile installed) apply, as
  they do in Safari.
- `system-proxy` reads the macOS HTTP and HTTPS proxy host and port, once, when the client is built
  (when the app opens its database). It ignores automatic proxy configuration (PAC), SOCKS and the
  bypass list.
- IMAP and SMTP reuse the same rustls setup.

## Alternatives

| | `reqwest` + rustls (chosen) | Swift `URLSession` behind a callback | `hyper` + `tokio-rustls` by hand |
| --- | --- | --- | --- |
| Size | Large dependency tree | Tiny on the Rust side | Same tree minus `reqwest`, plus our own glue |
| TLS for IMAP | Same stack and trust settings | IMAP still needs rustls: two TLS stacks | Same as `reqwest` |
| System trust | Yes, through the platform verifier | Yes | Yes, with the same verifier |
| System proxies | HTTP and HTTPS proxies only, read once | All, live | No |
| Rules | Network stays in Rust | HTTP providers (Graph) run through Swift | Network stays in Rust |

## Consequences

- One TLS stack for every protocol, configured in one place.
- HTTPS uses a manually configured HTTP or HTTPS proxy. On a network that needs a PAC file it goes
  direct and may fail; a proxy change needs a relaunch. IMAP connections (1.6) do not go through
  an HTTP proxy. Revisit if users behind such proxies report problems.
- The static library grows; the step's review log records by how much.
