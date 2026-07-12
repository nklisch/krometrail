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

Run the repaired candidate gate on Linux and macOS from the same committed immutable revision. Preserve runner output and exact report-byte digests. Validate only reports emitted by that run, then decide:

```bash
cargo run -p krometrail-cdp --features cdp-spike --bin cdp-transport-gate -- \
  validate-and-normalize --input target/cdp-transport-gate/cdpkit-linux.json \
  --output target/cdp-transport-gate/cdpkit-linux.v2.json
cargo run -p krometrail-cdp --features cdp-spike --bin cdp-transport-gate -- \
  decide --linux-report target/cdp-transport-gate/cdpkit-linux.v2.json \
  --macos-report target/cdp-transport-gate/cdpkit-macos.v2.json \
  --output target/cdp-transport-gate/decision.v2.json
```

Do not hand-edit or fabricate replacement evidence. Until both fresh reports pass, decision regeneration is expected to fail and no candidate is selected.
