## What changed

<!-- The user-visible change, in a sentence or two. Link the issue
it closes, if any. -->

## Safety reasoning

<!-- Required for anything near scanning, recommendations, plans,
execution, restore, or the updater — and "none needed, because …"
is a fine answer when it's true. Did you touch the safety gate,
protected paths, or the plan/executor contract? The webview must
keep referring to cleanup work by ID, never by path. -->

## Checks

- [ ] `npm run build` and `npm run lint` pass
- [ ] `npm run test` (vitest) passes, with tests for new pure logic
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes, with integration tests on real
      temporary folder trees for core changes
- [ ] CHANGELOG.md updated under *Unreleased* for notable changes
- [ ] Screenshots attached for UI changes (light and dark)
- [ ] No demo/stub content in the shipped product; no new webview
      powers (capabilities) without a sentence of justification above
