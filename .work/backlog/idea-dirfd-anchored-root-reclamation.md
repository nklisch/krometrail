---
id: idea-dirfd-anchored-root-reclamation
created: 2026-07-21
updated: 2026-07-21
tags: [storage, security]
---

Instance-root reclamation resolves its destructive operations by path, so the `(device,
inode)` identity check narrows the race window without closing it. Raised by cross-model
review (GPT-5.6) of `50a46112`; **deliberately not fixed in the 1.3.1 hotfix** — see the
adjudication below.

## The finding

`reclaim_instance_root` re-checks `(device, inode)` at
`crates/krometrail-store/src/instance.rs:300` and then deletes via `root.join(...)`. Between
the final identity check and each removal, a same-UID process can rename the inspected root
away and replace the path with a symlink or another directory. The reclaimer then resolves
the allowlisted names — and, since `50a46112`, `.owner.lock` and the root `rmdir` — against
a directory it never inspected. Inode reuse can also make the tuple match again.

The flock and identity logic is correct for every *cooperative* case: concurrent reclaimers,
orphaned-lock claimants after `remove_emptied_root` unlinks the lock, and roots that changed
identity between classification and claim. The gap is specifically an adversarial or
pathological filesystem mutation under the same UID.

## Adjudication: park, do not partially fix

- **Practical severity is low.** The precondition is a same-UID process mutating the data
  directory. Such a process can already delete the entire store outright without racing
  anything, so the attack buys an adversary nothing they did not already have. Non-adversarial
  inode reuse needs a directory deleted and recreated at the same path within a sub-millisecond
  window.
- **It is pre-existing, not a regression.** The path-resolved removals predate this cycle;
  `50a46112` extends the blast radius by two single-level operations (lock unlink, root
  `rmdir`), and the code comment at `instance.rs` already states the check "keeps the window
  as short as the filesystem lets it be" — an explicit narrowing, never a closure.
- **A partial fix is worse than none.** Anchoring only the two newly added operations would
  leave the recursive cache-member removals path-resolved while reading as though the hole
  were closed, and would complicate the real fix.
- **Blocking a data-loss hotfix on an unsafe-libc rewrite is the wrong trade.** 1.3.1 fixes
  a defect that destroys evidence during ordinary use, today. This one requires an attacker.

## Fix direction

Anchor every destructive operation to a directory descriptor rather than a path, throughout
`reclaim_instance_root` and `remove_recording_cache` — not selectively:

1. `open` the root with `O_DIRECTORY | O_NOFOLLOW`, `fstat` it, and compare against the
   identity recorded under the lock. A descriptor follows the inode, so a later rename or
   symlink swap cannot redirect it.
2. Remove through `unlinkat(dirfd, name, 0)` and `unlinkat(dirfd, name, AT_REMOVEDIR)`,
   recursing into `segments/`, `artifacts/`, and `.trash/` via `openat` rather than
   `remove_dir_all` on a rebuilt path.
3. Keep the existing identity re-checks. They stay useful for the classification-to-claim
   window, which a descriptor opened later cannot cover.

This is the same reasoning the census already applies to *enumeration* — `docs/SPEC.md`
explains why the instances directory is held open rather than re-resolved ("a path lookup
re-checks permissions on every call; a descriptor's access check happened at open time").
Destructive operations have a stronger claim on that treatment than reads do, so the
inconsistency is worth removing on its own merits, independent of the threat model.

## Also confirmed sound by the same review

Recorded so a later reader does not re-litigate these: lock lifetime across the normal and
doctor runtimes; `try_lock_exclusive`'s EINTR retry and fail-closed exhaustion; that the
shared-budget tests were not weakened; that always-installing the dialog signals persists
nothing and enables no optional domain; that frame paths cannot leak cross-origin via
`about:blank`/`srcdoc`, opaque origins, redirects, or frame navigation; that relaxed matching
is bounded and `saturated` is honest; and that excluding `wait_timed_out` from dialog
re-coding is required by `batch.rs:141` rather than convenient.
