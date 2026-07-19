---
id: epic-agent-surface-simplification-persistence-recovery-classify-writer-publication-failures
kind: story
stage: done
tags: [storage, diagnostics]
parent: epic-agent-surface-simplification-persistence-recovery
depends_on: []
release_binding: 1.2.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Classify segment publication failures and preserve safe writer reuse

Introduce the bounded persistence operation/category/recoverability contract in core and apply it at every segment-writer failure site. Only directory sync after a completed sealed-file rename is `writer_usable`; every ambiguous write, file sync, initial publication, or rename failure remains terminal.

## Acceptance evidence

- Inject a post-rename directory-sync failure and assert its exact operation, category, and `writer_usable` classification.
- Append and flush a later frame through the same writer; verify the already sealed segment and new segment are both readable with distinct frame identities and valid offsets.
- Inject representative pre-rename failures and prove the exact first terminal error is replayed without later filesystem mutation.
- Prove serialized/debug failures contain no paths or raw OS messages.

## Ordering

This checkpoint establishes the only persistence classification authority. Capture propagation depends on it.

## Implementation notes

- Execution capability: high; cross-layer storage durability work required precise filesystem-state classification and injected fault verification.
- Review weight: standard (caller/project default).
- Files changed: `crates/krometrail-core/src/error.rs`, `crates/krometrail-core/src/lib.rs`, `crates/krometrail-store/src/segments/writer.rs`, and this story.
- Tests added/removed: added bounded persistence serialization/privacy coverage and expanded the directory-sync fault test to prove exact terminal replay plus safe post-rename writer reuse and readable distinct segments; removed the legacy missing-field decoding assertion.
- Simplification: replaced free-form writer action strings and unconditional terminal latching with one typed persistence contract and one recoverability branch.
- Discrepancies from design: fault coverage lives in the writer module where the existing injectable `DirectorySync` seam is private, rather than the external smoke test named in the design.
- Adjacent issues parked: none.
- Verification: `cargo test -p krometrail-core error::tests --lib`; `cargo test -p krometrail-store segments::writer::tests::directory_publications_are_synced_and_failures_propagate --lib`; `cargo check -p krometrail-core -p krometrail-store --all-targets`.
