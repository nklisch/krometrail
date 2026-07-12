---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-cross-platform-requalification
kind: story
stage: implementing
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-evidence-v2-contract, epic-rust-cdp-capture-foundation-cdp-transport-gate-candidate-contract-endpoint-binding, epic-rust-cdp-capture-foundation-cdp-transport-gate-runtime-determinism]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Requalify cdpkit on Linux and macOS from one immutable revision

## Origin

Phase 2 feature review found that Linux provenance was edited after its run and that accepted Linux/macOS reports used materially different gate implementations. Existing reports remain historical rejected inputs after the replacement run.

## Failed attempt

Linux qualification at exact commit `1688178f3938876ec4f3aec2a41711b38deace87` failed before Chrome capture because the decisive candidate-contract helper started a scripted server without binding the supplied cdpkit factory to it. No report was produced. The fix is tracked by `epic-rust-cdp-capture-foundation-cdp-transport-gate-candidate-contract-endpoint-binding`.

At the later exact revision `8d01d50956650befe603bd4178afbbb2ff473105`, hosted macOS run 29202075722 passed the exact-path candidate test then failed that same contract with an immediate connection close; Linux exhausted the complete 120-second hard stop without stage context. No report from either attempt is accepted. `epic-rust-cdp-capture-foundation-cdp-transport-gate-runtime-determinism` tracks both defects; both platforms must rerun from its later fixed SHA.

## Scope

Consume the strict schema-v2 contract from `...-evidence-v2-contract` and commit the repaired harness/contract first. Run full unchanged qualification on Linux and hosted macOS from that same exact immutable SHA and fixture digest. Preserve runner-emitted revisions unchanged. Validate, normalize, hash, and commit only reports that pass every required observed gate under existing thresholds. Do not weaken thresholds, carry aliases, or fabricate portability evidence.

## Preparation note

Preparatory strict cross-platform runner, CLI, workflow, and v2 documentation are committed as `39149eac1f955b1533bce52dd3ae61f74f2ec723` (`chore: prepare strict cross-platform CDP requalification`). The story remains `stage: implementing`: no decisive evidence was generated or edited, no hosted dispatch was performed, and no qualification is claimed.

## Acceptance criteria

- [ ] Linux and macOS reports name one exact committed gate revision and unchanged candidate/configuration/fixture digest.
- [ ] Every required candidate-contract and real-Chrome gate is observed, schema-valid, redacted, and passes unchanged thresholds.
- [ ] Runner-emitted provenance is preserved; raw run references and sanitized report digests are documented.
- [ ] No production adapter or core-port change lands; a real failure follows the published fallback protocol.
