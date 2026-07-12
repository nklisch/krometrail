---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-attested-final-recapture
kind: story
stage: implementing
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-process-tree-runtime-root, epic-rust-cdp-capture-foundation-cdp-transport-gate-trace-reconstructability, epic-rust-cdp-capture-foundation-cdp-transport-gate-decisive-config-redaction, epic-rust-cdp-capture-foundation-cdp-transport-gate-browser-revision-identity, epic-rust-cdp-capture-foundation-cdp-transport-gate-workflow-config-digest-order]
release_binding: null
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

- [ ] Both reports pass from one clean exact revision and canonical configuration with reconstructable identical trace evidence.
- [ ] Process/profile cleanup is verified after each run; no gate leak remains.
- [ ] Reports/decision/docs are byte-reproducible and historical evidence is preserved.
- [ ] Temporary hosted branch is removed; full quality/docs gates pass with no production/core leakage.
