---
name: patterns
description: "Project code patterns and conventions. Auto-loads when implementing, designing, verifying, or reviewing code. Provides detailed pattern definitions with code examples."
user-invocable: false
allowed-tools: Read, Glob, Grep
---

# Project Patterns Reference

This skill contains detailed pattern documentation for this project.
See individual pattern files for full details with code examples.

Available patterns:
- [canonical-result-projection.md](canonical-result-projection.md) — Derive compact or omitted agent presentations only after canonical acquisition and mapping, preserving outcomes and drill-down authority.
- [ergonomic-input-canonicalization.md](ergonomic-input-canonicalization.md) — Materialize convenience inputs into existing explicit domain authorities before execution, retaining convenience metadata only as provenance.
- [authority-revalidated-handles.md](authority-revalidated-handles.md) — On every dereference, revalidate handle scope, generation, backing identity, and current availability against the owning authority.
- [privacy-bounded-debug.md](privacy-bounded-debug.md) — Give secret-bearing types explicit Debug implementations that emit only safe identities, counts, states, flags, or digests.
- [validated-wire-contracts.md](validated-wire-contracts.md) — Wire decoding delegates to domain validation while schemas remain source-aligned.
- [injected-core-ports.md](injected-core-ports.md) — Domain contracts flow inward; concrete adapters are wired at the composition root.
- [registry-declared-surfaces.md](registry-declared-surfaces.md) — One registry declaration drives identities, metadata, schemas, and registration.
- [bounded-loss-accounting.md](bounded-loss-accounting.md) — Bounded streams report every rejected or missed observation as explicit evidence quality state.
- [single-writer-effect-reducer.md](single-writer-effect-reducer.md) — Serialized inputs produce deterministic state and an explicit effect queue.
- [exact-release-managed-activation.md](exact-release-managed-activation.md) — Plugin/runtime activation derives one exact release version, verifies it before execution, and avoids unconstrained latest-driven updates.
- [hermetic-release-boundary-fixtures.md](hermetic-release-boundary-fixtures.md) — Distribution tests shadow external commands and release assets in temp state to verify release seams without network or user-home mutation.
- [layered-cdp-qualification.md](layered-cdp-qualification.md) — Deterministic doubles, boundary fault injection, and explicit real-browser qualification form one test ladder.
- [ordered-sql-migrations.md](ordered-sql-migrations.md) — Immutable numbered SQL revisions are centrally ordered and transactionally applied.
- [canonical-json-schema-artifacts.md](canonical-json-schema-artifacts.md) — Rust models generate canonical checked-in JSON and schemas verified by digest and byte equality.
- [narrowest-temporal-scope.md](narrowest-temporal-scope.md) — Constrain time-bearing values at every narrowing boundary, preserving semantic provenance while fitting concrete retained evidence.
- [lifecycle-complete-browser-overrides.md](lifecycle-complete-browser-overrides.md) — Persisted target overrides must apply, clear, roll back, and replay every external-state facet as one acknowledged lifecycle contract.
