---
id: feature-single-instance-store-ownership
kind: feature
stage: drafting
tags: [storage, bug, security, infra]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-20
updated: 2026-07-20
---

# Exclusive store ownership and non-destructive recovery

## Brief

**A second `krometrail` process silently destroys a running one's capture, and
in the worst case deletes the entire store.** Nothing guards the data directory.

Reproduced 2026-07-20 during the seventh shakedown. A second `krometrail mcp`
was started against the live data directory purely to probe the MCP surface.
Its startup recovery renamed the running writer's open segment, and the live
capture pipeline died terminally:

```
capture.pipeline.failed  failure_stage=frame_persistence
cause_code=persistence_failed
persistence_operation=sealed_segment_publication
persistence_category=not_found
persistence_recoverability=writer_terminal
```

Log confession from the intruding process, timestamped inside the failure
window (writer died 23:56:58; recovery completed 23:57:05):

```
"recording store recovery complete","open_segments_sealed":1,
"segments_repaired":1,"frames_recovered":0,"frames_removed":213
```

## Mechanism (proven)

1. Process A is capturing; its writer holds open segment S with
   `segments/<S>.open` on disk and index row `state='open'`.
2. Process B starts against the same directory. There is no instance lock:
   `data_directory()` (`src/app.rs:427-451`) is a fixed XDG path,
   `SqliteIndex::open` takes only a WAL connection with a 5s busy timeout
   (`index/mod.rs:145-160`), and `SegmentWriter::open` merely ensures the
   directory exists (`writer.rs:181-188`).
3. B runs `recover()` (`src/app.rs:372`), which enumerates the directory and for
   **every** `*.open` file — including A's live one — footers, truncates, and
   renames it to `.kts`: `recovery.rs:184-229` (discover),
   `recovery.rs:330-345` (footer + `set_len`), `recovery.rs:347-352` (rename),
   `recovery.rs:369` (re-register as `Sealed`).
4. A later seals S via `fs::rename(open_segment_path, sealed_segment_path)`
   (`writer.rs:585-588`). Source is gone -> ENOENT -> `WriterTerminal`.

This also explains the shakedown's `open_segment_count: 0` while a writer held a
segment open: that count is derived purely from the index
(`index/retention.rs:688`, `:708-722`), so a 0 means something outside the
writer rewrote the row to `sealed` — exactly `recovery.rs:369`.

**Worse variant:** if B's schema check trips `Incompatible`,
`clear_recording_cache` runs `remove_dir_all` on the entire live segments
directory (`index/mod.rs:163`, `:180-187`, `:199`) — total data loss, not just a
dead writer.

## Blast radius

- **Writer:** killed permanently and *globally*. `execute` latches
  `terminal_error` (`writer.rs:299-301`) and returns it for every subsequent
  command from every session and target (`writer.rs:281-283`). `SegmentWriter`
  is constructed once (`src/app.rs:365`) with no re-open path — capture is dead
  until process restart. Confirmed empirically: a fresh browser session did not
  recover; only an MCP process restart did.
- **Data:** none lost in the recovery variant (B footered and re-registered the
  segment). Total loss in the `clear_recording_cache` variant.

## Design directions

1. **Exclusive ownership.** Advisory lock (flock/lockfile) on the data directory
   in `open_storage_with_budget` (`src/app.rs:357`); fail startup with a clear,
   actionable error when held. Also reorder — `recover()` currently runs *after*
   `SegmentWriter::open` (`src/app.rs:365` vs `:372`); recovery must complete
   before any writer exists.
2. **Idempotent seal.** In `seal_segment` (`writer.rs:585`), treat `NotFound` as
   a reconciliation point: if the sealed path already exists with a matching
   segment id in its header, the segment is already published — return success.
   Otherwise fail as a **per-segment** error that drops the entry from
   `open_segments` and leaves the writer usable, rather than latching
   `terminal_error`. `WriterTerminal` is the wrong classification for a
   per-segment condition.
3. **Make "open" structurally undeletable.** Add `AND state='sealed'` to
   `session_segments` (`index/retention.rs:335`), or stop storing paths at all —
   store `segment_id` + `state` and derive filenames via
   `open_segment_path`/`sealed_segment_path`, so no deletion object can name a
   `.open` file. (`session_segments` currently has no state filter and feeds
   `delete_session`, `recording.rs:2005`/`:2016`; `flush_session` first plus
   tolerant staging makes it latent rather than active, but it is a live
   footgun and possible file-orphan leak.)

## Simplification opportunity

Direction 3 removes a stored redundancy rather than adding a guard: filenames
are already derivable from `segment_id` + `state`, so storing `relative_path`
duplicates truth and is the thing that makes a wrong deletion expressible.
Prefer deriving over validating.

## Acceptance

- A second process against a live data directory fails fast with a clear error
  instead of mutating the running store.
- Recovery cannot rename or delete a live writer's open segment.
- ENOENT at seal is per-segment and recoverable; the writer survives.
- No deletion path can name a `.open` file.
- Regression coverage drives the proven two-process interleaving.
