# Swift app rules

Adapted from Paul Hudson's agent guide for Swift and SwiftUI
(https://github.com/twostraws/SwiftAgents/blob/main/AGENTS.md), changed from iOS to macOS.
The root `AGENTS.md` still applies.

## Targets

- macOS 26.0 or later.
- Swift 6.2 or later, strict concurrency, async/await over closures.
- SwiftUI for all UI. Use AppKit only where SwiftUI has no equivalent, and say why in a comment.
- Follow Apple's Human Interface Guidelines for macOS: keyboard shortcuts, menus, toolbars,
  sidebars, Settings scene.
- Do not add third-party Swift packages without asking first.

## Structure

```
apps/macos/
  project.yml              XcodeGen spec for the app target
  ClientMail/              App target: entry point and composition root only
  Packages/ClientMailKit/  All app code, as Swift packages
    Sources/MailDomain/        Swift models and service protocols. No UI, no Rust.
    Sources/CoreBridge/        Generated UniFFI bindings. Never edit.
    Sources/CoreAdapter/       Implements MailDomain protocols with CoreBridge.
    Sources/<Name>Feature/     One folder per feature: views and view models.
    Tests/<Module>Tests/       One test target per module.
```

- Organise by feature, not by type.
- One type per file.
- Features depend on `MailDomain` only. Only `CoreAdapter` imports `CoreBridge`.
- The app target wires concrete adapters into features. It holds no logic.

## Swift

- `@Observable` classes for shared state, owned with `@State`, passed with `@Bindable` or
  `@Environment`. Mark every `@Observable` class `@MainActor`.
- Do not use `ObservableObject`, `@Published`, `@StateObject`, `@ObservedObject` or
  `@EnvironmentObject`.
- Work that blocks (Rust calls, disk) runs off the main actor, for example in a `@concurrent`
  function.
- No Grand Central Dispatch. Use Swift concurrency.
- Prefer Swift-native and modern Foundation APIs (`replacing(_:with:)`, `URL.applicationSupportDirectory`,
  `appending(path:)`).
- Format with `FormatStyle` (`date.formatted(...)`), never `DateFormatter` or `String(format:)`.
- Filter user-typed text with `localizedStandardContains()`.
- No force unwraps and no `try!` unless failure is unrecoverable.

## SwiftUI

- `foregroundStyle()`, not `foregroundColor()`. `clipShape(.rect(cornerRadius:))`, not
  `cornerRadius()`.
- `NavigationSplitView` or `NavigationStack` with `navigationDestination(for:)`.
- `Button` for actions, not `onTapGesture()`. Buttons with an image also carry text:
  `Button("Add", systemImage: "plus", action: add)`.
- Use `onChange()` with zero or two parameters only.
- `Task.sleep(for:)`, never `Task.sleep(nanoseconds:)`.
- Split views into separate `View` structs, not computed properties.
- Dynamic Type and system spacing. No fixed font sizes, padding or stack spacing unless asked.
- No `AnyView` unless required. Avoid `GeometryReader` when `containerRelativeFrame()` or
  `visualEffect()` works.
- Use `.scrollIndicators(.hidden)`, `ScrollPosition` and `defaultScrollAnchor`.
- Put view logic in view models so it can be tested.

## Strings

- User-facing text lives in each module's `Localizable.xcstrings` with symbol-style keys
  (`accountsTitle`) and `extractionState` set to `manual`.
- Offer translations when you add keys.

## Tests

- Unit-test view models and adapters with Swift Testing (`import Testing`, `@Test`, `#expect`).
- Features are tested against fake services that implement `MailDomain` protocols.
- `CoreAdapterTests` run against the real Rust core, using an in-memory database.
- Write UI tests only when a unit test cannot cover the behaviour.

## Before handing back

Run `make swift-lint swift-test app` from the repository root. All must pass with no warnings.
