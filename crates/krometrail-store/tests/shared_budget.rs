//! Multiple instances inside one total budget.
//!
//! The policy is an equal division: with `N` live instances each one enforces
//! `total / N` at every write. `N` comes from the instance lock files processes
//! already hold, so it is exact when it is read — there is no published usage, no
//! staleness window, and no failure path that could grant more than a share.

use std::{fs, path::PathBuf, sync::Arc, time::Duration};

use krometrail_core::{
    CaptureOrdinal, CapturedFrame, DeviceScaleFactor, DiskBudgetBytes, EncodedFrame, FrameId,
    ImageFormat, ObservedTime, PixelDimensions, RecordingSink, RetentionLifecycle, SessionId,
    SessionTime, TargetId,
};
use krometrail_store::{
    IndexStoreConfig, InstanceCensus, InstanceOwnership, OWNERSHIP_IS_ENFORCED, RecordingStore,
    RotationConfig, SegmentStoreConfig, SegmentWriter, SqliteIndex,
};
use tempfile::TempDir;
use uuid::Uuid;

/// Bytes an instance may hold beyond its accounted share.
///
/// Enforcement bounds the *accounted* usage, while these tests weigh the whole
/// instance root on disk. The two differ by SQLite bookkeeping — an
/// un-checkpointed WAL, page slack — which is small and bounded but not zero.
/// Every assertion below is far coarser than this allowance, so it never decides
/// a result.
const ACCOUNTING_SLACK: u64 = 512 * 1024;

fn frame(session: u128, target: u128, id: u128, ordinal: u64, bytes: usize) -> EncodedFrame {
    EncodedFrame::new(
        CapturedFrame::new(
            FrameId::from_uuid(Uuid::from_u128(id)),
            SessionId::from_uuid(Uuid::from_u128(session)),
            TargetId::from_uuid(Uuid::from_u128(target)),
            CaptureOrdinal::new(ordinal).unwrap(),
            None,
            ObservedTime::from_nanos(ordinal),
            SessionTime::from_nanos(ordinal),
            ImageFormat::Jpeg,
            PixelDimensions::new(1, 1).unwrap(),
            PixelDimensions::new(1, 1).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap(),
        vec![7; bytes],
    )
    .unwrap()
}

/// One instance: its own root, its own store, and a share of the total budget.
struct Instance {
    root: PathBuf,
    /// Held only when this instance has no census to hold it.
    ///
    /// A shared instance moves its ownership into its census, which is what
    /// keeps the lock alive for as long as the store enforces against it. An
    /// unshared instance has no census, so the lock has nowhere else to live.
    _ownership: Option<InstanceOwnership>,
    store: RecordingStore,
}

/// Opens an instance. `shared` decides whether it divides the total with its
/// peers or enforces the configured budget alone.
fn open_instance(data: &TempDir, total_budget: u64, shared: bool) -> Instance {
    let ownership = InstanceOwnership::acquire_new(data.path()).unwrap();
    let root = ownership.root().to_path_buf();
    let segments = root.join("segments");
    let index = Arc::new(
        SqliteIndex::open(IndexStoreConfig {
            database_path: root.join("index.sqlite3"),
            segments_directory: segments.clone(),
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap(),
    );
    let writer = Arc::new(
        SegmentWriter::open(SegmentStoreConfig {
            directory: segments,
            rotation: RotationConfig {
                max_duration: Duration::from_secs(60),
                max_size: 1,
            },
        })
        .unwrap(),
    );
    // A shared instance's lock lives in its census; an unshared one has no
    // census, so the instance keeps the lock itself.
    let (census, retained) = if shared {
        (Some(InstanceCensus::new(data.path(), ownership)), None)
    } else {
        (None, Some(ownership))
    };
    let store = RecordingStore::with_retention(
        writer,
        index,
        RetentionLifecycle::new(
            DiskBudgetBytes::new(total_budget).unwrap(),
            None,
            85,
            Duration::ZERO,
        )
        .unwrap(),
        census,
    )
    .unwrap();
    Instance {
        root,
        _ownership: retained,
        store,
    }
}

async fn fill(instance: &Instance, session: u128, frames: u64, bytes: usize) {
    for ordinal in 1..=frames {
        if !append_one(instance, session, ordinal, bytes).await {
            break;
        }
    }
}

/// Appends and flushes one frame, reporting whether the instance accepted it.
async fn append_one(instance: &Instance, session: u128, ordinal: u64, bytes: usize) -> bool {
    let value = frame(
        session,
        session + 1_000,
        session * 1_000 + u128::from(ordinal),
        ordinal,
        bytes,
    );
    // Budget exhaustion is an expected outcome once the share is consumed.
    if instance.store.append_frame(value.clone()).await.is_err() {
        return false;
    }
    let _ = instance.store.flush(value.metadata().session_id()).await;
    true
}

/// Grows every instance at the same time, one frame each per round.
///
/// This is the shape the shared budget has to survive: no instance reaches its
/// ceiling before the others start, so each one is deciding while its peers are
/// growing. Filling instances one after another cannot observe that.
async fn fill_concurrently(instances: &[(&Instance, u128)], frames: u64, bytes: usize) {
    let mut active: Vec<bool> = vec![true; instances.len()];
    for ordinal in 1..=frames {
        for (slot, (instance, session)) in instances.iter().enumerate() {
            if !active[slot] {
                continue;
            }
            active[slot] = append_one(instance, *session, ordinal, bytes).await;
        }
        if !active.iter().any(|live| *live) {
            break;
        }
    }
}

/// Bytes an instance actually occupies on disk.
///
/// Deliberately not `RetentionStatus`: a store mid-session holds open segments
/// that the status surface refuses to summarise, and the question these tests ask
/// — how much disk do these processes jointly hold right now — is answered by the
/// filesystem, not by the accounting rows.
fn instance_root_bytes(path: &std::path::Path) -> u64 {
    let mut total = 0;
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let metadata = entry.metadata().expect("instance root entry is readable");
        total += if metadata.is_dir() {
            instance_root_bytes(&entry.path())
        } else {
            metadata.len()
        };
    }
    total
}

/// A store nobody shares with gets the configured budget in full: `total / 1`.
#[tokio::test]
async fn a_lone_instance_gets_the_whole_total() {
    let data = TempDir::new().unwrap();
    let total = 4_000_000_u64;
    let alone = open_instance(&data, total, true);

    assert_eq!(alone.store.live_instances(), 1);

    fill(&alone, 1, 200, 40_000).await;
    let used = instance_root_bytes(&alone.root);
    assert!(
        used > total / 2,
        "a lone instance must be able to use more than an equal share of two: \
         used {used} of {total}"
    );
    assert!(
        used <= total + ACCOUNTING_SLACK,
        "a lone instance must still stay inside the configured total: used {used} of {total}"
    );
}

/// The guarantee: with two live instances each enforces `total / 2` at every
/// write, so neither can exceed its share whatever its peer is doing and the
/// combined footprint stays inside the total.
#[tokio::test]
async fn two_live_instances_each_enforce_half_the_total() {
    if !OWNERSHIP_IS_ENFORCED {
        return;
    }
    let total = 4_000_000_u64;
    let share = total / 2;

    let data = TempDir::new().unwrap();
    let first = open_instance(&data, total, true);
    let second = open_instance(&data, total, true);

    assert_eq!(first.store.live_instances(), 2);

    // The peer fills first, so the second instance decides against a peer that is
    // already holding everything it is entitled to. Under a usage-sharing policy
    // this is the case that produced overshoot; under equal division the peer's
    // usage is not an input at all.
    fill(&first, 1, 200, 40_000).await;
    fill(&second, 2, 200, 40_000).await;

    let first_bytes = instance_root_bytes(&first.root);
    let second_bytes = instance_root_bytes(&second.root);
    assert!(
        first_bytes <= share + ACCOUNTING_SLACK,
        "an instance exceeded its share: {first_bytes} of {share}"
    );
    assert!(
        second_bytes <= share + ACCOUNTING_SLACK,
        "an instance exceeded its share: {second_bytes} of {share}"
    );
    assert!(
        first_bytes + second_bytes <= total + 2 * ACCOUNTING_SLACK,
        "combined usage {} exceeded the total budget {total}",
        first_bytes + second_bytes
    );
}

/// The same guarantee under concurrent growth: neither instance ever reaches a
/// settled view of the other, because there is no view of the other to reach.
#[tokio::test]
async fn concurrently_growing_instances_stay_inside_one_total() {
    if !OWNERSHIP_IS_ENFORCED {
        return;
    }
    let total = 4_000_000_u64;

    let data = TempDir::new().unwrap();
    let first = open_instance(&data, total, true);
    let second = open_instance(&data, total, true);
    fill_concurrently(&[(&first, 1), (&second, 2)], 200, 40_000).await;

    let combined = instance_root_bytes(&first.root) + instance_root_bytes(&second.root);
    assert!(
        combined <= total + 2 * ACCOUNTING_SLACK,
        "combined usage {combined} exceeded the total budget {total}"
    );

    // Without shared accounting each instance would fill the whole budget, so the
    // bound above is a real constraint rather than an artefact of the write
    // volume.
    let isolated = TempDir::new().unwrap();
    let alone = open_instance(&isolated, total, false);
    let other = open_instance(&isolated, total, false);
    fill_concurrently(&[(&alone, 1), (&other, 2)], 200, 40_000).await;
    let unshared = instance_root_bytes(&alone.root) + instance_root_bytes(&other.root);
    assert!(
        combined < unshared,
        "shared accounting must bound the combined footprint: shared {combined} \
         unshared {unshared}"
    );
}

/// A frame carries no size limit of its own, so a single write can be larger than
/// a share. Under the old usage ledger such a frame escaped the bound: the share
/// was computed before the append and the ledger only ever heard what an instance
/// held *beforehand*. Equal division has no before-and-after to disagree on — the
/// share is a constant and the write is checked against it.
#[tokio::test]
async fn one_oversized_frame_cannot_escape_a_share() {
    if !OWNERSHIP_IS_ENFORCED {
        return;
    }
    let total = 8_000_000_u64;
    // Larger than an equal share of two, so admitting it would be unmistakable.
    let oversized = 6_000_000_usize;

    let data = TempDir::new().unwrap();
    let first = open_instance(&data, total, true);
    let second = open_instance(&data, total, true);

    for (slot, (instance, session)) in [(&first, 1_u128), (&second, 2_u128)].iter().enumerate() {
        let value = frame(
            *session,
            session + 1_000,
            session * 1_000 + 1,
            u64::try_from(slot).unwrap() + 1,
            oversized,
        );
        // No flush: the append path alone must decide, exactly as a live session
        // does. A frame that does not fit the share is refused, which is the
        // accepted cost of a share that does not depend on what peers hold.
        assert!(
            instance.store.append_frame(value).await.is_err(),
            "a frame larger than this instance's share must be refused"
        );
    }

    let combined = instance_root_bytes(&first.root) + instance_root_bytes(&second.root);
    assert!(
        combined <= total + 2 * ACCOUNTING_SLACK,
        "one oversized frame per instance pushed the combined footprint to {combined}, \
         past the total {total}"
    );

    // A write that does fit the share is still admitted, so the refusal above is
    // the size check and not a store that refuses everything.
    assert!(
        append_one(&first, 1, 10, 3_000_000).await,
        "a frame inside this instance's share must be admitted"
    );
}

/// Two is the smallest sharing case and the easiest one to get right by accident.
/// Three is the shape that broke the usage ledger: a sequential join order where
/// each instance decided against a different, later-invalidated view of the
/// others. Equal division has no view to invalidate — every instance enforces
/// `total / 3` from the count it reads at the write.
#[tokio::test]
async fn three_live_instances_each_enforce_a_third_of_the_total() {
    if !OWNERSHIP_IS_ENFORCED {
        return;
    }
    let total = 6_000_000_u64;
    let share = total / 3;

    let data = TempDir::new().unwrap();
    let first = open_instance(&data, total, true);
    let second = open_instance(&data, total, true);
    let third = open_instance(&data, total, true);

    assert_eq!(first.store.live_instances(), 3);

    // Sequential filling, which is what produced the ledger's overshoot: each
    // instance runs to its ceiling before the next one starts writing.
    fill(&first, 1, 300, 40_000).await;
    fill(&second, 2, 300, 40_000).await;
    fill(&third, 3, 300, 40_000).await;

    let roots = [
        instance_root_bytes(&first.root),
        instance_root_bytes(&second.root),
        instance_root_bytes(&third.root),
    ];
    for used in roots {
        assert!(
            used <= share + ACCOUNTING_SLACK,
            "an instance exceeded a third of the total: {used} of {share}"
        );
    }
    let combined: u64 = roots.iter().sum();
    assert!(
        combined <= total + 3 * ACCOUNTING_SLACK,
        "combined usage {combined} exceeded the total budget {total}"
    );
}

/// Every other test here flushes after each append, so a durability boundary sits
/// between one write and the next. Nothing in the guarantee depends on that: the
/// share is checked on the append path itself, before any segment is sealed. An
/// instance that never flushes must still stop at its share.
#[tokio::test]
async fn an_instance_cannot_exceed_its_share_without_flushing() {
    if !OWNERSHIP_IS_ENFORCED {
        return;
    }
    let total = 4_000_000_u64;
    let share = total / 2;
    let bytes = 40_000_usize;
    let attempts = 300_u64;

    let data = TempDir::new().unwrap();
    let first = open_instance(&data, total, true);
    let second = open_instance(&data, total, true);

    // Offered far more than the share, with no flush anywhere in the loop. The
    // append path is the only thing standing between this and the whole total.
    let offered = attempts * u64::try_from(bytes).unwrap();
    assert!(
        offered > 2 * share,
        "the test must overrun the share to test it"
    );
    for ordinal in 1..=attempts {
        let value = frame(1, 1_001, 1_000 + u128::from(ordinal), ordinal, bytes);
        if first.store.append_frame(value).await.is_err() {
            break;
        }
    }

    let used = instance_root_bytes(&first.root);
    assert!(
        used <= share + ACCOUNTING_SLACK,
        "an unflushed instance holds {used} against a {share} share"
    );
    // It really did write: the bound above is enforcement, not an empty store.
    assert!(
        used > share / 4,
        "an unflushed instance recorded almost nothing ({used} bytes), so the \
         bound above proves nothing"
    );

    drop(second);
}

/// Counting is coordination, and coordination that fails must fail closed.
///
/// This is the defect class the usage ledger died of: every optimistic grant it
/// issued — write failed, lock contended — was a grant made because coordination
/// had broken. A census that answered "one" when it could not enumerate would be
/// the same bug in a smaller machine, handing each of two live instances the full
/// total. The census instead reuses the highest live count it has already proved,
/// which can only ever narrow a share.
#[cfg(unix)]
#[tokio::test]
async fn a_failed_census_does_not_widen_a_share() {
    use std::os::unix::fs::PermissionsExt;

    if !OWNERSHIP_IS_ENFORCED {
        return;
    }
    let total = 4_000_000_u64;
    let share = total / 2;

    let data = TempDir::new().unwrap();
    let first = open_instance(&data, total, true);
    let second = open_instance(&data, total, true);

    assert_eq!(first.store.live_instances(), 2);

    // One operation with the peer already present, so the instance's own census
    // has proved a live count of two before enumeration is taken away.
    assert!(append_one(&first, 1, 1, 40_000).await);

    // Break enumeration without making the roots unreachable: execute-only still
    // permits traversal into each instance root, but denies the directory read the
    // census depends on.
    let instances = data.path().join("instances");
    let restore = fs::metadata(&instances).unwrap().permissions();
    fs::set_permissions(&instances, fs::Permissions::from_mode(0o111)).unwrap();
    if fs::read_dir(&instances).is_ok() {
        // A process that bypasses directory permissions (root) cannot have this
        // fault injected at all.
        fs::set_permissions(&instances, restore).unwrap();
        return;
    }

    assert_eq!(
        first.store.live_instances(),
        2,
        "a census that cannot enumerate must not report fewer live instances than it has proved"
    );

    fill(&first, 1, 300, 40_000).await;
    let used = instance_root_bytes(&first.root);
    fs::set_permissions(&instances, restore).unwrap();

    assert!(
        used <= share + ACCOUNTING_SLACK,
        "a failed census widened an instance's share: it holds {used} against a {share} share"
    );

    drop(second);
}

/// The census enumerates through a descriptor opened once, so a later permission
/// change on the instances directory cannot blind it.
///
/// The discriminator is deliberate: the peer exits *while* a path-based
/// `read_dir` would fail. A census that had fallen back to its proved floor would
/// answer `2`, the count it last proved. Only a census that actually enumerated
/// can see the departure and answer `1`. Passing this is therefore proof the
/// retained handle read the directory, not proof that the fallback is safe.
///
/// Skipped under root: a root process reads a `0o111` directory by path anyway,
/// so the fault cannot be injected and the assertion would prove nothing.
#[cfg(unix)]
#[tokio::test]
async fn a_retained_directory_handle_enumerates_after_permissions_change() {
    use std::os::unix::fs::PermissionsExt;

    if !OWNERSHIP_IS_ENFORCED {
        return;
    }
    let data = TempDir::new().unwrap();
    let total = 4_000_000_u64;

    let first = open_instance(&data, total, true);
    let second = open_instance(&data, total, true);

    // Constructed while the directory is still readable, which is when the
    // descriptor is opened and the access check happens.
    assert_eq!(first.store.live_instances(), 2);

    let instances = data.path().join("instances");
    let restore = fs::metadata(&instances).unwrap().permissions();
    fs::set_permissions(&instances, fs::Permissions::from_mode(0o111)).unwrap();
    if fs::read_dir(&instances).is_ok() {
        fs::set_permissions(&instances, restore).unwrap();
        return;
    }

    // Execute-only still permits traversal into each root, so releasing the
    // peer's lock is observable to anything that can list the directory.
    drop(second);

    let live = first.store.live_instances();
    fs::set_permissions(&instances, restore).unwrap();
    assert_eq!(
        live, 1,
        "the retained descriptor must keep enumerating after the directory is made \
         unreadable by path; a census reporting 2 fell back to its proved floor instead"
    );
}

/// An instance that has *never* seen the instances directory knows nothing about
/// its peers, and must not conclude it is alone.
///
/// This is the hole a monotonic floor cannot cover. The floor starts at this
/// instance's own `1`, so a census whose very first enumeration fails would hand
/// out `total / 1` — the whole budget — to every instance that started that way.
/// Two such instances would jointly hold twice the total. Fail closed instead: no
/// evidence means a conservative assumed peer count, which still leaves a usable
/// non-zero share.
///
/// Skipped under root: a root process opens and reads a `0o111` directory, so the
/// fault cannot be injected.
#[cfg(unix)]
#[tokio::test]
async fn a_census_that_never_enumerated_does_not_grant_the_whole_total() {
    use std::os::unix::fs::PermissionsExt;

    if !OWNERSHIP_IS_ENFORCED {
        return;
    }
    let data = TempDir::new().unwrap();
    let total = 4_000_000_u64;

    let first = open_instance(&data, total, true);
    let second = open_instance(&data, total, true);

    // Claimed while the directory is still writable, because the census under
    // test is built after it is not. A census owns its instance root, and no
    // root can be created once `instances/` is execute-only.
    let observer = InstanceOwnership::acquire_new(data.path()).unwrap();

    let instances = data.path().join("instances");
    let restore = fs::metadata(&instances).unwrap().permissions();
    fs::set_permissions(&instances, fs::Permissions::from_mode(0o111)).unwrap();
    if fs::read_dir(&instances).is_ok() {
        fs::set_permissions(&instances, restore).unwrap();
        return;
    }

    // Constructed only now: there is no descriptor to retain and no successful
    // enumeration anywhere in this census's history.
    let census = InstanceCensus::new(data.path(), observer);
    let live = census.live_instances();
    fs::set_permissions(&instances, restore).unwrap();

    assert!(
        live > 1,
        "a census with no evidence about its peers reported {live} live instances, \
         which grants this instance the whole {total}-byte total"
    );

    drop(second);
}

/// A root whose process exited is not live, so it must not divide the budget. The
/// survivor regains the whole total on its next operation.
#[tokio::test]
async fn a_dead_instance_root_does_not_count_toward_the_live_set() {
    if !OWNERSHIP_IS_ENFORCED {
        return;
    }
    let data = TempDir::new().unwrap();
    let total = 4_000_000_u64;

    let departed = open_instance(&data, total, true);
    fill(&departed, 1, 20, 40_000).await;
    assert!(instance_root_bytes(&departed.root) > 0);

    let survivor = open_instance(&data, total, true);
    assert_eq!(survivor.store.live_instances(), 2);

    // The departed process exits. Its data is still on disk — reclaiming it is a
    // separate tier — but it no longer holds a lock, so it no longer holds a
    // share.
    drop(departed);
    assert_eq!(survivor.store.live_instances(), 1);

    fill(&survivor, 2, 200, 40_000).await;
    let used = instance_root_bytes(&survivor.root);
    assert!(
        used > total / 2,
        "a lone survivor must regain the whole budget: used {used} of {total}"
    );
}

/// Reclaim is operation-driven, so an instance that grew while it was alone stays
/// over its new share until it does work again — and then trims down to it.
#[tokio::test]
async fn an_instance_that_grew_alone_trims_on_its_next_operation() {
    if !OWNERSHIP_IS_ENFORCED {
        return;
    }
    let data = TempDir::new().unwrap();
    let total = 4_000_000_u64;

    let first = open_instance(&data, total, true);
    fill(&first, 1, 200, 40_000).await;
    let grown = instance_root_bytes(&first.root);
    assert!(
        grown > total / 2,
        "the instance must first grow past what an equal share of two allows: \
         {grown} of {total}"
    );

    // A peer joins. Nothing happens to the first instance's disk until it acts:
    // there is no background trimmer.
    let second = open_instance(&data, total, true);
    assert_eq!(first.store.live_instances(), 2);
    assert_eq!(
        instance_root_bytes(&first.root),
        grown,
        "an idle instance must not shrink on its own; reclaim is operation-driven"
    );

    // Its next append is judged against the new share, and the in-session trim on
    // that same path reclaims down toward it.
    assert!(append_one(&first, 1, 500, 40_000).await);
    let trimmed = instance_root_bytes(&first.root);
    assert!(
        trimmed <= total / 2 + ACCOUNTING_SLACK,
        "the first operation after a peer joined must trim toward the new share: \
         {trimmed} of {}",
        total / 2
    );

    drop(second);
}
