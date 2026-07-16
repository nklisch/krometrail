---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-browser-revision-identity
kind: story
stage: done
tags: [bug, browser, infra, security, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-decisive-config-redaction]
release_binding: 1.0.0
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

- [x] Canonical Chrome revision values pass strict evidence validation.
- [x] Email, endpoint, malformed hash, uppercase, suffix, and encoded bypasses remain rejected.
- [x] Focused real-Chrome short gate and full candidate tests/clippy pass.
- [x] No evidence is hand-edited and no production/core change lands.

## Implementation notes

- Execution capability: inline single-stride implementation; this is a focused validator and regression-test change in two owned files, so coordination or an isolated worker would add risk without improving coverage.
- Review weight: standard, from the project default; left at `stage: review` for the requested review boundary.
- Root cause: `validate_sanitized_fields` applied the shared identity-byte allowlist to `browser.revision`, which rejected Chrome's documented `@` + 40 lowercase hexadecimal Chromium commit identity.
- Fix: added a field-specific exact grammar for `@` + 40 lowercase hexadecimal revisions, retaining only the documented `unavailable` pre-Chrome failure sentinel; the shared allowlist remains unchanged for other fields.
- Regression coverage: `crates/krometrail-cdp/tests/transport_contract.rs` accepts the exact Linux (`@07b52360cc15066f987c910ab34dfbcd4a8778d2`) and macOS (`@6a7b3dbec3b2ca25877c2553b5473b2f277ef644`) report revisions plus `unavailable`, and rejects email, endpoint, short/long hashes, uppercase, suffix, and percent-encoded mutations.
- Verification: focused revision test passed; full candidate suite passed (`42` tests), including the bounded short real-Chrome gate twice with zero process/profile cleanup leaks; `cargo clippy -p krometrail-cdp --features cdp-spike-cdpkit --all-targets -- -D warnings` passed.
- No evidence was edited, and no production/core runtime change was made.

## Review (2026-07-13)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane validator review verified exact Chrome revision grammar, accepted Linux/macOS identities, rejected encoded/endpoint/email/malformed near misses, 42 candidate-feature tests including real Chrome, zero cleanup leaks, and denied-warning clippy. Verdict: Approve - story verified by implement; fast-lane advance.
