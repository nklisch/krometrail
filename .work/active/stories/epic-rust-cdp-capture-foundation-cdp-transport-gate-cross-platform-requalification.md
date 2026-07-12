---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-cross-platform-requalification
kind: story
stage: implementing
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-evidence-v2-contract]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Requalify cdpkit on Linux and macOS from one immutable revision

## Origin

Phase 2 feature review found that Linux provenance was edited after its run and that accepted Linux/macOS reports used materially different gate implementations. Existing reports remain historical rejected inputs after the replacement run.

## Scope

Consume the strict schema-v2 contract from `...-evidence-v2-contract` and commit the repaired harness/contract first. Run full unchanged qualification on Linux and hosted macOS from that same exact immutable SHA and fixture digest. Preserve runner-emitted revisions unchanged. Validate, normalize, hash, and commit only reports that pass every required observed gate under existing thresholds. Do not weaken thresholds, carry aliases, or fabricate portability evidence.

## Preparation note

Preparatory strict cross-platform runner, CLI, workflow, and v2 documentation are committed as `39149eac1f955b1533bce52dd3ae61f74f2ec723` (`chore: prepare strict cross-platform CDP requalification`). The story remains `stage: implementing`: no decisive evidence was generated or edited, no hosted dispatch was performed, and no qualification is claimed.

## Acceptance criteria

- [ ] Linux and macOS reports name one exact committed gate revision and unchanged candidate/configuration/fixture digest.
- [ ] Every required candidate-contract and real-Chrome gate is observed, schema-valid, redacted, and passes unchanged thresholds.
- [ ] Runner-emitted provenance is preserved; raw run references and sanitized report digests are documented.
- [ ] No production adapter or core-port change lands; a real failure follows the published fallback protocol.
