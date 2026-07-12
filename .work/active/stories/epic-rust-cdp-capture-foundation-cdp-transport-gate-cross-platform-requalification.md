---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-cross-platform-requalification
kind: story
stage: done
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-evidence-v2-contract, epic-rust-cdp-capture-foundation-cdp-transport-gate-candidate-contract-endpoint-binding, epic-rust-cdp-capture-foundation-cdp-transport-gate-runtime-determinism]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Requalify cdpkit on Linux and macOS from one immutable revision (prior-v2 milestone)

## Origin

Phase 2 feature review found that Linux provenance was edited after its run and that accepted Linux/macOS reports used materially different gate implementations. Existing reports remain historical rejected inputs after the replacement run.

## Failed attempt history

- Linux at exact commit `1688178f3938876ec4f3aec2a41711b38deace87` failed before Chrome capture because the decisive candidate-contract helper started a scripted server without binding the supplied cdpkit factory. No report was produced.
- At exact revision `8d01d50956650befe603bd4178afbbb2ff473105`, hosted macOS run `29202075722` passed the exact-path candidate test and then failed the gate with an immediate connection close. The simultaneous Linux run exhausted the complete 120-second hard stop without stage context. Neither report was accepted.
- Preparation commit `39149eac1f955b1533bce52dd3ae61f74f2ec723` installed the strict runner, CLI, workflow, and v2 documentation but intentionally performed no hosted dispatch and produced no evidence.

The endpoint-binding and runtime-determinism repairs were complete at the prior-v2 gate revision `3d7c96ccf20862c47ab70ffbd7f724dceedfb4d2`. Thresholds were not weakened. That evidence was later superseded by final requalification at `07b0990c0d9e4fea9057fcab5c35e56691ff69eb`.

## Scope

Consume the strict schema-v2 contract from `...-evidence-v2-contract`, run the unchanged qualification on Linux and hosted macOS from one exact immutable SHA/configuration/fixture, and commit only runner-emitted reports that pass every required observed gate. Validate and normalize without editing report bytes. The follow-up decision/cleanup story owns generated decision rollup and architecture/bootstrap updates. No production adapter or core-port change is permitted.

## Accepted evidence for this prior-v2 milestone

These reports were accepted at the time of this story and are now historical inputs retained under `docs/evidence/cdp-transport/v2/historical/`. The current final v2 reports and decision are documented by the dependent final-v3 rollup story. The prior-v2 report paths were:

- `docs/evidence/cdp-transport/v2/cdpkit-linux.json`
- `docs/evidence/cdp-transport/v2/cdpkit-macos.json`

Both reports are schema v2 and use:

```text
gate/source revision: 3d7c96ccf20862c47ab70ffbd7f724dceedfb4d2
configuration: minimum_seconds=60, minimum_frames=1000, saturation_seconds=10,
              saturation_attempts=100, hard_stop_seconds=120
configuration_sha256: sha256:06388b5f8ad042093d22408dedb8d02d5a04a9e59d485158edc533334bab956e
candidate: cdpkit 0.4.0
candidate checksum: c3fdb566d913b31e0014391a94c0db4ed871dbb76577dd1b2f2c5f6df158bfaa
fixture: cdp-transport-gate
fixture digest: sha256sum-of-ordered-fixture-files:9b42ae730d12a95772a946bf55e4838a5443b6cb4c536424570219041b6e2a68:84ba666539a996012a781637c1a894d8c7a4789cfca84661bd7cf8b79efa2e13
candidate trace: sha256:6c6be028c511d4d8c28cbecec368a7d4f09e0d87612741d02ac19a8663964d54
trace observations: 942
trace results: drift_fixtures=3, connection_survived=true, routing_commands=200,
  routing_events=200, routing_cross_delivery=0, event_before_response=true,
  detach_during_pending=true, pending_calls_closed=true, subscriptions_closed=true,
  socket_closed=true, reconnect_connections=2, sessions_rebuilt=2
```

Linux was the local strict run. Its sanitized report is 6,951 bytes with digest `0d11c4c8168d8ef2e988b2f71400696dc8a9521add23ba645b9ea65a03e0b148`. Hosted macOS run `29202919716` emitted the raw and sanitized artifacts after exact-SHA checkout and strict workflow assertions. Its sanitized report is 6,944 bytes with digest `c206b1a04651421b8b88f42d75920800a75ee85ed83756f8792191a5e9b3b998`.

The exact gate measurements are preserved in the reports and exhaustively tabulated in `docs/evidence/cdp-transport/v2/README.md`. The decisive platform-specific values are:

| Gate | Linux | macOS |
| --- | --- | --- |
| deterministic-routing | commands 200, events 200, cross-delivery 0 | commands 200, events 200, cross-delivery 0 |
| typed-domains | typed operations 5 | typed operations 5 |
| flat-session-isolation | 2 sessions, 100 commands/session, 100 events/session, cross-delivery 0 | 2 sessions, 100 commands/session, 100 events/session, cross-delivery 0 |
| raw-browser-command / raw-session-command | 1 / 1 command | 1 / 1 command |
| named-raw-event-params | 1 named event | 1 named event |
| protocol-drift-survival | 3 fixtures, survived 1, wildcard envelope 0 | 3 fixtures, survived 1, wildcard envelope 0 |
| sustained-screencast | 60.013945058 s, 3601 received/acknowledged, p50/p95/p99/max 16.655679/17.205039/17.795669/19.721226 ms, RSS 51 samples, 12,976,128 peak bytes | 60.019783292 s, 3571 received/acknowledged, p50/p95/p99/max 16.535749999999997/20.596917/22.518792/556.233375 ms, RSS 51 samples, 10,780,672 peak bytes |
| prompt-acknowledgement | before handoff 1; same latency values as sustained gate | before handoff 1; same latency values as sustained gate |
| bounded-handoff-saturation | 3601 attempts, 1 accepted, 3600 dropped, 60.013945058 s | 3571 attempts, 1 accepted, 3570 dropped, 60.019783292 s |
| bounded-memory-proxy | RSS growth 0, Theil-Sen 0 bytes/min, first/last median 12,976,128/12,976,128 | RSS growth 0, Theil-Sen 0 bytes/min, first/last median 10,780,672/10,780,672 |
| disconnect-cleanup | pending/subscription elapsed 0.009479799/0.009483069 s; all observed outcomes 1 | pending/subscription elapsed 0.0211495/0.021154 s; all observed outcomes 1 |
| explicit-reconnect-rebuild | 2 connections, 2 sessions, 0.22490539 s | 2 connections, 2 sessions, 2.97012425 s |

All 13 gates are present exactly once, status `pass`, with `failure: null`. Canonical RSS fields are `rss_samples=51`, `rss_sampling_interval_seconds=1`, and `rss_warmup_seconds=10` on both RSS gates; no `rss_sample_count` alias exists. Disconnect uses observed elapsed fields and no nominal `deadline_seconds`.

## Commands and verification

The local strict run used this immutable revision binding and unchanged configuration:

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

Hosted macOS run `29202919716` used the equivalent command from `.github/workflows/cdp-transport-gate.yml`, checked out and verified the same full SHA, compared the generated schema byte-for-byte, ran the candidate/transport tests, emitted raw and sanitized artifacts, and ran the strict schema-v2 assertions before upload. The committed reports were independently re-normalized to temporary outputs and `cmp`-matched byte-for-byte.

Focused quality gates passed without production/core changes:

```text
cargo fmt --all --check
cargo test --locked -p krometrail-cdp --features cdp-spike-cdpkit --test transport_contract --test cdpkit_transport_contract
cargo clippy --locked -p krometrail-cdp --features cdp-spike-cdpkit --tests -- -D warnings
```

The focused candidate suites passed (14 tests across 2 suites; 25.23s) and clippy reported no issues. Independent validation also confirmed schema v2, exact source and implementation revision, candidate Cargo version/checksum, configuration/fixture equality, all 13 gates, unchanged thresholds, canonical RSS/deadline fields, trace hash/results, redaction, and exact full report digests.

## Limitations

The reports retain these limitations verbatim:

- cdpkit exposes named event params through an unbounded subscriber; wildcard/full-envelope receive and queue-depth introspection are unavailable.
- Ack latency is a receive-to-ack-completion proxy, not a wire-enqueue timestamp.
- RSS is a process-level proxy from a continuously drained reader; it does not prove that the hidden cdpkit subscriber queue is bounded.
- The candidate-contract trace is scripted evidence, not a real-Chrome protocol-drift measurement; its exact digest and derived results are attached to each report.

## Acceptance criteria

- [x] Linux and macOS reports name one exact committed gate revision and unchanged candidate/configuration/fixture digest.
- [x] Every required candidate-contract and real-Chrome gate is observed, schema-valid, redacted, and passes unchanged thresholds.
- [x] Runner-emitted provenance is preserved; raw run references and sanitized report digests are documented.
- [x] No production adapter or core-port change lands; no fallback protocol was triggered because cdpkit passed every unchanged gate.

## Implementation notes

- Added only the two runner-emitted sanitized v2 reports and their evidence/documentation updates; no report JSON was hand-edited.
- Restored `.work/bin/work-view` to the committed binary and left `.pi/` ignored.
- Supplied the accepted runner-emitted reports to the follow-up decision/cleanup story; the generated v2 decision and documentation/bootstrap updates are recorded there.

## Review (2026-07-12)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane evidence review independently reproduced strict decision validation, exact report digests, platform/source/implementation identity, all 13 passing observed gates, canonical RSS/deadline fields, and identical trace-bound candidate results. The decision function reaches `adopt_cdpkit`; final decision emission and bootstrap cleanup are owned by the follow-up story. Verdict: Approve - story verified by implement; fast-lane advance.
