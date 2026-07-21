//! Exclusive per-instance ownership of recording storage.
//!
//! A Krometrail process owns exactly one instance root under
//! `<data_dir>/instances/<uuid>/` and holds an advisory lock on it for its whole
//! lifetime. Nothing outside that root is written by the recording store, and no
//! instance mutates a root it does not hold the lock for.
//!
//! This exists because a second process starting against a shared data directory
//! used to run startup recovery over the running process's live segments,
//! renaming an open segment out from under its writer and killing capture
//! globally. Isolation makes that interleaving unrepresentable rather than
//! guarded.

use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use uuid::Uuid;

use crate::{permissions, persistence_error};

/// Directory holding every instance root.
pub const INSTANCES_DIRECTORY: &str = "instances";
/// Advisory lock file inside an instance root.
pub const INSTANCE_LOCK_FILE: &str = ".owner.lock";

/// Recording-cache members of one storage root.
///
/// This allowlist is the authority for every destructive operation on retained
/// recording data: clearing an incompatible cache, and reclaiming an abandoned
/// instance root. Configuration, managed browser profiles, diagnostics, plugin
/// state, and downloads are *not* recording cache and never appear here.
const RECORDING_CACHE_FILES: [&str; 4] = [
    "index.sqlite3",
    "index.sqlite3-wal",
    "index.sqlite3-shm",
    "index.sqlite3-journal",
];
const RECORDING_CACHE_DIRECTORIES: [&str; 3] = ["segments", "artifacts", ".trash"];

/// Whether this platform can *prove* that a root is not held by a live process.
///
/// Unix proves it with `flock`. Nothing in the standard library expresses a
/// deny-share open on Windows, and Krometrail ships a Windows binary, so the
/// honest answer there is "cannot prove". Every decision that rests on the proof
/// then has to take the safe branch rather than the convenient one: an
/// unprovable root is treated as *live*, never as reclaimable. A second Windows
/// process gets its own root and full isolation; what it does not get is the
/// right to delete anyone else's evidence.
pub const OWNERSHIP_IS_ENFORCED: bool = cfg!(unix);

/// The identity of a directory *object*, independent of the path it was found at.
///
/// Reclamation classifies a root at enumeration time and deletes from it some
/// time later. Between those two moments the path can be made to name something
/// else entirely — replaced with a symlink, or renamed aside with a fresh
/// directory put in its place. A path is therefore not a safe handle for a
/// destructive operation; the inode it resolved to is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryIdentity {
    device: u64,
    inode: u64,
}

/// Reads the identity of `path` without following a final symlink.
///
/// `None` means "this is not a directory we can pin", which includes every
/// non-Unix target. Reclamation refuses to act on an unpinnable root, so the
/// platform that cannot prove ownership also cannot delete.
#[cfg(unix)]
fn directory_identity(path: &Path) -> Option<DirectoryIdentity> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(path).ok()?;
    metadata.is_dir().then(|| DirectoryIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(not(unix))]
fn directory_identity(_path: &Path) -> Option<DirectoryIdentity> {
    None
}

/// A sibling instance root found by enumeration, pinned to the directory object
/// that was classified rather than to the path it was seen at.
#[derive(Clone, Debug)]
pub struct InstanceRootCandidate {
    path: PathBuf,
    identity: DirectoryIdentity,
}

impl InstanceRootCandidate {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Takes ownership of this root if nothing live holds it.
    ///
    /// `Ok(None)` covers both "a live process holds it" and "the path no longer
    /// resolves to the directory that was classified". The second case is a
    /// refusal, not a failure: a root that changed identity under us is exactly
    /// the case where continuing would delete from somewhere we never inspected.
    pub fn claim(&self) -> krometrail_core::Result<Option<InstanceOwnership>> {
        let Some(ownership) = InstanceOwnership::acquire_existing(&self.path)? else {
            return Ok(None);
        };
        // Re-read under the held lock. Everything after this point trusts the
        // identity recorded on the ownership, not the path.
        if ownership.identity != Some(self.identity) {
            return Ok(None);
        }
        Ok(Some(ownership))
    }
}

/// An owned instance root, held for the lifetime of the process.
///
/// Dropping this releases the advisory lock. The lock is `flock`-based rather
/// than a pid file so that it is released automatically when the process exits
/// for any reason, including a crash: a stale lock file must never be able to
/// permanently brick startup.
#[derive(Debug)]
pub struct InstanceOwnership {
    root: PathBuf,
    /// The directory this ownership was taken over, read under the lock.
    identity: Option<DirectoryIdentity>,
    // Held purely for its lock; closing the handle releases the advisory lock.
    _lock: File,
}

impl InstanceOwnership {
    /// Claims a fresh instance root under `data_directory`.
    ///
    /// A fresh root is named by a random UUID, so exclusivity here follows from
    /// the name even where the platform cannot take a lock. This is the one
    /// ownership question a non-Unix host may still answer affirmatively.
    ///
    /// The claim is therefore made with an exclusive create rather than a
    /// create-if-missing: an existing directory means something else already
    /// holds this name, and on a host with no locking that is the *only* signal
    /// there is. Establishing the claim by construction, rather than by trusting
    /// UUID collision probability, is what makes "exclusive by name" a fact.
    pub fn acquire_new(data_directory: &Path) -> krometrail_core::Result<Self> {
        let instances = data_directory.join(INSTANCES_DIRECTORY);
        permissions::ensure_private_directory(&instances)
            .map_err(|_| persistence_error("could not create the instance directory"))?;
        let root = instances.join(Uuid::new_v4().to_string());
        permissions::create_private_directory_exclusive(&root)
            .map_err(|_| persistence_error("could not create the instance root"))?;
        let lock = open_lock_file(&root)?;
        if OWNERSHIP_IS_ENFORCED && !try_lock_exclusive(&lock)? {
            return Err(persistence_error(
                "a freshly created instance root was already owned",
            ));
        }
        Ok(Self {
            identity: directory_identity(&root),
            root,
            _lock: lock,
        })
    }

    /// Attempts to take ownership of an existing root.
    ///
    /// Returns `Ok(None)` when another live process holds it, and also whenever
    /// this platform cannot prove otherwise. Acquiring the lock *is* the liveness
    /// test: there is no window in which a caller has decided a root is abandoned
    /// but does not yet own it, so a reclaimer always acts as the root's
    /// legitimate owner rather than reaching into someone else's.
    pub fn acquire_existing(root: &Path) -> krometrail_core::Result<Option<Self>> {
        let lock = open_lock_file(root)?;
        if !try_lock_exclusive(&lock)? {
            return Ok(None);
        }
        Ok(Some(Self {
            root: root.to_path_buf(),
            identity: directory_identity(root),
            _lock: lock,
        }))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Confirms the root path still resolves to the directory this ownership was
    /// taken over.
    fn still_owns_its_root(&self) -> bool {
        self.identity.is_some() && directory_identity(&self.root) == self.identity
    }
}

fn open_lock_file(root: &Path) -> krometrail_core::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    permissions::configure_private_file(&mut options);
    options
        .open(root.join(INSTANCE_LOCK_FILE))
        .map_err(|_| persistence_error("could not open the instance lock"))
}

/// Removes the recording-cache members of a storage root.
///
/// Shared by incompatible-cache clearing and abandoned-root reclamation: both are
/// "remove a recording cache that nothing live owns". Removal is restricted to
/// the allowlist above, so an unexpected member — an operator's file, a future
/// non-cache directory — is left untouched rather than swept away.
pub fn remove_recording_cache(root: &Path) -> krometrail_core::Result<()> {
    for name in RECORDING_CACHE_FILES {
        remove_file_if_present(&root.join(name))?;
    }
    for name in RECORDING_CACHE_DIRECTORIES {
        remove_directory_if_present(&root.join(name))?;
    }
    Ok(())
}

/// Reclaims one abandoned instance root, returning the bytes recovered.
///
/// The caller must already hold this root's lock. Only the allowlisted cache
/// members are removed, so an unexpected member survives rather than being swept
/// away with the root.
///
/// The identity check is not belt-and-braces: the allowlist constrains *which
/// names* are removed and says nothing about *which directory* those names are
/// resolved in. Without it, replacing the root path with a symlink between
/// classification and deletion would aim every allowlisted removal at a
/// directory this process never inspected. Re-checking before each removal keeps
/// the window between "this is the directory I locked" and "delete from it" as
/// short as the filesystem lets it be.
pub fn reclaim_instance_root(ownership: &InstanceOwnership) -> krometrail_core::Result<u64> {
    let root = ownership.root();
    if !ownership.still_owns_its_root() {
        return Err(swapped_root_error());
    }
    let bytes = recording_cache_bytes(root);
    for name in RECORDING_CACHE_FILES {
        if !ownership.still_owns_its_root() {
            return Err(swapped_root_error());
        }
        remove_file_if_present(&root.join(name))?;
    }
    for name in RECORDING_CACHE_DIRECTORIES {
        if !ownership.still_owns_its_root() {
            return Err(swapped_root_error());
        }
        remove_directory_if_present(&root.join(name))?;
    }
    Ok(bytes)
}

fn swapped_root_error() -> krometrail_core::KrometrailError {
    persistence_error("an instance root changed identity before it could be reclaimed")
}

/// Lists instance roots other than the one held, pinned to the directories that
/// were classified.
///
/// Returns nothing on a platform that cannot prove a root is abandoned: without
/// that proof there is no such thing as a reclaimable sibling, only roots a live
/// process might still be writing to.
pub fn sibling_instance_roots(
    data_directory: &Path,
    owned: &Path,
) -> krometrail_core::Result<Vec<InstanceRootCandidate>> {
    if !OWNERSHIP_IS_ENFORCED {
        return Ok(Vec::new());
    }
    let instances = data_directory.join(INSTANCES_DIRECTORY);
    let entries = match fs::read_dir(&instances) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(persistence_error("could not enumerate instance roots")),
    };
    let mut roots = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| persistence_error("could not inspect an instance root"))?;
        // Not following symlinks keeps reclamation from escaping the data root.
        let file_type = entry
            .file_type()
            .map_err(|_| persistence_error("could not classify an instance root"))?;
        if !file_type.is_dir() {
            continue;
        }
        let path = entry.path();
        if path == owned {
            continue;
        }
        if Uuid::parse_str(&entry.file_name().to_string_lossy()).is_err() {
            continue;
        }
        // Pin the directory that was just classified. A candidate that cannot be
        // pinned is skipped rather than carried forward as a bare path.
        let Some(identity) = directory_identity(&path) else {
            continue;
        };
        roots.push(InstanceRootCandidate { path, identity });
    }
    roots.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(roots)
}

/// How many instances are currently sharing one data directory.
///
/// This is the whole input to shared budget enforcement: each instance is
/// allowed `total / live_instances()`, so the policy needs a *count* and never a
/// peer's byte usage. That distinction is the point. A count is derived from the
/// lock files instances already hold, so it is exact at the moment it is read and
/// there is nothing to publish, nothing to cache, and no failure path that could
/// hand out a grant nobody recorded.
#[derive(Debug)]
pub struct InstanceCensus {
    data_directory: PathBuf,
    owned_root: PathBuf,
}

impl InstanceCensus {
    pub fn new(data_directory: &Path, owned_root: &Path) -> Self {
        Self {
            data_directory: data_directory.to_path_buf(),
            owned_root: owned_root.to_path_buf(),
        }
    }

    /// Instances holding a root right now, including this one. Never zero.
    ///
    /// Liveness is `acquire_existing` — the same primitive that decides whether a
    /// root may be reclaimed — so "who counts toward the total" and "whose root
    /// may be deleted" can never disagree. A sibling that cannot be classified
    /// counts as live: that tightens this instance's own share rather than
    /// letting the total silently overshoot.
    ///
    /// Read afresh at every budget decision. There is deliberately no cache: a
    /// cached count is a stale count, and staleness in shared budget accounting
    /// is precisely the defect class this design removes. The read is one
    /// directory scan plus a non-blocking lock attempt per sibling — measured at
    /// roughly 3.5 µs alone and 7 µs with one peer, against an append that
    /// already performs a segment write and a SQLite transaction.
    pub fn live_instances(&self) -> u64 {
        let Ok(siblings) = sibling_instance_roots(&self.data_directory, &self.owned_root) else {
            return 1;
        };
        siblings.iter().fold(1, |live, candidate| {
            // `Ok(Some(_))` means the root was claimable, so nothing owns it. The
            // claim is released immediately; reclamation re-acquires it later.
            let dead = matches!(candidate.claim(), Ok(Some(_)));
            if dead { live } else { live.saturating_add(1) }
        })
    }
}

/// Reports whether `data_directory` still holds a pre-isolation flat store.
pub fn has_legacy_flat_store(data_directory: &Path) -> bool {
    RECORDING_CACHE_FILES
        .iter()
        .any(|name| data_directory.join(name).is_file())
        || RECORDING_CACHE_DIRECTORIES
            .iter()
            .any(|name| data_directory.join(name).is_dir())
}

/// Clears a pre-isolation flat store.
///
/// The legacy layout is an incompatible retained format with no supported
/// consumer, so it is cleared rather than migrated. Only recording-cache members
/// are removed: configuration, managed browser profiles, diagnostics, plugin
/// state, and downloads live in the same directory and must survive.
pub fn clear_legacy_flat_store(data_directory: &Path) -> krometrail_core::Result<u64> {
    let bytes = recording_cache_bytes(data_directory);
    remove_recording_cache(data_directory)?;
    Ok(bytes)
}

fn recording_cache_bytes(root: &Path) -> u64 {
    let mut total = 0_u64;
    for name in RECORDING_CACHE_FILES {
        total = total.saturating_add(file_bytes(&root.join(name)));
    }
    for name in RECORDING_CACHE_DIRECTORIES {
        total = total.saturating_add(directory_bytes(&root.join(name)));
    }
    total
}

fn file_bytes(path: &Path) -> u64 {
    fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .map_or(0, |metadata| metadata.len())
}

fn directory_bytes(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0_u64;
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            total = total.saturating_add(directory_bytes(&entry.path()));
        } else if file_type.is_file() {
            total = total.saturating_add(file_bytes(&entry.path()));
        }
    }
    total
}

fn remove_file_if_present(path: &Path) -> krometrail_core::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(persistence_error("could not clear a recording cache file")),
    }
}

fn remove_directory_if_present(path: &Path) -> krometrail_core::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(persistence_error(
            "could not clear a recording cache directory",
        )),
    }
}

#[cfg(unix)]
pub(crate) fn try_lock_exclusive(file: &File) -> krometrail_core::Result<bool> {
    use std::os::unix::io::AsRawFd;

    // SAFETY: `file` owns a valid descriptor for the duration of this call.
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(code) if code == libc::EWOULDBLOCK || code == libc::EINTR => Ok(false),
        _ => Err(persistence_error("could not acquire the instance lock")),
    }
}

// Windows is a best-effort target. The standard library cannot express a
// deny-share open, so no lock can be taken — and "no lock" must read as "not
// acquired", never as "acquired". Reporting success here would let a second
// process conclude that a root a live process is writing to is abandoned, and
// delete its segments and index. Each process still gets its own instance root,
// which is what prevents the cross-instance mutation this module exists to stop;
// what is given up is reclamation of old roots and shared budget accounting,
// both of which cost disk rather than evidence.
#[cfg(not(unix))]
pub(crate) fn try_lock_exclusive(_file: &File) -> krometrail_core::Result<bool> {
    Ok(false)
}
