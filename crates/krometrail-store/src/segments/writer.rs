use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use krometrail_core::{
    ByteOffset, CaptureGap, EncodedFrame, ErrorCode, FrameAddress, KrometrailError, NonEmptyText,
    ObservedTime, PortFuture, RecordingSink, SegmentId, SessionId, SessionTime, TargetId,
};
use uuid::Uuid;

use super::{SEGMENT_HEADER_LEN, SealedFooter, SegmentHeader, encode_frame_record};
use crate::persistence_error;

pub const OPEN_SEGMENT_EXTENSION: &str = "open";
pub const SEALED_SEGMENT_EXTENSION: &str = "kts";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RotationConfig {
    pub max_duration: Duration,
    pub max_size: u64,
}

impl RotationConfig {
    pub const fn suggested() -> Self {
        Self {
            max_duration: Duration::from_secs(120),
            max_size: 128 * 1024 * 1024,
        }
    }

    fn validate(self) -> krometrail_core::Result<Self> {
        if self.max_duration.is_zero() {
            return Err(persistence_error(
                "segment rotation duration must be greater than zero",
            ));
        }
        if self.max_size == 0 {
            return Err(persistence_error(
                "segment rotation size must be greater than zero",
            ));
        }
        Ok(self)
    }

    fn max_duration_nanos(self) -> u64 {
        u64::try_from(self.max_duration.as_nanos()).unwrap_or(u64::MAX)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SegmentStoreConfig {
    pub directory: PathBuf,
    pub rotation: RotationConfig,
}

pub struct SegmentWriter {
    directory: PathBuf,
    rotation: RotationConfig,
    open_segments: Mutex<HashMap<(SessionId, TargetId), OpenSegment>>,
}

struct OpenSegment {
    header: SegmentHeader,
    writer: BufWriter<File>,
    file_len: u64,
    record_count: u64,
    total_payload: u64,
    first_session_time: SessionTime,
    last_session_time: SessionTime,
    last_observed_time: ObservedTime,
}

impl SegmentWriter {
    pub fn open(config: SegmentStoreConfig) -> krometrail_core::Result<Self> {
        let rotation = config.rotation.validate()?;
        fs::create_dir_all(&config.directory)
            .map_err(|error| io_error("create the segment directory", error))?;
        verify_writable(&config.directory)?;
        Ok(Self {
            directory: config.directory,
            rotation,
            open_segments: Mutex::new(HashMap::new()),
        })
    }

    fn append(&self, frame: EncodedFrame) -> krometrail_core::Result<FrameAddress> {
        let key = (frame.metadata().session_id(), frame.metadata().target_id());
        let mut segments = self
            .open_segments
            .lock()
            .map_err(|_| persistence_error("segment writer state lock is poisoned"))?;

        if let Some(open) = segments.get(&key)
            && open.should_rotate(&frame, self.rotation)
        {
            let open = segments.remove(&key).expect("checked open segment exists");
            seal_segment(&self.directory, open)?;
        }
        let open = match segments.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(open_segment(&self.directory, self.rotation, &frame)?)
            }
        };
        open.append(frame)
    }

    fn flush_session(&self, session_id: SessionId) -> krometrail_core::Result<()> {
        let mut segments = self
            .open_segments
            .lock()
            .map_err(|_| persistence_error("segment writer state lock is poisoned"))?;
        let keys: Vec<_> = segments
            .keys()
            .filter(|(candidate, _)| *candidate == session_id)
            .copied()
            .collect();
        for key in keys {
            let open = segments.remove(&key).expect("collected key exists");
            seal_segment(&self.directory, open)?;
        }
        Ok(())
    }
}

impl RecordingSink for SegmentWriter {
    fn append_frame(
        &self,
        frame: EncodedFrame,
    ) -> PortFuture<'_, krometrail_core::Result<FrameAddress>> {
        Box::pin(std::future::ready(self.append(frame)))
    }

    fn append_gap(&self, _gap: CaptureGap) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(std::future::ready(Err(KrometrailError::new(
            ErrorCode::Unsupported,
            NonEmptyText::new(
                "capture-gap persistence is owned by the SQLite metadata feature and is not yet wired",
            )
            .expect("static gap-persistence message is non-empty"),
        ))))
    }

    fn flush(&self, session_id: SessionId) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(std::future::ready(self.flush_session(session_id)))
    }
}

impl OpenSegment {
    fn should_rotate(&self, frame: &EncodedFrame, rotation: RotationConfig) -> bool {
        if self.record_count == 0 {
            return false;
        }
        let elapsed = frame
            .metadata()
            .session_time()
            .as_nanos()
            .saturating_sub(self.header.start_session_time.as_nanos());
        elapsed >= rotation.max_duration_nanos() || self.file_len >= rotation.max_size
    }

    fn append(&mut self, frame: EncodedFrame) -> krometrail_core::Result<FrameAddress> {
        let record = encode_frame_record(&frame)?;
        let offset = self.file_len;
        self.writer
            .write_all(&record)
            .map_err(|error| io_error("append a frame record", error))?;
        // A returned address always names a complete record in the OS page cache.
        // Power-loss durability is promoted at seal/rotation/session flush.
        self.writer
            .flush()
            .map_err(|error| io_error("flush a frame record", error))?;
        self.file_len = self
            .file_len
            .checked_add(record.len() as u64)
            .ok_or_else(|| persistence_error("segment file length overflow"))?;
        self.record_count = self
            .record_count
            .checked_add(1)
            .ok_or_else(|| persistence_error("segment record count overflow"))?;
        self.total_payload = self
            .total_payload
            .checked_add(frame.byte_len().get())
            .ok_or_else(|| persistence_error("segment payload count overflow"))?;
        self.last_session_time = frame.metadata().session_time();
        self.last_observed_time = frame.metadata().observed_time();
        Ok(FrameAddress::new(
            self.header.segment_id,
            ByteOffset::new(offset),
        ))
    }
}

fn open_segment(
    directory: &Path,
    rotation: RotationConfig,
    first_frame: &EncodedFrame,
) -> krometrail_core::Result<OpenSegment> {
    let (segment_id, file) = create_open_file(directory)?;
    let metadata = first_frame.metadata();
    let header = SegmentHeader::new(
        segment_id,
        metadata.session_id(),
        metadata.target_id(),
        metadata.session_time(),
        metadata.observed_time(),
        rotation.max_duration_nanos(),
        rotation.max_size,
    );
    let mut writer = BufWriter::new(file);
    writer
        .write_all(&header.encode())
        .map_err(|error| io_error("write a segment header", error))?;
    writer
        .flush()
        .map_err(|error| io_error("flush a segment header", error))?;
    Ok(OpenSegment {
        header,
        writer,
        file_len: SEGMENT_HEADER_LEN as u64,
        record_count: 0,
        total_payload: 0,
        first_session_time: metadata.session_time(),
        last_session_time: metadata.session_time(),
        last_observed_time: metadata.observed_time(),
    })
}

fn create_open_file(directory: &Path) -> krometrail_core::Result<(SegmentId, File)> {
    for _ in 0..4 {
        let segment_id = SegmentId::from_uuid(Uuid::new_v4());
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(open_segment_path(directory, segment_id))
        {
            Ok(file) => return Ok((segment_id, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(io_error("create an open segment", error)),
        }
    }
    Err(persistence_error(
        "could not allocate a unique segment identifier",
    ))
}

fn seal_segment(directory: &Path, mut open: OpenSegment) -> krometrail_core::Result<()> {
    let footer = SealedFooter::new(
        open.header.segment_id,
        open.record_count,
        open.total_payload,
        open.first_session_time,
        open.last_session_time,
        open.last_observed_time,
    );
    open.writer
        .write_all(&footer.encode())
        .map_err(|error| io_error("write a sealed segment footer", error))?;
    open.writer
        .flush()
        .map_err(|error| io_error("flush a sealed segment", error))?;
    open.writer
        .get_ref()
        .sync_data()
        .map_err(|error| io_error("sync a sealed segment", error))?;
    drop(open.writer);
    fs::rename(
        open_segment_path(directory, open.header.segment_id),
        sealed_segment_path(directory, open.header.segment_id),
    )
    .map_err(|error| io_error("publish a sealed segment", error))?;
    Ok(())
}

pub fn open_segment_path(directory: &Path, segment_id: SegmentId) -> PathBuf {
    directory.join(format!("{segment_id}.{OPEN_SEGMENT_EXTENSION}"))
}

pub fn sealed_segment_path(directory: &Path, segment_id: SegmentId) -> PathBuf {
    directory.join(format!("{segment_id}.{SEALED_SEGMENT_EXTENSION}"))
}

fn verify_writable(directory: &Path) -> krometrail_core::Result<()> {
    let probe = directory.join(format!(".write-probe-{}", Uuid::new_v4()));
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
        .map_err(|error| io_error("verify segment-directory writability", error))?;
    drop(file);
    fs::remove_file(probe)
        .map_err(|error| io_error("remove the segment-directory write probe", error))
}

fn io_error(action: &str, error: std::io::Error) -> KrometrailError {
    persistence_error(format!("could not {action}: {}", error.kind()))
}
