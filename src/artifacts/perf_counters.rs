use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub(crate) struct Snapshot {
    pub decode_calls: u64,
    pub decoded_frames: u64,
    pub normalize_calls: u64,
    pub normalized_frames: u64,
}

static DECODE_CALLS: AtomicU64 = AtomicU64::new(0);
static DECODED_FRAMES: AtomicU64 = AtomicU64::new(0);
static NORMALIZE_CALLS: AtomicU64 = AtomicU64::new(0);
static NORMALIZED_FRAMES: AtomicU64 = AtomicU64::new(0);

pub(crate) fn reset() {
    DECODE_CALLS.store(0, Ordering::SeqCst);
    DECODED_FRAMES.store(0, Ordering::SeqCst);
    NORMALIZE_CALLS.store(0, Ordering::SeqCst);
    NORMALIZED_FRAMES.store(0, Ordering::SeqCst);
}

pub(crate) fn record_decode() {
    DECODE_CALLS.fetch_add(1, Ordering::SeqCst);
    DECODED_FRAMES.fetch_add(1, Ordering::SeqCst);
}

pub(crate) fn record_normalize(frame_count: usize) {
    NORMALIZE_CALLS.fetch_add(1, Ordering::SeqCst);
    NORMALIZED_FRAMES.fetch_add(frame_count as u64, Ordering::SeqCst);
}

pub(crate) fn snapshot() -> Snapshot {
    Snapshot {
        decode_calls: DECODE_CALLS.load(Ordering::SeqCst),
        decoded_frames: DECODED_FRAMES.load(Ordering::SeqCst),
        normalize_calls: NORMALIZE_CALLS.load(Ordering::SeqCst),
        normalized_frames: NORMALIZED_FRAMES.load(Ordering::SeqCst),
    }
}
