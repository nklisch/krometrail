---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-decisive-config-redaction
kind: story
stage: implementing
tags: [bug, browser, infra, security, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Pin decisive configuration and close redaction bypasses

## Origin

Final adversarial review proved Rust accepts an arbitrarily large recomputed hard stop and recursive redaction accepts host:port names, emails, bracketed IPv6, and percent-encoded URLs/endpoints.

## Scope

Define one canonical decisive configuration (60 seconds, 1,000 frames, 10 seconds, 100 attempts, 120-second hard stop) and require exact equality and canonical digest in decisive report/decision validation. Require observed capture elapsed below hard stop. Harden all untrusted strings with field-specific allowlists where possible; normalize percent encoding and reject hostnames-with-ports, email identities, bracketed IPv6, encoded URLs/endpoints, credentials, paths, and usernames recursively. Add exact adversarial regressions.

## Acceptance criteria

- [ ] Any decisive configuration or digest deviation is rejected; elapsed capture must be below hard stop.
- [ ] All reproduced encoded/hostname/email/IPv6 redaction bypasses are rejected recursively.
- [ ] Legitimate browser/Rust/candidate/revision/digest/fixture identities remain valid by explicit contract.
- [ ] Default/spike/candidate tests and denied-warning clippy pass; no production/core change or evidence edit lands.
