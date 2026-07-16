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
