---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-final-requalification
kind: story
stage: implementing
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-drift-trace-authenticity, epic-rust-cdp-capture-foundation-cdp-transport-gate-provenance-redaction-hardening, epic-rust-cdp-capture-foundation-cdp-transport-gate-capture-deadline-ack-semantics, epic-rust-cdp-capture-foundation-cdp-transport-gate-architecture-ack-order]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Recapture final strict Linux and macOS qualification

## Scope

After all second-review repairs commit, capture Linux and hosted macOS evidence from one exact immutable revision using the strict canonical contract. Prefer hosted Linux CI or use a clean detached exact-SHA worktree with before/after source attestation and retained raw/sanitized provenance. Run unchanged 60-second/1,000-frame thresholds, validate exact fixture params/trace equality, new acknowledgement timing, cancellation/deadline contract, redaction, and all 13 gates. Preserve failed attempts as history; do not rewrite older reports.

## Acceptance criteria

- [ ] Both reports derive from one clean exact revision/config/fixture and carry valid tree attestation.
- [ ] All observed gates and exact drift fixture params pass unchanged thresholds; candidate traces are identical.
- [ ] Raw/sanitized run provenance and report digests are retained/documented without hand edits.
- [ ] A failure triggers published fallback rules rather than a waiver.
