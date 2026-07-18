//! Exclusive ownership of Krometrail-managed profile directories.

use krometrail_core::{ManagedProfile, ManagedProfileSummary, ProfileIdentity, ProfileRef};
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
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
        tighten_directory(root).map_err(|_| ProfileError::Root)?;
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
        tighten_directory(&profiles_root).map_err(|_| ProfileError::Prepare)?;
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
        tighten_tree(&path).map_err(|_| ProfileError::Prepare)?;
        let lock_path = path.join(".krometrail.lock");
        let lock = exclusive_lock(&lock_path)?;
        let _ = fs::remove_file(path.join("DevToolsActivePort"));
        Ok(Self {
            path,
            lock_path,
            lock: Some(lock),
            profile: ProfileRef::managed(name.clone()),
            kind: ProfileLeaseKind::Reusable,
            cleanup_temporary: false,
        })
    }

    fn acquire_temporary(root: &Path) -> Result<Self, ProfileError> {
        let directory = root.join("tmp");
        fs::create_dir_all(&directory).map_err(|_| ProfileError::Root)?;
        tighten_directory(&directory).map_err(|_| ProfileError::Root)?;
        let directory = fs::canonicalize(&directory).map_err(|_| ProfileError::Root)?;
        if !directory.starts_with(root) {
            return Err(ProfileError::Root);
        }
        let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!("profile-{}-{sequence}", std::process::id()));
        fs::create_dir(&path).map_err(|_| ProfileError::Prepare)?;
        tighten_directory(&path).map_err(|_| ProfileError::Prepare)?;
        let lock_path = path.join(".krometrail.lock");
        let lock = exclusive_lock(&lock_path)?;
        Ok(Self {
            path,
            lock_path,
            lock: Some(lock),
            profile: ProfileRef::temporary(
                ProfileIdentity::new(format!("temporary-{sequence}"))
                    .expect("generated profile identity"),
            ),
            kind: ProfileLeaseKind::Temporary,
            cleanup_temporary: true,
        })
    }

    fn cleanup(&mut self) {
        // Dropping the file before deleting the directory is required on Windows. Reusable
        // profile data is intentionally retained even if lock cleanup itself fails. Removing the
        // active-port handoff avoids a stale endpoint being mistaken for a future launch.
        self.lock.take();
        let _ = fs::remove_file(self.path.join("DevToolsActivePort"));
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

/// List reusable managed profiles without exposing their filesystem locations or contents.
pub fn list_reusable_profiles(root: &Path) -> Result<Vec<ManagedProfileSummary>, ProfileError> {
    let profiles_root = root.join("profiles");
    let entries = match fs::read_dir(&profiles_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(ProfileError::Root),
    };
    let mut profiles = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| ProfileError::Root)?;
        let metadata = entry.file_type().map_err(|_| ProfileError::Root)?;
        if !metadata.is_dir() || metadata.is_symlink() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if validate_name(&name).is_err() {
            continue;
        }
        let Ok(identity) = ProfileIdentity::new(name) else {
            continue;
        };
        profiles.push(ManagedProfileSummary {
            identity,
            in_use: entry.path().join(".krometrail.lock").is_file(),
        });
    }
    profiles.sort_by(|left, right| left.identity.as_str().cmp(right.identity.as_str()));
    Ok(profiles)
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
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    private_file_options(&mut options);
    options.open(path).map_err(|error| match error.kind() {
        io::ErrorKind::AlreadyExists => ProfileError::InUse,
        _ => ProfileError::Prepare,
    })
}

#[cfg(unix)]
fn private_file_options(options: &mut OpenOptions) {
    options.mode(0o600);
}

#[cfg(not(unix))]
fn private_file_options(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn tighten_directory(path: &Path) -> io::Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn tighten_directory(_path: &Path) -> io::Result<()> {
    // Windows ACLs are not representable through the standard library's mode API. Managed
    // profiles still retain exclusive Krometrail locking; platform ACL hardening is explicit
    // future infrastructure rather than a false cross-platform mode claim.
    Ok(())
}

#[cfg(unix)]
fn tighten_file(path: &Path) -> io::Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn tighten_file(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn tighten_tree(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            tighten_tree(&entry?.path())?;
        }
        tighten_directory(path)
    } else {
        tighten_file(path)
    }
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

    #[cfg(unix)]
    #[test]
    fn managed_profile_roots_files_and_lock_are_owner_only_and_stale_endpoint_is_removed() {
        use std::os::unix::fs::PermissionsExt;

        let root = root();
        let path = root.join("profiles").join("named");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("Preferences"), b"profile").unwrap();
        fs::write(
            path.join("DevToolsActivePort"),
            b"9222\n/devtools/browser/stale\n",
        )
        .unwrap();
        fs::set_permissions(path.join("Preferences"), fs::Permissions::from_mode(0o644)).unwrap();

        let lease = ProfileLease::acquire(
            &root,
            &ManagedProfile::Reusable {
                name: ProfileIdentity::new("named").unwrap(),
            },
        )
        .unwrap();
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(lease.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(lease.path().join("Preferences"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(lease.path().join(".krometrail.lock"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(!lease.path().join("DevToolsActivePort").exists());
        drop(lease);
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

    #[test]
    fn inventory_is_sorted_private_and_excludes_temporary_and_symlink_entries() {
        let root = root();
        let active = ProfileLease::acquire(
            &root,
            &ManagedProfile::Reusable {
                name: ProfileIdentity::new("z-active").unwrap(),
            },
        )
        .unwrap();
        drop(
            ProfileLease::acquire(
                &root,
                &ManagedProfile::Reusable {
                    name: ProfileIdentity::new("a-idle").unwrap(),
                },
            )
            .unwrap(),
        );
        let temporary = ProfileLease::acquire(&root, &ManagedProfile::Temporary).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(active.path(), root.join("profiles/link")).unwrap();

        let profiles = list_reusable_profiles(&root).unwrap();
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].identity.as_str(), "a-idle");
        assert!(!profiles[0].in_use);
        assert_eq!(profiles[1].identity.as_str(), "z-active");
        assert!(profiles[1].in_use);
        assert!(temporary.path().starts_with(root.join("tmp")));

        drop(temporary);
        drop(active);
        let _ = fs::remove_dir_all(root);
    }
}
