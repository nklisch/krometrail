---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-decisive-config-redaction
kind: story
stage: review
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

- [x] Any decisive configuration or digest deviation is rejected; elapsed capture and handoff are positive, thresholded, and strictly below hard stop.
- [x] All reproduced encoded/hostname/email/IPv6 redaction bypasses are rejected recursively.
- [x] Legitimate browser/Rust/candidate/revision/digest/fixture identities remain valid by explicit contract.
- [x] Default/spike/candidate tests and denied-warning clippy pass; no production/core change or evidence edit lands.

## Implementation notes

- Execution capability: inline; one spike-only evidence/CLI/schema/workflow surface with no production or core ownership.
- Review weight: standard, default project review lane.
- Files changed: `crates/krometrail-cdp/src/spike/evidence.rs`, `crates/krometrail-cdp/src/bin/cdp-transport-gate.rs`, `crates/krometrail-cdp/src/spike/mod.rs`, `crates/krometrail-cdp/tests/transport_contract.rs`, `docs/evidence/cdp-transport/v2/schema.json`, `docs/evidence/cdp-transport/v2/README.md`, `.github/workflows/cdp-transport-gate.yml`.
- Tests added: exact canonical configuration/digest and recomputed `999999` hard-stop regressions; capture/handoff elapsed boundary regressions; recursive hostname/email/bracketed IPv6/percent-encoding/credential redaction regressions; canonical identity false-positive coverage; CLI noncanonical hard-stop regression.
- Discrepancies from design: none.
- Adjacent issues parked: none.
