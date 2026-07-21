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
        let value = frame(
            session,
            session + 1_000,
            session * 1_000 + u128::from(ordinal),
            ordinal,
            bytes,
        );
        // Budget exhaustion is an expected outcome once the share is consumed.
        if instance.store.append_frame(value.clone()).await.is_err() {
            break;
        }
        let _ = instance.store.flush(value.metadata().session_id()).await;
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
    fill(&first, 1, 40, 40_000).await;
    fill(&second, 2, 40, 40_000).await;
    fill(&third, 3, 40, 40_000).await;
    let shared_total = usage(&first).await + usage(&second).await + usage(&third).await;

    let isolated = TempDir::new().unwrap();
    let alone = open_instance(&isolated, total, false);
    let other = open_instance(&isolated, total, false);
    let another = open_instance(&isolated, total, false);
    fill(&alone, 1, 40, 40_000).await;
    fill(&other, 2, 40, 40_000).await;
    fill(&another, 3, 40, 40_000).await;
    let unshared_total = usage(&alone).await + usage(&other).await + usage(&another).await;

    assert!(
        shared_total < unshared_total,
        "shared accounting must bound the combined footprint: shared {shared_total} \
         unshared {unshared_total}"
    );
    // Each instance floors at an equal share, so the bound is the total plus the
    // overshoot that floor permits, not an unbounded multiple.
    assert!(
        shared_total <= total + 2 * (total / 3),
        "combined usage {shared_total} exceeded the documented overshoot bound"
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
