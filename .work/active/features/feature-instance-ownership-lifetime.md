---
id: feature-instance-ownership-lifetime
kind: feature
stage: drafting
tags: [storage, security]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Instance ownership lifetime

## Brief

The v1.3.0 instance-isolation guarantee does not hold at runtime. Any second
`krometrail` process started against the same data directory deletes the retained
recording store of every already-running instance, silently. `docs/SPEC.md:387` already
states the correct contract — "holds an advisory lock on it for its lifetime … a second
process cannot disturb a running one's capture" — so this is the code failing its own
specification, not a specification that needs to move. No foundation-doc roll-forward.

This feature restores that contract, removes the directory leak the same subsystem causes,
and corrects one imprecise errno mapping found alongside it. All three live in
`crates/krometrail-store/src/instance.rs` and `src/app.rs`.

## Strategic decisions

- **Release shape**: ship as hotfix **1.3.1** carrying all three storage fixes (guard
  lifetime, instance-directory leak, `EINTR` mapping) — they share one file and one mental
  model, and v1.3.0 is losing user evidence in the field today. The agent-surface
  frictions found in the same shakedown are deliberately *not* in this release; they go to
  1.4.0 as `feature-agent-surface-diagnosability`.

## Simplification opportunity

The fix should make the misuse unrepresentable rather than merely corrected. Options for
design to weigh: `#[must_use]` on the acquire functions; or having `InstanceCensus` /
`RecordingStore` take the `InstanceOwnership` by value instead of a borrowed `&Path`, so
the type system ties the lock to the store that depends on it and a future refactor cannot
silently drop it again. Prefer the structural option if it does not distort the ports.

No code is expected to be deleted. The added bootstrap-level test is net-new surface that
existing store-level tests do not cover.

---

## Findings (from the eighth shakedown, v1.3.0)

Critical: the v1.3.0 instance-isolation guarantee does not hold at runtime. Any second
`krometrail` process started against the same data directory **deletes the retained
recording store of every already-running instance**. Found during the eighth shakedown
against released v1.3.0; reproduced deterministically.

## Mechanism

`InstanceOwnership` is a stack local in `open_storage_with_budget` (`src/app.rs:365-366`):

```rust
let ownership = InstanceOwnership::acquire_new(data_directory)?;
let instance_root = ownership.root().to_path_buf();
```

`ownership` is never used again. Everything downstream takes the *path*
(`InstanceCensus::new(data_directory, &instance_root)` at `src/app.rs:423`), and
`StorageDependencies` (`src/app.rs:83-99`) has no ownership field. The guard therefore
drops when the function returns at `src/app.rs:444`, closing the `File` and releasing the
`flock` — while the process runs on for hours.

The type documents exactly the invariant being broken
(`crates/krometrail-store/src/instance.rs:138-151`): "An owned instance root, held for the
lifetime of the process. Dropping this releases the advisory lock."

Consequence chain — every layer below the drop behaves exactly as designed:
guard drops -> flock released -> a later process enumerates siblings ->
`acquire_existing` succeeds on the live root -> `claim()` returns `Ok(Some(_))` ->
`reclaim_instance_root` deletes what it correctly believes is an abandoned cache.

There is no TOCTOU and no bug in the reclaimer. The liveness evidence is simply discarded.

## Runtime proof

`/proc/<live-pid>/fd` has **no descriptor for any instance `.owner.lock`**, while the
index/WAL/SHM/open-segment descriptors are all present and marked `(deleted)`:

```
14 -> .../instances                                   (census dir handle, still open)
11 -> .../0b79781b-.../index.sqlite3      (deleted)
12 -> .../0b79781b-.../index.sqlite3-wal  (deleted)
13 -> .../0b79781b-.../index.sqlite3-shm  (deleted)
16 -> .../0b79781b-.../segments/<uuid>.open (deleted)
10 -> .../browser-profiles/profiles/default/.krometrail.lock   (a lock correctly held)
```

fd 10 is the useful contrast: the CDP profile subsystem keeps its guard alive; the storage
bootstrap does not.

Repro: start `krometrail mcp`, drive a browser session, then run
`krometrail mcp < /dev/null` once. The first process's `segments/`, `artifacts/`,
`index.sqlite3*` are gone. The victim keeps writing to deleted inodes and **continues to
report healthy status** — no error, no log line, no degraded state on the victim side. The
only signal is a `retention.instance_reclaimed` info log in the killer.

Downstream symptom actually observed: subsequent capture fails with
`sealed_segment_publication` / `not_found` — "open segment disappeared before publication
and no complete sealed segment stands in its place". This is the same error signature
mis-attributed to budget pressure in a previous cycle.

## Blast radius

Not an edge case. Any of: a second editor with an MCP server attached, an MCP client
restart, a harness-spawned probe, or a manual `krometrail mcp` while an agent session is
live. Unix only in effect (`OWNERSHIP_IS_ENFORCED`); on Windows nothing is reclaimed, so
data survives there.

Secondary effect, same root cause: a live root reads as unlocked, so
`InstanceCensus::count_live` (`instance.rs:560-570`) counts live instances as dead and the
shared budget over-grants — each instance believes it is more alone than it is.

## Why the tests missed it

`crates/krometrail-store/tests/instance_ownership.rs` tests the store primitives correctly
and thoroughly, including `a_second_instance_gets_its_own_root_and_cannot_take_the_first`.
But every test binds the guard to a live local and releases it only with an explicit
`drop`. `tests/shared_budget.rs` keeps an `_ownership` field alive. The primitive was never
wrong; **nothing tested that the application keeps the guard**.

## Fix direction (not yet implemented)

1. Move `InstanceOwnership` into `StorageDependencies` (and onward into
   `RuntimeDependencies`) so its lifetime is the process's — it is inert once held and just
   needs a home that outlives bootstrap, the way `recovery` is handled at
   `src/app.rs:97-98`.
2. Make the misuse hard to repeat: `#[must_use]`, or have `InstanceCensus`/`RecordingStore`
   take the `InstanceOwnership` itself rather than a borrowed path, so the type system ties
   the lock to the store that depends on it.
3. Add the missing bootstrap-level test: open storage via `open_storage_with_budget`, keep
   the returned dependencies alive, and assert `InstanceOwnership::acquire_existing` on the
   returned root yields `Ok(None)`. That assertion fails today.
4. Consider having a live process's periodic census notice that its own root's cache
   vanished, rather than reporting healthy from deleted inodes.

## Related smaller findings from the same shakedown

- Reclaiming an abandoned root removes only allowlisted cache members, never the root
  directory or `.owner.lock`. Correct-by-design for safety, but every process start leaks a
  permanent empty instance directory. Verified: 3 startups -> 3 new roots, none collected.
- `EINTR` is mapped to `Ok(false)` alongside `EWOULDBLOCK` in `try_lock_exclusive`
  (`instance.rs:703`). `EINTR` means "retry", not "held". Fails safe (treats as live), but
  imprecise.
