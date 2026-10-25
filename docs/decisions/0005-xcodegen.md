# 0005: XcodeGen for the app project

## Status

Accepted.

## Decision

- `apps/macos/project.yml` describes the app target. `make app-project` generates
  `ClientMail.xcodeproj`, which is not committed.
- All code lives in the `ClientMailKit` Swift package; the app target holds only the composition
  root.

## Rationale

- Project files are unreadable in review and conflict on every merge. A short YAML file does not.
- Swift packages build and test from the command line without the project.

## Consequences

- Contributors need `brew install xcodegen`.
- Build settings change in `project.yml`, never in Xcode's UI.
