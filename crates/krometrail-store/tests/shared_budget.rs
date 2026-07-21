//! Multiple instances inside one total budget.

use std::{fs, sync::Arc, time::Duration};

use krometrail_core::{
    CaptureOrdinal, CapturedFrame, DeviceScaleFactor, DiskBudgetBytes, EncodedFrame, FrameId,
    ImageFormat, ObservedTime, PixelDimensions, RecordingSink, RetentionLifecycle, RetentionStore,
    SessionId, SessionTime, TargetId,
};
use krometrail_store::{
    BudgetRegistry, IndexStoreConfig, InstanceOwnership, RecordingStore, RotationConfig,
    SegmentStoreConfig, SegmentWriter, SqliteIndex,
};
use tempfile::TempDir;
use uuid::Uuid;

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

fn open_instance(data: &TempDir, total_budget: u64, registry: bool) -> Instance {
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
        registry.then(|| Arc::new(BudgetRegistry::open(data.path(), &root).unwrap())),
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
/// This is the shape the shared budget actually has to survive: no instance
/// reaches its ceiling before the others start, so each one's view of "what my
/// peers are using" is being invalidated by the others as it decides. Filling
/// instances one after another cannot observe that — the first instance sees an
/// empty ledger exactly once and every later instance sees a settled one.
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

async fn usage(instance: &Instance) -> u64 {
    instance
        .store
        .status()
        .await
        .unwrap()
        .usage
        .total_bytes()
        .unwrap()
}

/// The requirement: N instances share one total budget rather than consuming N
/// budgets. Without the registry each instance would independently fill to the
/// whole budget, so the combined footprint would be a multiple of the intent.
#[tokio::test]
async fn concurrent_instances_share_one_total_budget() {
    let total = 2_000_000_u64;

    let shared = TempDir::new().unwrap();
    let first = open_instance(&shared, total, true);
    let second = open_instance(&shared, total, true);
    let third = open_instance(&shared, total, true);
    fill_concurrently(&[(&first, 1), (&second, 2), (&third, 3)], 60, 40_000).await;
    let shared_total = usage(&first).await + usage(&second).await + usage(&third).await;

    let isolated = TempDir::new().unwrap();
    let alone = open_instance(&isolated, total, false);
    let other = open_instance(&isolated, total, false);
    let another = open_instance(&isolated, total, false);
    fill_concurrently(&[(&alone, 1), (&other, 2), (&another, 3)], 60, 40_000).await;
    let unshared_total = usage(&alone).await + usage(&other).await + usage(&another).await;

    assert!(
        shared_total < unshared_total,
        "shared accounting must bound the combined footprint: shared {shared_total} \
         unshared {unshared_total}"
    );
    // The bound the product promises: the combined footprint of concurrently
    // growing instances stays inside one total budget.
    assert!(
        shared_total <= total,
        "combined usage {shared_total} exceeded the total budget {total}"
    );
}

/// A dead instance's bytes must stop counting the moment its root becomes
/// reclaimable, so an exited process cannot permanently hold the total hostage.
#[tokio::test]
async fn a_dead_instance_stops_counting_toward_the_total() {
    let data = TempDir::new().unwrap();
    let total = 1_000_000_u64;

    let departed = open_instance(&data, total, true);
    fill(&departed, 1, 20, 40_000).await;
    let departed_usage = usage(&departed).await;
    assert!(departed_usage > 0);

    let survivor = open_instance(&data, total, true);
    let registry = BudgetRegistry::open(data.path(), survivor._ownership.root()).unwrap();

    let while_alive = registry.publish(0, total).unwrap();
    assert_eq!(while_alive.live_instances, 2);
    assert!(while_alive.other_live_usage > 0);

    drop(departed);

    let after_exit = registry.publish(0, total).unwrap();
    assert_eq!(after_exit.live_instances, 1);
    assert_eq!(after_exit.other_live_usage, 0);
    assert_eq!(
        after_exit.effective_budget, total,
        "a lone survivor should regain the whole budget"
    );
}

/// Degraded accounting must always beat stalled capture.
#[tokio::test]
async fn a_corrupt_registry_degrades_instead_of_blocking_capture() {
    let data = TempDir::new().unwrap();
    let total = 1_000_000_u64;
    let instance = open_instance(&data, total, true);

    fs::write(
        data.path().join("instances/.budget-registry.json"),
        b"{ not json at all",
    )
    .unwrap();

    // Capture continues, and the instance still gets a usable budget.
    let value = frame(9, 90, 900, 1, 40_000);
    instance.store.append_frame(value.clone()).await.unwrap();
    instance
        .store
        .flush(value.metadata().session_id())
        .await
        .unwrap();

    let registry = BudgetRegistry::open(data.path(), instance._ownership.root()).unwrap();
    let share = registry
        .publish(1_000, total)
        .expect("a corrupt ledger is treated as empty, not fatal");
    assert_eq!(share.live_instances, 1);
    assert_eq!(share.effective_budget, total);

    // The next successful transaction repairs the file.
    let repaired = fs::read(data.path().join("instances/.budget-registry.json")).unwrap();
    assert!(serde_json::from_slice::<serde_json::Value>(&repaired).is_ok());
}

/// The registry lives beside instance roots and must not be mistaken for one.
#[tokio::test]
async fn registry_files_are_not_treated_as_instance_roots() {
    let data = TempDir::new().unwrap();
    let instance = open_instance(&data, 1_000_000, true);
    let registry = BudgetRegistry::open(data.path(), instance._ownership.root()).unwrap();
    registry.publish(1_000, 1_000_000).unwrap();

    let siblings =
        krometrail_store::sibling_instance_roots(data.path(), instance._ownership.root()).unwrap();
    assert!(
        siblings.is_empty(),
        "registry bookkeeping files must never be reclaimed as instance roots: {siblings:?}"
    );
}
/// Bytes an instance actually occupies on disk.
///
/// Deliberately not `RetentionStatus`: a store mid-session holds open segments
/// that the status surface refuses to summarise, and the question this test asks
/// — how much disk do these processes jointly hold right now — is answered by
/// the filesystem, not by the accounting rows.
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

/// The overshoot bound: growth between accounting transactions is capped by a
/// fraction of the total, not by the capture write rate.
///
/// Mirrors `BUDGET_SHARE_REFRESH_DIVISOR` in the store.
const SHARE_DRIFT_DIVISOR: u64 = 32;

/// Instances grow without ever reaching a durability boundary.
///
/// This is the shape real capture has: `flush` runs when a session stops, so a
/// live session's only budget check is the append path. If that path reuses a
/// share computed while the ledger still showed this instance empty, two
/// instances that start together each spend the *whole* total before either one
/// tells the other anything. Recorded on the pre-fix build: 3_394_431 bytes
/// against a 2_000_000 total.
#[tokio::test]
async fn instances_that_never_flush_still_share_one_total_budget() {
    let total = 2_000_000_u64;
    let live = 2;
    let data = TempDir::new().unwrap();
    let first = open_instance(&data, total, true);
    let second = open_instance(&data, total, true);

    for ordinal in 1..=100_u64 {
        for (instance, session) in [(&first, 1_u128), (&second, 2_u128)] {
            let value = frame(
                session,
                session + 1_000,
                session * 1_000 + u128::from(ordinal),
                ordinal,
                40_000,
            );
            // No flush: budget pressure must be resolved by the append path alone.
            let _ = instance.store.append_frame(value).await;
        }
    }

    let combined = instance_root_bytes(first._ownership.root())
        + instance_root_bytes(second._ownership.root());
    let bound = total + live * (total / SHARE_DRIFT_DIVISOR);
    assert!(
        combined <= bound,
        "combined footprint {combined} exceeded the total {total} by more than the \
         per-instance drift allowance (bound {bound})"
    );
}

/// One append, one frame, one bound.
///
/// The drift allowance bounds how far an instance may grow *between* accounting
/// transactions, but a frame carries no size limit of its own. If the share is
/// judged before the append without regard for the bytes about to be written,
/// a single frame larger than the allowance escapes the bound entirely — and
/// because the ledger is only told what the instance held *before* the write,
/// two instances starting together each measure themselves against a peer the
/// ledger still reports as empty. Many small frames cannot show this; exactly
/// one oversized frame each can.
#[tokio::test]
async fn one_oversized_frame_cannot_escape_the_shared_bound() {
    let total = 8_000_000_u64;
    let live = 2;
    // Comfortably larger than the drift allowance (total / 32) and larger than
    // an equal share, so admitting it against a stale grant is unmistakable.
    let oversized = 6_000_000_usize;

    let data = TempDir::new().unwrap();
    let first = open_instance(&data, total, true);
    let second = open_instance(&data, total, true);

    let mut admitted = 0;
    for (slot, (instance, session)) in [(&first, 1_u128), (&second, 2_u128)].iter().enumerate() {
        let value = frame(
            *session,
            session + 1_000,
            session * 1_000 + 1,
            u64::try_from(slot).unwrap() + 1,
            oversized,
        );
        // No flush: the append path alone must decide, exactly as a live session does.
        if instance.store.append_frame(value).await.is_ok() {
            admitted += 1;
        }
    }

    assert!(
        admitted >= 1,
        "shared accounting must not refuse every instance an oversized frame that \
         fits inside the total budget"
    );

    let combined = instance_root_bytes(first._ownership.root())
        + instance_root_bytes(second._ownership.root());
    let bound = total + live * (total / SHARE_DRIFT_DIVISOR);
    assert!(
        combined <= bound,
        "one oversized frame per instance pushed the combined footprint to {combined}, \
         past the total {total} plus the per-instance drift allowance (bound {bound})"
    );
}

/// A ledger write that fails leaves peers unable to see this instance's
/// reservation, so the generous `total - other_live_usage` grant is no longer
/// safe to act on: two instances in this state would each admit a large append
/// against the same unclaimed capacity and jointly exceed the total.
#[tokio::test]
async fn an_unrecorded_reservation_does_not_buy_a_shared_grant() {
    let data = TempDir::new().unwrap();
    let total = 1_000_000_u64;
    let first = open_instance(&data, total, true);
    let second = open_instance(&data, total, true);

    let first_registry = BudgetRegistry::open(data.path(), first._ownership.root()).unwrap();
    let second_registry = BudgetRegistry::open(data.path(), second._ownership.root()).unwrap();

    // Both instances live and idle: a recorded reservation earns the whole
    // total, because an idle peer holds nothing.
    first_registry.publish(0, total).unwrap();
    let recorded = second_registry.publish(0, total).unwrap();
    assert_eq!(recorded.live_instances, 2);
    assert_eq!(recorded.effective_budget, total);

    // Block the atomic-write target with a directory, so the reservation can
    // never be recorded.
    let instance_id = second
        ._ownership
        .root()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    fs::create_dir_all(
        data.path()
            .join("instances")
            .join(format!(".budget-registry.tmp-{instance_id}")),
    )
    .unwrap();

    let degraded = second_registry
        .publish(900_000, total)
        .expect("an unwritable ledger must degrade, not stall capture");
    assert_eq!(
        degraded.effective_budget,
        total / 2,
        "a reservation that was never recorded must fall back to self-only enforcement"
    );

    // The reservation really is invisible to the peer, which is why the grant
    // has to be conservative rather than merely inaccurate.
    assert_eq!(
        first_registry.publish(0, total).unwrap().other_live_usage,
        0
    );
}
