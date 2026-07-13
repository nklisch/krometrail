mod footer;
mod header;
mod record;
mod scanner;
mod wire;
mod writer;

pub use footer::{SEALED_FOOTER_LEN, SEALED_FOOTER_MAGIC, SealedFooter};
pub use header::{FORMAT_VERSION, SEGMENT_HEADER_LEN, SEGMENT_MAGIC, SegmentHeader};
pub use record::{
    DecodedFrameRecord, FRAME_RECORD_KIND, FRAME_RECORD_PREFIX_LEN, FrameRecord,
    decode_frame_record, encode_frame_record,
};
pub use scanner::{
    RecordSpan, ScanResult, Trailing, read_frame_at, scan_complete_records,
    scan_complete_records_from,
};
pub use writer::{
    OPEN_SEGMENT_EXTENSION, RotationConfig, SEALED_SEGMENT_EXTENSION, SEGMENT_WRITE_QUEUE_CAPACITY,
    SegmentStoreConfig, SegmentWriter, open_segment_path, sealed_segment_path,
};
