---
id: feature-instance-ownership-lifetime
kind: feature
stage: done
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

## Architectural choice

Three approaches were weighed for keeping the lock alive:

1. **Add an `ownership` field to `StorageDependencies`** with `#[allow(dead_code)]`, the way
   `recovery` is handled at `src/app.rs:97-98`. Smallest diff, but it fixes *this*
   occurrence and leaves the same trap set for the next refactor: the field is inert, so
   nothing complains when a later change drops it.
2. **`#[must_use]` on the acquire functions.** Necessary but not sufficient — the current
   code *does* use the value (it calls `.root()` on it), so `must_use` would not have
   fired here.
3. **Give the lock to `InstanceCensus`** — chosen. `InstanceCensus` exists to answer "how
   many instances are live", and this process's own liveness is the one input it already
   hardcodes (`proved_live` starts at 1, commented "the one peer no census can miss"). Its
   claim to be live *is* the lock. Making the census own the `InstanceOwnership` puts the
   proof and the claim in the same place, and the census is stored in `RecordingStore`,
   which lives for the process. The lock can then only be dropped by dropping the store
   that depends on it.

Chosen 3, with 2 added as a cheap backstop.

## Implementation Units

### Unit 1: `InstanceCensus` owns the instance lock

**Files**: `crates/krometrail-store/src/instance.rs`, `src/app.rs`

```rust
impl InstanceCensus {
    // was: pub fn new(data_directory: &Path, owned_root: &Path) -> Self
    pub fn new(data_directory: &Path, ownership: InstanceOwnership) -> Self
}
```

`owned_root` becomes `ownership.root().to_path_buf()`, read before the move. The census
gains an `ownership: InstanceOwnership` field. `src/app.rs` copies `instance_root` from
`ownership.root()` first (it is needed for the index and segments paths), then moves
`ownership` into the census at `src/app.rs:423`.

Add `#[must_use]` to `acquire_new` and `acquire_existing`.

**Acceptance criteria**:
- [ ] `InstanceOwnership` cannot be dropped without dropping the `RecordingStore`.
- [ ] A running process's instance root is not claimable by another process.

### Unit 2: Reclaim removes the emptied root

**File**: `crates/krometrail-store/src/instance.rs` (`reclaim_instance_root`)

After the allowlisted members are removed, remove `.owner.lock` and then `rmdir` the root —
but **only** if nothing else remains in it. An unexpected member still means "leave the
whole root alone", preserving the existing safety philosophy; the root simply survives
along with the member that blocked it.

The concurrency window is already covered and must stay that way: a claimant that opened
the lock file before the unlink can flock the orphaned inode, but `acquire_existing`
records `directory_identity` *after* locking, and `still_owns_its_root()` requires
`identity.is_some()`. Once the root is gone, identity is `None`, so that claimant fails
closed instead of writing into a deleted directory. Do not weaken either check.

Between unlink and `rmdir` a claimant may create a fresh lock file, making `rmdir` fail.
That is harmless — step over it and leave the root.

**Acceptance criteria**:
- [ ] N startups against one data directory leave no accumulating empty instance roots.
- [ ] A root containing an unexpected member is left intact, lock included.

### Unit 3: `EINTR` retries rather than reporting "held"

**File**: `crates/krometrail-store/src/instance.rs` (`try_lock_exclusive`)

`EINTR` means the call was interrupted and should be reissued; it says nothing about
whether the lock is held. Today it is folded in with `EWOULDBLOCK` and reported as
"held" — which fails safe but can make a genuinely free root look live forever. Retry a
bounded number of times, and map only `EWOULDBLOCK`/`EAGAIN` to `Ok(false)`.

**Acceptance criteria**:
- [ ] Only would-block maps to `Ok(false)`; `EINTR` retries; other errno still errors.

### Unit 4: The end-to-end test that was missing

**File**: `tests/rust-runtime-smoke.rs`

Store-level tests all bind the guard to a live local, so they prove the primitive and not
the application. The honest test is end-to-end against the real binary, which
`rust-runtime-smoke.rs` is already set up for (`CARGO_BIN_EXE_krometrail`,
`KROMETRAIL_DATA_DIR`, and a `krometrail_store` dependency):

1. Spawn `krometrail mcp` with stdin held **open** and a temp data directory.
2. Wait for its instance root to appear.
3. From the test process, assert the root's lock **cannot** be taken while the child runs.
4. Close stdin, wait for exit, assert the lock **can** now be taken.

Step 3 fails against today's code. Steps 3 and 4 together are the whole guarantee: held
while alive, released on exit.

**Acceptance criteria**:
- [ ] Test fails before Unit 1 and passes after.

## Implementation Order

1. Unit 4 (write the failing test first — it is the regression proof)
2. Unit 1 (the fix)
3. Unit 2, Unit 3 (independent, either order)

## Testing

- **Regression (Unit 4)**: the end-to-end lock-held-while-running test. This is the test
  whose absence allowed the bug, so it is the one that matters most.
- **Interface**: extend `crates/krometrail-store/tests/instance_ownership.rs` for Unit 2 —
  an emptied root is removed, and a root with an unexpected member is not.
- **Unit**: `EINTR` handling in `try_lock_exclusive` if it can be driven without a real
  signal; skip rather than build a signal harness for it.
- **No test removal expected.** The existing store-level tests are correct and stay; they
  simply were never the tests that could catch this.

## Implementation notes

- **Unit 4 first, and it failed as predicted** against unmodified v1.3.0 code:
  "a running process's instance root was claimable". That is the bug reproduced at the
  application level, which no store-level test could do.
- **Unit 1** landed as designed. The lock now lives in `InstanceCensus._ownership`, the
  census lives in `RecordingStore`, and the store reaches the runtime as `Arc` clones
  (`storage.frames` and friends are the same `Arc<RecordingStore>`). So the lock is held
  for as long as storage exists — and if nothing held storage, there would be no capture
  at all. The lifetime is therefore tied to something that visibly cannot be dropped.
- **The compile errors were the design working.** Moving ownership by value broke
  `crates/krometrail-store/tests/shared_budget.rs` in nine places, all of the same shape:
  the tests stood up a *second, lookalike* census over a root they did not own, purely to
  read `live_instances()`. That lookalike holds no lock — it is exactly the divergence
  between the tested shape and the running one that let this defect through. Rather than
  reconstruct the lookalike, `RecordingStore::live_instances()` was added so the tests read
  the count from the census the store actually enforces against. One test
  (`a_census_that_never_enumerated_...`) legitimately needs a census built *after*
  enumeration is broken, so it now claims its observer ownership up front, before
  `instances/` is made execute-only — a root cannot be created once it is.
- **Unit 2** removes the root only when nothing but `.owner.lock` remains, and steps over
  every failure. The unlink/`rmdir` race is closed by the identity checks that already
  existed, not by new locking; the comment says so explicitly so a later change does not
  "simplify" them away.
- **Unit 3** now retries `EINTR` up to a bounded 8 attempts and maps only
  `EWOULDBLOCK`/`EAGAIN` to `Ok(false)`.
- **Existing leaked roots self-heal**: with Unit 2 in place, the first 1.3.1 start reclaims
  the empty roots left by earlier versions. No migration or manual cleanup is needed.

## Risks

- Moving `InstanceOwnership` into `InstanceCensus` makes the census non-`Clone` and ties it
  to a real lock. If any construction path builds a census without owning a root, this
  will surface as a compile error — that is the design working, but it may force those
  paths to acquire ownership explicitly.
- Unit 2 deletes a directory. The identity re-verification described above is what makes
  that safe; it must not be relaxed to make the removal more convenient.

## Review (cross-model, GPT-5.6)

Lock lifetime confirmed fixed in both the normal runtime and the doctor path: ownership moves
into `InstanceCensus`, `RecordingStore` owns it, and the store survives through its `Arc`
service clones. `try_lock_exclusive` confirmed correct — the EWOULDBLOCK/EAGAIN guard is
portable and fail-closed exhaustion of the EINTR retries is the right call. The shared-budget
tests were confirmed **not** weakened: the observer *is* the census under test and the two
other roots are real peers.

`RecordingStore::live_instances()` was called minor test-driven API bloat, but not a
correctness issue, since it observes the real census and prevents lookalike tests. Kept.

One finding **parked rather than fixed**: reclamation still resolves its destructive
operations by path, so the `(device, inode)` re-check narrows the TOCTOU window without
closing it against a same-UID process that renames the root and substitutes a symlink. Parked
as `idea-dirfd-anchored-root-reclamation` with the adjudication: it is pre-existing rather
than a regression, its precondition is an attacker who could already delete the store
outright, a partial fix would read as safety it does not provide, and blocking a
today-loses-evidence hotfix on an unsafe-libc rewrite is the wrong trade.

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
