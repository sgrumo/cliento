# Rust core rules

Adapted from Max Woolf's agent guidelines for Rust
(https://gist.github.com/minimaxir/068ef4137a1b6c1dcefa785349c91728), trimmed to what this
workspace uses. The root `AGENTS.md` still applies.

## Principles

- Write efficient code: pick the right algorithm and data structure first, avoid needless
  allocations and copies.
- Write only the code the task needs. No speculative abstractions, no dead code.
- When a small, well-maintained crate removes a lot of code, use it. Ask before adding a large one.
- Prefer clarity over cleverness.

## Crates and their concerns

| Crate | Concern | May depend on |
| --- | --- | --- |
| `clientmail-domain` | Types, validation, port traits. No I/O. | external pure crates only |
| `clientmail-store` | SQLite persistence behind the domain ports | `clientmail-domain` |
| `clientmail-engine` | Use cases. Talks to the outside only through ports. | `clientmail-domain` |
| `clientmail-oauth` | OAuth protocols (Google): PKCE, URLs, token endpoint over HTTPS. Implements `OAuthClient` | `clientmail-domain` |
| `clientmail-imap` | IMAP over TLS: XOAUTH2, folders. Implements `MailConnector` | `clientmail-domain` |
| `clientmail-ffi` | UniFFI API for Swift. Composition root. | all of the above |
| `tools/uniffi-bindgen` | CLI that generates the Swift bindings | `uniffi` |

Future crates follow the same shape: protocol crates (IMAP, Gmail, Microsoft Graph, SMTP),
`clientmail-sync`, `clientmail-search`. Each implements ports from `clientmail-domain`.

## Style

- `rustfmt` formats everything. Line length 100, four spaces.
- `snake_case` for functions, variables and modules. `PascalCase` for types and traits.
  `SCREAMING_SNAKE_CASE` for constants.
- Imports are grouped: standard library, external crates, then this workspace. No wildcard imports,
  except `use super::*` in test modules.
- No emoji in code, comments or output.
- The reader knows TypeScript well. Add a short comment when Rust behaves in a way a TypeScript
  developer would not expect (ownership moves, borrowing, `?` conversions, trait objects). Skip
  comments that repeat what the code says.

## Documentation

- Every public item has a doc comment: what it does, its arguments, what it returns, and an
  `# Errors` section when it returns `Result`.
- Add an `# Examples` section when the call shape is not obvious.
- `cargo doc` must build without warnings.

## Types and errors

- Use newtypes for values with meaning (`AccountId`, `EmailAddress`), not bare `String` or `Uuid`.
- Use `Option<T>`, never sentinel values.
- Library crates define errors with `thiserror`. Each crate has its own error type.
- Never call `.unwrap()` outside tests. Use `.expect("why this cannot fail")` only for real
  invariants.
- Propagate errors with `?`.
- Match enums exhaustively. Avoid `_` arms on enums this workspace owns.

## Functions and types

- One responsibility per function and per type.
- Borrow (`&T`, `&str`) unless the function needs ownership.
- At most five parameters. Use a struct for more.
- Return early instead of nesting.
- Prefer iterators and combinators where they read better than loops.
- Derive `Debug`, `Clone`, `PartialEq`, `Eq` where they make sense. Derive `Default` only when a
  default is meaningful.
- Keep struct fields private. Expose accessors.

## Concurrency

- `tokio` for async I/O, `rayon` for CPU-bound parallel work.
- Never block a Tokio worker. In `clientmail-ffi`, SQLite calls and platform callbacks run through
  `CoreRuntime::blocking` (decision record 0008).
- Types shared across threads state their `Send` and `Sync` bounds.
- SQLite connections are not `Sync`: wrap them in a `Mutex` and keep the lock short.
- Prefer channels to shared mutable state.

## Security

- Never put secrets in code or in the repository.
- Wrap secrets in `secrecy::SecretString`.
- Never log passwords, tokens, addresses or message content.
- No logger (`log`) or `tracing` subscriber is installed today. One added later must cap the
  `imap_next` and `imap_codec` targets at `warn`: they trace raw IMAP bytes, XOAUTH2 tokens
  included (decision record 0011).
- Errors of protocol libraries can carry raw server text (folder names, addresses). Map them to
  this workspace's errors with fixed messages; never keep them as `#[source]`.
- No `unsafe`, except what UniFFI generates in `clientmail-ffi`.

## Tests

- Every new function and type gets unit tests in a `#[cfg(test)] mod tests` block next to it.
- Follow Arrange, Act, Assert.
- Replace external systems with fakes: in-memory SQLite for storage, fake ports for the engine,
  never a real mail server.
- Cross-crate behaviour is tested in `clientmail-ffi/tests/`.
- No commented-out tests.

## Lints

Workspace lints live in `core/Cargo.toml` and `core/clippy.toml`. `clippy::all` is an error,
`pedantic`, `nursery` and `cargo` are warnings, and selected restriction lints forbid `unwrap`,
`dbg!`, `todo!` and printing. CI runs clippy with `-D warnings`, so every warning blocks.

- Fix the cause of a lint. Do not silence it.
- When an exception is truly needed, use `#[expect(lint, reason = "...")]` on the smallest item.

## Before handing back

Run `make core-lint core-test core-doc` from the repository root. All three must pass.
