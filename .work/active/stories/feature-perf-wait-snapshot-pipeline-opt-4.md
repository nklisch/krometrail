---
id: feature-perf-wait-snapshot-pipeline-opt-4
kind: story
stage: done
tags: [perf]
parent: feature-perf-wait-snapshot-pipeline
depends_on: [feature-perf-wait-snapshot-pipeline-opt-3]
release_binding: null
gate_origin: perf-design
created: 2026-07-23
updated: 2026-07-23
---

# Unchanged-payload short-circuit (transport-seam gated)

Optimization 4 of the parent feature — the only lever that reaches the
~1–5 ms quiescent-poll target, and the only one gated on a transport-seam
decision. Every quiescent poll re-parses (66 ms) and re-decodes (68 ms) an
identical payload with zero reuse. See the parent feature body (Optimization 4)
for the resolved design decisions and the short-circuit pre-mortem.

## Transport decision required (host attention)

cdpkit 0.4.0 parses every inbound frame to `Value` in its read loop
(`inner.rs:230-233`) and discards the bytes, so a **pre-parse** byte
fingerprint is not reachable behind the unmodified adapter. The headline
~1–5 ms depends on choosing one of:
- (a) a minimal cdpkit read-loop hook / patched fork that hashes the text frame
  before `from_str` (a genuine transport decision per `docs/ARCHITECTURE.md`,
  keeping the seam replaceable); or
- (b) hold this story until that decision is made — opts 1–3 already deliver
  the transport-independent wins.

Do not silently downgrade to a parsed-`Value` digest (it forfeits the
parse-skip and, post opt-1/opt-3, costs about the decode it would save).

## Scope

- Additive `CdpTransport::send_raw_fingerprinted(scope, method, params) ->
  (Value, ResponseFingerprint)` (fast 128-bit non-cryptographic digest of the
  raw response body). Generic `send_raw` for all other commands untouched.
  Test transports hash the double they already hold.
- `ActiveSnapshot` gains `ax_fingerprint` + `dom_fingerprint: Option<_>`.
- In `capture_snapshot_for_frame` (geometry-free path only): fetch AX (+DOM)
  fingerprinted; if `reuse_candidate` matches, rebuild the `PageSnapshot` from
  the retained decoded nodes under the same generation; else run the existing
  decode+install miss path (byte-identical to today).
- `reuse_candidate` matches only on: same `attachment_generation`, same
  `document` fingerprint, matching `ax_fingerprint`, matching `dom_fingerprint`
  when a DOM payload was fetched, and `dom_semantics_captured >=
  requires_dom`. It still re-reads the document fingerprint after the DOM fetch
  (existing stale guard) before trusting reuse.
- **Scope**: waits AND `query_page` captures, gated to
  `include_document_geometry == false`. Geometry-bearing `snapshot_page`
  captures skip the short-circuit.

## Files

- `crates/krometrail-cdp/src/transport/mod.rs`,
  `crates/krometrail-cdp/src/transport/cdpkit.rs` — `send_raw_fingerprinted`,
  `ResponseFingerprint`, test-transport impls.
- `crates/krometrail-cdp/src/control/snapshot.rs` — `ActiveSnapshot` fields,
  `reuse_candidate`, `capture_snapshot_for_frame` short-circuit,
  `PageSnapshot` rebuild from retained nodes.

## Acceptance criteria

- [ ] Two consecutive byte-identical captures decode once; the second reuses
      (decode-call counter on a test transport).
- [ ] A one-byte payload change, a document-fingerprint change, and an
      attachment-generation change each force a full decode; the latter two also
      force a new generation.
- [ ] A DOM-needing request does not reuse an AX-only active snapshot.
- [ ] `snapshot_page` (geometry) captures never take the short-circuit.
- [ ] Hash-miss path latency is within noise of post-opt-3 behavior; a real page
      change is observed on the next poll with no added latency beyond the digest.
- [ ] `perf_poll_role_name_50k` / `perf_poll_text_50k` quiescent runs reach
      ~1–5 ms with the transport fingerprint enabled.

## Implementation notes

Closed unimplemented per the parent feature's `## Host adjudication (scope
decision)`: opt-4 is deferred because cdpkit 0.4.0 discards pre-parse bytes,
and no parsed-`Value` fingerprint or transport hook was added. This story's
runtime units and stage fields were intentionally left unchanged.
