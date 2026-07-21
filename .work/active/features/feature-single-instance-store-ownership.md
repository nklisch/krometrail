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

## Architectural choice

**Per-instance isolation, not a shared-directory guard.** Each process owns
`<data_dir>/instances/<uuid>/` and holds an `flock` advisory lock on it for its
whole lifetime. The proven interleaving is not defended against — it is made
unrepresentable, because no process can name another's storage.

The originally proposed single-directory lock was rejected: it makes a second
process *fail*, which is a worse agent experience than letting it run in its own
root, and it leaves every cross-instance mutation path intact for any future
caller that bypasses the lock.

## Design decisions

- **`flock`, not a pid/lockfile.** Released automatically on process exit
  including a crash, so a stale lock can never permanently brick startup. A pid
  file would need liveness probing and staleness heuristics, both of which can be
  wrong. Non-Unix falls back to layout-only isolation (`instance.rs`), since the
  standard library cannot express a deny-share open; Linux and macOS are the
  supported production hosts.
- **Acquiring the lock *is* the liveness test.** `acquire_existing` returns
  `Ok(None)` when a root is live and `Ok(Some(ownership))` when it is not. There
  is no window where a caller has decided a root is abandoned but does not yet
  own it, so a reclaimer always acts as that root's legitimate owner. This is why
  reclamation does not violate the "only mutate your own root" invariant: the
  invariant applies to *live* roots, and acquisition transfers ownership.
- **Instance-scoped reads; no federation.** *Accepted regression:* after an MCP
  process restart, evidence recorded by a previous process is no longer
  queryable. Rationale: all eleven read ports are served by one `SqliteIndex`, so
  cross-instance reads would mean an N-way union across ~69 query methods with
  merge-ordering semantics and a concurrent-reclaim-mid-read hazard. Against that
  cost, the practical loss is small — an agent almost never holds the
  session/target/artifact IDs needed to address a dead process's evidence, since
  those only ever arrive in that process's own responses. **This makes age-out
  correctness higher-stakes: reclaim is now the only thing that reaches old
  data.** Because nothing reads a root it does not own, reclaiming a dead root
  cannot race a reader, which removes the hazard entirely.
- **Legacy flat store is cleared, not migrated.** Per Current Contract
  Discipline. Clearing is allowlist-scoped
  (`RECORDING_CACHE_FILES` / `RECORDING_CACHE_DIRECTORIES`): an unexpected member
  is left in place rather than swept away. `clear_legacy_flat_store` and
  `reclaim_instance_root` share `remove_recording_cache`, since both are "remove
  a recording cache that nothing live owns".
- **Direction 3 taken in full: `relative_path` removed from the source of
  truth.** Filenames derive from `segment_id` + `state` via `segment_file_name`.
  The fallback (`AND state='sealed'` only) was not needed. `session_segments`
  *also* gained the state filter, so the guard and the derivation reinforce each
  other: even if a future query forgets the filter, `segment_object` derives the
  sealed name and cannot emit a `.open` path.
- **Seal ENOENT is `WriterUsable`, never `WriterTerminal`.** Both callers already
  `remove` the entry from `open_segments` before calling `seal_segment`
  (`writer.rs:318`, `:367`), so per-segment scoping needed no bookkeeping — only
  the correct recoverability classification. A sealed file whose header carries a
  *different* segment id is explicitly not a reconciliation.
- **Recovery ordering enforced, not documented.** `recover()` now runs before
  `SegmentWriter::open`, and `RecordingStore` fails construction if any `open`
  segment row remains — recovery seals every one it finds, so a surviving `open`
  row proves recovery was skipped.

## Implementation Units

1. `crates/krometrail-store/src/instance.rs` (new) — ownership, allowlisted
   recording-cache removal, legacy detection/clear, sibling scan.
2. `crates/krometrail-store/src/segments/writer.rs` — idempotent seal,
   `segment_file_name`, `relative_path` removed from `SegmentRegistration`.
3. Schema v8 — `segments.relative_path` dropped; derivation sites updated in
   `index/{schema,segments,frames,reconcile,retention}.rs`, `recovery.rs`.
4. `src/app.rs` — instance acquisition, legacy clear, dead-root reclaim,
   recovery-before-writer ordering.
5. `crates/krometrail-store/src/budget_registry.rs` — shared total-budget ledger
   built on the same ownership primitive (see
   `feature-retention-lifecycle-and-trimming` for the allocation policy).

## Testing

- `tests/instance_ownership.rs` — legacy clear preserves browser profiles,
  diagnostics, downloads, plugin state, and config byte-for-byte; live roots are
  unclaimable; abandoned roots are reclaimable; unrecognised members survive.
- `segments::writer::tests` — the proven interleaving (already-published segment
  reconciles and the writer survives), vanished segment fails per-segment, and a
  foreign sealed publication is rejected.
- `tests/rust-runtime-smoke.rs` — end-to-end through the real binary: flat store
  cleared, one instance root at schema 8, browser profile intact.

## Risks

- **Accepted data loss on first run after upgrade.** The 9.6 GB flat store is
  cleared. User-confirmed.
- **Instance roots accumulate if reclamation fails.** Reclaim is best effort and
  never blocks startup, so repeated failures leak roots. Bounded by the shared
  budget once the registry lands.
- **`flock` semantics on network filesystems** are unreliable. A data directory
  on NFS/SMB would weaken isolation to layout-only. Not currently detected.

## Acceptance

- A second process against a live data directory fails fast with a clear error
  instead of mutating the running store.
- Recovery cannot rename or delete a live writer's open segment.
- ENOENT at seal is per-segment and recoverable; the writer survives.
- No deletion path can name a `.open` file.
- Regression coverage drives the proven two-process interleaving.
