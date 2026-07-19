---
id: idea-temporal-artifacts-busy-range-limits
created: 2026-07-19
updated: 2026-07-19
tags: [bug, temporal, agent-ux]
---

Found in the 2026-07-19 motion workload (dev build of main at v1.2.3-19): on a
continuously animating page captured at ~51 fps (19.6 ms median cadence, zero
gaps), the temporal artifact surface breaks down exactly where it matters most.

1. **Default bundle artifacts refuse busy ranges.** A 7.6 s interaction-anchored
   `temporal_debug_bundle` (367 frames) returned no storyboard and no primary
   image: `resource_limit_exceeded: "resolved range exceeds the source-frame
   limit"`. After narrowing to 0.9 s (44 frames), both default generators still
   failed with `"no exact integer analysis scale fits configured limits"` — while
   a 0.7 s / 34-frame range succeeded with identical settings, and an explicit
   `generate_artifacts` with `scale: {down, factor: 7}` and `tile_limit: 8`
   succeeded **on the same 44-frame handle** (viewport 1673×1288; 1673 = 7×239).
   The `fit_limits` resolution appears to size against the full in-range frame
   count rather than the tile-limit selection. The recovery text ("shorten the
   requested interval") does not converge — 7.6 s → 0.9 s still failed.
2. **Source-frame listing hard-fails instead of paginating.** `list_source_frames`
   on the 367-frame handle failed first with "source read limits exceed runtime
   ceilings" (ceiling values not stated) and then, with small limits, with
   "selected source frame count exceeds the request limit" — there is no truncated
   page, so frame ids in a busy range cannot be discovered at all. Since
   `generate_region_filmstrip` region variants all require a `source_frame_id`,
   this is a catch-22: the only escape is re-resolving narrower ranges by guessed
   session times.
3. **Region filmstrip normalizes before cropping.** An 87-frame range for a
   293×70 viewport region failed with "normalization result exceeds configured
   processing limits"; the same request on a 44-frame handle succeeded. The
   processing budget appears to be charged for full 1673×1288 frames rather than
   the requested region.

Fix directions: make `fit_limits` account for tile selection (or fall back to the
best exact-divisor scale), paginate/truncate source-frame listing with explicit
omission counts instead of erroring, state the actual ceilings in limit errors,
and crop before normalization (or budget on the cropped region) for region
filmstrips. Context note for sizing: this workload also showed motion capture
costs ~3 MB/s of retention (1.55 GB in ~8 minutes against the 10 GB budget).
