# <milestone>.<step>: <title>

Status: draft | approved | implemented | reviewed | done

## Goal

One or two sentences: what the user or the codebase can do after this step that it cannot today.

## Feature link

Which part of `docs/features.md` or `docs/roadmap.md` this step serves. "Foundation" if none.

## Scope

- What this step changes, as a short list.

## Out of scope

- What looks related but waits for a later step, and which step.

## Design

- Types, traits, tables, API surface, data flow. Name every new crate, module, file and public
  item.
- Which layer owns each piece, checked against the layer rules in `AGENTS.md`.
- Alternatives considered and why they lost. A real trade-off also gets a decision record.

## Tasks

1. Ordered, each small enough to review on its own.

## Tests

| Layer | Test | Kind |
| --- | --- | --- |
| crate or module | what it proves | unit / integration / end-to-end |

## Acceptance

- What a reviewer checks by hand, beyond `make check` passing.

## Risks and open questions

- Unknowns, with how this step resolves them or which question the user must answer.

## Review log

Filled in during phases 3 to 5.

- Adversarial review: findings and what was decided for each.
- Deslop: what changed.
