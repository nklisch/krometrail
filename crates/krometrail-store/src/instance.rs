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
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
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
///
/// `identity` is `None` when the root could not be pinned at enumeration time.
/// Such a root is undecidable, and both consumers must take the safe branch from
/// the same fact: it is never reclaimed, and it always counts as live.
#[derive(Clone, Debug)]
pub struct InstanceRootCandidate {
    path: PathBuf,
    identity: Option<DirectoryIdentity>,
}

impl InstanceRootCandidate {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Takes ownership of this root if nothing live holds it.
    ///
    /// `Ok(None)` covers "a live process holds it", "the root could never be
    /// pinned", and "the path no longer resolves to the directory that was
    /// classified". None of these is a failure; each is a refusal. A root that
    /// changed identity under us, or that was never identifiable in the first
    /// place, is exactly the case where continuing would delete from somewhere
    /// this process never inspected.
    ///
    /// Refusing also settles the count: the census reads an unclaimable root as
    /// live, so an undecidable root tightens this instance's share instead of
    /// vanishing from the total.
    pub fn claim(&self) -> krometrail_core::Result<Option<InstanceOwnership>> {
        let Some(identity) = self.identity else {
            return Ok(None);
        };
        let Some(ownership) = InstanceOwnership::acquire_existing(&self.path)? else {
            return Ok(None);
        };
        // Re-read under the held lock. Everything after this point trusts the
        // identity recorded on the ownership, not the path.
        if ownership.identity != Some(identity) {
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
    instance_root_candidates(data_directory, owned, None)
}

/// `sibling_instance_roots`, optionally reading through a descriptor opened
/// earlier rather than resolving the instances path again.
///
/// The census supplies a handle; reclamation does not need one, because a
/// reclaim pass that cannot enumerate simply reclaims nothing.
fn instance_root_candidates(
    data_directory: &Path,
    owned: &Path,
    handle: Option<&InstancesDirectoryHandle>,
) -> krometrail_core::Result<Vec<InstanceRootCandidate>> {
    if !OWNERSHIP_IS_ENFORCED {
        return Ok(Vec::new());
    }
    let instances = data_directory.join(INSTANCES_DIRECTORY);
    let names = instance_directory_names(&instances, handle)?;
    let mut roots = Vec::new();
    for name in names {
        if Uuid::parse_str(&name.to_string_lossy()).is_err() {
            continue;
        }
        let path = instances.join(&name);
        if path == owned {
            continue;
        }
        // Pin the directory that was just enumerated. This doubles as the
        // is-it-a-directory test, and it does not follow a final symlink, which
        // keeps reclamation from escaping the data root.
        //
        // A candidate that cannot be pinned is carried forward unpinned rather
        // than dropped: dropping it would hide a root from the census and widen
        // every instance's share. `claim` refuses an unpinned root, which makes
        // it unreclaimable and live at once. That is also why a stat failure is
        // not promoted to a census failure — an entry we cannot classify is
        // strictly safer counted than it is used to void the whole count.
        let identity = directory_identity(&path);
        roots.push(InstanceRootCandidate { path, identity });
    }
    roots.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(roots)
}

/// Lists the entry names of the instances directory.
///
/// A retained handle is tried first and the path is the fallback, so a handle
/// that was never opened — or that fails at read time — degrades to exactly the
/// behaviour there was before handles existed.
fn instance_directory_names(
    instances: &Path,
    handle: Option<&InstancesDirectoryHandle>,
) -> krometrail_core::Result<Vec<OsString>> {
    if let Some(names) = handle.and_then(InstancesDirectoryHandle::entry_names) {
        return Ok(names);
    }
    let entries = match fs::read_dir(instances) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(persistence_error("could not enumerate instance roots")),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| persistence_error("could not inspect an instance root"))?;
        names.push(entry.file_name());
    }
    Ok(names)
}

/// The instances directory, held open for the lifetime of the census.
///
/// `read_dir` on a path re-resolves the path and re-checks permissions on every
/// call, so anything that changes the directory's mode — or replaces what the
/// path names — blinds every later census. A descriptor opened once does not
/// re-check: the access decision was made at open time and the kernel keeps
/// honouring it. Enumerating through the retained descriptor therefore keeps
/// working across exactly the faults that used to force the fallback.
///
/// The handle is deliberately *not* a cache of the answer. Every census still
/// walks the whole directory and re-tests every sibling's lock; only the way the
/// directory is reached changed.
#[derive(Debug)]
struct InstancesDirectoryHandle {
    /// Behind a `Mutex` because `dup` shares the open file description, and with
    /// it the directory cursor. Two concurrent enumerations of one description
    /// would each read part of the directory and neither would see all of it —
    /// an undercount, which is the one error direction this module cannot
    /// tolerate.
    directory: Mutex<File>,
}

impl InstancesDirectoryHandle {
    /// Opens `instances` for enumeration, or `None` if it cannot be opened.
    ///
    /// Only Linux and macOS are wired up. Enumerating a descriptor needs
    /// `fdopendir`, and distinguishing end-of-directory from a read error needs
    /// the platform's `errno` slot; both are spelled per-platform. Any other
    /// Unix simply gets no handle and falls back to the path, which is correct,
    /// just less resilient.
    #[cfg(any(target_os = "linux", target_vendor = "apple"))]
    fn open(instances: &Path) -> Option<Self> {
        let directory = File::open(instances).ok()?;
        if !directory.metadata().ok()?.is_dir() {
            return None;
        }
        Some(Self {
            directory: Mutex::new(directory),
        })
    }

    #[cfg(not(any(target_os = "linux", target_vendor = "apple")))]
    fn open(_instances: &Path) -> Option<Self> {
        None
    }

    /// Reads every entry name through the retained descriptor.
    ///
    /// `None` means "this handle produced no trustworthy listing" and sends the
    /// caller to the path fallback. A partial listing is never returned: a read
    /// error mid-directory discards what was collected, because a short list
    /// would undercount the live set.
    #[cfg(any(target_os = "linux", target_vendor = "apple"))]
    fn entry_names(&self) -> Option<Vec<OsString>> {
        use std::{
            ffi::CStr,
            os::unix::{ffi::OsStrExt, io::AsRawFd},
        };

        // A poisoned lock only means some other thread panicked mid-enumeration.
        // The descriptor is still valid and the stream is rewound below, so the
        // listing is unaffected.
        let directory = self
            .directory
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // `fdopendir` takes ownership of the descriptor it is given and
        // `closedir` closes it, so it gets a duplicate rather than the retained
        // one.
        // SAFETY: the retained `File` keeps its descriptor valid for this call.
        let duplicate = unsafe { libc::dup(directory.as_raw_fd()) };
        if duplicate < 0 {
            return None;
        }
        // SAFETY: `duplicate` is a valid directory descriptor this call owns.
        let stream = unsafe { libc::fdopendir(duplicate) };
        if stream.is_null() {
            // SAFETY: `fdopendir` failed, so ownership was not transferred.
            unsafe { libc::close(duplicate) };
            return None;
        }
        // The duplicate shares the retained description's cursor, which a
        // previous enumeration left at end-of-directory.
        // SAFETY: `stream` is a live directory stream owned by this call.
        unsafe { libc::rewinddir(stream) };

        let mut names = Vec::new();
        let failed = loop {
            // `readdir` returns null for both end-of-directory and failure, and
            // only `errno` tells them apart. It is left untouched on success, so
            // it has to be cleared before each call.
            // SAFETY: the platform errno slot is valid for the current thread.
            unsafe { *errno_slot() = 0 };
            // SAFETY: `stream` is a live directory stream owned by this call.
            let entry = unsafe { libc::readdir(stream) };
            if entry.is_null() {
                // SAFETY: the platform errno slot is valid for the current thread.
                break unsafe { *errno_slot() } != 0;
            }
            // SAFETY: `readdir` returned a non-null entry that stays valid until
            // the next call on this stream, and `d_name` is NUL-terminated.
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
            let name = name.to_bytes();
            if name == b"." || name == b".." {
                continue;
            }
            names.push(std::ffi::OsStr::from_bytes(name).to_os_string());
        };
        // SAFETY: `stream` is a live directory stream owned by this call and is
        // not used afterwards; this also closes `duplicate`.
        unsafe { libc::closedir(stream) };
        drop(directory);

        (!failed).then_some(names)
    }

    #[cfg(not(any(target_os = "linux", target_vendor = "apple")))]
    fn entry_names(&self) -> Option<Vec<OsString>> {
        None
    }
}

/// The current thread's `errno` storage.
#[cfg(target_os = "linux")]
unsafe fn errno_slot() -> *mut i32 {
    // SAFETY: glibc's accessor is always safe to call and returns a pointer
    // valid for the calling thread.
    unsafe { libc::__errno_location() }
}

#[cfg(target_vendor = "apple")]
unsafe fn errno_slot() -> *mut i32 {
    // SAFETY: as above, for the Darwin spelling of the same accessor.
    unsafe { libc::__error() }
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
    /// The instances directory, opened once at construction.
    ///
    /// `None` means the directory could not be opened then — which is the one
    /// state in which this census has no way to learn about peers at all, and
    /// the reason `proved_live` may start above one.
    handle: Option<InstancesDirectoryHandle>,
    /// The highest live count this census has ever *proved*.
    ///
    /// Not a cache of the answer — the answer is always recomputed. This is the
    /// floor a failed count falls back to, and it exists so that failing to
    /// count can never be more permissive than counting. It only rises, so the
    /// fallback share only ever narrows.
    proved_live: AtomicU64,
}

/// Peers assumed live by a census that has never once read the instances
/// directory.
///
/// The monotonic floor closes every case *after* one successful count, and the
/// retained descriptor closes the ordinary causes of losing that ability. What
/// neither closes is a census whose very first enumeration fails: it has no
/// evidence at all, and the honest floor of "this instance itself" would hand it
/// `total / 1`. Two instances that both start that way jointly claim twice the
/// total, which is exactly the optimistic grant this design exists to remove.
///
/// So an instance that cannot see its siblings assumes it has some. Four is
/// chosen to be larger than the common concurrent-instance count while still
/// leaving a quarter of the total — a usable, non-zero share, so capture never
/// stalls on this. It is a fail-closed guess, not a measurement, and it is only
/// ever consulted while the directory is unreadable: the first successful
/// enumeration replaces it with the real count.
const ASSUMED_LIVE_INSTANCES_WITHOUT_EVIDENCE: u64 = 4;

impl InstanceCensus {
    pub fn new(data_directory: &Path, owned_root: &Path) -> Self {
        let census = Self {
            handle: InstancesDirectoryHandle::open(&data_directory.join(INSTANCES_DIRECTORY)),
            data_directory: data_directory.to_path_buf(),
            owned_root: owned_root.to_path_buf(),
            // This instance itself, which is the one peer no census can miss.
            proved_live: AtomicU64::new(1),
        };
        // Establish the floor from the first count rather than assuming one.
        // Only a census that cannot enumerate *at construction* fails closed;
        // every other census starts from a number it actually proved.
        if census.count_live().is_none() {
            census
                .proved_live
                .store(ASSUMED_LIVE_INSTANCES_WITHOUT_EVIDENCE, Ordering::Relaxed);
        }
        census
    }

    /// Counts live instances, or `None` when the directory could not be read.
    fn count_live(&self) -> Option<u64> {
        let siblings =
            instance_root_candidates(&self.data_directory, &self.owned_root, self.handle.as_ref())
                .ok()?;
        Some(siblings.iter().fold(1_u64, |live, candidate| {
            // `Ok(Some(_))` means the root was claimable, so nothing owns it. The
            // claim is released immediately; reclamation re-acquires it later.
            let dead = matches!(candidate.claim(), Ok(Some(_)));
            if dead { live } else { live.saturating_add(1) }
        }))
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
    /// already performs a segment write and a SQLite transaction. Those figures
    /// predate the retained descriptor, which trades the scan's `open` of the
    /// path for a `dup` of a descriptor already held and leaves the syscall
    /// shape otherwise unchanged.
    ///
    /// Enumeration can still fail, and that failure must not widen a share.
    /// Granting the full total whenever counting broke is the same
    /// optimistic-grant defect the usage ledger was deleted for: two instances
    /// that both stumble would each be handed `total`. Three things keep that
    /// shut, in order of preference:
    ///
    /// 1. The directory is read through a descriptor opened at construction, so
    ///    a permission change or a path swap after startup does not blind the
    ///    census at all. This is the ordinary case and it stays exact.
    /// 2. If a read still fails, the count falls back to the highest count this
    ///    census has already proved. Monotonic, so it only ever narrows.
    /// 3. If the census never enumerated *even once*, it has no evidence about
    ///    peers and assumes `ASSUMED_LIVE_INSTANCES_WITHOUT_EVIDENCE` of them
    ///    rather than assuming solitude.
    ///
    /// None of the three can block capture: every branch yields a number of at
    /// least one, and each self-corrects the moment enumeration works.
    pub fn live_instances(&self) -> u64 {
        let Some(live) = self.count_live() else {
            return self.proved_live.load(Ordering::Relaxed).max(1);
        };
        self.proved_live.fetch_max(live, Ordering::Relaxed);
        live
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
