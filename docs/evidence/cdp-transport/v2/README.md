# CDP transport qualification evidence v2

This directory contains the **current schema-v2 canonical qualification reports**. The final decision is intentionally not installed here; the next story generates it from these two reports. The schema remains the Rust-generated v2 contract at [`schema.json`](./schema.json).

- [`cdpkit-linux.json`](./cdpkit-linux.json) — clean detached-worktree Linux run
- [`cdpkit-macos.json`](./cdpkit-macos.json) — hosted manual `workflow_dispatch` run `29207244853`
- [`historical provenance`](./historical/README) — byte-for-byte prior v2 reports and decision, retained with provenance

## Final qualification identity

Both reports were captured from exact revision `07b0990c0d9e4fea9057fcab5c35e56691ff69eb` and use exact `cdpkit` `0.4.0`, Cargo checksum `c3fdb566d913b31e0014391a94c0db4ed871dbb76577dd1b2f2c5f6df158bfaa`.

| Identity | Value |
| --- | --- |
| Linux report SHA-256 | `a7195eda1667e613b1b3f857fd56cc60153500544493a86afac8448706d20270` |
| macOS report SHA-256 | `46901e41bb2a4bb674d76d9dce41fc4200032280cd9720daaaad965ee89d257b` |
| Configuration SHA-256 | `sha256:06388b5f8ad042093d22408dedb8d02d5a04a9e59d485158edc533334bab956e` |
| Browser fixture identity | `sha256sum-of-ordered-fixture-files:9b42ae730d12a95772a946bf55e4838a5443b6cb4c536424570219041b6e2a68:84ba666539a996012a781637c1a894d8c7a4789cfca84661bd7cf8b79efa2e13` |
| Candidate-contract fixture digest | `sha256:6dc599e64e0245b5f29eae0644dddb3a5e7222a234b7e2602a6a8577a25e677e` |
| Candidate-contract trace digest | `sha256:6c6be028c511d4d8c28cbecec368a7d4f09e0d87612741d02ac19a8663964d54` |
| Candidate-contract observations | `942` |
| Source-attestation digest | `sha256:b4147b12577e980123bfb711d314dd17f22b0639303956e97441af74a8b297b0` |

The source attestation is embedded identically in both reports and was independently checked against the clean exact-SHA checkout. Its complete file digest inventory is recorded below. `validate-decisive` rejected any revision, tracked relevant-file, or relevant-untracked-file mismatch before this evidence was installed.

### Source attestation file digests

| File | SHA-256 |
| --- | --- |
| `.github/workflows/cdp-transport-gate.yml` | `sha256:727f7733863dae78f462bbb5e4edee1ddf6751f07b55dd95843496115e14de62` |
| `Cargo.lock` | `sha256:7fdefbc19ef2a241da2c9cc79fd1ad64c53ea725124da67cc87672f052be9a37` |
| `Cargo.toml` | `sha256:08c290aaf1d1bcd7f0507500da7ac48f03c51d9f1e24bc3ec1787002a4680625` |
| `crates/krometrail-cdp/Cargo.toml` | `sha256:fc75d31498d93de7cba67bdb7c296dd8d117400cc5e4d7befec5562bdfbd69aa` |
| `crates/krometrail-cdp/src/bin/cdp-transport-gate.rs` | `sha256:83a7def532d251056af5d5db32572eccf76fcb6e614e8ba97dff5b90c6cba666` |
| `crates/krometrail-cdp/src/spike/cdpkit_adapter.rs` | `sha256:6b743cf84e23da7e38a0eeb66a2f4cfea5b9d06dc42943b49d9bcbf12b614c31` |
| `crates/krometrail-cdp/src/spike/chrome_harness.rs` | `sha256:f12ec4bdde841c3f26063eec3c3ffbeb1409f925d1e61ceb2b288be422361810` |
| `crates/krometrail-cdp/src/spike/contract.rs` | `sha256:52e84b958db6b628edf19dc96a86a726ccf8f77ad41fb8321dcf4918f62f2c47` |
| `crates/krometrail-cdp/src/spike/error.rs` | `sha256:4da7b931559d8fa66f6dd3c34a132bce3514089583912396fb6846f7593f8ca5` |
| `crates/krometrail-cdp/src/spike/evidence.rs` | `sha256:832b7e580b02682b8560d2fe34a5065f55e3945e00d94dec7e1c17679c823f1c` |
| `crates/krometrail-cdp/src/spike/fake.rs` | `sha256:07f2613c18c01a3dfe546f6c9b4fb579e740a421e2b622c9ad1c41277a28d8bd` |
| `crates/krometrail-cdp/src/spike/fixture_server.rs` | `sha256:8f7db255c22c6791e61ef0362eec4c76867eab6e4e7f8d98389f8172ac2ea87c` |
| `crates/krometrail-cdp/src/spike/mod.rs` | `sha256:9ac0ffc885d48f9024e9fcaefa78c3d242fae5744734f663f0470794ce9633b4` |
| `crates/krometrail-cdp/src/spike/scenarios.rs` | `sha256:15bee4fe567c174ad247cd68640e6d3ce04c8e8571356cf32d8a9f5573b621e0` |
| `crates/krometrail-cdp/src/spike/scripted_peer.rs` | `sha256:d0d31c3e0315bedb579003eb4ba4d3b54749dad09c58ee5de2b97d90541b8af1` |
| `crates/krometrail-cdp/tests/cdpkit_transport_contract.rs` | `sha256:bb3b1d38147a007be63cd699b3cad33a2d48a76e58af98166141002240c7997e` |
| `crates/krometrail-cdp/tests/fixtures/protocol/additive-field.json` | `sha256:72be8368a42eb36816a4d08942f8d7b7ad18790000db5a96cca532f699b203e5` |
| `crates/krometrail-cdp/tests/fixtures/protocol/unknown-enum.json` | `sha256:98189617ef6632ce738917ed88c1d273bc95f65f33300311f3f58c34cafe4a89` |
| `crates/krometrail-cdp/tests/fixtures/protocol/unknown-event.json` | `sha256:de70ce94d3a5af666382a9ca2077d6cb3756a647d6e54a1139397029c848ed78` |
| `crates/krometrail-cdp/tests/transport_contract.rs` | `sha256:78cfe572e263ec0041a7f18e4906f02d11dc540cce6390bcf8da38df1bab8ff2` |
| `tests/fixtures/browser/cdp-transport-gate/animation.js` | `sha256:84ba666539a996012a781637c1a894d8c7a4789cfca84661bd7cf8b79efa2e13` |
| `tests/fixtures/browser/cdp-transport-gate/index.html` | `sha256:9b42ae730d12a95772a946bf55e4838a5443b6cb4c536424570219041b6e2a68` |

## Raw and sanitized provenance

The parent qualification supplied these ignored final inputs:

```text
target/cdp-transport-gate/final/cdpkit-linux.json
  sha256 a7195eda1667e613b1b3f857fd56cc60153500544493a86afac8448706d20270
target/cdp-transport-gate/final/cdpkit-macos.json
  sha256 46901e41bb2a4bb674d76d9dce41fc4200032280cd9720daaaad965ee89d257b
```

Each input was strict-normalized to an ignored output and compared byte-for-byte before installation:

```bash
GATE_SHA=07b0990c0d9e4fea9057fcab5c35e56691ff69eb
for platform in linux macos; do
  cargo run --locked -p krometrail-cdp --features cdp-spike-cdpkit \
    --bin cdp-transport-gate -- validate-and-normalize \
    --input target/cdp-transport-gate/final/cdpkit-${platform}.json \
    --output target/cdp-transport-gate/${platform}.final.normalized.json
  cmp -- target/cdp-transport-gate/final/cdpkit-${platform}.json \
    target/cdp-transport-gate/${platform}.final.normalized.json
  cargo run --locked -p krometrail-cdp --features cdp-spike-cdpkit \
    --bin cdp-transport-gate -- validate-decisive \
    --input target/cdp-transport-gate/${platform}.final.normalized.json \
    --platform "$platform" --expected-git-revision "$GATE_SHA"
done
```

Both `cmp` checks passed. The normalized output digests are therefore exactly the canonical report digests above. The raw runner output was not copied into tracked evidence; the sanitized JSON inputs are the only current reports. The normalizer and decisive validator reject absolute/private paths, HTTP/WS endpoints, IP endpoints, credentials, usernames, and secret-bearing failure text recursively; both reports contain no gate failures, redaction violations, or machine-local endpoint data.

The hosted macOS capture used manual `workflow_dispatch` run `29207244853`, exact ref/SHA verification, the unchanged schema-v2 gate, fixture, candidate tests, thresholds, normalization, and decisive validation. The local Linux capture was a clean detached-worktree run at the same exact SHA. No macOS-only waiver or threshold change was used.

## Configuration and exact fixture contract

Both reports use:

```text
minimum_seconds=60
minimum_frames=1000
saturation_seconds=10
saturation_attempts=100
hard_stop_seconds=120
```

The three ordered committed drift fixtures were consumed through the cdpkit path. Exact observed methods were `Protocol.unknownEvent`, `Runtime.additiveField`, and `Runtime.unknownEnum`; the additive fixture carried `new_field: 7`, and the unknown-enum fixture carried `value: future-value`. The candidate trace has 942 observations, three fixtures, zero routing cross-delivery, event-before-response, detach-during-pending, socket closure, two explicit reconnects, and two rebuilt sessions. Linux and macOS candidate-contract fixture digest, trace digest, observations, and complete wire/runtime result object are byte-for-byte equal.

## All 13 gate measurements

Every gate is `pass` with `failure: null` on both reports. Values below are the complete runner-emitted measurement maps.

### Linux (`x86_64`, Chrome/149.0.7827.155, rustc 1.95.0)

| Gate | Measurements |
| --- | --- |
| deterministic-routing | `commands=200.0`, `events=200.0`, `cross_delivery=0.0` |
| typed-domains | `typed_operations=5.0` |
| flat-session-isolation | `sessions=2.0`, `commands_per_session=100.0`, `events_per_session=100.0`, `cross_delivery=0.0` |
| raw-browser-command | `commands=1.0` |
| raw-session-command | `commands=1.0` |
| named-raw-event-params | `named_events=1.0` |
| protocol-drift-survival | `fixtures=3.0`, `connection_survived=1.0`, `wildcard_envelope_available=0.0` |
| sustained-screencast | `capture_elapsed_seconds=60.012037205`, `frames_received=3601.0`, `frames_acknowledged=3601.0`, `handoff_accepted=1.0`, `handoff_dropped=3600.0`, `handoff_elapsed_seconds=60.012037205`, `saturation_attempts=3601.0`, `ack_latency_ms_p50=0.14341`, `ack_latency_ms_p95=0.217149`, `ack_latency_ms_p99=0.3979589999999999`, `ack_latency_ms_max=2.785427`, `rss_samples=51.0`, `rss_sampling_interval_seconds=1.0`, `rss_warmup_seconds=10.0`, `rss_first_window_median_bytes=13283328.0`, `rss_last_window_median_bytes=13479936.0`, `rss_peak_bytes=13479936.0`, `rss_theil_sen_bytes_per_minute=245760.0`, `upstream_queue_depth_available=0.0` |
| prompt-acknowledgement | `ack_before_handoff=1.0`, `ack_latency_ms_p50=0.14341`, `ack_latency_ms_p95=0.217149`, `ack_latency_ms_p99=0.3979589999999999`, `ack_latency_ms_max=2.785427` |
| bounded-handoff-saturation | `handoff_attempts=3601.0`, `handoff_accepted=1.0`, `handoff_dropped=3600.0`, `handoff_elapsed_seconds=60.012037205` |
| bounded-memory-proxy | `rss_samples=51.0`, `rss_sampling_interval_seconds=1.0`, `rss_warmup_seconds=10.0`, `rss_growth_bytes=196608.0`, `rss_first_window_median_bytes=13283328.0`, `rss_last_window_median_bytes=13479936.0`, `rss_peak_bytes=13479936.0`, `rss_theil_sen_bytes_per_minute=245760.0`, `upstream_queue_depth_available=0.0` |
| disconnect-cleanup | `pending_command_started=1.0`, `pending_calls_closed=1.0`, `subscriptions_closed=1.0`, `close_reason_observed=1.0`, `pending_command_elapsed_seconds=0.010723987`, `subscription_elapsed_seconds=0.010727187` |
| explicit-reconnect-rebuild | `connections=2.0`, `sessions_rebuilt=2.0`, `elapsed_seconds=0.224187241` |

### macOS (`aarch64`, Chrome/149.0.7827.155, rustc 1.97.0)

| Gate | Measurements |
| --- | --- |
| deterministic-routing | `commands=200.0`, `events=200.0`, `cross_delivery=0.0` |
| typed-domains | `typed_operations=5.0` |
| flat-session-isolation | `sessions=2.0`, `commands_per_session=100.0`, `events_per_session=100.0`, `cross_delivery=0.0` |
| raw-browser-command | `commands=1.0` |
| raw-session-command | `commands=1.0` |
| named-raw-event-params | `named_events=1.0` |
| protocol-drift-survival | `fixtures=3.0`, `connection_survived=1.0`, `wildcard_envelope_available=0.0` |
| sustained-screencast | `capture_elapsed_seconds=60.011273167`, `frames_received=3566.0`, `frames_acknowledged=3566.0`, `handoff_accepted=1.0`, `handoff_dropped=3565.0`, `handoff_elapsed_seconds=60.011273167`, `saturation_attempts=3566.0`, `ack_latency_ms_p50=0.20575`, `ack_latency_ms_p95=0.4225`, `ack_latency_ms_p99=1.062666`, `ack_latency_ms_max=7.058083`, `rss_samples=51.0`, `rss_sampling_interval_seconds=1.0`, `rss_warmup_seconds=10.0`, `rss_first_window_median_bytes=11894784.0`, `rss_last_window_median_bytes=11911168.0`, `rss_peak_bytes=11911168.0`, `rss_theil_sen_bytes_per_minute=21370.434782608696`, `upstream_queue_depth_available=0.0` |
| prompt-acknowledgement | `ack_before_handoff=1.0`, `ack_latency_ms_p50=0.20575`, `ack_latency_ms_p95=0.4225`, `ack_latency_ms_p99=1.062666`, `ack_latency_ms_max=7.058083` |
| bounded-handoff-saturation | `handoff_attempts=3566.0`, `handoff_accepted=1.0`, `handoff_dropped=3565.0`, `handoff_elapsed_seconds=60.011273167` |
| bounded-memory-proxy | `rss_samples=51.0`, `rss_sampling_interval_seconds=1.0`, `rss_warmup_seconds=10.0`, `rss_growth_bytes=16384.0`, `rss_first_window_median_bytes=11894784.0`, `rss_last_window_median_bytes=11911168.0`, `rss_peak_bytes=11911168.0`, `rss_theil_sen_bytes_per_minute=21370.434782608696`, `upstream_queue_depth_available=0.0` |
| disconnect-cleanup | `pending_command_started=1.0`, `pending_calls_closed=1.0`, `subscriptions_closed=1.0`, `close_reason_observed=1.0`, `pending_command_elapsed_seconds=0.023100583`, `subscription_elapsed_seconds=0.023088375` |
| explicit-reconnect-rebuild | `connections=2.0`, `sessions_rebuilt=2.0`, `elapsed_seconds=3.751358542` |

The acknowledgement contract is receive → ack completion → bounded handoff. Both reports show equal received/acknowledged frame counts, `ack_before_handoff=1`, explicit handoff drops, post-receive ack latency below the p99/max limits, and measured elapsed handoff. The 120-second hard stop remains the authoritative configured deadline when the frame minimum is unmet. Disconnect cleanup is below one second and explicit reconnect/session rebuild is below five seconds on both platforms. RSS uses 51 one-second samples with a 10-second warmup; the report retains the process-level limitation that cdpkit's subscriber queue depth is not introspectable.

## Limitations

- cdpkit exposes named event params through an unbounded subscriber; wildcard/full-envelope receive and queue-depth introspection are unavailable.
- Ack latency values are receive-to-ack-completion proxies, not wire-enqueue timestamps.
- RSS is a process-level proxy from a continuously drained reader; it does not prove the hidden cdpkit subscriber queue is bounded.
- The candidate-contract trace digest is attached to the structured candidate contract and report limitations.

## Failed histories

The earlier failed attempts remain documented in repository history and in the historical v2 provenance. The final requalification did not rewrite or waive them:

- Exact commit `1688178f3938876ec4f3aec2a41711b38deace87`: Linux failed before Chrome capture because the scripted candidate-contract server was not bound to the supplied cdpkit factory.
- Exact commit `8d01d50956650befe603bd4178afbbb2ff473105`: hosted macOS run `29202075722` passed the exact-path candidate test, then failed with an immediate connection close; the simultaneous Linux run exhausted the 120-second hard stop without stage context.
- Preparation commit `39149eac1f955b1533bce52dd3ae61f74f2ec723`: installed the strict runner/workflow/docs but intentionally produced no hosted dispatch or evidence.

The accepted final inputs are the only current canonical reports. No fallback rule was invoked: if a decisive gate had failed, the report would have remained a schema-valid failure and the conditional chromey/owned-transport selection rules would have applied rather than a waiver.
