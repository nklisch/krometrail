---
id: feature-retention-trim-transparency
kind: feature
stage: drafting
tags: [store]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Retention trim correctness and transparency

## Brief

In-session retention trimming evicted freshly generated artifacts while every
agent-visible signal said retention was healthy. Found during the v1.6.0
shakedown under sustained ~100 fps WebGL ingest (~18 MB/s, 25k frames,
4.16 GB stored of a 10 GB configured budget). Grounding against the code shows
two of the three surprises are defect-shaped:

- **Phantom-instance budget halving.** `effective_budget()` divides the
  configured budget by census `live_instances()`. The session enforced
  85% × (10 GB ÷ 2) = 4.25 GB because the census counted a second live
  instance — almost certainly the pre-restart server whose recording cache the
  startup log had already reclaimed as abandoned
  (`retention.instance_reclaimed`). A lone session should trim at 8.5 GB.
  `browser_status` also reports the configured budget (10 GB) while enforcing
  the effective one (5 GB), which is actively misleading.
- **Hollow artifact grace.** `DEFAULT_ARTIFACT_GRACE` (15 min) exists "so a
  returned resource link is not already dying when the agent receives it", but
  the strictly oldest-first reclaim walk hits the segments backing fresh
  artifacts first (agents naturally derive artifacts from the oldest retained
  window — investigate what just happened, keep recording) and the
  `artifact_grace_overridden` path then evicts them anyway. Observed: artifacts
  1–4 minutes old evicted; `usage.artifact_bytes` ended at 0.
- **No trimming signal.** `RecordingBudgetState` only knows
  Available/PausedBudget; continuous in-session trimming surfaced nowhere —
  `budget_state: "available"`, `eviction_blocked: false` throughout.

Also folded in (same surface, found same pass): `resolve_temporal_range`'s
`capture_quality.capture_status.at_range_start/at_range_end` always echo the
session-initial all-zero status block (session_time ~11 ms) instead of the
capture status at those range bounds.

Eviction throughput itself is excellent (trims interleaved with zero capture
disruption; drops stayed at 0.5% attributed queue blips) — the fixes here are
correctness and transparency, not performance.

## Strategic decisions

- **Grace policy — skip and reclaim newer**: the reclaim walk treats artifact
  grace as an ordering exception: skip graced artifacts and their backing
  segments and reclaim the next-oldest instead. Override grace only when
  nothing else is reclaimable (true emergency), and surface that override as an
  explicit response warning. — Makes the documented grace real in the common
  agent pattern.
- **Budget split — fix staleness, keep equal split**: root-cause and fix the
  stale live-instance census count (an instance whose cache was reclaimed as
  abandoned must not still count as live); keep the deliberate equal-split
  policy; surface `effective_budget` and `live_instances` in `browser_status`
  so the enforced number is visible. — Equal split stays for predictability
  (prior review history), the bug and the opacity go.
- **Signaling — informational, not alarming**: `browser_status` gains a
  trimming/pressure state plus effective budget and instance count; temporal /
  artifact / query responses note active trimming calmly with a concrete
  how-far-back reference (the trimmed-through boundary / oldest-retained
  session time or index) so range work is never surprised. Tone: a factual
  note, not a warning klaxon — most sessions on ordinary pages will never hit
  the limit. Recovery text may name `pin_resolved_range` where relevant.
