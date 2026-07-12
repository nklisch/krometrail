# CDP transport qualification evidence v2

Version 2 is the strict, platform-faithful evidence contract after wire-authenticity and deadline remediation. The previously committed reports are historical artifacts and are obsolete after the drift-trace contract revision; fresh qualification must emit replacement reports. No historical report is hand-edited:

- [`cdpkit-linux.json`](./cdpkit-linux.json) — historical local Linux strict run (obsolete)
- [`cdpkit-macos.json`](./cdpkit-macos.json) — historical hosted macOS run `29202919716` (obsolete)
- [`decision.json`](./decision.json) — historical v2 decision (obsolete until fresh reports are requalified)

The historical generated decision selected exact `cdpkit` 0.4.0 (`adopt_cdpkit`). It preserved all 13 platform-labelled gate results and the then-current candidate-contract results for Linux and macOS; it did not collapse platform measurements into a Linux-only rollup. It is not a decision under the revised contract.

## Decision provenance

The decision was generated solely from the accepted sanitized reports by `decide_from_files`:

```bash
cargo run --locked -p krometrail-cdp --features cdp-spike --bin cdp-transport-gate -- decide \
  --linux-report docs/evidence/cdp-transport/v2/cdpkit-linux.json \
  --macos-report docs/evidence/cdp-transport/v2/cdpkit-macos.json \
  --output docs/evidence/cdp-transport/v2/decision.json
```

The report digests bound into `decision.json` are `sha256:0d11c4c8168d8ef2e988b2f71400696dc8a9521add23ba645b9ea65a03e0b148` (Linux) and `sha256:c206b1a04651421b8b88f42d75920800a75ee85ed83756f8792191a5e9b3b998` (macOS). The generated decision digest is `0288aa9a379b467042409ac27056107b443ea0d91bd21fc4fc8c2beae44c075b`.

## Immutable qualification identity

Both reports use the exact immutable gate revision `3d7c96ccf20862c47ab70ffbd7f724dceedfb4d2` (`HEAD` at qualification time), configuration digest `sha256:06388b5f8ad042093d22408dedb8d02d5a04a9e59d485158edc533334bab956e`, and fixture identity:

```text
name: cdp-transport-gate
sha256: sha256sum-of-ordered-fixture-files:9b42ae730d12a95772a946bf55e4838a5443b6cb4c536424570219041b6e2a68:84ba666539a996012a781637c1a894d8c7a4789cfca84661bd7cf8b79efa2e13
```

The exact candidate in both reports is `cdpkit` `0.4.0`, Cargo checksum `c3fdb566d913b31e0014391a94c0db4ed871dbb76577dd1b2f2c5f6df158bfaa`. The checksum is independently confirmed by the `cdpkit 0.4.0` entry in the locked dependency graph (`Cargo.lock` / `cargo tree --locked`).

The unchanged thresholds are:

```text
minimum_seconds=60
minimum_frames=1000
saturation_seconds=10
saturation_attempts=100
hard_stop_seconds=120
```

## Run provenance and exact report bytes

### Linux

The local strict run used the committed gate at the exact revision above, the cdpkit feature, the same five threshold arguments, an explicit expected revision, and emitted a raw report before validation. Its accepted sanitized report is 6,951 bytes with the exact SHA-256 digest:

```text
0d11c4c8168d8ef2e988b2f71400696dc8a9521add23ba645b9ea65a03e0b148  cdpkit-linux.json
```

Equivalent command sequence:

```bash
export GATE_SHA=3d7c96ccf20862c47ab70ffbd7f724dceedfb4d2
cargo run --locked -p krometrail-cdp --features cdp-spike-cdpkit --bin cdp-transport-gate -- gate \
  --chrome-binary "$CHROME_BIN" --expected-git-revision "$GATE_SHA" \
  --minimum-seconds 60 --minimum-frames 1000 --saturation-seconds 10 \
  --saturation-attempts 100 --hard-stop-seconds 120 \
  --output target/cdp-transport-gate/cdpkit-linux.raw.json
cargo run --locked -p krometrail-cdp --features cdp-spike-cdpkit --bin cdp-transport-gate -- validate-and-normalize \
  --input target/cdp-transport-gate/cdpkit-linux.raw.json \
  --output target/cdp-transport-gate/cdpkit-linux.sanitized.json
cargo run --locked -p krometrail-cdp --features cdp-spike-cdpkit --bin cdp-transport-gate -- validate-decisive \
  --input target/cdp-transport-gate/cdpkit-linux.sanitized.json --platform linux \
  --expected-git-revision "$GATE_SHA"
```

### macOS

Hosted GitHub Actions run `29202919716` checked out and verified the same full SHA, ran the same gate, schema comparison, strict contract assertions, and validation. The workflow emitted `cdpkit-macos.raw.json` and `cdpkit-macos.sanitized.json`; only the sanitized report is retained here. It is 6,944 bytes with the exact SHA-256 digest:

```text
c206b1a04651421b8b88f42d75920800a75ee85ed83756f8792191a5e9b3b998  cdpkit-macos.json
```

The hosted run's effective gate command is identical to the Linux command, with `--platform macos` during decisive validation and the macOS Chrome binary supplied by the runner. The hosted workflow also compared its generated schema byte-for-byte with [`schema.json`](./schema.json).

## Exact configuration and schema checks

The generated schema is the Rust evidence types' source of truth. Regenerate/check it from the repository root:

```bash
CDP_SPIKE_WRITE_SCHEMA=1 cargo test --locked -p krometrail-cdp --features cdp-spike --test transport_contract checked_schema_is_generated_by_the_rust_evidence_types
```

For each accepted report, the validation pass was run into a temporary output and compared byte-for-byte with the committed report. This verifies both strict normalization and byte stability without changing the source report:

```bash
for platform in linux macos; do
  cargo run --locked -p krometrail-cdp --features cdp-spike-cdpkit --bin cdp-transport-gate -- validate-and-normalize \
    --input docs/evidence/cdp-transport/v2/cdpkit-${platform}.json \
    --output target/cdp-transport-gate/cdpkit-${platform}.normalized.json
  cmp -- docs/evidence/cdp-transport/v2/cdpkit-${platform}.json \
    target/cdp-transport-gate/cdpkit-${platform}.normalized.json
  cargo run --locked -p krometrail-cdp --features cdp-spike-cdpkit --bin cdp-transport-gate -- validate-decisive \
    --input docs/evidence/cdp-transport/v2/cdpkit-${platform}.json \
    --platform "$platform" --expected-git-revision "$GATE_SHA"
done
```

The historical reports were schema v2 and contained exactly 13 unique passing gates, with no `rss_sample_count` alias or nominal `deadline_seconds`; they are retained only for provenance. Fresh reports must validate against the regenerated schema and revised candidate-contract fields.

## Candidate-contract trace

The candidate-contract shape now binds the exact ordered protocol fixtures and classifies its projections:

```text
fixture_sha256: sha256:6dc599e64e0245b5f29eae0644dddb3a5e7222a234b7e2602a6a8577a25e677e
wire.drift_methods: Protocol.unknownEvent, Runtime.additiveField, Runtime.unknownEnum
wire.drift_fixtures: count(wire.drift_methods)
wire.*: projected only from recorded scripted WebSocket observations
runtime.*: cdpkit close-status assertions, not wire observations
```

The scripted server loads `crates/krometrail-cdp/tests/fixtures/protocol/{unknown-event,additive-field,unknown-enum}.json` in that order. The scenario asserts each exact method, session scope, and parameter object, including `new_field: 7` and `value: future-value`, through cdpkit. The trace digest is over the ordered observation bytes; fixture count and methods are projected from those observations. Chrome cannot be instructed to emit unknown future protocol fields, so this contract remains separate from real-Chrome measurements.

Decision generation rejects Linux/macOS reports unless fixture digest, trace hash, and the complete deterministic wire/runtime result object match exactly. Historical JSON reports still use the retired flat result shape and must not be normalized or hand-edited.

## Exact observed gate measurements

Every value below is the runner-emitted JSON measurement. Values are intentionally platform-labelled; there is no Linux-only rollup.

### Linux (`cdpkit-linux.json`)

| Gate | Measurements |
| --- | --- |
| deterministic-routing | `commands=200.0`, `events=200.0`, `cross_delivery=0.0` |
| typed-domains | `typed_operations=5.0` |
| flat-session-isolation | `sessions=2.0`, `commands_per_session=100.0`, `events_per_session=100.0`, `cross_delivery=0.0` |
| raw-browser-command | `commands=1.0` |
| raw-session-command | `commands=1.0` |
| named-raw-event-params | `named_events=1.0` |
| protocol-drift-survival | `fixtures=3.0`, `connection_survived=1.0`, `wildcard_envelope_available=0.0` |
| sustained-screencast | `elapsed_seconds=60.013945058`, `frames_received=3601.0`, `frames_acknowledged=3601.0`, `ack_latency_ms_p50=16.655679`, `ack_latency_ms_p95=17.205039`, `ack_latency_ms_p99=17.795669`, `ack_latency_ms_max=19.721226`, `handoff_accepted=1.0`, `handoff_dropped=3600.0`, `saturation_attempts=3601.0`, `saturation_seconds=60.013945058`, `rss_samples=51.0`, `rss_sampling_interval_seconds=1.0`, `rss_warmup_seconds=10.0`, `rss_first_window_median_bytes=12976128.0`, `rss_last_window_median_bytes=12976128.0`, `rss_peak_bytes=12976128.0`, `rss_theil_sen_bytes_per_minute=0.0`, `upstream_queue_depth_available=0.0` |
| prompt-acknowledgement | `ack_before_handoff=1.0`, `ack_latency_ms_p50=16.655679`, `ack_latency_ms_p95=17.205039`, `ack_latency_ms_p99=17.795669`, `ack_latency_ms_max=19.721226` |
| bounded-handoff-saturation | `handoff_attempts=3601.0`, `handoff_accepted=1.0`, `handoff_dropped=3600.0`, `saturation_seconds=60.013945058` |
| bounded-memory-proxy | `rss_samples=51.0`, `rss_sampling_interval_seconds=1.0`, `rss_warmup_seconds=10.0`, `rss_growth_bytes=0.0`, `rss_first_window_median_bytes=12976128.0`, `rss_last_window_median_bytes=12976128.0`, `rss_peak_bytes=12976128.0`, `rss_theil_sen_bytes_per_minute=0.0`, `upstream_queue_depth_available=0.0` |
| disconnect-cleanup | `pending_command_started=1.0`, `pending_calls_closed=1.0`, `subscriptions_closed=1.0`, `close_reason_observed=1.0`, `pending_command_elapsed_seconds=0.009479799`, `subscription_elapsed_seconds=0.009483069` |
| explicit-reconnect-rebuild | `connections=2.0`, `sessions_rebuilt=2.0`, `elapsed_seconds=0.22490539` |

### macOS (`cdpkit-macos.json`)

| Gate | Measurements |
| --- | --- |
| deterministic-routing | `commands=200.0`, `events=200.0`, `cross_delivery=0.0` |
| typed-domains | `typed_operations=5.0` |
| flat-session-isolation | `sessions=2.0`, `commands_per_session=100.0`, `events_per_session=100.0`, `cross_delivery=0.0` |
| raw-browser-command | `commands=1.0` |
| raw-session-command | `commands=1.0` |
| named-raw-event-params | `named_events=1.0` |
| protocol-drift-survival | `fixtures=3.0`, `connection_survived=1.0`, `wildcard_envelope_available=0.0` |
| sustained-screencast | `elapsed_seconds=60.019783292`, `frames_received=3571.0`, `frames_acknowledged=3571.0`, `ack_latency_ms_p50=16.535749999999997`, `ack_latency_ms_p95=20.596917`, `ack_latency_ms_p99=22.518792`, `ack_latency_ms_max=556.233375`, `handoff_accepted=1.0`, `handoff_dropped=3570.0`, `saturation_attempts=3571.0`, `saturation_seconds=60.019783292`, `rss_samples=51.0`, `rss_sampling_interval_seconds=1.0`, `rss_warmup_seconds=10.0`, `rss_first_window_median_bytes=10780672.0`, `rss_last_window_median_bytes=10780672.0`, `rss_peak_bytes=10780672.0`, `rss_theil_sen_bytes_per_minute=0.0`, `upstream_queue_depth_available=0.0` |
| prompt-acknowledgement | `ack_before_handoff=1.0`, `ack_latency_ms_p50=16.535749999999997`, `ack_latency_ms_p95=20.596917`, `ack_latency_ms_p99=22.518792`, `ack_latency_ms_max=556.233375` |
| bounded-handoff-saturation | `handoff_attempts=3571.0`, `handoff_accepted=1.0`, `handoff_dropped=3570.0`, `saturation_seconds=60.019783292` |
| bounded-memory-proxy | `rss_samples=51.0`, `rss_sampling_interval_seconds=1.0`, `rss_warmup_seconds=10.0`, `rss_growth_bytes=0.0`, `rss_first_window_median_bytes=10780672.0`, `rss_last_window_median_bytes=10780672.0`, `rss_peak_bytes=10780672.0`, `rss_theil_sen_bytes_per_minute=0.0`, `upstream_queue_depth_available=0.0` |
| disconnect-cleanup | `pending_command_started=1.0`, `pending_calls_closed=1.0`, `subscriptions_closed=1.0`, `close_reason_observed=1.0`, `pending_command_elapsed_seconds=0.0211495`, `subscription_elapsed_seconds=0.021154` |
| explicit-reconnect-rebuild | `connections=2.0`, `sessions_rebuilt=2.0`, `elapsed_seconds=2.97012425` |

## Limitations

These are retained verbatim in both reports:

- cdpkit exposes named event params through an unbounded subscriber; wildcard/full-envelope receive and queue-depth introspection are unavailable.
- Ack latency values are receive-to-ack-completion proxies, not wire-enqueue timestamps.
- RSS is a process-level proxy from a continuously drained reader; it does not prove the hidden cdpkit subscriber queue is bounded.
- The candidate-contract trace digest is attached to the limitations as well as the structured `candidate_contract` object.

## Rejected attempts

- At exact commit `1688178f3938876ec4f3aec2a41711b38deace87`, Linux failed before Chrome capture because the candidate-contract helper started a scripted server without binding the supplied cdpkit factory. No report was produced.
- At exact commit `8d01d50956650befe603bd4178afbbb2ff473105`, hosted macOS run `29202075722` passed the exact-path candidate test and then failed the gate with an immediate connection close. The simultaneous Linux run exhausted the 120-second hard stop without stage context. Neither report was accepted.
- Preparation commit `39149eac1f955b1533bce52dd3ae61f74f2ec723` only installed the strict runner/workflow/docs; it intentionally produced no hosted dispatch or evidence.

The runtime-determinism and endpoint-binding repairs were completed before the accepted run at `3d7c96ccf20862c47ab70ffbd7f724dceedfb4d2`. Thresholds were not weakened, and no production adapter/core-port change was made.
