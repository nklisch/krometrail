---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-attested-final-recapture
kind: story
stage: done
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-process-tree-runtime-root, epic-rust-cdp-capture-foundation-cdp-transport-gate-trace-reconstructability, epic-rust-cdp-capture-foundation-cdp-transport-gate-decisive-config-redaction, epic-rust-cdp-capture-foundation-cdp-transport-gate-browser-revision-identity, epic-rust-cdp-capture-foundation-cdp-transport-gate-workflow-config-digest-order]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Recapture reconstructable attested evidence

## Failed attempt

Clean Linux qualification and hosted macOS run `29211340202` at exact revision `a0593c041f541fdc43e5e4f732eeb2a5a0dea777` both failed after capture because the new allowlist rejected Chrome's canonical `@` + 40-hex browser revision. No report was accepted. The fix is tracked by `epic-rust-cdp-capture-foundation-cdp-transport-gate-browser-revision-identity`.

At exact revision `365d02eaec088b954cabe65cab6b8a34a27d424d`, Linux passed. Hosted macOS run `29211668813` also completed the gate plus Rust normalization/decisive validation, but a redundant Python workflow assertion reserialized alphabetically ordered `canonical-config.json` rather than canonical struct-field order and falsely rejected the valid digest. No macOS report is accepted. `epic-rust-cdp-capture-foundation-cdp-transport-gate-workflow-config-digest-order` tracks the check; both platforms must rerun from its later SHA for one-revision evidence.

## Scope

After final validator/runtime fixes, run clean exact-SHA Linux and hosted manual macOS qualification with canonical configuration. Commit sanitized reports containing reconstructable candidate trace evidence, clean source attestation, and all observed gates. Preserve current canonical reports/decision under historical provenance. Regenerate decision and roll exact evidence through all docs/items, then remove temporary remote branch.

## Acceptance criteria

- [x] Both reports pass from one clean exact revision and canonical configuration with reconstructable identical trace evidence.
- [x] Process/profile cleanup is verified after each run; no gate leak remains.
- [x] Reports/decision/docs are byte-reproducible and historical evidence is preserved.
- [x] Temporary hosted branch is removed; full local quality/docs gates pass with no production/core leakage.

## Implementation notes

- Execution capability: inline/direct-read evidence installation; the caller explicitly prohibited subagents and questions, and the supplied final5 inputs were staged ignored artifacts at the exact current revision.
- Review weight: standard; caller explicitly required this story to remain `implementing` for parent review/cleanup.
- Files changed: `docs/evidence/cdp-transport/v2/cdpkit-linux.json`, `docs/evidence/cdp-transport/v2/cdpkit-macos.json`, `docs/evidence/cdp-transport/v2/decision.json`, `docs/evidence/cdp-transport/v2/README.md`, `docs/evidence/cdp-transport/v2/historical/README.md`, `docs/evidence/cdp-transport/v2/historical/final-v2-07b0990/` (byte-preserved reports/decision plus provenance), `docs/research/rust-cdp-transport-2026-07.md`, `.agents/skills/rust-cdp-transport/SKILL.md`, `.work/active/features/epic-rust-cdp-capture-foundation-cdp-transport-gate.md`, `.work/active/epics/epic-rust-cdp-capture-foundation.md`, `docs/ARCHITECTURE.md`, and stale transport-gate story narratives.
- Final provenance: exact revision `a0e98ad6bd9c53d10385020bc43629f7ac246173`; clean Linux report `sha256:c5ed8bfab9cb829f0d1e1622755667084abc09129ed1f2928cdc5f577d3761f8`; hosted manual macOS run `29212145045`, report `sha256:7b2d7c61d61400f47281423d35ea57d51b1292cc78a95c4d7cef3118476c2264`; recomputed decision `sha256:dfbd51c9e7a1f8e051c173df35962bc6f443d2b5c28037e406c3a72beda6472a`.
- Recomputed evidence: canonical configuration `sha256:06388b5f8ad042093d22408dedb8d02d5a04a9e59d485158edc533334bab956e`; source attestation `sha256:96acbed658fb89a71a90107ac0bfec0ab78860e57f95a374cc9e183d672a4c5a`; candidate fixture `sha256:622fb296e0b50bf0dc81123c5f54a797040cdc48bd6b5f9ca96167bbe87fce76`; identical candidate trace `sha256:33ccc161726cc35f68e6a260c129a06f9050af4a616a76c8b957525f557a6e00` with 942 observations and identical wire/runtime results.
- Measurements: Linux 3,601 frames / 60.015619792 s, ack p99/max `0.214389/0.889178` ms, RSS growth `0` bytes, reconnect/rebuild `0.219646228` s; macOS 3,566 frames / 60.012583042 s, ack p99/max `0.582458/12.67025` ms, RSS growth `49,152` bytes, reconnect/rebuild `3.285154` s. Both reports have 51 RSS samples, equal received/acknowledged frames, explicit saturated-handoff drops, all 13 gates pass, and no redaction failure.
- Normalization/validation: `canonical-config` plus `verify-canonical-config`; `validate-and-normalize` and `cmp` for both final5 reports; `validate-decisive` for Linux and macOS; `decide` from only normalized reports plus `cmp` against supplied decision; independent SHA-256/JSON checks of trace material, duplicated results, configuration, source attestation, report, and decision bytes.
- Historical preservation: prior canonical 07b0990 report/decision bytes remain unchanged under `docs/evidence/cdp-transport/v2/historical/final-v2-07b0990/` with digests Linux `a7195eda...6d20270`, macOS `46901e41...9d257b`, decision `91f90323...a5c015`; earlier prior-v2 history remains alongside it.
- Workflow/cleanup: workflow remains manual-only exact-ref+SHA with default/spike/cdpkit/schema/normalization/decisive/docs gates and no push trigger. `.work/bin/work-view` was restored, `.pi/` remains ignored, and no production/core files or temporary branch were deleted locally.
- Discrepancies from design: supplied final5 reports were already sanitized, so normalization proved byte stability rather than changing report bytes; parent completed the separately authorized remote branch deletion after the evidence commit.
- Adjacent issues parked: none.

## Review (2026-07-13)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane final evidence review reproduced decision bytes and all canonical digests, verified exact clean revision, reconstructable identical trace material, canonical configuration, hosted run success, zero gate profiles, preserved historical generations, and absent temporary remote branch. Verdict: Approve - story verified by implement; fast-lane advance.
