---
id: browser-runtime-manual-test-hardening-frame-query
kind: story
stage: done
tags: [browser, testing]
parent: browser-runtime-manual-test-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Acquire only semantic data required by the query

Role/name queries use the frame-scoped accessibility tree without an unnecessary full DOM snapshot. DOM-dependent query variants retain exact semantic acquisition and completeness checks, with bounded diagnostics that identify which selected acquisition exceeded its limit.

## Implementation

- `SemanticQuery::requires_dom_semantics` is the single query-variant policy: plain role/name queries acquire only the frame-scoped accessibility tree, while container, label, text, and test-id queries additionally acquire DOM semantics.
- Query completeness remains fail-closed for accessibility truncation. Accessibility and selected-document DOM limit failures now report the bounded acquisition and actionable narrowing guidance without exposing page content or frame identifiers.
- Frame regressions prove that a role/name query sends no `DOMSnapshot.captureSnapshot` command and that a small child document succeeds for a DOM-dependent query even when an unrelated parent document exceeds the node bound.

## Verification

- `cargo fmt --all -- --check`
- `cargo test -p krometrail-core semantic_query_wire_defaults_and_bounds_are_validated --locked`
- `cargo test -p krometrail-cdp --lib control::snapshot::tests --locked` (17 passed)

## Tooling deviation

`.work/bin/work-view` is a Linux executable and cannot run on this macOS host. The item and dependency state were inspected directly from the `.work/` Markdown substrate.
