---
id: epic-a-grade-reliability-doctor-discovery-only
kind: feature
stage: backlog
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

- **Priority:** P1 — wave 2 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Reproduced in isolated storage: doctor removed abandoned recording evidence and preserved managed profiles.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

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
