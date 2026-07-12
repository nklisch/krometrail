---
id: epic-rust-cdp-capture-foundation
kind: epic
stage: implementing
tags: [browser, infra]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-11
updated: 2026-07-12
---

# Rust CDP Capture Foundation

## Brief

This epic delivers a trustworthy Rust foundation that launches or attaches to Chrome and continuously receives timestamped visual frames through CDP. It establishes the workspace, domain contracts, Chrome lifecycle, flat target sessions, screencast acknowledgement, normalized session timing, bounded ingestion, and explicit capture-gap reporting that every browser capability relies on.

The work proves the riskiest technical assumption before broader investment: the selected Rust CDP path can sustain real browser capture with sufficient fidelity and expose the raw commands and events Krometrail needs. Compatibility and capture behavior are measured against real Chrome rather than inferred from library APIs.

This epic does not deliver durable history, complete browser automation, temporal artifacts, or agent-facing debugging bundles. It supplies the validated live frame stream and contracts those capabilities consume.

## Foundation references

- `docs/VISION.md` — Local-First Operation and Success
- `docs/SPEC.md` — Browser Lifecycle, Sessions and Targets, Continuous Visual Capture, and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Rust Workspace, Browser Connection, Target Lifecycle, Frame Ingestion, and Capture Tasks
- `docs/EVALUATION.md` — Capture-Fidelity Evaluation and Timing Integrity

## Design decisions

- **Rust CDP client selection:** Final5 schema-v2 evidence selects exact `cdpkit` 0.4.0. Current Linux and macOS reports use gate revision `a0e98ad6bd9c53d10385020bc43629f7ac246173`, the same canonical configuration/fixture/source attestation, canonical RSS fields, observed lifecycle measurements, and reconstructable candidate-contract trace/results. The reports are `docs/evidence/cdp-transport/v2/cdpkit-linux.json` (`c5ed8bfab9cb829f0d1e1622755667084abc09129ed1f2928cdc5f577d3761f8`) and `cdpkit-macos.json` (`7b2d7c61d61400f47281423d35ea57d51b1292cc78a95c4d7cef3118476c2264`, hosted run `29212145045`); generated decision `decision.json` is `dfbd51c9e7a1f8e051c173df35962bc6f443d2b5c28037e406c3a72beda6472a`. The selected client remains behind a replaceable `krometrail-cdp` adapter boundary; Krometrail owns reconnect, bounded handoff, backpressure, and capture gaps. `chromey` and an owned transport remain late-bound fallbacks only after demonstrated failure.
- **Final transport evidence identity:** The reports bind source-attestation `sha256:96acbed658fb89a71a90107ac0bfec0ab78860e57f95a374cc9e183d672a4c5a`, configuration `sha256:06388b5f8ad042093d22408dedb8d02d5a04a9e59d485158edc533334bab956e`, browser fixture `sha256sum-of-ordered-fixture-files:9b42ae730d12a95772a946bf55e4838a5443b6cb4c536424570219041b6e2a68:84ba666539a996012a781637c1a894d8c7a4789cfca84661bd7cf8b79efa2e13`, candidate-contract fixture `sha256:622fb296e0b50bf0dc81123c5f54a797040cdc48bd6b5f9ca96167bbe87fce76`, and candidate trace `sha256:33ccc161726cc35f68e6a260c129a06f9050af4a616a76c8b957525f557a6e00` with 942 observations. Ack is measured after frame return through ack completion, before bounded handoff; p99/max are Linux `0.214389/0.889178 ms` and macOS `0.582458/12.67025 ms`.
- **Legacy runtime removal:** Remove the TypeScript/DAP implementation while establishing the Rust workspace rather than keeping two buildable runtimes. Git tag `v0.2.20` remains the implementation reference if the spike requires recovering prior browser lifecycle or framework-state behavior.

## Other agent review

- Phase 1 advisory: GLM 5.2 reviewed the architectural seams before decomposition; local reads of the foundation docs, sibling epics, and the v0.2.20 browser recorder verified the recommendations. No further exploratory dispatch was needed.
- Accepted: five capability arcs with a transport gate before production lifecycle and ingestion; a disposable spike isolated from production; an explicit evidence-based transport fallback; revisable core ports with enforced inward dependency direction; remote legacy-tag verification; and a deliberately narrow cross-platform smoke.
- Rejected: keeping launch/profile lifecycle and target/reconnect supervision as separate features, because together they form one browser-session continuity capability and would share the selected transport boundary too tightly. Also rejected expanding the final smoke into the full defect-duration or agent-evaluation corpus, which belongs to `epic-prove-temporal-advantage`.

## Decomposition

The epic is split into five end-to-end capabilities along the evidence path: establish one Rust runtime and its inward-facing contracts, qualify the transport against real Chrome, supervise browser and target sessions through that transport, ingest screencast frames with bounded and explicit loss semantics, then prove the complete path on supported platforms. The dependency chain is intentionally linear because each stage supplies evidence or contracts required to commit safely to the next; artificial parallelism would let production code outrun the transport and lifecycle gates.

### Child features

- `epic-rust-cdp-capture-foundation-rust-runtime-contracts` — establish the Rust workspace, core capture contracts and ports, and the immediate TypeScript/DAP single-runtime cutover — depends on: `[]`
- `epic-rust-cdp-capture-foundation-cdp-transport-gate` — qualify `cdpkit` against required domains, flat sessions, raw protocol access, and sustained acknowledgement, with an explicit fallback decision — depends on: `[epic-rust-cdp-capture-foundation-rust-runtime-contracts]`
- `epic-rust-cdp-capture-foundation-chrome-target-supervision` — deliver production Chrome lifecycle, compatibility probing, flat target supervision, and reconnect behavior — depends on: `[epic-rust-cdp-capture-foundation-cdp-transport-gate]`
- `epic-rust-cdp-capture-foundation-bounded-screencast-ingestion` — deliver prompt acknowledgement, bounded ingestion, distinct clocks, explicit capture and visibility gaps, statistics, cancellation, and flush — depends on: `[epic-rust-cdp-capture-foundation-chrome-target-supervision]`
- `epic-rust-cdp-capture-foundation-cross-platform-capture-smoke` — prove the live stream with minimal real-browser fidelity smoke on Linux and macOS high-DPI — depends on: `[epic-rust-cdp-capture-foundation-bounded-screencast-ingestion]`

### Decomposition risks

- The transport gate selected cdpkit only after valid schema-v2 evidence; the retained v1 selection remains historical. Keep the production adapter boundary minimal and replaceable while enforcing that `krometrail-core` never imports infrastructure.
- Spike scaffolding could leak into production and hide unsupported behavior. Keep it disposable, require a recorded pass/fail decision for every transport gate, and make fallback selection explicit rather than silently weakening requirements.
- The runtime cutover removes the convenient local legacy reference. The remote `v0.2.20` tag was verified at commit `3fa4ffa16659648c6f4e229c2f7ae14d2fbc6558`; the cutover must preserve that reference and avoid compatibility shims or dual runtimes.
- Screencast acknowledgement can appear healthy while clocks, queue loss, or visibility pauses misrepresent continuity. The ingestion capability must preserve source, observed, and normalized session times separately and classify every known gap.
- The final smoke could grow into the evaluation epic and lengthen this foundation's critical path. Limit it to transport, scaling, timing, loss-reporting, and shutdown confidence on the two supported platforms.
