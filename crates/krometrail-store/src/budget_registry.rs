//! Shared accounting so concurrent instances fit inside one total budget.
//!
//! Instance isolation gives each process its own storage, but isolation alone
//! means N processes consume N budgets. This registry is the shared ledger that
//! keeps the *total* bounded: every live instance publishes its own usage, reads
//! what the others are using, and enforces against the whole.
//!
//! Two properties govern the design:
//!
//! - **The lock is held only for the accounting transaction**, never across data
//!   I/O, so instances never serialize on each other's capture writes.
//! - **The registry is never allowed to block capture.** Corruption, a
//!   mid-transaction death, a busy lock, or a read-only filesystem all degrade to
//!   per-instance enforcement rather than stalling a live instance. Degraded
//!   accounting always beats stalled capture.

use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    instance::{INSTANCES_DIRECTORY, InstanceOwnership},
    permissions,
};

const REGISTRY_FILE: &str = ".budget-registry.json";
const REGISTRY_LOCK_FILE: &str = ".budget-registry.lock";

/// How the total budget is divided across live instances right now.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BudgetShare {
    /// Bytes this instance may occupy.
    pub effective_budget: u64,
    /// Combined usage of every other live instance.
    pub other_live_usage: u64,
    /// Number of live instances including this one.
    pub live_instances: u64,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct RegistryFile {
    /// Instance root directory name (a UUID) to last published usage.
    instances: BTreeMap<String, u64>,
}

/// Shared, lock-protected usage ledger for one data directory.
#[derive(Debug)]
pub struct BudgetRegistry {
    instances_directory: PathBuf,
    registry_path: PathBuf,
    lock_path: PathBuf,
    instance_id: String,
}

impl BudgetRegistry {
    /// Opens the registry for the instance rooted at `instance_root`.
    ///
    /// Infallible by construction: a registry that cannot be prepared simply
    /// never contributes shared accounting.
    pub fn open(data_directory: &Path, instance_root: &Path) -> Option<Self> {
        let instance_id = instance_root.file_name()?.to_str()?.to_owned();
        let instances_directory = data_directory.join(INSTANCES_DIRECTORY);
        Some(Self {
            registry_path: instances_directory.join(REGISTRY_FILE),
            lock_path: instances_directory.join(REGISTRY_LOCK_FILE),
            instances_directory,
            instance_id,
        })
    }

    /// Publishes this instance's usage and returns the resulting share.
    ///
    /// Returns `None` when shared accounting is unavailable for any reason; the
    /// caller must then fall back to enforcing the total budget alone, which is
    /// strictly more conservative for a single instance and never blocks.
    pub fn publish(&self, my_usage: u64, total_budget: u64) -> Option<BudgetShare> {
        let _guard = RegistryLock::acquire(&self.lock_path)?;

        // A registry we cannot parse is treated as empty rather than fatal: a
        // corrupt ledger must not brick a live instance, and the next successful
        // write repairs it.
        let mut file = self.read().unwrap_or_default();
        file.instances.insert(self.instance_id.clone(), my_usage);

        // Liveness is decided by the same primitive that decides reclaimability,
        // so "who counts toward the total" and "whose root may be reclaimed" can
        // never disagree. A dead instance's bytes stop counting the moment its
        // root becomes claimable — which is exactly when tier-0 reclaim will
        // free them.
        let mut other_live_usage = 0_u64;
        let mut live_instances = 1_u64;
        let mut live: BTreeMap<String, u64> = BTreeMap::new();
        live.insert(self.instance_id.clone(), my_usage);
        for (id, usage) in &file.instances {
            if *id == self.instance_id {
                continue;
            }
            if self.is_live(id) {
                other_live_usage = other_live_usage.saturating_add(*usage);
                live_instances = live_instances.saturating_add(1);
                live.insert(id.clone(), *usage);
            }
        }
        file.instances = live;

        // A failed write costs accuracy, not liveness: this instance still gets a
        // share computed from what it just read.
        let _ = self.write(&file);

        Some(BudgetShare {
            effective_budget: effective_budget(total_budget, other_live_usage, live_instances),
            other_live_usage,
            live_instances,
        })
    }

    /// Removes this instance from the ledger on clean shutdown.
    ///
    /// Best effort only. A crashed instance leaves its entry behind, and the
    /// liveness probe drops it on the next transaction.
    pub fn withdraw(&self) {
        let Some(_guard) = RegistryLock::acquire(&self.lock_path) else {
            return;
        };
        let Some(mut file) = self.read() else {
            return;
        };
        if file.instances.remove(&self.instance_id).is_some() {
            let _ = self.write(&file);
        }
    }

    fn is_live(&self, instance_id: &str) -> bool {
        let root = self.instances_directory.join(instance_id);
        if !root.is_dir() {
            return false;
        }
        match InstanceOwnership::acquire_existing(&root) {
            // Claimable, so nothing owns it: dead.
            Ok(Some(_)) => false,
            // Held by a live process.
            Ok(None) => true,
            // Undecidable. Counting it as live is the conservative choice: it
            // tightens this instance's own share rather than letting the total
            // silently overshoot.
            Err(_) => true,
        }
    }

    fn read(&self) -> Option<RegistryFile> {
        let bytes = fs::read(&self.registry_path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Writes the ledger atomically.
    ///
    /// A temporary file plus rename means a death mid-write leaves the previous
    /// ledger intact rather than a truncated one.
    fn write(&self, file: &RegistryFile) -> Option<()> {
        let encoded = serde_json::to_vec(file).ok()?;
        let temporary = self
            .registry_path
            .with_extension(format!("tmp-{}", self.instance_id));
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        permissions::configure_private_file(&mut options);
        let mut handle = options.open(&temporary).ok()?;
        handle.write_all(&encoded).ok()?;
        handle.sync_data().ok()?;
        drop(handle);
        fs::rename(&temporary, &self.registry_path).ok()
    }
}

/// Divides the total budget across live instances.
///
/// An instance may occupy whatever the others are not using, but never less than
/// an equal share. The two halves answer the two failure modes directly:
///
/// - `total - others` lets a single busy instance use the whole budget while its
///   peers are idle, so an idle instance cannot permanently hold capacity it is
///   not using.
/// - the equal-share floor guarantees a busy instance is never starved to
///   nothing by peers that grew first.
///
/// The floor is what allows the sum to exceed the total: an instance already
/// holding more than its share is not forced to give it back, because isolation
/// forbids one instance from reclaiming another's data. Overshoot is bounded by
/// `(live - 1) * total / live` and is transient — the over-sized instance trims
/// on its own append path, its evidence ages out, and when it exits its root
/// becomes reclaimable and its bytes stop counting immediately.
fn effective_budget(total_budget: u64, other_live_usage: u64, live_instances: u64) -> u64 {
    let equal_share = total_budget / live_instances.max(1);
    total_budget
        .saturating_sub(other_live_usage)
        .max(equal_share)
}

/// Exclusive hold on the registry, released on drop.
struct RegistryLock {
    _file: fs::File,
}

impl RegistryLock {
    /// Takes the registry lock without blocking.
    ///
    /// Contention means another instance is mid-transaction; those transactions
    /// are short, but waiting on one would put a lock acquisition on the capture
    /// path. Skipping instead degrades this pass to per-instance enforcement.
    fn acquire(path: &Path) -> Option<Self> {
        if let Some(parent) = path.parent() {
            permissions::ensure_private_directory(parent).ok()?;
        }
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        permissions::configure_private_file(&mut options);
        let file = options.open(path).ok()?;
        crate::instance::try_lock_exclusive(&file)
            .ok()?
            .then_some(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_idle_peer_does_not_hold_capacity_it_is_not_using() {
        // One busy instance, one idle peer: the busy one may use everything the
        // idle peer is not.
        assert_eq!(effective_budget(10_000, 0, 2), 10_000);
        assert_eq!(effective_budget(10_000, 1_000, 2), 9_000);
    }

    #[test]
    fn a_busy_instance_is_never_starved_below_an_equal_share() {
        // Peers already hold almost everything; this instance still gets its
        // equal share rather than nothing.
        assert_eq!(effective_budget(10_000, 9_800, 2), 5_000);
        assert_eq!(effective_budget(10_000, 10_000, 4), 2_500);
    }

    #[test]
    fn a_lone_instance_gets_the_whole_budget() {
        assert_eq!(effective_budget(10_000, 0, 1), 10_000);
    }

    #[test]
    fn balanced_instances_sum_to_the_total() {
        // Three instances each at their equal share: the sum is exactly the
        // total, so shared accounting holds when nobody is over-sized.
        let share = effective_budget(9_000, 6_000, 3);
        assert_eq!(share, 3_000);
        assert_eq!(share * 3, 9_000);
    }
}
