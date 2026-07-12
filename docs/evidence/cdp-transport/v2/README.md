# CDP transport qualification evidence v2

Version 2 is the strict decisive evidence contract after wire-authenticity and deadline remediation. No v2 Linux or macOS report is committed yet: the retained `v1/` reports are historical, obsolete inputs and must not be edited, normalized into v2, or used to claim a transport selection.

## Contract

Every decisive platform report must use schema version 2 and carry:

- the exact candidate, fixture digest, gate implementation revision, and configuration digest;
- all 13 gates with every observed measurement required by `TransportGateId::ALL`;
- canonical RSS measurements (`rss_samples`, one-second `rss_sampling_interval_seconds`, and ten-second `rss_warmup_seconds`) on both RSS gates; `rss_sample_count` is rejected;
- observed disconnect/rebuild elapsed and outcome fields;
- the complete scripted candidate-contract trace digest, observation count, and results derived from that trace.

The decision contains one platform-labelled result for Linux and one for macOS, including each platform's gates and candidate-contract results. It never copies Linux measurements into a cross-platform rollup. Selection also requires identical immutable gate implementation revision, configuration digest, candidate, and fixture across both reports.

The generated schema is `schema.json`; Rust evidence types are its source of truth. Generate/check it from the repository root:

```bash
CDP_SPIKE_WRITE_SCHEMA=1 cargo test -p krometrail-cdp --features cdp-spike --test transport_contract checked_schema_is_generated_by_the_rust_evidence_types
```

## Requalification workflow

Run the repaired candidate gate on Linux and macOS from the same committed immutable revision. The hosted workflow keeps the temporary `ci/cdp-macos-evidence` push trigger until the final cleanup story; dispatches require both a ref and its exact full SHA. Linux must use the same binary, feature, thresholds, and explicit expected revision as macOS:

```bash
export GATE_SHA="$(git rev-parse HEAD)"
export CHROME_BIN="/path/to/chrome"
mkdir -p target/cdp-transport-gate
cargo run --locked -p krometrail-cdp \
  --features cdp-spike-cdpkit \
  --bin cdp-transport-gate -- gate \
  --chrome-binary "$CHROME_BIN" \
  --expected-git-revision "$GATE_SHA" \
  --minimum-seconds 60 \
  --minimum-frames 1000 \
  --saturation-seconds 10 \
  --saturation-attempts 100 \
  --hard-stop-seconds 120 \
  --output target/cdp-transport-gate/cdpkit-linux.raw.json
cargo run --locked -p krometrail-cdp \
  --features cdp-spike-cdpkit \
  --bin cdp-transport-gate -- validate-and-normalize \
  --input target/cdp-transport-gate/cdpkit-linux.raw.json \
  --output target/cdp-transport-gate/cdpkit-linux.sanitized.json
cargo run --locked -p krometrail-cdp \
  --features cdp-spike-cdpkit \
  --bin cdp-transport-gate -- validate-decisive \
  --input target/cdp-transport-gate/cdpkit-linux.sanitized.json \
  --platform linux \
  --expected-git-revision "$GATE_SHA"
```

The hosted macOS workflow runs this same gate with the resolved checkout SHA, compares the generated schema to this `v2/schema.json`, and performs the strict schema-v2 field/lifecycle/trace assertions before artifact upload. Preserve runner output and exact report-byte digests. Validate only reports emitted by that run, then decide:

```bash
cargo run --locked -p krometrail-cdp --features cdp-spike-cdpkit --bin cdp-transport-gate -- \
  decide --linux-report target/cdp-transport-gate/cdpkit-linux.sanitized.json \
  --macos-report target/cdp-transport-gate/cdpkit-macos.sanitized.json \
  --output target/cdp-transport-gate/decision.v2.json
```

Do not hand-edit or fabricate replacement evidence. Until both fresh reports pass, decision regeneration is expected to fail and no candidate is selected.
