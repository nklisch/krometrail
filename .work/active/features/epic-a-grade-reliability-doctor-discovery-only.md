---
id: epic-a-grade-reliability-doctor-discovery-only
kind: feature
stage: implementing
tags: [infra, storage, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Keep doctor independent of recording storage and reclamation

## Outcome and priority

The documented discovery-only diagnostic initializes the recording runtime and performs cache reclamation before browser discovery. A browser health check can therefore delete retained cache and require writable storage it does not need.

- **Priority:** P1 — wave 2 of [epic-a-grade-reliability](../../backlog/epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Reproduced in isolated storage: doctor removed abandoned recording evidence and preserved managed profiles.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Authorized scoped implementation after the user asked to continue; the design below owns this unit. No release or model-effectiveness study is authorized.

## Evidence

- src/main.rs:58 — runtime built before Doctor/Mcp dispatch
- src/app.rs:375,412 — storage initialization and abandoned-root reclamation

## Acceptance criteria

- [ ] Doctor discovers browsers or returns browser_not_found without initializing, recovering, reclaiming, or changing recording-cache members.
- [ ] Read-only or unusable recording storage does not prevent browser discovery; test with injected or genuinely non-writable storage rather than a root-bypass permission assertion.
- [ ] An abandoned recording cache and its known contents survive doctor byte-for-byte; profiles, configuration, and downloads also survive.
- [ ] MCP startup retains its legitimate ownership-checked cache policy. Document any intentionally retained diagnostic logging side effect separately.

## Implementation direction and boundaries

Compose the discovery command from discovery dependencies rather than constructing the full recording/browser runtime.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Authorized design and implementation boundary — 2026-09-05

The user authorized continued work. The current main constructs `build_runtime` before dispatching Doctor/Mcp; `Runtime::run(Doctor)` therefore discovers only after instance ownership, cache reclamation/recovery, and storage validation have already run. Split command composition before recording runtime construction. Doctor should invoke the existing bounded browser discovery/launcher authority directly, with an injectable seam for deterministic tests, not construct inert storage or a fake full runtime. MCP alone composes recording/runtime services and preserves its legitimate startup cleanup. Remove obsolete Doctor-through-recording-runtime branches/test scaffolding rather than retaining two operational routes.

Keep existing discovery results, browser-not-found errors, and output stable. Best-effort diagnostic logging may remain, but state that side effect precisely; unusable recording storage/configuration cannot block discovery. Test an abandoned cache and protected root members byte-for-byte, a path structurally unusable for storage (not chmod-only under root), invalid recording-specific configuration, browser absence, and normal discovery. Use existing binary smoke tests and fake bounded browser executables/ports; never launch Chrome just to test doctor. Do not change store reclamation, profile ownership, compiler/release tooling, or root Cargo metadata. Parent reviews src/main.rs, app composition and binary-boundary tests before acceptance.

Demonstrate the old doctor deleting isolated abandoned cache or failing on unusable recording storage, then green after the split. Use isolated temporary data and never the operator's actual recording root. Run focused smoke/composition regressions, formatting and relevant lint/tests. Update essential troubleshooting/runtime documentation only where needed; do not hand-edit generated llms-full output. No generic dependency-injection framework or new CLI capabilities are needed.
