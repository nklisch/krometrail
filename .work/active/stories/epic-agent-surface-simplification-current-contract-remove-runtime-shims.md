---
id: epic-agent-surface-simplification-current-contract-remove-runtime-shims
kind: story
stage: done
tags: [infra, storage]
parent: epic-agent-surface-simplification-current-contract
depends_on: [epic-agent-surface-simplification-current-contract-current-store-schema]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Remove unsupported runtime and distribution shims

Make progressive frame reads required, remove the `ListPagesRequest` value shim, rename transport spike code directly to V2, and delete the installer’s historical release cutoff/comparison fixtures. Preserve current schema, protocol, checksum, identity, privacy, and release-activation correctness. Verify current instructions and patterns no longer require unsupported compatibility machinery.

## Implementation notes

- Execution capability: high; this checkpoint removes public/core/distribution shims across several crates while retaining current protocol and installation guarantees.
- Review weight: standard from the delegated caller; this child closes on focused evidence and the integrated feature receives review.
- Files changed: core `FrameSource` and `ListPagesRequest` contracts plus explicit implementations/call sites; CDP spike evidence/harness/tests; installer and hermetic fixtures; current schema pattern and foundation storage contract.
- Tests added/removed: compilation now requires every `FrameSource` implementation to state progressive-read behavior; transport contract runs only under V2 Rust names; installer fixtures replace historical cutoff cases with malformed-version and current identity/integrity coverage and now pin the fixture platform.
- Simplification: deleted three trait defaults and shared fallback, the request value shim, two transport type aliases, release comparison/cutoff functions, historical cutoff fixtures, and the ordered-migration pattern.
- Discrepancies from design: `SqliteIndex` explicitly returns unsupported for coherent progressive reads because only `RecordingStore` owns deletion/revalidation authority; this behavior moved out of the core trait default rather than being removed.
- Adjacent issues parked: none.
- Verification: formatting, core all-target check, full store library tests, CDP spike transport contract tests, and hermetic installer fixtures passed. Aggregate workspace compilation was deferred to the feature boundary while a concurrent persistence worker updates the shared MCP shutdown contract.
