<!-- PR title = conventional-commit subject. E.g. `feat(lexer): add column-72 truncation` -->

## What

<!-- One paragraph. User-visible description of what changes. Not implementation details. -->

## Why

<!-- Motivation. Reference the roadmap milestone or ADR by name (e.g. "Roadmap v0.1 — lexer"). Paraphrase, don't copy doc text. -->

## Changes

<!-- Bullet list of the actual changes. Keep it tight. -->

-
-
-

## Testing

<!-- How the change was verified. Be specific. -->

- [ ] Unit tests added / updated
- [ ] Snapshot tests reviewed with `cargo insta review`
- [ ] `cargo test --workspace` passes locally
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo fmt --all -- --check` clean
- [ ] Manual verification, if relevant: describe below

<!-- Manual test notes (or delete this block): -->

## Scope checklist

- [ ] Single logical change (or explain coupling below)
- [ ] Diff under ~400 lines (or explain / link follow-ups)
- [ ] No drive-by refactors
- [ ] No touched code unrelated to this PR's goal
- [ ] Public API changes have doc comments

## Breaking changes

<!-- Default: None. If breaking, describe the break and the migration. -->

None.

## Follow-ups

<!-- Known next steps, deferred work, or technical debt introduced. Link to issues if filed. -->

-

## Notes for reviewer

<!-- Anything non-obvious. Tradeoffs considered. Alternatives rejected and why. -->