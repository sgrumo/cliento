# ClientMail

A light, local-first macOS email client. A Rust core owns every rule and all data. A Swift app only draws the UI.

## Repository map

| Path | What it holds | Rules |
| --- | --- | --- |
| `core/` | Rust workspace: domain, storage, engine, Swift bindings | `core/AGENTS.md` |
| `apps/macos/` | macOS app (SwiftUI) and its Swift packages | `apps/macos/AGENTS.md` |
| `docs/` | Architecture, roadmap, feature specs, decisions | this file |
| `scripts/` | Build glue between Rust and Swift | this file |

Read the `AGENTS.md` of every folder on the path of a file before you change it.

## Layers and dependency direction

```
apps/macos  ->  CoreBridge (generated)  ->  clientmail-ffi
                                              |-> clientmail-engine -> clientmail-domain
                                              |-> clientmail-store  -> clientmail-domain
```

- `clientmail-domain` depends on no other crate of this repo. It holds types, validation and the port traits.
- `clientmail-engine` holds use cases. It talks to storage and network only through port traits.
- `clientmail-store` and future protocol crates implement ports. They hold no business rules.
- `clientmail-ffi` composes the crates and exposes the UniFFI API. It holds no business rules.
- Swift features depend on the `MailDomain` protocol module, never on `CoreBridge` directly. Only `CoreAdapter` imports `CoreBridge`.

## Commands

Run from the repository root.

| Command | What it does |
| --- | --- |
| `make check` | Everything below, in order. Run it before you hand work back. |
| `make core-lint` | `cargo fmt --check` and `cargo clippy` with warnings as errors |
| `make core-test` | `cargo test` for the whole workspace |
| `make core-doc` | `cargo doc` with warnings as errors |
| `make bridge` | Builds the Rust static library, the XCFramework and the Swift bindings |
| `make swift-lint` | `swift-format lint --strict` on handwritten Swift |
| `make swift-format` | Rewrites handwritten Swift in place with `swift-format` |
| `make swift-test` | `swift test` for the `ClientMailKit` package |
| `make app` | Generates the Xcode project with XcodeGen and builds the app |

## Global rules

- Write code, comments, docs and commit messages in English.
- Keep a change in one concern. A feature that needs a new rule adds it in Rust first, then exposes it.
- Every new behaviour gets a test in the layer that owns it.
- Never store secrets, tokens or passwords in the repository. Account secrets go to the macOS Keychain.
- Never log message bodies, addresses or tokens.
- Generated files (`Artifacts/`, `Sources/CoreBridge/Generated/`, the `.xcodeproj`) are never edited by hand and never committed.

## Docs

- A change to layers, storage, sync or threading updates `docs/architecture.md` in the same change.
- A choice between real alternatives gets a decision record in `docs/decisions/`, numbered in order.
- Finishing or reshaping a milestone updates `docs/roadmap.md`.

## Git

- Read-only git commands are always fine.
- Commit, branch, rebase, merge or push only when the user asks.
