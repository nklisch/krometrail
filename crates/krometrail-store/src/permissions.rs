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

#[cfg(unix)]
pub(crate) fn tighten_existing_file(path: &Path) -> io::Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
pub(crate) fn tighten_existing_file(_path: &Path) -> io::Result<()> {
    Ok(())
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
