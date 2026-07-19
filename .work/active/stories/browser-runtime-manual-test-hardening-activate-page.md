---
id: browser-runtime-manual-test-hardening-activate-page
kind: story
stage: done
tags: [browser, agent-ux, testing]
parent: browser-runtime-manual-test-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Explicitly foreground one controlled page

Add one named page activation operation that deliberately foregrounds a target and returns live evidence without mutating the managed session's preserve-focus policy.

## Implementation

- Added the registry-declared `activate_page` state-changing operation with an optional target. Omission activates the logical selection; an explicit target is one-shot and does not reduce selected-target state.
- Explicit activation reuses the control adapter's bounded foreground authority: `Target.activateTarget`, `Page.bringToFront`, and a generation/cancellation-fenced wait for visible document state. The session focus policy remains immutable.
- Success returns the normal live observation and persists a page-operation interaction anchor. Failure remains `target_hidden` with unavailable observation and no input dispatch.
- MCP routing and input schema remain generated from the core operation registry. Response/evidence/batch projections were made exhaustive for the new result variant, while `activate_page` remains intentionally non-batchable.
- The specification, architecture, and plugin skill now distinguish logical selection, immutable automatic-focus policy, and deliberate one-shot activation.

The feature design named `crates/krometrail-core/src/browser/action.rs`, but this repository's current page request/result authority is `browser/control.rs`; the request and `PageChange` were implemented there rather than creating a second domain authority.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --locked`
- `cargo test -p krometrail-cdp --lib explicit_activation_always_foregrounds_and_waits_for_visible_state --locked`
- `cargo test -p krometrail-cdp --lib hidden_pointer_target_in_preserve_mode_fails_without_foreground_or_input --locked`
- `cargo test -p krometrail-cdp --test page_lifecycle status_and_page_mutations_share_exact_selected_target_state --features cdpkit-transport --locked`
- `cargo test -p krometrail-core browser::operation::tests --locked` (4 passed)
- `cargo test -p krometrail-mcp activate_page_schema_keeps_the_target_optional --locked`
- `bun run docs:build`

The first MCP schema assertion indexed an intentionally absent optional `required` key through schemars' panicking indexer. Converting the generated schema to `serde_json::Value` made the assertion accurately verify that `target` is advertised and optional.

## Tooling deviation

`.work/bin/work-view` is a Linux executable and cannot run on this macOS host. The item and dependency state were inspected directly from the `.work/` Markdown substrate.
