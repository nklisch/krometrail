//! Permission policy for Krometrail-owned evidence paths.
//!
//! Unix mode bits are the portable guarantee available to this crate. Other platforms use their
//! native ACLs and remain explicit rather than pretending that Unix modes exist there.

use std::{fs, io, path::Path};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
pub(crate) fn ensure_private_directory(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
pub(crate) fn ensure_private_directory(path: &Path) -> io::Result<()> {
    // Windows ACLs and platform-specific permission inheritance are not expressible with the
    // standard library mode API. Creation still occurs at the same boundary; callers must rely on
    // the platform's native ACL policy rather than receiving a false owner-only claim.
    fs::create_dir_all(path)
}

/// Creates one private directory that must not already exist.
///
/// `create_dir` on the leaf is the load-bearing difference from
/// `ensure_private_directory`: it fails with `AlreadyExists` rather than
/// succeeding on a directory someone else made. Callers that claim a path *by
/// name* — where the name itself is the exclusivity argument, as on hosts that
/// cannot take a lock — need that failure, because `create_dir_all` would let
/// two claimants agree they each own the same path. The parent is still created
/// on demand; only the leaf is exclusive.
pub(crate) fn create_private_directory_exclusive(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        ensure_private_directory(parent)?;
    }
    fs::create_dir(path)?;
    tighten_new_directory(path)
}

#[cfg(unix)]
fn tighten_new_directory(path: &Path) -> io::Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn tighten_new_directory(_path: &Path) -> io::Result<()> {
    // See `ensure_private_directory`: no portable owner-only claim exists here.
    Ok(())
}

fn existing_regular_file(path: &Path) -> io::Result<fs::Metadata> {
    // `symlink_metadata` is intentional: permission hardening must not follow an
    // extension-shaped link into operator-owned or unrelated storage.
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "permission policy requires a regular file",
        ));
    }
    Ok(metadata)
}

#[cfg(unix)]
pub(crate) fn tighten_existing_file(path: &Path) -> io::Result<()> {
    let mut permissions = existing_regular_file(path)?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
pub(crate) fn tighten_existing_file(path: &Path) -> io::Result<()> {
    existing_regular_file(path).map(|_| ())
}

#[cfg(unix)]
pub(crate) fn configure_private_file(options: &mut fs::OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
pub(crate) fn configure_private_file(_options: &mut fs::OpenOptions) {}

#[cfg(unix)]
pub(crate) fn tighten_existing_directory(path: &Path) -> io::Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
pub(crate) fn tighten_existing_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Instance roots claim a path *by name*. On a host that cannot take a lock
    /// the name is the entire exclusivity argument, so the create must fail on a
    /// path that already exists rather than adopting someone else's directory.
    #[test]
    fn an_exclusive_private_directory_refuses_a_path_that_already_exists() {
        let base = tempfile::TempDir::new().unwrap();
        let leaf = base.path().join("nested").join("root");

        create_private_directory_exclusive(&leaf).expect("a fresh leaf is created");
        assert!(leaf.is_dir());

        let error = create_private_directory_exclusive(&leaf)
            .expect_err("a second claim on the same name must not succeed");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    }

    /// The parent chain is still created on demand; only the leaf is exclusive.
    #[test]
    fn an_exclusive_private_directory_still_creates_missing_parents() {
        let base = tempfile::TempDir::new().unwrap();
        let leaf = base.path().join("a").join("b").join("c");
        create_private_directory_exclusive(&leaf).unwrap();
        assert!(leaf.is_dir());

        #[cfg(unix)]
        {
            let mode = fs::metadata(&leaf).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "an exclusive claim is still owner-only");
        }
    }
}
