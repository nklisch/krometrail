---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-final-requalification
kind: story
stage: review
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

- [x] Both reports derive from one clean exact revision/config/fixture and carry valid tree attestation.
- [x] All observed gates and exact drift fixture params pass unchanged thresholds; candidate traces are identical.
- [x] Raw/sanitized run provenance and report digests are retained/documented without hand edits.
- [x] A failure triggers published fallback rules rather than a waiver; no fallback was needed because both decisive reports passed.

## Implementation notes

- Execution capability: inline/direct-read evidence requalification; the caller explicitly prohibited subagents and questions, and the work was an evidence installation plus strict validation rather than a production-code change.
- Review weight: standard; caller requested the implementing-to-review boundary.
- Files changed: `docs/evidence/cdp-transport/v2/cdpkit-linux.json`, `docs/evidence/cdp-transport/v2/cdpkit-macos.json`, `docs/evidence/cdp-transport/v2/README.md`, `docs/evidence/cdp-transport/v2/historical/README.md`, byte-for-byte historical v2 JSON relocations under `docs/evidence/cdp-transport/v2/historical/`, and generated `docs/public/llms-full.txt`.
- Final identity: exact revision `07b0990c0d9e4fea9057fcab5c35e56691ff69eb`; Linux report `sha256:a7195eda1667e613b1b3f857fd56cc60153500544493a86afac8448706d20270`; hosted macOS run `29207244853`, report `sha256:46901e41bb2a4bb674d76d9dce41fc4200032280cd9720daaaad965ee89d257b`; configuration `sha256:06388b5f8ad042093d22408dedb8d02d5a04a9e59d485158edc533334bab956e`; source attestation `sha256:b4147b12577e980123bfb711d314dd17f22b0639303956e97441af74a8b297b0`; fixture, candidate-contract trace, exact params, full measurements, raw/sanitized paths, failed histories, and limitations are documented in the current v2 README.
- Validation: strict `validate-and-normalize` plus byte-for-byte `cmp` and `validate-decisive` passed for both reports; all 13 gates are `pass` with `failure: null`; source revision/tree attestation, candidate trace/runtime equality, post-receive ack ordering, 60-second/1,000-frame thresholds, 120-second hard stop, disconnect/rebuild deadlines, RSS proxies, recursive redaction, schema generation, focused fake/candidate contract tests, clippy, and docs build passed.
- History: prior v2 Linux/macOS reports and decision were moved byte-for-byte to `docs/evidence/cdp-transport/v2/historical/` with provenance README. The generated final decision remains ignored under `target/cdp-transport-gate/final/` and was not generated, installed, or committed by this story; the next story owns it.
- Discrepancies from design: the supplied final artifacts were already sanitized JSON inputs, so normalization proved byte stability rather than producing a changed sanitized copy; no raw unsanitized payload was promoted into tracked evidence.
- Adjacent issues parked: none.
