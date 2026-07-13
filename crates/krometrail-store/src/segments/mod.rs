mod footer;
mod header;
mod record;
mod scanner;
mod wire;

pub use footer::{SEALED_FOOTER_LEN, SEALED_FOOTER_MAGIC, SealedFooter};
pub use header::{FORMAT_VERSION, SEGMENT_HEADER_LEN, SEGMENT_MAGIC, SegmentHeader};
pub use record::{
    DecodedFrameRecord, FRAME_RECORD_KIND, FRAME_RECORD_PREFIX_LEN, FrameRecord,
    decode_frame_record, encode_frame_record,
};
pub use scanner::{RecordSpan, ScanResult, Trailing, read_frame_at, scan_complete_records};
