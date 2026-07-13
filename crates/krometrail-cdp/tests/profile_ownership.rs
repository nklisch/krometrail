use krometrail_cdp::{ProfileError, ProfileLease, ProfileLeaseKind};
use krometrail_core::{ManagedProfile, ProfileIdentity};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "krometrail-profile-integration-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn reusable_profiles_are_exclusive_and_retained() {
    let root = root();
    let request = ManagedProfile::Reusable {
        name: ProfileIdentity::new("integration").unwrap(),
    };
    let lease = ProfileLease::acquire(&root, &request).unwrap();
    assert_eq!(lease.kind(), ProfileLeaseKind::Reusable);
    assert!(matches!(
        ProfileLease::acquire(&root, &request),
        Err(ProfileError::InUse)
    ));
    let profile_path = lease.path().to_owned();
    drop(lease);
    assert!(profile_path.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn temporary_cleanup_is_owned_and_attach_has_no_profile_api() {
    let root = root();
    let lease = ProfileLease::acquire(&root, &ManagedProfile::Temporary).unwrap();
    let profile_path = lease.path().to_owned();
    drop(lease);
    assert!(!profile_path.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn profile_names_cannot_escape_the_managed_root() {
    let root = root();
    for name in ["../escape", "a/b", "a\\b", "."] {
        let request = ManagedProfile::Reusable {
            name: ProfileIdentity::new(name).unwrap(),
        };
        assert!(matches!(
            ProfileLease::acquire(&root, &request),
            Err(ProfileError::InvalidName)
        ));
    }
    let _ = fs::remove_dir_all(root);
}
