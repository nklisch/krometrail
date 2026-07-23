---
id: idea-silent-trim-evicts-fresh-artifacts
created: 2026-07-23
updated: 2026-07-23
tags: [store, retention]
---

In-session retention trimming runs at a high-water mark well below the
configured budget and silently evicts freshly generated artifacts, while every
agent-visible signal says retention is healthy. Found during the v1.6.0
shakedown under sustained ~100 fps WebGL ingest (~18 MB/s, 25k frames,
4.16 GB stored of a 10 GB budget).

Observed on 1.6.0:

- `retention.trimmed` events fired continuously with
  `high_water_bytes: 4250000000` (42.5% of the 10 GB budget); oldest_retained
  advanced from 0.4 s to 252 s while `used_bytes` stayed ~4.16 GB.
- The trim evicted artifacts generated 3-5 minutes earlier in the same
  investigation (trim log byte sizes match the exact storyboard/difference-map/
  filmstrip artifacts returned to the agent; `usage.artifact_bytes` ended at 0).
  Their returned `krometrail://` URIs invalidate with no warning at generation
  time and no pressure signal afterward.
- Throughout, `browser_status` reported `budget_state: "available"`,
  `eviction_blocked: false`, `recording_blocked: false` — nothing tells an
  agent that active trimming is consuming the evidence it just produced.

Eviction throughput itself was excellent (trims interleaved with zero capture
disruption; drops stayed at 0.5% attributed queue blips) — this is a
transparency/policy question, not a performance one.

Questions/fix direction: (a) surface an explicit "in-session trimming active"
retention state (and/or the high-water threshold) in `browser_status` and as a
response warning when a returned resource's segment/artifact is at risk or
already reclaimed; (b) reconsider whether derived artifacts should be
first-evicted ahead of source segments, or whether recent artifacts deserve a
short protection window / pin guidance in tool responses; (c) document the
high-water policy in the retention contract so the 10 GB budget is not read as
the live-session bound.

Related smaller staleness found in the same pass: `resolve_temporal_range`'s
`capture_quality.capture_status.at_range_start/at_range_end` always echo the
session-initial all-zero status block (session_time ~11 ms) instead of status
at those times.
