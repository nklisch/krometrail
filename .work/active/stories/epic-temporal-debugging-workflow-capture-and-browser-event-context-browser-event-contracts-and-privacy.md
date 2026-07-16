---
id: epic-temporal-debugging-workflow-capture-and-browser-event-context-browser-event-contracts-and-privacy
kind: story
stage: done
tags: [browser, storage, security]
parent: epic-temporal-debugging-workflow-capture-and-browser-event-context
depends_on: []
release_binding: 1.0.0
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

## Implementation decisions

- The single `define_browser_event_registry!` declaration generates the kind enum, stable names, payload enum/matching, classes, compact defaults, validated payload deserialization, and registry rows. Dynamic priority and severity remain payload-derived while kind/class/default priority cannot drift from the registry row.
- `BrowserEvent` stores generation internally as `NonZeroU64`, validates non-nil event/session/target IDs, validates source-clock-tagged non-negative native time independently from normalized ordering time, and derives kind/class/priority from the payload. Capture-status scope and collection-gap affected ranges receive additional cross-field validation.
- Payload constructors deterministically cap console arguments and stack frames. Serde takes the stricter boundary path: unknown fields, nil request IDs, oversized collections/text/stacks/payloads, severity disagreement, and malformed ordering/ranges are rejected rather than normalized silently.
- `EventRedactor` is the single constructor for console/exception/stack text. It removes secret assignments and authorization/cookie values, full URL tokens, and absolute POSIX/Windows/file paths before UTF-8-safe truncation. `RedactedText::new` rejects values that would still be changed by that redactor, preventing a public-constructor bypass.
- `SanitizedUrl` is core-owned data rather than a dependency on a URL-parser crate. Its bounded sanitizer recognizes allowlisted network/file/data/blob classifications; retains only normalized lowercase network origin, non-default port, path SHA-256/count/allowlisted extension, and removal flags; and fully redacts data/blob/unknown schemes. Query, fragment, and credentials are removed before path hashing.
- Batch accounting sums serialized event rows, requires one non-empty session batch, unique event IDs, and strictly increasing ordinals independently per target. The object-safe `BrowserEventSink` accepts only this validated aggregate.
- Legacy external timeline kinds remain intact for schema-v5 migration. This checkpoint adds only typed `ObservationKind::BrowserEvent` / `ObservationPayloadRef::BrowserEvent`.

## Implementation notes

- Execution capability: highest; selected by the caller because the registry and privacy vocabulary are durable security/storage boundaries. Dispatch was direct-read only with no nested agent or peer review, as required.
- Review weight: standard from the feature design; review is not applicable to this child-story checkpoint, which advances directly to `done` after green verification.
- Files changed: `crates/krometrail-core/src/browser/events.rs`, `crates/krometrail-core/src/browser/privacy.rs`, `crates/krometrail-core/src/browser/mod.rs`, `crates/krometrail-core/src/ids.rs`, `crates/krometrail-core/src/timeline/observation.rs`, `crates/krometrail-core/src/ports/browser_events.rs`, `crates/krometrail-core/src/ports/mod.rs`, and `crates/krometrail-core/src/lib.rs`.
- Tests added: table-driven registry/payload compatibility across all 15 kinds; validated event/source-clock/ordinal/batch Serde and bounds; duplicate ID and per-target ordering rejection; privacy corpus for credentials, URL query/fragment/path/basename, POSIX/Windows/file paths, secret text, stack frames, hash-only uncommon methods, and dialog structural absence; object-safety compile test for `BrowserEventSink`.
- Simplification: reused the existing typed-ID macro, `SessionRange`, source/observed/session time types, `TargetCaptureStatus`, generic observation registry, capability registry, and `PortFuture`; added no parser/runtime/CDP/SQLite/tracing abstraction and no dependency or lockfile change.
- Discrepancies from design: no capability source edit was needed because `CapabilityId::BrowserEvents` was already default-enabled and already mapped to `RecordingSubsystem::BrowserEvents`; no core Cargo edit was needed because `serde_json` and `sha2` were already direct dependencies. The public generation getter remains `u64` as designed while internal storage makes zero unrepresentable.
- Adjacent issues parked: none.

## Verification

Verified from an isolated detached worktree at committed base `bf0e74f` containing only this checkpoint patch, so concurrent artifact/store/root changes and the shared lockfile did not affect evidence:

- `rustup run 1.85.0 cargo fmt --package krometrail-core -- --check`
- `rustup run 1.85.0 cargo check -p krometrail-core --all-targets --locked`
- `rustup run 1.85.0 cargo test -p krometrail-core --all-targets --locked` — 87 passed
- `rustup run 1.85.0 cargo clippy -p krometrail-core --all-targets --locked -- -D warnings`