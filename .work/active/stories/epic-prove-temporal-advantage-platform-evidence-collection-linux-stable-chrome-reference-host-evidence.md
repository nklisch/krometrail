---
id: epic-prove-temporal-advantage-platform-evidence-collection-linux-stable-chrome-reference-host-evidence
kind: story
stage: implementing
tags: [testing, browser, infra, visual]
parent: epic-prove-temporal-advantage-platform-evidence-collection
depends_on: [epic-prove-temporal-advantage-platform-evidence-collection-lane-contract-and-shared-runner]
release_binding: null
gate_origin: null
created: 2026-07-15
updated: 2026-07-15
---

# Collect the Linux stable-Chrome reference-host evidence

## Checkpoint

Produce one declared, operator-authorized Linux stable-Chrome live qualification manifest. This is
the exact evidence checkpoint required before manual multimodal interpretation can begin; it is
not a cross-platform aggregate and does not wait for macOS.

## Exact implementation

Add the Linux lane orchestration under `src/app/platform_evidence.rs` using the shared lane
registry and `run_live_qualification`. Select `BrowserProduct::Chrome` explicitly, require Linux,
canonical 800x450/default-DPI capture, stable observed browser/protocol identity, complete source
intervals and cleanup, and the existing live qualification gates. Validate the resulting
`RunManifest` against `PlatformLaneId::LinuxStableChromeReferenceHost` before exposing its digest
to the later matrix or manual lane.

A missing installation or authorization is `Blocked`; an incomplete, gapped, evicted, corrupt, or
unretained run is `Inconclusive`; complete below-threshold evidence is `Fail`; only complete
passing evidence satisfies this checkpoint. Keep all frames, artifacts, transcripts, and logs in
ignored `target/temporal-evaluation/live/` output. Do not launch Chrome during ordinary tests.

## Acceptance evidence

- [ ] A real operator-authorized Linux stable-Chrome run validates as `Pass` with exact browser,
      protocol, platform, architecture, revision, viewport/scale, fixture, capture, seed,
      threshold, timing, gap, retention, cleanup, and non-claim identity.
- [ ] The checkpoint exposes the manifest and input digests needed by manual interpretation and
      rejects a Chromium, non-Linux, wrong-scale, incomplete, or fabricated substitute.
- [ ] Browser absence, authorization failure, capture gap, retention loss, unsupported protocol,
      and cleanup failure retain explicit recovery records and cannot become a pass.
- [ ] No macOS/default-DPI/high-DPI result is required for this checkpoint or its downstream
      manual interpretation dependency.

## Ordering and blocker

Depends only on the shared lane contract. The story cannot be closed as usable evidence until the
operator supplies the Linux Chrome run; design and ordinary verification remain browser-free.
