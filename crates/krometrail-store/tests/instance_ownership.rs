use std::fs;

use krometrail_store::{
    InstanceOwnership, clear_legacy_flat_store, has_legacy_flat_store, reclaim_instance_root,
    sibling_instance_roots,
};
use tempfile::TempDir;

/// Builds a pre-isolation flat data directory: recording cache alongside the
/// operator-owned state that shares the same directory.
fn legacy_data_directory() -> TempDir {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    fs::write(root.join("index.sqlite3"), b"legacy index").unwrap();
    fs::write(root.join("index.sqlite3-wal"), b"legacy wal").unwrap();
    fs::create_dir_all(root.join("segments")).unwrap();
    fs::write(root.join("segments/a.kts"), b"legacy segment").unwrap();
    fs::create_dir_all(root.join("artifacts")).unwrap();
    fs::write(root.join("artifacts/a.png"), b"legacy artifact").unwrap();
    fs::create_dir_all(root.join(".trash/batch")).unwrap();
    fs::write(root.join(".trash/batch/staged"), b"legacy staged").unwrap();

    // Everything below is NOT recording cache and must survive.
    fs::create_dir_all(root.join("browser-profiles/default")).unwrap();
    fs::write(root.join("browser-profiles/default/Cookies"), b"profile").unwrap();
    fs::create_dir_all(root.join("diagnostics")).unwrap();
    fs::write(root.join("diagnostics/krometrail.log"), b"log").unwrap();
    fs::create_dir_all(root.join("browser-downloads")).unwrap();
    fs::write(root.join("browser-downloads/report.pdf"), b"download").unwrap();
    fs::create_dir_all(root.join("plugin")).unwrap();
    fs::write(root.join("plugin/manifest.json"), b"plugin").unwrap();
    fs::write(root.join("config.toml"), b"config").unwrap();
    directory
}

/// The legacy clear is the single operation most able to destroy irreplaceable
/// user state: managed browser profiles, diagnostics, downloads, plugin state,
/// and configuration all live in the same directory as the recording cache.
/// Scoped wrongly it would take a signed-in browser profile with it.
#[test]
fn clearing_the_legacy_flat_store_preserves_everything_that_is_not_recording_cache() {
    let directory = legacy_data_directory();
    let root = directory.path();
    assert!(has_legacy_flat_store(root));

    let reclaimed = clear_legacy_flat_store(root).unwrap();

    // Recording cache is gone.
    assert!(!root.join("index.sqlite3").exists());
    assert!(!root.join("index.sqlite3-wal").exists());
    assert!(!root.join("segments").exists());
    assert!(!root.join("artifacts").exists());
    assert!(!root.join(".trash").exists());
    assert!(!has_legacy_flat_store(root));
    assert!(reclaimed > 0, "reclaimed byte count should be reported");

    // Operator-owned state survives, byte for byte.
    assert_eq!(
        fs::read(root.join("browser-profiles/default/Cookies")).unwrap(),
        b"profile"
    );
    assert_eq!(
        fs::read(root.join("diagnostics/krometrail.log")).unwrap(),
        b"log"
    );
    assert_eq!(
        fs::read(root.join("browser-downloads/report.pdf")).unwrap(),
        b"download"
    );
    assert_eq!(
        fs::read(root.join("plugin/manifest.json")).unwrap(),
        b"plugin"
    );
    assert_eq!(fs::read(root.join("config.toml")).unwrap(), b"config");
}

#[test]
fn a_second_instance_gets_its_own_root_and_cannot_take_the_first() {
    let directory = TempDir::new().unwrap();
    let first = InstanceOwnership::acquire_new(directory.path()).unwrap();
    let second = InstanceOwnership::acquire_new(directory.path()).unwrap();
    assert_ne!(first.root(), second.root());

    // A live root cannot be taken over, which is what makes reclamation safe.
    assert!(
        InstanceOwnership::acquire_existing(first.root())
            .unwrap()
            .is_none()
    );
}

#[test]
fn releasing_an_instance_makes_its_root_reclaimable() {
    let directory = TempDir::new().unwrap();
    let owner = InstanceOwnership::acquire_new(directory.path()).unwrap();
    let abandoned = owner.root().to_path_buf();
    fs::create_dir_all(abandoned.join("segments")).unwrap();
    fs::write(abandoned.join("segments/a.kts"), vec![7; 4_096]).unwrap();
    fs::write(abandoned.join("index.sqlite3"), vec![7; 1_024]).unwrap();

    // While the owner is alive the root is not reclaimable.
    assert!(
        InstanceOwnership::acquire_existing(&abandoned)
            .unwrap()
            .is_none()
    );
    drop(owner);

    let live = InstanceOwnership::acquire_new(directory.path()).unwrap();
    let siblings = sibling_instance_roots(directory.path(), live.root()).unwrap();
    assert_eq!(siblings, vec![abandoned.clone()]);

    let claimed = InstanceOwnership::acquire_existing(&abandoned)
        .unwrap()
        .expect("an abandoned root is claimable");
    let reclaimed = reclaim_instance_root(&claimed).unwrap();

    assert_eq!(reclaimed, 4_096 + 1_024);
    assert!(!abandoned.join("segments").exists());
    assert!(!abandoned.join("index.sqlite3").exists());
}

/// Reclamation is allowlist-scoped, so an unexpected member keeps its root alive
/// rather than being swept away with the recording cache.
#[test]
fn reclamation_leaves_unrecognised_members_untouched() {
    let directory = TempDir::new().unwrap();
    let owner = InstanceOwnership::acquire_new(directory.path()).unwrap();
    let root = owner.root().to_path_buf();
    fs::create_dir_all(root.join("segments")).unwrap();
    fs::write(root.join("segments/a.kts"), b"segment").unwrap();
    fs::write(root.join("operator-notes.txt"), b"keep me").unwrap();

    reclaim_instance_root(&owner).unwrap();

    assert!(!root.join("segments").exists());
    assert_eq!(
        fs::read(root.join("operator-notes.txt")).unwrap(),
        b"keep me"
    );
}

#[test]
fn sibling_scan_ignores_non_instance_entries() {
    let directory = TempDir::new().unwrap();
    let live = InstanceOwnership::acquire_new(directory.path()).unwrap();
    let instances = directory.path().join("instances");
    fs::create_dir_all(instances.join("not-a-uuid")).unwrap();
    fs::write(instances.join("stray-file"), b"stray").unwrap();

    assert!(
        sibling_instance_roots(directory.path(), live.root())
            .unwrap()
            .is_empty()
    );
}
