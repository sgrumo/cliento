# 0009: Async use cases reach synchronous ports through a `Blocking` port

## Status

Accepted (step 1.4).

## Context

Google sign-in and token refresh are the first async use cases: they wait on HTTPS, then read or
write the account store and the Keychain, which are synchronous. Decision record 0008 says nothing
may block a Tokio worker, and `CoreRuntime::blocking` lives in `clientmail-ffi`, which the engine
cannot depend on. Business rules (which account, which token, in which order) must stay in the
engine.

## Decision

- `clientmail-domain` defines a `Blocking` port: `run(work)` takes a synchronous closure and returns
  a future of its result.
- Async use cases in the engine wrap every storage and secret-store call in `blocking.run(...)`.
  Ports used inside it are shared through `Arc`, because the work runs on another thread.
- `clientmail-ffi` implements `Blocking` with the runtime's blocking pool (`RuntimeBlocking`, which
  holds a runtime `Handle`, so spawned tasks can use it without the client).
- Synchronous use cases (`AccountService`) stay synchronous; the FFI keeps running them inside
  `CoreRuntime::blocking`.

## Alternatives

| | `Blocking` port (chosen) | Async twins of every port | Engine depends on Tokio |
| --- | --- | --- | --- |
| How | One trait; the engine wraps sync calls | `AsyncAccountRepository`, `AsyncSecretStore`, ...; the FFI adapts each | The engine calls `tokio::task::spawn_blocking` |
| New code | One small trait and one adapter | One trait and one adapter per port, growing with every port | None |
| Engine | Depends on the domain only | Depends on the domain only | Gains a runtime dependency; tests need a runtime |
| Tests | A fake that runs the work inline | Fakes for every async twin | A Tokio runtime per test |

## Consequences

- The engine stays free of any runtime; its async tests poll futures with a tiny executor.
- Ports an async use case touches must be `Send + Sync + 'static`.
- A lock taken inside `run` is a plain mutex held on a blocking thread, never across an `.await`.
