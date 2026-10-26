# 0008: Async core on an owned Tokio runtime

## Status

Accepted (step 1.5).

## Context

The core is about to do network I/O: Google token refresh (1.4), IMAP (1.6). Its UniFFI API was
synchronous, and Swift parked a thread on every call. UniFFI can export `async fn`s, which Swift
sees as `async` functions; Swift polls the Rust future through callbacks. Two questions follow:
on which threads the Rust work runs, and whether the Keychain callback becomes async too.

## Decision

- `clientmail-ffi` owns one multi-thread Tokio runtime per `CoreClient` (`CoreRuntime`). Each
  exported method hands its work to that runtime and awaits the `JoinHandle`, which any executor can
  await without a Tokio context.
- Code in the FFI crate never blocks a Tokio worker. SQLite calls and platform callbacks run on the
  runtime's blocking pool (`CoreRuntime::blocking`).
- The secret store callback (`SecretStoreBridge`) and the domain's `SecretStore` port stay
  synchronous.

## Alternatives

| | Own runtime, spawn onto it (chosen) | UniFFI's `async_runtime = "tokio"` |
| --- | --- | --- |
| How | Work is spawned onto our runtime; the exported future awaits the `JoinHandle` | UniFFI wraps each future in `async_compat::Compat`, which enters a lazily created global Tokio runtime while Swift polls it |
| Where Rust code runs | Tokio's worker and blocking threads; only argument conversion and the hand-off run on the Swift thread that polls | Swift's cooperative thread pool (one thread per core); anything slow inside a future stalls Swift tasks |
| Control | Thread counts, names, shutdown with the client | One hidden global runtime |
| Blocking work | `spawn_blocking` on our runtime | `tokio::task::spawn_blocking` on the global runtime |
| Dependencies | `tokio` | `tokio`, `async-compat` |

**An async Keychain callback.** `SecItem` calls are synchronous, so an async Swift implementation
would block a thread of Swift's cooperative pool instead of a Tokio worker: the same problem in a
smaller pool. Since every call that reaches the bridge already runs on the blocking pool, nothing is
gained.

## Consequences

- Swift awaits Rust calls; `CoreAdapter` needs no `@concurrent` for them.
- Every exported method moves its inputs into the spawned work, so services live behind `Arc` and
  arguments are owned.
- UniFFI 0.32's Swift bindings have no cancellation: a cancelled Swift task keeps waiting until the
  Rust call finishes. Long calls (sign-in in 1.4, sync in 1.8) need a cancel of their own.
- The exported method's body is polled on a Swift thread with no Tokio context, so async network
  work must be spawned onto the runtime (1.4 adds that).
- Async code added later (token refresh, sync) must not call the synchronous store or secret store
  from a worker. How engine use cases reach them through the blocking pool is decided in 1.4.
