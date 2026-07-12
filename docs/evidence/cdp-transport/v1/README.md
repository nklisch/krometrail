# CDP transport qualification evidence (historical)

This directory contains retained version-1 machine-readable evidence. It is obsolete and is not accepted by the strict version-2 contract under `../v2/`; reports and the decision remain byte-for-byte historical records. Raw runner logs and browser profiles remain outside Git under `target/`.

## Selected transport

The historical Rust decision selected exact published `cdpkit` **0.4.0** (`Cargo.lock` checksum `c3fdb566d913b31e0014391a94c0db4ed871dbb76577dd1b2f2c5f6df158bfaa`) because both reports passed the then-current 13 gates. That is not a current qualification claim. Phase 2 wire-authenticity remediation changed the qualification harness after these reports were captured; the reports and `decision.json` are retained unchanged and must not be treated as refreshed decisive evidence until both platforms are requalified.

- Linux report: `cdpkit-linux.json`
  - SHA-256: `081259729e2495e999745bcd7caa509ec7effc844f50b2a4d786d6cc744c7feb`
  - Platform: Linux x86_64; Chrome 149.0.7827.155
- macOS report: `cdpkit-macos.json`
  - SHA-256: `3ffe94f405038fd8d9efd9fa7f8acbf15e8cb02c1f9e19bf24397f180981d401`
  - Platform: macOS aarch64; Chrome 149.0.7827.201

Both reports use schema version 1, the same exact candidate, fixture digest, and unchanged configuration: at least 60 seconds, 1,000 frames, 10 seconds of capacity-1 saturation, and 100 saturation attempts. Every gate is present and `pass`; no platform exception or threshold waiver exists. They are now explicitly historical/obsolete inputs: the strict contract additionally requires a positive `hard_stop_seconds` and observed disconnect/rebuild elapsed fields, which these unchanged reports do not contain. They must not be edited or used for a new decision; both platforms require fresh qualification.

## Validation and decision reproduction

From the repository root:

```bash
cargo run -p krometrail-cdp --features cdp-spike --bin cdp-transport-gate -- \
  schema --output /tmp/cdp-transport-schema.json
cargo run -p krometrail-cdp --features cdp-spike --bin cdp-transport-gate -- \
  validate-and-normalize --input docs/evidence/cdp-transport/v1/cdpkit-linux.json \
  --output /tmp/cdpkit-linux.normalized.json
cargo run -p krometrail-cdp --features cdp-spike --bin cdp-transport-gate -- \
  validate-and-normalize --input docs/evidence/cdp-transport/v1/cdpkit-macos.json \
  --output /tmp/cdpkit-macos.normalized.json
cargo run -p krometrail-cdp --features cdp-spike --bin cdp-transport-gate -- \
  decide --linux-report docs/evidence/cdp-transport/v1/cdpkit-linux.json \
  --macos-report docs/evidence/cdp-transport/v1/cdpkit-macos.json \
  --output /tmp/decision.json
sha256sum docs/evidence/cdp-transport/v1/cdpkit-linux.json \
  docs/evidence/cdp-transport/v1/cdpkit-macos.json
cmp /tmp/decision.json docs/evidence/cdp-transport/v1/decision.json
```

The decision command verifies schema decoding, strict fields, complete gate registry, observed deadline measurements, measured thresholds, candidate/version/checksum consistency, Linux/macOS platform identity, fixture/configuration consistency, and redaction before hashing the exact report bytes. Against the retained historical reports it now fails during strict decoding/validation by design; re-run it only after fresh Linux and macOS reports are captured. The schema is generated from the Rust evidence types; `schema.json` is not a separate source of truth.

## Gate interpretation and limitations

The 13 gates cover deterministic routing, typed domains, flat-session isolation, browser/session raw commands, named raw event parameters, protocol-drift survival, sustained screencast, prompt acknowledgement, bounded handoff saturation, RSS trend, disconnect cleanup, and explicit reconnect/session rebuild.

The decision preserves these limitations exactly:

- cdpkit exposes **named event parameters**, not wildcard or full-envelope receive. The remediated runner binds unknown-event/additive-field/unknown-enum fixtures to a candidate-only observed-wire trace; it does not call those fixtures real-Chrome measurements.
- cdpkit's event subscriber is unbounded and its queue depth is not inspectable; the continuously drained RSS and handoff counters are process-level proxies only and do not prove the hidden queue is bounded.
- acknowledgement latency is a receive-to-ack-completion proxy, not a wire-enqueue timestamp.
- Krometrail owns reconnect/session restoration, bounded handoff, backpressure, capture gaps, cancellation, and flush behavior.

`chromey` was not tested because no cdpkit lifecycle, ordering, or sustained-capture failure was demonstrated. An owned Tokio/tokio-tungstenite transport remains the fallback if a later cdpkit integration needs a fork, loses raw protocol evolution before a preserved boundary, misroutes sessions, or obscures acknowledgement/backpressure. Neither fallback is wired or selected by this evidence.

The spike remains non-default and is not root-wired. This rollup selects the mechanism for the later production adapter; it does not implement production lifecycle, capture, reconnect, or core-port changes.
