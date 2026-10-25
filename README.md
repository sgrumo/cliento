# ClientMail

A light, local-first email client for macOS. A Rust core owns sync, storage and every rule; a
native SwiftUI app draws the interface.

## Requirements

- macOS 26 on Apple Silicon
- Xcode 26 (Swift 6.2)
- Rust via `rustup`: `core/rust-toolchain.toml` pins the version and installs it on first use
- XcodeGen: `brew install xcodegen`

## Quick start

```bash
make check
```

Builds and checks everything: Rust lint, tests and docs, the Rust-to-Swift bridge, Swift lint and
tests, then the app. To work in Xcode:

```bash
make app-project
open apps/macos/ClientMail.xcodeproj
```

Run `make bridge` again after every change to `core/`, so the app links the new Rust code.

## Layout

| Path | Content |
| --- | --- |
| `core/` | Rust workspace: `clientmail-domain`, `-store`, `-engine`, `-ffi` |
| `apps/macos/` | XcodeGen spec, app target, and the `ClientMailKit` Swift package |
| `scripts/` | `build-core-bridge.sh`: Rust static library, XCFramework and Swift bindings |
| `docs/` | Architecture, feature specs, roadmap and decision records |

Rules for contributors and coding agents are in `AGENTS.md`, `core/AGENTS.md` and
`apps/macos/AGENTS.md`.

## Documentation

- [Architecture](docs/architecture.md)
- [Features](docs/features.md)
- [Roadmap](docs/roadmap.md)
- [Decision records](docs/decisions/)
