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
- [validated-wire-contracts.md](validated-wire-contracts.md) — Wire decoding delegates to domain validation while schemas remain source-aligned.
- [injected-core-ports.md](injected-core-ports.md) — Domain contracts flow inward; concrete adapters are wired at the composition root.
- [registry-declared-surfaces.md](registry-declared-surfaces.md) — One registry declaration drives identities, metadata, schemas, and registration.
- [bounded-loss-accounting.md](bounded-loss-accounting.md) — Bounded streams report every rejected or missed observation as explicit evidence quality state.
- [single-writer-effect-reducer.md](single-writer-effect-reducer.md) — Serialized inputs produce deterministic state and an explicit effect queue.
- [layered-cdp-qualification.md](layered-cdp-qualification.md) — Deterministic doubles, boundary fault injection, and explicit real-browser qualification form one test ladder.
- [ordered-sql-migrations.md](ordered-sql-migrations.md) — Immutable numbered SQL revisions are centrally ordered and transactionally applied.
- [canonical-json-schema-artifacts.md](canonical-json-schema-artifacts.md) — Rust models generate canonical checked-in JSON and schemas verified by digest and byte equality.
