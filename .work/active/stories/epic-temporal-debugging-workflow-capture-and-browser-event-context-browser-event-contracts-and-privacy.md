---
id: epic-temporal-debugging-workflow-capture-and-browser-event-context-browser-event-contracts-and-privacy
kind: story
stage: implementing
tags: [browser, storage, security]
parent: epic-temporal-debugging-workflow-capture-and-browser-event-context
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Define Browser Event Contracts and Privacy Boundary

## Checkpoint

Create the single core browser-event registry and its privacy-safe payload vocabulary. Add typed event/request identities, per-target ordinal, clock-tagged source time, typed generic-timeline reference, bounded text/stack/URL values, batch/store ports, and capability status. Sensitive CDP fields must be absent from the representable contract before any adapter or store implementation exists.

## Files

- `crates/krometrail-core/src/browser/{events.rs,privacy.rs}` (new)
- `crates/krometrail-core/src/browser/mod.rs`
- `crates/krometrail-core/src/ids.rs`
- `crates/krometrail-core/src/timeline/{observation.rs,mod.rs}`
- `crates/krometrail-core/src/ports/{browser_events.rs,mod.rs}` (new)
- `crates/krometrail-core/src/capabilities/mod.rs`
- `crates/krometrail-core/src/lib.rs`

## Acceptance evidence

- One macro-backed registry generates stable kind names, classes, payload compatibility, compact defaults, and exhaustive registry tests.
- Event/session/target/request IDs, attachment generation, ordinal, session/source/observed times, severity, and payload validate and round-trip.
- Console/exception/network/navigation/lifecycle/target/visibility/dialog/capture/collection payloads enforce the feature body's exact limits and optionality.
- `SanitizedUrl` retains only origin/path hash/count/extension and removal flags; credentials, query, fragment, raw/local path, and basename are unrepresentable.
- Headers, cookies, authentication, request/response bodies, raw CDP params/session IDs, dialog prompts, fill values, and upload paths are unrepresentable.
- Table-driven privacy/Serde tests cover the redaction corpus without one test per event variant.

## Ordering

This is the first checkpoint. CDP routing can use a fake `BrowserEventSink` as soon as it lands; schema v5 separately waits on artifact v4.