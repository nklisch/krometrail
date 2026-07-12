---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-browser-revision-identity
kind: story
stage: implementing
tags: [bug, browser, infra, security, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-decisive-config-redaction]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-12
---

# Permit canonical Chrome revision identity

## Reproduction

Clean Linux qualification at exact revision `a0593c041f541fdc43e5e4f732eeb2a5a0dea777` completed the browser gate but failed sanitization with `browser.revision contains a non-canonical identity character`. Chrome reports its legitimate revision as `@` followed by a lowercase 40-hex commit, which the new field-specific allowlist omitted. No report was accepted.

## Scope

Validate `browser.revision` with an exact field-specific grammar matching observed Chrome/CDP revision identities (including `@` + 40 lowercase hex), without allowing arbitrary email/user/endpoint characters elsewhere. Add accepted canonical and rejected malicious/near-miss regressions. Ensure the real gate reaches validated evidence.

## Acceptance criteria

- [ ] Canonical Chrome revision values pass strict evidence validation.
- [ ] Email, endpoint, malformed hash, uppercase, suffix, and encoded bypasses remain rejected.
- [ ] Focused real-Chrome short gate and full candidate tests/clippy pass.
- [ ] No evidence is hand-edited and no production/core change lands.
