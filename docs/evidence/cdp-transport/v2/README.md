# CDP transport qualification evidence v2

This directory contains the **current schema-v2 qualification reports and generated decision**. They were independently normalized, decisively validated, and regenerated from the ignored final5 inputs at exact revision `a0e98ad6bd9c53d10385020bc43629f7ac246173`. The bounded sanitized candidate trace, wire observations, runtime assertions, canonical configuration, source attestation, thresholds, and redaction checks all pass. The schema remains the Rust-generated v2 contract at [`schema.json`](./schema.json).

- [`cdpkit-linux.json`](./cdpkit-linux.json) — clean Linux qualification
- [`cdpkit-macos.json`](./cdpkit-macos.json) — hosted manual `workflow_dispatch` run `29212145045`
- [`decision.json`](./decision.json) — generated solely by `decide_from_files`
- `historical/final-v2-07b0990/` — byte-for-byte prior canonical reports and decision from revision `07b0990c0d9e4fea9057fcab5c35e56691ff69eb`
- `historical/README.md` — earlier prior-v2 provenance

## Final qualification identity

Both reports were captured from exact revision `a0e98ad6bd9c53d10385020bc43629f7ac246173` and use exact `cdpkit` `0.4.0`, Cargo checksum `c3fdb566d913b31e0014391a94c0db4ed871dbb76577dd1b2f2c5f6df158bfaa`.

| Identity | Value |
| --- | --- |
| Linux report SHA-256 | `c5ed8bfab9cb829f0d1e1622755667084abc09129ed1f2928cdc5f577d3761f8` |
| macOS report SHA-256 | `7b2d7c61d61400f47281423d35ea57d51b1292cc78a95c4d7cef3118476c2264` |
| Generated decision SHA-256 | `dfbd51c9e7a1f8e051c173df35962bc6f443d2b5c28037e406c3a72beda6472a` |
| Configuration SHA-256 | `sha256:06388b5f8ad042093d22408dedb8d02d5a04a9e59d485158edc533334bab956e` |
| Browser fixture identity | `sha256sum-of-ordered-fixture-files:9b42ae730d12a95772a946bf55e4838a5443b6cb4c536424570219041b6e2a68:84ba666539a996012a781637c1a894d8c7a4789cfca84661bd7cf8b79efa2e13` |
| Candidate-contract fixture digest | `sha256:622fb296e0b50bf0dc81123c5f54a797040cdc48bd6b5f9ca96167bbe87fce76` |
| Candidate-contract trace digest | `sha256:33ccc161726cc35f68e6a260c129a06f9050af4a616a76c8b957525f557a6e00` |
| Candidate-contract observations | `942` |
| Source-attestation digest | `sha256:96acbed658fb89a71a90107ac0bfec0ab78860e57f95a374cc9e183d672a4c5a` |

The source attestation is embedded identically in both reports and was independently recomputed against the exact clean checkout. `validate-decisive` rejected any revision, tracked relevant-file, relevant-untracked-file, canonical configuration, threshold, or redaction mismatch before this evidence was installed.

### Source attestation file digests

| File | SHA-256 |
| --- | --- |
| `.github/workflows/cdp-transport-gate.yml` | `sha256:a62d4b38e926757ad05384e6228e9450677d27cea57feb533d1e0097d8b0bf01` |
| `Cargo.lock` | `sha256:19921d3f80b037a62b0a8b07f283f1533e2d5a03928360090d78bec6fbdcc73f` |
| `Cargo.toml` | `sha256:e199dbd2f92a7574b6c46159b8578992e079e11f636199c4ca03a82ce41afeb2` |
| `crates/krometrail-cdp/Cargo.toml` | `sha256:0b90eb4381925ee6aad8b4c5f62527a108380740c818cefe4e4572e801045058` |
| `crates/krometrail-cdp/src/bin/cdp-transport-gate.rs` | `sha256:f67e3bd25133b1914af9cfd852fcbe932f014956f79d036c4a93b8c43a332017` |
| `crates/krometrail-cdp/src/spike/cdpkit_adapter.rs` | `sha256:6b743cf84e23da7e38a0eeb66a2f4cfea5b9d06dc42943b49d9bcbf12b614c31` |
| `crates/krometrail-cdp/src/spike/chrome_harness.rs` | `sha256:a4df3be0454cda3cbbcfd8ebdfe7481262607dcf077a4bffb826080adbeed498` |
| `crates/krometrail-cdp/src/spike/contract.rs` | `sha256:7cda1c1e86d54a3f27816d45c4ec680fc80d4b458cc8b2a026d30c79d4b05c34` |
| `crates/krometrail-cdp/src/spike/error.rs` | `sha256:4da7b931559d8fa66f6dd3c34a132bce3514089583912396fb6846f7593f8ca5` |
| `crates/krometrail-cdp/src/spike/evidence.rs` | `sha256:ffd749ad1962bb70091c1e7e1461d02e11a661dd4c1ba02cc04b9a6f39e8b38d` |
| `crates/krometrail-cdp/src/spike/fake.rs` | `sha256:07f2613c18c01a3dfe546f6c9b4fb579e740a421e2b622c9ad1c41277a28d8bd` |
| `crates/krometrail-cdp/src/spike/fixture_server.rs` | `sha256:8f7db255c22c6791e61ef0362eec4c76867eab6e4e7f8d98389f8172ac2ea87c` |
| `crates/krometrail-cdp/src/spike/mod.rs` | `sha256:745e0e75d052709d79df62f961ea2f9bdcf46eb14f681403b405ae57e31b6ff4` |
| `crates/krometrail-cdp/src/spike/scenarios.rs` | `sha256:012c6c23062bb49abd791abe0ec1496080ea96334f2a0accd8cd60903334501e` |
| `crates/krometrail-cdp/src/spike/scripted_peer.rs` | `sha256:d4765e4bac6118dcb7f1b3d9a9822e0b75fc3d224bb26d855f226c1d9ac1b452` |
| `crates/krometrail-cdp/tests/cdpkit_transport_contract.rs` | `sha256:c5be17cf5056d96b6d11f18ac68f65b6a994c187869f351d21ca234d56d4df0c` |
| `crates/krometrail-cdp/tests/fixtures/protocol/additive-field.json` | `sha256:72be8368a42eb36816a4d08942f8d7b7ad18790000db5a96cca532f699b203e5` |
| `crates/krometrail-cdp/tests/fixtures/protocol/unknown-enum.json` | `sha256:98189617ef6632ce738917ed88c1d273bc95f65f33300311f3f58c34cafe4a89` |
| `crates/krometrail-cdp/tests/fixtures/protocol/unknown-event.json` | `sha256:de70ce94d3a5af666382a9ca2077d6cb3756a647d6e54a1139397029c848ed78` |
| `crates/krometrail-cdp/tests/transport_contract.rs` | `sha256:6202f9061b475a2c192c1eb22dadd81b94559cd9e534188733f82c5c08d89095` |
| `scripts/cdp-transport-gate-cross-worktree.sh` | `sha256:4d0f1e293c631414d88c5b91c108d47cc9313fe5ef32ba12cef19a7f7bd29705` |
| `tests/fixtures/browser/cdp-transport-gate/animation.js` | `sha256:84ba666539a996012a781637c1a894d8c7a4789cfca84661bd7cf8b79efa2e13` |
| `tests/fixtures/browser/cdp-transport-gate/index.html` | `sha256:9b42ae730d12a95772a946bf55e4838a5443b6cb4c536424570219041b6e2a68` |

## Raw and sanitized provenance

The parent qualification supplied these ignored final5 inputs:

```text
target/cdp-transport-gate/final5/cdpkit-linux.json
  sha256 c5ed8bfab9cb829f0d1e1622755667084abc09129ed1f2928cdc5f577d3761f8
target/cdp-transport-gate/final5/cdpkit-macos.json
  sha256 7b2d7c61d61400f47281423d35ea57d51b1292cc78a95c4d7cef3118476c2264
target/cdp-transport-gate/final5/decision.json
  sha256 dfbd51c9e7a1f8e051c173df35962bc6f443d2b5c28037e406c3a72beda6472a
```

Each report input was strict-normalized to an ignored output and compared byte-for-byte before installation. The generated decision was independently recomputed from only those normalized reports and compared byte-for-byte:

```bash
GATE_SHA=a0e98ad6bd9c53d10385020bc43629f7ac246173
for platform in linux macos; do
  cargo run --locked -p krometrail-cdp --features cdp-spike-cdpkit \
    --bin cdp-transport-gate -- validate-and-normalize \
    --input target/cdp-transport-gate/final5/cdpkit-${platform}.json \
    --output target/cdp-transport-gate/final5/cdpkit-${platform}.normalized.json
  cmp -- target/cdp-transport-gate/final5/cdpkit-${platform}.json \
    target/cdp-transport-gate/final5/cdpkit-${platform}.normalized.json
  cargo run --locked -p krometrail-cdp --features cdp-spike-cdpkit \
    --bin cdp-transport-gate -- validate-decisive \
    --repo-root "$PWD" --input target/cdp-transport-gate/final5/cdpkit-${platform}.normalized.json \
    --platform "$platform" --expected-git-revision "$GATE_SHA"
done
cargo run --locked -p krometrail-cdp --features cdp-spike-cdpkit \
  --bin cdp-transport-gate -- decide --repo-root "$PWD" \
  --linux-report target/cdp-transport-gate/final5/cdpkit-linux.normalized.json \
  --macos-report target/cdp-transport-gate/final5/cdpkit-macos.normalized.json \
  --output target/cdp-transport-gate/final5/decision.recomputed.json
cmp -- target/cdp-transport-gate/final5/decision.json \
  target/cdp-transport-gate/final5/decision.recomputed.json
```

The normalized report digests are therefore exactly the current canonical digests above. The Rust normalizer and decisive validator recursively reject absolute/private paths, HTTP/WS endpoints, IP endpoints, credentials, usernames, email identities, bracketed IPv6, encoded endpoint forms, and secret-bearing failure text. They also reject unknown fields, non-canonical configuration, threshold drift, mismatched duplicated trace/result/digest fields, and any gate failure; the installed reports contain no leakage or failures.

The hosted macOS capture used manual `workflow_dispatch` run `29212145045`, exact ref/SHA verification, the full default, spike, cdpkit, and documentation/schema checks, unchanged candidate tests, the real-Chrome gate, normalization, and decisive validation. The local Linux capture used the clean exact revision and the same canonical configuration. No macOS-only waiver or threshold change was used. The temporary hosted branch remains intentionally untouched; its authorized deletion is parent-owned.

## Configuration and exact fixture contract

Every decisive report and decision uses the one canonical configuration below. The Rust report, CLI, workflow, and decision validators reject any field deviation, even when a caller recomputes a matching digest (for example, a `999999`-second hard stop).

```text
minimum_seconds=60
minimum_frames=1000
saturation_seconds=10
saturation_attempts=100
hard_stop_seconds=120
configuration_sha256=sha256:06388b5f8ad042093d22408dedb8d02d5a04a9e59d485158edc533334bab956e
```

The observed capture and handoff elapsed measurements are positive, meet their configured thresholds, and remain strictly below the 120-second hard stop. Sanitization applies recursively to candidate trace values and rejects hostnames with ports, email identities, bracketed IPv6, percent-encoded URLs/endpoints after bounded repeated decoding, and generic endpoint or user credentials. Field-specific identity allowlists preserve browser, Rust, candidate, revision, digest, fixture, and summary evidence that is valid by contract.

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
| sustained-screencast | `capture_elapsed_seconds=60.015619792`, `frames_received=3601.0`, `frames_acknowledged=3601.0`, `handoff_accepted=1.0`, `handoff_dropped=3600.0`, `handoff_elapsed_seconds=60.015619792`, `saturation_attempts=3601.0`, `ack_latency_ms_p50=0.13815`, `ack_latency_ms_p95=0.17198`, `ack_latency_ms_p99=0.214389`, `ack_latency_ms_max=0.889178`, `rss_samples=51.0`, `rss_sampling_interval_seconds=1.0`, `rss_warmup_seconds=10.0`, `rss_first_window_median_bytes=15519744.0`, `rss_last_window_median_bytes=15519744.0`, `rss_peak_bytes=15544320.0`, `rss_theil_sen_bytes_per_minute=0.0`, `upstream_queue_depth_available=0.0` |
| prompt-acknowledgement | `ack_before_handoff=1.0`, `ack_latency_ms_p50=0.13815`, `ack_latency_ms_p95=0.17198`, `ack_latency_ms_p99=0.214389`, `ack_latency_ms_max=0.889178` |
| bounded-handoff-saturation | `handoff_attempts=3601.0`, `handoff_accepted=1.0`, `handoff_dropped=3600.0`, `handoff_elapsed_seconds=60.015619792` |
| bounded-memory-proxy | `rss_samples=51.0`, `rss_sampling_interval_seconds=1.0`, `rss_warmup_seconds=10.0`, `rss_growth_bytes=0.0`, `rss_first_window_median_bytes=15519744.0`, `rss_last_window_median_bytes=15519744.0`, `rss_peak_bytes=15544320.0`, `rss_theil_sen_bytes_per_minute=0.0`, `upstream_queue_depth_available=0.0` |
| disconnect-cleanup | `pending_command_started=1.0`, `pending_calls_closed=1.0`, `subscriptions_closed=1.0`, `close_reason_observed=1.0`, `pending_command_elapsed_seconds=0.036807502`, `subscription_elapsed_seconds=0.036811722` |
| explicit-reconnect-rebuild | `connections=2.0`, `sessions_rebuilt=2.0`, `elapsed_seconds=0.219646228` |

### macOS (`aarch64`, Chrome/149.0.7827.201, rustc 1.97.0)

| Gate | Measurements |
| --- | --- |
| deterministic-routing | `commands=200.0`, `events=200.0`, `cross_delivery=0.0` |
| typed-domains | `typed_operations=5.0` |
| flat-session-isolation | `sessions=2.0`, `commands_per_session=100.0`, `events_per_session=100.0`, `cross_delivery=0.0` |
| raw-browser-command | `commands=1.0` |
| raw-session-command | `commands=1.0` |
| named-raw-event-params | `named_events=1.0` |
| protocol-drift-survival | `fixtures=3.0`, `connection_survived=1.0`, `wildcard_envelope_available=0.0` |
| sustained-screencast | `capture_elapsed_seconds=60.012583042`, `frames_received=3566.0`, `frames_acknowledged=3566.0`, `handoff_accepted=1.0`, `handoff_dropped=3565.0`, `handoff_elapsed_seconds=60.012583042`, `saturation_attempts=3566.0`, `ack_latency_ms_p50=0.183792`, `ack_latency_ms_p95=0.28049999999999997`, `ack_latency_ms_p99=0.582458`, `ack_latency_ms_max=12.67025`, `rss_samples=51.0`, `rss_sampling_interval_seconds=1.0`, `rss_warmup_seconds=10.0`, `rss_first_window_median_bytes=13041664.0`, `rss_last_window_median_bytes=13090816.0`, `rss_peak_bytes=13090816.0`, `rss_theil_sen_bytes_per_minute=67025.45454545454`, `upstream_queue_depth_available=0.0` |
| prompt-acknowledgement | `ack_before_handoff=1.0`, `ack_latency_ms_p50=0.183792`, `ack_latency_ms_p95=0.28049999999999997`, `ack_latency_ms_p99=0.582458`, `ack_latency_ms_max=12.67025` |
| bounded-handoff-saturation | `handoff_attempts=3566.0`, `handoff_accepted=1.0`, `handoff_dropped=3565.0`, `handoff_elapsed_seconds=60.012583042` |
| bounded-memory-proxy | `rss_samples=51.0`, `rss_sampling_interval_seconds=1.0`, `rss_warmup_seconds=10.0`, `rss_growth_bytes=49152.0`, `rss_first_window_median_bytes=13041664.0`, `rss_last_window_median_bytes=13090816.0`, `rss_peak_bytes=13090816.0`, `rss_theil_sen_bytes_per_minute=67025.45454545454`, `upstream_queue_depth_available=0.0` |
| disconnect-cleanup | `pending_command_started=1.0`, `pending_calls_closed=1.0`, `subscriptions_closed=1.0`, `close_reason_observed=1.0`, `pending_command_elapsed_seconds=0.3884075`, `subscription_elapsed_seconds=0.388418625` |
| explicit-reconnect-rebuild | `connections=2.0`, `sessions_rebuilt=2.0`, `elapsed_seconds=3.285154` |

The acknowledgement contract is receive → ack completion → bounded handoff. Both reports show equal received/acknowledged frame counts, `ack_before_handoff=1`, explicit handoff drops, post-receive ack latency below the p99/max limits, and measured elapsed handoff. The 120-second hard stop remains the authoritative configured deadline when the frame minimum is unmet. Disconnect cleanup is below one second and explicit reconnect/session rebuild is below five seconds on both platforms. RSS uses 51 one-second samples with a 10-second warmup; the report retains the process-level limitation that cdpkit's subscriber queue depth is not introspectable.

## Limitations and workflow

- cdpkit exposes named event params through an unbounded subscriber; wildcard/full-envelope receive and queue-depth introspection are unavailable.
- Ack latency values are receive-to-ack-completion proxies, not wire-enqueue timestamps.
- RSS is a process-level proxy from a continuously drained reader; it does not prove the hidden cdpkit subscriber queue is bounded.
- The candidate-contract trace digest is attached to the structured candidate contract and report limitations.
- The workflow is manual-only `workflow_dispatch` with exact ref plus full SHA inputs; there is no push trigger. Default, spike, cdpkit, schema, normalization, decisive, documentation, and full quality gates remain required. Raw artifacts and profiles remain under ignored `target/`; no path, endpoint, credential, username, or other host identity is committed.

## Historical and failed runs

The superseded 07b0990 canonical reports and decision were copied byte-for-byte into `historical/final-v2-07b0990/` before installing these final5 reports. Their original digests are Linux `a7195eda1667e613b1b3f857fd56cc60153500544493a86afac8448706d20270`, macOS `46901e41bb2a4bb674d76d9dce41fc4200032280cd9720daaaad965ee89d257b`, and decision `91f9032315dd3501068e1dd692b12fbda7ce0d7a57c9b5a49444db73c2a5c015`. Earlier prior-v2 artifacts remain directly under `historical/`.

Earlier failed attempts remain documented in repository history and the historical narratives. No threshold was waived and no fallback rule was invoked: if a decisive gate had failed, the report would have remained a schema-valid failure and the conditional chromey/owned-transport selection rules would have applied.
