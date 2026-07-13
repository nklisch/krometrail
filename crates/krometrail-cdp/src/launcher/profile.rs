//! Exclusive ownership of Krometrail-managed profile directories.

use krometrail_core::{ManagedProfile, ProfileIdentity, ProfileRef};
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use thiserror::Error;

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileLeaseKind {
    Reusable,
    Temporary,
}

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("profile name is invalid")]
    InvalidName,
    #[error("profile is already in use")]
    InUse,
    #[error("profile root could not be prepared")]
    Root,
    #[error("profile could not be prepared")]
    Prepare,
}

/// A held lock and its cleanup policy. The lock file is kept inside the profile so an existing
/// reusable profile is safe to reopen, while the temporary guard owns the complete directory.
pub struct ProfileLease {
    path: PathBuf,
    lock_path: PathBuf,
    lock: Option<File>,
    profile: ProfileRef,
    kind: ProfileLeaseKind,
    cleanup_temporary: bool,
}

impl ProfileLease {
    pub fn acquire(
        root: impl AsRef<Path>,
        requested: &ManagedProfile,
    ) -> Result<Self, ProfileError> {
        let root = root.as_ref();
        fs::create_dir_all(root).map_err(|_| ProfileError::Root)?;
        let root = fs::canonicalize(root).map_err(|_| ProfileError::Root)?;
        match requested {
            ManagedProfile::Reusable { name } => Self::acquire_reusable(&root, name),
            ManagedProfile::Temporary => Self::acquire_temporary(&root),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn profile_ref(&self) -> &ProfileRef {
        &self.profile
    }

    pub fn kind(&self) -> ProfileLeaseKind {
        self.kind
    }

    /// Release a lease without deleting reusable data. Temporary cleanup remains idempotent.
    pub fn release(mut self) {
        self.cleanup();
    }

    fn acquire_reusable(root: &Path, name: &ProfileIdentity) -> Result<Self, ProfileError> {
        validate_name(name.as_str())?;
        let profiles_root = root.join("profiles");
        fs::create_dir_all(&profiles_root).map_err(|_| ProfileError::Prepare)?;
        let profiles_root = fs::canonicalize(&profiles_root).map_err(|_| ProfileError::Prepare)?;
        if !profiles_root.starts_with(root) {
            return Err(ProfileError::InvalidName);
        }
        let path = profiles_root.join(name.as_str());
        fs::create_dir_all(&path).map_err(|_| ProfileError::Prepare)?;
        let path = fs::canonicalize(&path).map_err(|_| ProfileError::Prepare)?;
        if !path.starts_with(&profiles_root) || path != profiles_root.join(name.as_str()) {
            return Err(ProfileError::InvalidName);
        }
        let lock_path = path.join(".krometrail.lock");
        let lock = exclusive_lock(&lock_path)?;
        Ok(Self {
            path,
            lock_path,
            lock: Some(lock),
            profile: ProfileRef::Managed(name.clone()),
            kind: ProfileLeaseKind::Reusable,
            cleanup_temporary: false,
        })
    }

    fn acquire_temporary(root: &Path) -> Result<Self, ProfileError> {
        let directory = root.join("tmp");
        fs::create_dir_all(&directory).map_err(|_| ProfileError::Root)?;
        let directory = fs::canonicalize(&directory).map_err(|_| ProfileError::Root)?;
        if !directory.starts_with(root) {
            return Err(ProfileError::Root);
        }
        let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!("profile-{}-{sequence}", std::process::id()));
        fs::create_dir(&path).map_err(|_| ProfileError::Prepare)?;
        let lock_path = path.join(".krometrail.lock");
        let lock = exclusive_lock(&lock_path)?;
        Ok(Self {
            path,
            lock_path,
            lock: Some(lock),
            profile: ProfileRef::Managed(
                ProfileIdentity::new(format!("temporary-{sequence}"))
                    .expect("generated profile identity"),
            ),
            kind: ProfileLeaseKind::Temporary,
            cleanup_temporary: true,
        })
    }

    fn cleanup(&mut self) {
        // Dropping the file before deleting the directory is required on Windows. Reusable
        // profile data is intentionally retained even if lock cleanup itself fails.
        self.lock.take();
        let _ = fs::remove_file(&self.lock_path);
        if self.cleanup_temporary {
            let _ = fs::remove_dir_all(&self.path);
            self.cleanup_temporary = false;
        }
    }
}

impl Drop for ProfileLease {
    fn drop(&mut self) {
        self.cleanup();
    }
}

fn validate_name(name: &str) -> Result<(), ProfileError> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(ProfileError::InvalidName);
    }
    Ok(())
}

fn exclusive_lock(path: &Path) -> Result<File, ProfileError> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| match error.kind() {
            io::ErrorKind::AlreadyExists => ProfileError::InUse,
            _ => ProfileError::Prepare,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "krometrail-profile-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn named_profiles_are_exclusive_and_reusable_data_survives() {
        let root = root();
        let profile = ManagedProfile::Reusable {
            name: ProfileIdentity::new("named").unwrap(),
        };
        let lease = ProfileLease::acquire(&root, &profile).unwrap();
        assert!(matches!(
            ProfileLease::acquire(&root, &profile),
            Err(ProfileError::InUse)
        ));
        let path = lease.path().to_owned();
        drop(lease);
        assert!(path.exists());
        drop(ProfileLease::acquire(&root, &profile).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn temporary_profiles_are_owned_by_their_guard() {
        let root = root();
        let lease = ProfileLease::acquire(&root, &ManagedProfile::Temporary).unwrap();
        let path = lease.path().to_owned();
        assert!(path.exists());
        drop(lease);
        assert!(!path.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn traversal_and_separator_names_are_rejected() {
        let root = root();
        for name in ["..", "../escape", "a/b", "a\\b"] {
            let profile = ManagedProfile::Reusable {
                name: ProfileIdentity::new(name).unwrap(),
            };
            assert!(matches!(
                ProfileLease::acquire(&root, &profile),
                Err(ProfileError::InvalidName)
            ));
        }
        let _ = fs::remove_dir_all(root);
    }
}
