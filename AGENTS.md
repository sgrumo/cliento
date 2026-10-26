# ClientMail

A light, local-first macOS email client. A Rust core owns every rule and all data. A Swift app draws the UI and implements the platform services the core asks for (decision record 0007).

## Repository map

| Path | What it holds | Rules |
| --- | --- | --- |
| `core/` | Rust workspace: domain, storage, engine, Swift bindings | `core/AGENTS.md` |
| `apps/macos/` | macOS app (SwiftUI) and its Swift packages | `apps/macos/AGENTS.md` |
| `docs/` | Architecture, roadmap, feature specs, decisions | this file |
| `docs/plans/` | Step index and one plan per step | this file |
| `scripts/` | Build glue between Rust and Swift | this file |
| `.claude/agents/` | Project subagents (`deslop`) | this file |

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
- Platform services (Keychain now, notifications later) are ports the core defines and UniFFI callback interfaces Swift implements in `CoreAdapter`. These Swift adapters translate calls and errors only; they hold no business rules.

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
| `make keychain-test` | Opt-in test against the real login Keychain; leaves nothing behind |
| `make imap-smoke` | Opt-in test against your Gmail's IMAP server; needs a test access token (`docs/google-oauth.md`) |
| `make app` | Generates the Xcode project with XcodeGen and builds the app |

## Global rules

- Write code, comments, docs and commit messages in English.
- Keep a change in one concern. A feature that needs a new rule adds it in Rust first, then exposes it.
- Every new behaviour gets a test in the layer that owns it.
- Never store secrets, tokens or passwords in the repository. Account secrets go to the macOS Keychain.
- Never log message bodies, addresses or tokens.
- Generated files (`Artifacts/`, `Sources/CoreBridge/Generated/`, the `.xcodeproj`) are never edited by hand and never committed.

## Workflow for every step

Work happens in small steps listed in `docs/plans/README.md`. Every step goes through these phases
in order. Do not skip or merge phases.

1. **Plan.** Write `docs/plans/<milestone>.<step>-<slug>.md` from `docs/plans/TEMPLATE.md`. The
   user reviews and approves it before any code is written.
2. **Implement.** Code and tests exactly as planned. If the plan turns out wrong, stop, update the
   plan, and get it approved again. Finish with `make check` green.
3. **Adversarial review.** Run the `adversary` agent on both the plan (and the feature spec it
   implements in `docs/features.md`) and the step's code changes. Brief it with the plan path and
   `git diff` of the step. It reports problems only, it changes nothing. Report its findings to the
   user unfiltered, each with a recommendation.
4. **User check.** The user reviews everything and decides which findings to fix. Apply the changes
   they ask for, then run `make check` again.
5. **Deslop.** Only after the user has signed off on the review phase, run the `deslop` agent
   (`.claude/agents/deslop.md`) on the step's diff. Review its edits, run `make check`, and show the
   user what it changed.
6. **Commit.** When the user asks, commit the step as one conventional commit. Mark the step done in
   `docs/plans/README.md`.

## Docs

- A change to layers, storage, sync or threading updates `docs/architecture.md` in the same change.
- A choice between real alternatives gets a decision record in `docs/decisions/`, numbered in order.
- Finishing or reshaping a milestone updates `docs/roadmap.md`.

## Git

- Read-only git commands are always fine.
- Commit, branch, rebase, merge or push only when the user asks.
