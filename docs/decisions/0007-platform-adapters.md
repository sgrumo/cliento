# 0007: Platform adapters in Swift, defined by the Rust core

## Status

Accepted (step 1.3).

## Context

The core needs services only the operating system offers: the Keychain now, notifications and
background scheduling later. Rules that use them (refresh a token, remove an account and its
credentials) must live in Rust, because sync runs without the UI. The question is which side
calls the operating system.

## Decision

- The core defines each platform service as a port in `clientmail-domain` and as a UniFFI callback
  interface in `clientmail-ffi`.
- Swift implements the callback interface with Apple's first-party APIs, in `CoreAdapter`, and the
  app passes the implementation to the core when it opens it.
- These Swift types are *platform adapters*: they translate calls and errors and hold no business
  rules. Deciding when to read, write or delete stays in Rust.
- The first adapter is secret storage: `SecretStoreBridge`, implemented by `KeychainSecretStore`
  over `SecItem`.

## Alternatives

- **Rust calls the Keychain itself** (`keyring` or `security-framework` crates). No callback across
  the FFI boundary, but it depends on third-party native bindings with `unsafe`, `keyring` uses the
  older keychain calls, and tests would touch the real Keychain or need feature flags.

## Consequences

- Every Apple API stays in Swift; every rule stays in Rust.
- Another platform implements the same small interfaces (Android Keystore, Windows Credential
  Manager) and reuses the whole core.
- Secrets cross the FFI boundary as plain strings. The Rust adapter wraps them in `SecretString`,
  which zeroes Rust's copy when dropped; the copies UniFFI and Swift make on the way are freed
  without zeroing. Accepted: they live briefly in the app's own memory, and moving to byte buffers
  both sides scrub is possible if that ever matters.
- Callbacks run on the core's threads, so adapters must be thread-safe and never touch the main
  actor.
