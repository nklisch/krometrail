use std::fs;

use krometrail_store::{
    InstanceOwnership, OWNERSHIP_IS_ENFORCED, clear_legacy_flat_store, has_legacy_flat_store,
    reclaim_instance_root, sibling_instance_roots,
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
    assert_eq!(
        siblings
            .iter()
            .map(|candidate| candidate.path().to_path_buf())
            .collect::<Vec<_>>(),
        vec![abandoned.clone()]
    );

    let claimed = siblings[0]
        .claim()
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

/// Reclamation deletes, so it may only ever run where ownership can be *proved*.
///
/// On Unix the proof is `flock`. Krometrail also ships a Windows binary, and
/// nothing in the standard library expresses a deny-share open there — so the
/// answer to "is this root abandoned?" must be "cannot tell", which reads as
/// "leave it alone". This test states that contract on whichever platform it
/// runs: a released root is claimable exactly where ownership is enforced, and
/// nowhere else.
#[test]
fn a_root_is_only_ever_claimable_where_ownership_is_provable() {
    let directory = TempDir::new().unwrap();
    let owner = InstanceOwnership::acquire_new(directory.path()).unwrap();
    let root = owner.root().to_path_buf();
    fs::write(root.join("index.sqlite3"), b"evidence").unwrap();
    drop(owner);

    let claimable = InstanceOwnership::acquire_existing(&root)
        .unwrap()
        .is_some();
    assert_eq!(
        claimable, OWNERSHIP_IS_ENFORCED,
        "a platform that cannot prove a root is dead must never report it as reclaimable"
    );

    let live = InstanceOwnership::acquire_new(directory.path()).unwrap();
    let siblings = sibling_instance_roots(directory.path(), live.root()).unwrap();
    assert_eq!(
        siblings.is_empty(),
        !OWNERSHIP_IS_ENFORCED,
        "unprovable ownership must yield no reclaimable siblings at all"
    );
}

/// Discovery classifies a root; reclamation deletes from it some time later.
/// A path is not a safe handle across that gap — it can be made to name a
/// different directory in between, and the allowlist does not care which
/// directory the allowlisted names are resolved in.
#[cfg(unix)]
#[test]
fn a_root_swapped_after_discovery_is_never_reclaimed() {
    for swap_to_symlink in [true, false] {
        let directory = TempDir::new().unwrap();
        let departed = InstanceOwnership::acquire_new(directory.path()).unwrap();
        let abandoned = departed.root().to_path_buf();
        fs::write(abandoned.join("index.sqlite3"), b"abandoned index").unwrap();
        drop(departed);

        let live = InstanceOwnership::acquire_new(directory.path()).unwrap();
        let candidates = sibling_instance_roots(directory.path(), live.root()).unwrap();
        assert_eq!(candidates.len(), 1);

        // The swap, after classification and before the claim.
        let decoy = directory.path().join("decoy");
        fs::create_dir_all(&decoy).unwrap();
        fs::write(decoy.join("index.sqlite3"), b"someone else's evidence").unwrap();
        fs::rename(&abandoned, directory.path().join("moved-aside")).unwrap();
        if swap_to_symlink {
            std::os::unix::fs::symlink(&decoy, &abandoned).unwrap();
        } else {
            fs::create_dir_all(&abandoned).unwrap();
        }

        assert!(
            candidates[0].claim().unwrap().is_none(),
            "a root that changed identity after discovery must not be claimable"
        );
        assert_eq!(
            fs::read(decoy.join("index.sqlite3")).unwrap(),
            b"someone else's evidence"
        );
    }
}

/// The same defence, one step later: an ownership already held must stop
/// deleting the moment its root path stops naming the directory it locked.
#[cfg(unix)]
#[test]
fn reclamation_refuses_a_root_that_changes_identity_under_the_lock() {
    let directory = TempDir::new().unwrap();
    let owner = InstanceOwnership::acquire_new(directory.path()).unwrap();
    let root = owner.root().to_path_buf();
    fs::create_dir_all(root.join("segments")).unwrap();
    fs::write(root.join("segments/a.kts"), b"segment").unwrap();

    fs::rename(&root, directory.path().join("moved-aside")).unwrap();
    let decoy = root;
    fs::create_dir_all(decoy.join("segments")).unwrap();
    fs::write(decoy.join("segments/a.kts"), b"someone else's segment").unwrap();
    fs::write(decoy.join("index.sqlite3"), b"someone else's index").unwrap();

    let error = reclaim_instance_root(&owner).unwrap_err();
    assert!(error.message.as_str().contains("changed identity"));
    assert_eq!(
        fs::read(decoy.join("segments/a.kts")).unwrap(),
        b"someone else's segment"
    );
    assert!(decoy.join("index.sqlite3").is_file());
}
