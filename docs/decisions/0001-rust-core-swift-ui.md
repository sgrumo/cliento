# 0001: Rust core, SwiftUI app, UniFFI bridge

## Status

Accepted.

## Context

The app must be light, native on macOS, and keep the option of other platforms later. Mail sync,
parsing and storage are the hard, platform-independent parts.

## Decision

- All rules, sync, parsing and storage live in a Rust workspace.
- The macOS UI is SwiftUI, with AppKit only where SwiftUI falls short.
- UniFFI generates the Swift bindings from Rust proc-macro annotations. The Rust core ships as a
  static library inside an XCFramework.
- Swift features depend on `MailDomain` protocols, never on the generated bindings.

## Consequences

- A second platform reuses the whole core and writes only UI and an adapter.
- Every core change needs `make bridge` before the app sees it.
- Generated Swift is not strict-concurrency clean, so `CoreBridge` builds in Swift 5 mode and
  `CoreAdapter` enforces isolation.
