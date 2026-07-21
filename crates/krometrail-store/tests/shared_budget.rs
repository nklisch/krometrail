//! Multiple instances inside one total budget.
//!
//! The policy is an equal division: with `N` live instances each one enforces
//! `total / N` at every write. `N` comes from the instance lock files processes
//! already hold, so it is exact when it is read — there is no published usage, no
//! staleness window, and no failure path that could grant more than a share.

use std::{fs, sync::Arc, time::Duration};

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
    _ownership: InstanceOwnership,
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
        shared.then(|| InstanceCensus::new(data.path(), &root)),
    )
    .unwrap();
    Instance {
        _ownership: ownership,
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

    assert_eq!(
        InstanceCensus::new(data.path(), alone._ownership.root()).live_instances(),
        1
    );

    fill(&alone, 1, 200, 40_000).await;
    let used = instance_root_bytes(alone._ownership.root());
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

    assert_eq!(
        InstanceCensus::new(data.path(), first._ownership.root()).live_instances(),
        2
    );

    // The peer fills first, so the second instance decides against a peer that is
    // already holding everything it is entitled to. Under a usage-sharing policy
    // this is the case that produced overshoot; under equal division the peer's
    // usage is not an input at all.
    fill(&first, 1, 200, 40_000).await;
    fill(&second, 2, 200, 40_000).await;

    let first_bytes = instance_root_bytes(first._ownership.root());
    let second_bytes = instance_root_bytes(second._ownership.root());
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

    let combined = instance_root_bytes(first._ownership.root())
        + instance_root_bytes(second._ownership.root());
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
    let unshared =
        instance_root_bytes(alone._ownership.root()) + instance_root_bytes(other._ownership.root());
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

    let combined = instance_root_bytes(first._ownership.root())
        + instance_root_bytes(second._ownership.root());
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
    assert!(instance_root_bytes(departed._ownership.root()) > 0);

    let survivor = open_instance(&data, total, true);
    let census = InstanceCensus::new(data.path(), survivor._ownership.root());
    assert_eq!(census.live_instances(), 2);

    // The departed process exits. Its data is still on disk — reclaiming it is a
    // separate tier — but it no longer holds a lock, so it no longer holds a
    // share.
    drop(departed);
    assert_eq!(census.live_instances(), 1);

    fill(&survivor, 2, 200, 40_000).await;
    let used = instance_root_bytes(survivor._ownership.root());
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
    let grown = instance_root_bytes(first._ownership.root());
    assert!(
        grown > total / 2,
        "the instance must first grow past what an equal share of two allows: \
         {grown} of {total}"
    );

    // A peer joins. Nothing happens to the first instance's disk until it acts:
    // there is no background trimmer.
    let second = open_instance(&data, total, true);
    assert_eq!(
        InstanceCensus::new(data.path(), first._ownership.root()).live_instances(),
        2
    );
    assert_eq!(
        instance_root_bytes(first._ownership.root()),
        grown,
        "an idle instance must not shrink on its own; reclaim is operation-driven"
    );

    // Its next append is judged against the new share, and the in-session trim on
    // that same path reclaims down toward it.
    assert!(append_one(&first, 1, 500, 40_000).await);
    let trimmed = instance_root_bytes(first._ownership.root());
    assert!(
        trimmed <= total / 2 + ACCOUNTING_SLACK,
        "the first operation after a peer joined must trim toward the new share: \
         {trimmed} of {}",
        total / 2
    );

    drop(second);
}
