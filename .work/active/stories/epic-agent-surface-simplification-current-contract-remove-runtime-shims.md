---
id: epic-agent-surface-simplification-current-contract-remove-runtime-shims
kind: story
stage: implementing
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
