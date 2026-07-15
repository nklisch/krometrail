---
id: epic-prove-temporal-advantage-platform-evidence-collection-matrix-aggregation-and-claim-boundary
kind: story
stage: implementing
tags: [testing, browser, infra, visual]
parent: epic-prove-temporal-advantage-platform-evidence-collection
depends_on:
  - epic-prove-temporal-advantage-platform-evidence-collection-linux-stable-chrome-reference-host-evidence
  - epic-prove-temporal-advantage-platform-evidence-collection-macos-chrome-default-dpi-evidence
  - epic-prove-temporal-advantage-platform-evidence-collection-macos-chrome-high-dpi-evidence
  - epic-prove-temporal-advantage-platform-evidence-collection-linux-chromium-optional-evidence
release_binding: null
gate_origin: null
created: 2026-07-15
updated: 2026-07-15
---

# Aggregate platform evidence and enforce claim boundaries

## Checkpoint

Build one deterministic platform matrix assessment from the four lane manifests. Keep the Linux
reference-host assessment independent from the cross-platform assessment, and make macOS absence
honestly inconclusive rather than a blocker or silent claim.

## Exact implementation

Add the matrix builder and validation tests under `crates/temporal-evaluation/src/platform.rs`,
`src/app/platform_evidence.rs`, and `crates/temporal-evaluation/tests/platform.rs`. Consume exact
lane IDs and loaded `RunManifest` values, validate each against the registry, and emit ordered
manifest/input digests, statuses, failure/recovery records, fixed non-claims, and three assessment
statuses:

- Linux stable Chrome `reference_host_status`;
- required Linux + macOS default-DPI + macOS high-DPI `cross_platform_status`; and
- optional Linux Chromium status.

A valid Linux reference pass with missing or non-decisive macOS rows leaves the reference-host
status usable while setting cross-platform and overall matrix status to `Inconclusive`. All three
required rows must pass before cross-platform status can pass. Optional Chromium absence is
`Skipped` only with explicit row-level optional-unavailability evidence. Complete measured
threshold failures are `Fail`; missing/gapped/evicted/corrupt/unauthorized evidence is
`Blocked`/`Inconclusive`, never a pass.

Write only ignored aggregate/run output and a generated schema/sample if the existing contract
artifact pattern requires one. Do not copy frames, artifacts, transcripts, page text, or private
paths into the matrix.

## Acceptance evidence

- [ ] Duplicate/missing lane IDs, wrong manifest digest, wrong environment/product/scale,
      inconsistent benchmark/input identity, and forged status are rejected.
- [ ] Linux reference pass plus unavailable macOS is `reference_host=Pass` and
      `cross_platform=Inconclusive`; no downstream manual or agent work waits on the matrix.
- [ ] Cross-platform `Pass` requires all three required rows; optional Chromium cannot satisfy it.
- [ ] Complete below-threshold required data is `Fail`; incomplete, unavailable, gapped, evicted,
      corrupt, or unauthorized data remains non-decisive with recovery.
- [ ] Canonical output order and bytes are stable, and generation does not touch
      `docs/public/llms-full.txt` or `.work/bin/work-view`.

## Ordering and operator boundary

This is the final platform checkpoint and depends on all lane stories. Aggregation itself is
browser-free; the referenced manifests require operator-authorized collection. MacOS may be
unavailable, in which case the matrix remains inconclusive and the Linux reference-host evidence
still supports the separate manual dependency.
