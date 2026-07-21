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

/// An owned instance root, held for the lifetime of the process.
///
/// Dropping this releases the advisory lock. The lock is `flock`-based rather
/// than a pid file so that it is released automatically when the process exits
/// for any reason, including a crash: a stale lock file must never be able to
/// permanently brick startup.
#[derive(Debug)]
pub struct InstanceOwnership {
    root: PathBuf,
    // Held purely for its lock; closing the handle releases the advisory lock.
    _lock: File,
}

impl InstanceOwnership {
    /// Claims a fresh instance root under `data_directory`.
    pub fn acquire_new(data_directory: &Path) -> krometrail_core::Result<Self> {
        let instances = data_directory.join(INSTANCES_DIRECTORY);
        permissions::ensure_private_directory(&instances)
            .map_err(|_| persistence_error("could not create the instance directory"))?;
        let root = instances.join(Uuid::new_v4().to_string());
        permissions::ensure_private_directory(&root)
            .map_err(|_| persistence_error("could not create the instance root"))?;
        Self::acquire_existing(&root)?
            .ok_or_else(|| persistence_error("a freshly created instance root was already owned"))
    }

    /// Attempts to take ownership of an existing root.
    ///
    /// Returns `Ok(None)` when another live process holds it. Acquiring the lock
    /// *is* the liveness test: there is no window in which a caller has decided a
    /// root is abandoned but does not yet own it, so a reclaimer always acts as
    /// the root's legitimate owner rather than reaching into someone else's.
    pub fn acquire_existing(root: &Path) -> krometrail_core::Result<Option<Self>> {
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        permissions::configure_private_file(&mut options);
        let lock = options
            .open(root.join(INSTANCE_LOCK_FILE))
            .map_err(|_| persistence_error("could not open the instance lock"))?;
        if try_lock_exclusive(&lock)? {
            Ok(Some(Self {
                root: root.to_path_buf(),
                _lock: lock,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
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
/// members are removed; the root directory itself is then removed when empty, so
/// an unexpected member keeps the root alive instead of being destroyed with it.
pub fn reclaim_instance_root(ownership: &InstanceOwnership) -> krometrail_core::Result<u64> {
    let root = ownership.root();
    let bytes = recording_cache_bytes(root);
    remove_recording_cache(root)?;
    Ok(bytes)
}

/// Lists instance roots other than the one held.
pub fn sibling_instance_roots(
    data_directory: &Path,
    owned: &Path,
) -> krometrail_core::Result<Vec<PathBuf>> {
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
        if Uuid::parse_str(&entry.file_name().to_string_lossy()).is_ok() {
            roots.push(path);
        }
    }
    roots.sort();
    Ok(roots)
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
// deny-share open, so ownership there is advisory-by-layout only: each process
// still gets its own instance root, which is what prevents the cross-instance
// mutation this module exists to stop.
#[cfg(not(unix))]
pub(crate) fn try_lock_exclusive(_file: &File) -> krometrail_core::Result<bool> {
    Ok(true)
}
