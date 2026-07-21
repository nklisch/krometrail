use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use krometrail_core::{
    ByteOffset, EncodedFrame, FrameAddress, KrometrailError, ObservedTime, PersistenceFailure,
    PersistenceFailureCategory, PersistenceOperation, PersistenceRecoverability, SegmentId,
    SessionId, SessionTime, TargetId,
};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use super::{SEGMENT_HEADER_LEN, SealedFooter, SegmentHeader, encode_frame_record};
use crate::{permissions, persistence_error};

pub const OPEN_SEGMENT_EXTENSION: &str = "open";
pub const SEALED_SEGMENT_EXTENSION: &str = "kts";
/// Bounds accepted persistence work independently from the CDP ingestion queue.
pub const SEGMENT_WRITE_QUEUE_CAPACITY: usize = 64;

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

    pub const fn max_size(self) -> u64 {
        self.max_size
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SegmentState {
    Open,
    Sealed,
}

impl SegmentState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Sealed => "sealed",
        }
    }
}

/// Metadata for one segment publication.
///
/// The on-disk file name is intentionally *not* stored: it is always
/// `segment_id` plus the extension implied by `state`. Deriving it at every use
/// site keeps a wrong name — most dangerously a live `.open` file named by a
/// deletion object — structurally inexpressible.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SegmentRegistration {
    pub segment_id: SegmentId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub state: SegmentState,
    pub start_time: SessionTime,
    pub end_time: Option<SessionTime>,
    pub file_bytes: u64,
    pub payload_bytes: u64,
    pub record_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameWriteCommit {
    pub address: FrameAddress,
    pub active_segment: SegmentRegistration,
    pub sealed_segment: Option<SegmentRegistration>,
}

/// Async payload primitive backed by one dedicated blocking filesystem worker.
///
/// The bounded channel applies backpressure without running file operations on
/// the caller's executor thread. Once a command enters the channel, cancellation
/// of its response future does not cancel the filesystem operation; `flush`
/// follows all previously accepted commands in FIFO order.
pub struct SegmentWriter {
    commands: mpsc::Sender<WriterCommand>,
    rotation_max_size: u64,
}

struct WorkerState {
    directory: PathBuf,
    rotation: RotationConfig,
    directory_sync: Arc<dyn DirectorySync>,
    open_segments: HashMap<(SessionId, TargetId), OpenSegment>,
    terminal_error: Option<KrometrailError>,
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

enum WriterCommand {
    Append {
        frame: EncodedFrame,
        reply: oneshot::Sender<krometrail_core::Result<FrameWriteCommit>>,
    },
    Flush {
        session_id: SessionId,
        reply: oneshot::Sender<krometrail_core::Result<Vec<SegmentRegistration>>>,
    },
    FlushAll {
        reply: oneshot::Sender<krometrail_core::Result<Vec<SegmentRegistration>>>,
    },
}

trait DirectorySync: Send + Sync {
    fn sync(&self, directory: &Path) -> std::io::Result<()>;
}

struct OsDirectorySync;

impl DirectorySync for OsDirectorySync {
    fn sync(&self, directory: &Path) -> std::io::Result<()> {
        sync_directory(directory)
    }
}

impl SegmentWriter {
    pub fn open(config: SegmentStoreConfig) -> krometrail_core::Result<Self> {
        Self::open_with_worker(
            config,
            SEGMENT_WRITE_QUEUE_CAPACITY,
            Arc::new(OsDirectorySync),
        )
    }

    fn open_with_worker(
        config: SegmentStoreConfig,
        queue_capacity: usize,
        directory_sync: Arc<dyn DirectorySync>,
    ) -> krometrail_core::Result<Self> {
        let rotation = config.rotation.validate()?;
        if queue_capacity == 0 {
            return Err(persistence_error(
                "segment writer queue capacity must be greater than zero",
            ));
        }
        permissions::ensure_private_directory(&config.directory).map_err(|error| {
            io_error(
                PersistenceOperation::SegmentDirectoryPreparation,
                error,
                PersistenceRecoverability::WriterTerminal,
            )
        })?;
        verify_writable(&config.directory)?;

        let (commands, receiver) = mpsc::channel(queue_capacity);
        let state = WorkerState {
            directory: config.directory,
            rotation,
            directory_sync,
            open_segments: HashMap::new(),
            terminal_error: None,
        };
        std::thread::Builder::new()
            .name("krometrail-segment-writer".to_owned())
            .spawn(move || state.run(receiver))
            .map_err(|error| {
                io_error(
                    PersistenceOperation::SegmentWriterWorker,
                    error,
                    PersistenceRecoverability::WriterTerminal,
                )
            })?;
        Ok(Self {
            commands,
            rotation_max_size: rotation.max_size(),
        })
    }
}

impl SegmentWriter {
    pub const fn rotation_max_size(&self) -> u64 {
        self.rotation_max_size
    }

    pub async fn append_indexable(
        &self,
        frame: EncodedFrame,
    ) -> krometrail_core::Result<FrameWriteCommit> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(WriterCommand::Append { frame, reply })
            .await
            .map_err(|_| worker_unavailable())?;
        response.await.map_err(|_| worker_unavailable())?
    }

    pub async fn flush_indexable(
        &self,
        session_id: SessionId,
    ) -> krometrail_core::Result<Vec<SegmentRegistration>> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(WriterCommand::Flush { session_id, reply })
            .await
            .map_err(|_| worker_unavailable())?;
        response.await.map_err(|_| worker_unavailable())?
    }

    pub async fn flush_all_indexable(&self) -> krometrail_core::Result<Vec<SegmentRegistration>> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(WriterCommand::FlushAll { reply })
            .await
            .map_err(|_| worker_unavailable())?;
        response.await.map_err(|_| worker_unavailable())?
    }
}

impl WorkerState {
    fn run(mut self, mut commands: mpsc::Receiver<WriterCommand>) {
        while let Some(command) = commands.blocking_recv() {
            match command {
                WriterCommand::Append { frame, reply } => {
                    let result = self.execute(|state| state.append(frame));
                    let _ = reply.send(result);
                }
                WriterCommand::Flush { session_id, reply } => {
                    let result = self.execute(|state| state.flush_session(session_id));
                    let _ = reply.send(result);
                }
                WriterCommand::FlushAll { reply } => {
                    let result = self.execute(Self::flush_all);
                    let _ = reply.send(result);
                }
            }
        }
        // Explicit RecordingSink::flush is the reportable durability boundary.
        // Dropping the last sender leaves open files recoverable rather than
        // performing an unreportable best-effort seal during process teardown.
    }

    fn execute<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> krometrail_core::Result<T>,
    ) -> krometrail_core::Result<T> {
        if let Some(error) = &self.terminal_error {
            return Err(error.clone());
        }
        match operation(self) {
            Ok(value) => Ok(value),
            Err(mut error) => {
                let recoverability = match error.persistence.as_ref() {
                    Some(failure) => failure.recoverability(),
                    None => {
                        error = store_error(
                            PersistenceOperation::SegmentWriterWorker,
                            PersistenceFailureCategory::InvalidData,
                            PersistenceRecoverability::WriterTerminal,
                            "segment writer returned an unclassified failure",
                        );
                        PersistenceRecoverability::WriterTerminal
                    }
                };
                if recoverability == PersistenceRecoverability::WriterTerminal {
                    self.terminal_error = Some(error.clone());
                }
                Err(error)
            }
        }
    }

    fn append(&mut self, frame: EncodedFrame) -> krometrail_core::Result<FrameWriteCommit> {
        let key = (frame.metadata().session_id(), frame.metadata().target_id());
        let sealed_segment = if self
            .open_segments
            .get(&key)
            .is_some_and(|open| open.should_rotate(&frame, self.rotation))
        {
            let open = self
                .open_segments
                .remove(&key)
                .expect("checked open segment exists");
            Some(seal_segment(
                &self.directory,
                self.directory_sync.as_ref(),
                open,
            )?)
        } else {
            None
        };
        let open = match self.open_segments.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => entry.insert(open_segment(
                &self.directory,
                self.directory_sync.as_ref(),
                self.rotation,
                &frame,
            )?),
        };
        let address = open.append(frame)?;
        Ok(FrameWriteCommit {
            address,
            active_segment: open.registration(SegmentState::Open),
            sealed_segment,
        })
    }

    fn flush_session(
        &mut self,
        session_id: SessionId,
    ) -> krometrail_core::Result<Vec<SegmentRegistration>> {
        let keys: Vec<_> = self
            .open_segments
            .keys()
            .filter(|(candidate, _)| *candidate == session_id)
            .copied()
            .collect();
        self.flush_keys(keys)
    }

    fn flush_all(&mut self) -> krometrail_core::Result<Vec<SegmentRegistration>> {
        let keys: Vec<_> = self.open_segments.keys().copied().collect();
        self.flush_keys(keys)
    }

    fn flush_keys(
        &mut self,
        keys: Vec<(SessionId, TargetId)>,
    ) -> krometrail_core::Result<Vec<SegmentRegistration>> {
        let mut registrations = Vec::with_capacity(keys.len());
        for key in keys {
            let open = self
                .open_segments
                .remove(&key)
                .expect("collected key exists");
            registrations.push(seal_segment(
                &self.directory,
                self.directory_sync.as_ref(),
                open,
            )?);
        }
        Ok(registrations)
    }
}

impl OpenSegment {
    fn registration(&self, state: SegmentState) -> SegmentRegistration {
        SegmentRegistration {
            segment_id: self.header.segment_id,
            session_id: self.header.session_id,
            target_id: self.header.target_id,
            state,
            start_time: self.header.start_session_time,
            end_time: (state == SegmentState::Sealed).then_some(self.last_session_time),
            file_bytes: self.file_len,
            payload_bytes: self.total_payload,
            record_count: self.record_count,
        }
    }

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
        let record = encode_frame_record(&frame).map_err(|_| {
            store_error(
                PersistenceOperation::FrameRecordAppend,
                PersistenceFailureCategory::InvalidData,
                PersistenceRecoverability::WriterTerminal,
                "frame record encoding failed",
            )
        })?;
        let offset = self.file_len;
        self.writer.write_all(&record).map_err(|error| {
            io_error(
                PersistenceOperation::FrameRecordAppend,
                error,
                PersistenceRecoverability::WriterTerminal,
            )
        })?;
        // A returned address always names a complete record in the OS page cache.
        // Power-loss durability is promoted at seal/rotation/session flush.
        self.writer.flush().map_err(|error| {
            io_error(
                PersistenceOperation::FrameRecordFlush,
                error,
                PersistenceRecoverability::WriterTerminal,
            )
        })?;
        self.file_len = self
            .file_len
            .checked_add(record.len() as u64)
            .ok_or_else(|| writer_invalid("segment file length overflow"))?;
        self.record_count = self
            .record_count
            .checked_add(1)
            .ok_or_else(|| writer_invalid("segment record count overflow"))?;
        self.total_payload = self
            .total_payload
            .checked_add(frame.byte_len().get())
            .ok_or_else(|| writer_invalid("segment payload count overflow"))?;
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
    directory_sync: &dyn DirectorySync,
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
    writer.write_all(&header.encode()).map_err(|error| {
        io_error(
            PersistenceOperation::OpenSegmentCreation,
            error,
            PersistenceRecoverability::WriterTerminal,
        )
    })?;
    writer.flush().map_err(|error| {
        io_error(
            PersistenceOperation::OpenSegmentCreation,
            error,
            PersistenceRecoverability::WriterTerminal,
        )
    })?;
    writer.get_ref().sync_data().map_err(|error| {
        io_error(
            PersistenceOperation::OpenSegmentCreation,
            error,
            PersistenceRecoverability::WriterTerminal,
        )
    })?;
    directory_sync.sync(directory).map_err(|error| {
        io_error(
            PersistenceOperation::OpenSegmentPublicationSync,
            error,
            PersistenceRecoverability::WriterTerminal,
        )
    })?;
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
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        permissions::configure_private_file(&mut options);
        match options.open(open_segment_path(directory, segment_id)) {
            Ok(file) => return Ok((segment_id, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(io_error(
                    PersistenceOperation::OpenSegmentCreation,
                    error,
                    PersistenceRecoverability::WriterTerminal,
                ));
            }
        }
    }
    Err(store_error(
        PersistenceOperation::OpenSegmentCreation,
        PersistenceFailureCategory::AlreadyExists,
        PersistenceRecoverability::WriterTerminal,
        "open segment identifier allocation failed",
    ))
}

fn seal_segment(
    directory: &Path,
    directory_sync: &dyn DirectorySync,
    mut open: OpenSegment,
) -> krometrail_core::Result<SegmentRegistration> {
    let footer = SealedFooter::new(
        open.header.segment_id,
        open.record_count,
        open.total_payload,
        open.first_session_time,
        open.last_session_time,
        open.last_observed_time,
    );
    let footer_bytes = footer.encode();
    open.writer.write_all(&footer_bytes).map_err(|error| {
        io_error(
            PersistenceOperation::SealedSegmentFooterWrite,
            error,
            PersistenceRecoverability::WriterTerminal,
        )
    })?;
    open.writer.flush().map_err(|error| {
        io_error(
            PersistenceOperation::SealedSegmentFooterWrite,
            error,
            PersistenceRecoverability::WriterTerminal,
        )
    })?;
    open.writer.get_ref().sync_data().map_err(|error| {
        io_error(
            PersistenceOperation::SealedSegmentFileSync,
            error,
            PersistenceRecoverability::WriterTerminal,
        )
    })?;
    open.file_len = open
        .file_len
        .checked_add(footer_bytes.len() as u64)
        .ok_or_else(|| writer_invalid("sealed segment file length overflow"))?;
    let registration = open.registration(SegmentState::Sealed);
    let segment_id = open.header.segment_id;
    drop(open.writer);
    let sealed_path = sealed_segment_path(directory, segment_id);
    match fs::rename(open_segment_path(directory, segment_id), &sealed_path) {
        Ok(()) => {}
        // The open file is gone. Either this segment was already published under
        // its sealed name by a reconciler — in which case sealing is complete and
        // this call is simply idempotent — or the source genuinely vanished. Both
        // outcomes are scoped to *this* segment: the caller has already removed
        // the entry from `open_segments`, so the writer stays usable either way
        // and `WriterTerminal` would be the wrong classification.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if !sealed_publication_matches(&sealed_path, segment_id) {
                return Err(store_error(
                    PersistenceOperation::SealedSegmentPublication,
                    PersistenceFailureCategory::NotFound,
                    PersistenceRecoverability::WriterUsable,
                    "open segment disappeared before publication",
                ));
            }
            return Ok(registration);
        }
        Err(error) => {
            return Err(io_error(
                PersistenceOperation::SealedSegmentPublication,
                error,
                PersistenceRecoverability::WriterUsable,
            ));
        }
    }
    directory_sync.sync(directory).map_err(|error| {
        io_error(
            PersistenceOperation::SealedSegmentPublicationSync,
            error,
            PersistenceRecoverability::WriterUsable,
        )
    })?;
    Ok(registration)
}

/// Reports whether `path` is a published segment whose header carries `segment_id`.
///
/// This is the reconciliation test for an absent open file at seal time. A
/// matching header proves the segment reached its sealed name intact, so the
/// seal has already succeeded and re-reporting it as a failure would destroy a
/// writer over work that is already durable. Anything else — missing, short,
/// corrupt, or a different segment — is not a reconciliation and must fail.
fn sealed_publication_matches(path: &Path, segment_id: SegmentId) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut header = [0_u8; SEGMENT_HEADER_LEN];
    if std::io::Read::read_exact(&mut file, &mut header).is_err() {
        return false;
    }
    SegmentHeader::decode(&header).is_ok_and(|header| header.segment_id == segment_id)
}

pub fn open_segment_path(directory: &Path, segment_id: SegmentId) -> PathBuf {
    directory.join(segment_file_name(segment_id, SegmentState::Open))
}

/// The single authority for a segment's on-disk file name.
///
/// Every path, deletion object, and staging name derives from this, so a name
/// that does not correspond to a real `(segment_id, state)` pair cannot be
/// constructed anywhere in the store.
pub fn segment_file_name(segment_id: SegmentId, state: SegmentState) -> String {
    let extension = match state {
        SegmentState::Open => OPEN_SEGMENT_EXTENSION,
        SegmentState::Sealed => SEALED_SEGMENT_EXTENSION,
    };
    format!("{segment_id}.{extension}")
}

pub fn sealed_segment_path(directory: &Path, segment_id: SegmentId) -> PathBuf {
    directory.join(segment_file_name(segment_id, SegmentState::Sealed))
}

fn verify_writable(directory: &Path) -> krometrail_core::Result<()> {
    let probe = directory.join(format!(".write-probe-{}", Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    permissions::configure_private_file(&mut options);
    let file = options.open(&probe).map_err(|error| {
        io_error(
            PersistenceOperation::SegmentDirectoryPreparation,
            error,
            PersistenceRecoverability::WriterTerminal,
        )
    })?;
    drop(file);
    fs::remove_file(probe).map_err(|error| {
        io_error(
            PersistenceOperation::SegmentDirectoryPreparation,
            error,
            PersistenceRecoverability::WriterTerminal,
        )
    })
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> std::io::Result<()> {
    File::open(directory)?.sync_all()
}

// Rust's standard library cannot portably open a directory handle on every
// platform. Linux and macOS, the supported production hosts, use the Unix path
// above; other targets retain file sync and rename semantics without claiming a
// directory-fsync guarantee.
#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> std::io::Result<()> {
    Ok(())
}

fn worker_unavailable() -> KrometrailError {
    store_error(
        PersistenceOperation::SegmentWriterWorker,
        PersistenceFailureCategory::Unavailable,
        PersistenceRecoverability::WriterTerminal,
        "segment writer worker is unavailable",
    )
}

fn io_error(
    operation: PersistenceOperation,
    error: std::io::Error,
    recoverability: PersistenceRecoverability,
) -> KrometrailError {
    let category = match error.kind() {
        std::io::ErrorKind::NotFound => PersistenceFailureCategory::NotFound,
        std::io::ErrorKind::PermissionDenied => PersistenceFailureCategory::PermissionDenied,
        std::io::ErrorKind::AlreadyExists => PersistenceFailureCategory::AlreadyExists,
        std::io::ErrorKind::Interrupted => PersistenceFailureCategory::Interrupted,
        std::io::ErrorKind::WouldBlock => PersistenceFailureCategory::ResourceBusy,
        std::io::ErrorKind::StorageFull => PersistenceFailureCategory::StorageFull,
        std::io::ErrorKind::ReadOnlyFilesystem => PersistenceFailureCategory::ReadOnlyFilesystem,
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::InvalidInput => {
            PersistenceFailureCategory::InvalidData
        }
        _ => PersistenceFailureCategory::Other,
    };
    store_error(
        operation,
        category,
        recoverability,
        "segment persistence operation failed",
    )
}

fn writer_invalid(message: &'static str) -> KrometrailError {
    store_error(
        PersistenceOperation::FrameRecordAppend,
        PersistenceFailureCategory::InvalidData,
        PersistenceRecoverability::WriterTerminal,
        message,
    )
}

fn store_error(
    operation: PersistenceOperation,
    category: PersistenceFailureCategory,
    recoverability: PersistenceRecoverability,
    message: &'static str,
) -> KrometrailError {
    KrometrailError::new(
        krometrail_core::ErrorCode::PersistenceFailed,
        krometrail_core::NonEmptyText::new(message).expect("store error message is non-empty"),
    )
    .with_retry(krometrail_core::RetryAdvice::AfterRecovery)
    .with_persistence(PersistenceFailure::new(operation, category, recoverability))
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
            mpsc as std_mpsc,
        },
        task::{Context, Poll},
        thread::ThreadId,
    };

    use krometrail_core::{
        CaptureOrdinal, CapturedFrame, DeviceScaleFactor, FrameId, ImageFormat, PixelDimensions,
        SourceTime,
    };
    use tempfile::TempDir;

    use super::*;

    fn test_frame(session_id: SessionId, ordinal: u64) -> EncodedFrame {
        EncodedFrame::new(
            CapturedFrame::new(
                FrameId::from_uuid(Uuid::from_u128(100 + u128::from(ordinal))),
                session_id,
                TargetId::from_uuid(Uuid::from_u128(200)),
                CaptureOrdinal::new(ordinal).unwrap(),
                Some(SourceTime::from_nanos(i128::from(ordinal))),
                ObservedTime::from_nanos(ordinal),
                SessionTime::from_nanos(ordinal),
                ImageFormat::Jpeg,
                PixelDimensions::new(1, 1).unwrap(),
                PixelDimensions::new(1, 1).unwrap(),
                DeviceScaleFactor::new(1.0).unwrap(),
                vec![],
            )
            .unwrap(),
            vec![ordinal as u8],
        )
        .unwrap()
    }

    fn config(directory: &TempDir) -> SegmentStoreConfig {
        SegmentStoreConfig {
            directory: directory.path().to_path_buf(),
            rotation: RotationConfig::suggested(),
        }
    }

    #[derive(Default)]
    struct CountingDirectorySync {
        calls: AtomicUsize,
        fail_on: Option<usize>,
        failure_kind: Option<std::io::ErrorKind>,
    }

    impl DirectorySync for CountingDirectorySync {
        fn sync(&self, _directory: &Path) -> std::io::Result<()> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if self.fail_on == Some(call) {
                Err(std::io::Error::new(
                    self.failure_kind.unwrap_or(std::io::ErrorKind::Other),
                    "raw failure at /private/recordings/secret.kts",
                ))
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn directory_publications_are_synced_and_failures_propagate() {
        let directory = TempDir::new().unwrap();
        let sync = Arc::new(CountingDirectorySync::default());
        let sink = SegmentWriter::open_with_worker(config(&directory), 4, sync.clone()).unwrap();
        let session_id = SessionId::from_uuid(Uuid::from_u128(1));
        sink.append_indexable(test_frame(session_id, 1))
            .await
            .unwrap();
        assert_eq!(sync.calls.load(Ordering::SeqCst), 1);
        sink.flush_indexable(session_id).await.unwrap();
        assert_eq!(sync.calls.load(Ordering::SeqCst), 2);

        let initial_failure = TempDir::new().unwrap();
        let sink = SegmentWriter::open_with_worker(
            config(&initial_failure),
            4,
            Arc::new(CountingDirectorySync {
                calls: AtomicUsize::new(0),
                fail_on: Some(1),
                failure_kind: Some(std::io::ErrorKind::PermissionDenied),
            }),
        )
        .unwrap();
        let error = sink
            .append_indexable(test_frame(SessionId::from_uuid(Uuid::from_u128(2)), 1))
            .await
            .unwrap_err();
        let persistence = error.persistence.as_ref().unwrap();
        assert_eq!(
            persistence.operation(),
            PersistenceOperation::OpenSegmentPublicationSync
        );
        assert_eq!(
            persistence.category(),
            PersistenceFailureCategory::PermissionDenied
        );
        assert_eq!(
            persistence.recoverability(),
            PersistenceRecoverability::WriterTerminal
        );
        let first_entries = fs::read_dir(initial_failure.path()).unwrap().count();
        let replayed = sink
            .append_indexable(test_frame(SessionId::from_uuid(Uuid::from_u128(2)), 2))
            .await
            .unwrap_err();
        assert_eq!(replayed, error);
        assert_eq!(
            fs::read_dir(initial_failure.path()).unwrap().count(),
            first_entries
        );

        let rename_failure = TempDir::new().unwrap();
        let sync = Arc::new(CountingDirectorySync {
            calls: AtomicUsize::new(0),
            fail_on: Some(2),
            failure_kind: Some(std::io::ErrorKind::PermissionDenied),
        });
        let sink =
            SegmentWriter::open_with_worker(config(&rename_failure), 4, sync.clone()).unwrap();
        let session_id = SessionId::from_uuid(Uuid::from_u128(3));
        let first = sink
            .append_indexable(test_frame(session_id, 1))
            .await
            .unwrap()
            .address;
        let error = sink.flush_indexable(session_id).await.unwrap_err();
        let persistence = error.persistence.as_ref().unwrap();
        assert_eq!(
            persistence.operation(),
            PersistenceOperation::SealedSegmentPublicationSync
        );
        assert_eq!(
            persistence.category(),
            PersistenceFailureCategory::PermissionDenied
        );
        assert_eq!(
            persistence.recoverability(),
            PersistenceRecoverability::WriterUsable
        );
        let serialized = serde_json::to_string(&error).unwrap();
        assert!(!serialized.contains("/private/recordings"));
        assert!(!serialized.contains("raw failure"));

        let sealed = sealed_segment_path(rename_failure.path(), first.segment_id);
        let sealed_bytes = fs::read(&sealed).unwrap();
        assert_eq!(
            crate::segments::read_frame_at(&sealed_bytes, first).unwrap(),
            test_frame(session_id, 1)
        );
        let second = sink
            .append_indexable(test_frame(session_id, 2))
            .await
            .unwrap()
            .address;
        assert_ne!(first.segment_id, second.segment_id);
        sink.flush_indexable(session_id).await.unwrap();
        let second_bytes = fs::read(sealed_segment_path(
            rename_failure.path(),
            second.segment_id,
        ))
        .unwrap();
        assert_eq!(
            crate::segments::read_frame_at(&second_bytes, second).unwrap(),
            test_frame(session_id, 2)
        );
        assert_eq!(sync.calls.load(Ordering::SeqCst), 4);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn created_segment_files_are_owner_only_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let directory = TempDir::new().unwrap();
        let sink = SegmentWriter::open(config(&directory)).unwrap();
        sink.append_indexable(test_frame(SessionId::from_uuid(Uuid::from_u128(9)), 1))
            .await
            .unwrap();
        let path = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.extension()
                    .is_some_and(|extension| extension == OPEN_SEGMENT_EXTENSION)
            })
            .unwrap();
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    /// Drives the interleaving proven in the seventh shakedown: a second process
    /// ran startup recovery against a live data directory, footered and renamed
    /// this writer's open segment to its sealed name, and the writer then sealed
    /// onto a source that no longer existed. That ENOENT latched `terminal_error`
    /// and killed capture globally until process restart.
    ///
    /// The segment is already published, so the seal is complete: it must report
    /// success and the writer must stay usable for every other session.
    #[tokio::test]
    async fn seal_reconciles_when_the_segment_is_already_published() {
        let directory = TempDir::new().unwrap();
        let sink = SegmentWriter::open(config(&directory)).unwrap();
        let session_id = SessionId::from_uuid(Uuid::from_u128(40));
        let address = sink
            .append_indexable(test_frame(session_id, 1))
            .await
            .unwrap()
            .address;

        // Stand in for the intruding process's recovery: publish the live open
        // segment under its sealed name behind this writer's back.
        fs::rename(
            open_segment_path(directory.path(), address.segment_id),
            sealed_segment_path(directory.path(), address.segment_id),
        )
        .unwrap();

        let registrations = sink.flush_indexable(session_id).await.unwrap();
        assert_eq!(registrations.len(), 1);
        assert_eq!(registrations[0].segment_id, address.segment_id);
        assert_eq!(registrations[0].state, SegmentState::Sealed);

        // The writer survived: an unrelated session still records and seals.
        let other = SessionId::from_uuid(Uuid::from_u128(41));
        let next = sink
            .append_indexable(test_frame(other, 2))
            .await
            .unwrap()
            .address;
        assert_ne!(next.segment_id, address.segment_id);
        sink.flush_indexable(other).await.unwrap();
        assert!(sealed_segment_path(directory.path(), next.segment_id).is_file());
    }

    /// An open segment that vanished without being published is a genuine loss,
    /// but it is still scoped to one segment. It must not latch `terminal_error`:
    /// `WriterTerminal` would take down every other session's capture over a
    /// per-segment fault.
    #[tokio::test]
    async fn vanished_open_segment_fails_only_its_own_segment() {
        let directory = TempDir::new().unwrap();
        let sink = SegmentWriter::open(config(&directory)).unwrap();
        let session_id = SessionId::from_uuid(Uuid::from_u128(50));
        let address = sink
            .append_indexable(test_frame(session_id, 1))
            .await
            .unwrap()
            .address;
        fs::remove_file(open_segment_path(directory.path(), address.segment_id)).unwrap();

        let error = sink.flush_indexable(session_id).await.unwrap_err();
        let persistence = error.persistence.as_ref().unwrap();
        assert_eq!(
            persistence.operation(),
            PersistenceOperation::SealedSegmentPublication
        );
        assert_eq!(persistence.category(), PersistenceFailureCategory::NotFound);
        assert_eq!(
            persistence.recoverability(),
            PersistenceRecoverability::WriterUsable
        );

        // Not latched: the next append on a fresh session succeeds rather than
        // replaying the previous failure.
        let other = SessionId::from_uuid(Uuid::from_u128(51));
        let next = sink
            .append_indexable(test_frame(other, 2))
            .await
            .unwrap()
            .address;
        sink.flush_indexable(other).await.unwrap();
        assert!(sealed_segment_path(directory.path(), next.segment_id).is_file());
    }

    /// A file sitting at the sealed name that belongs to a *different* segment is
    /// not a reconciliation and must not be reported as a successful seal.
    #[tokio::test]
    async fn foreign_sealed_publication_is_not_treated_as_reconciliation() {
        let directory = TempDir::new().unwrap();
        let sink = SegmentWriter::open(config(&directory)).unwrap();
        let session_id = SessionId::from_uuid(Uuid::from_u128(60));
        let address = sink
            .append_indexable(test_frame(session_id, 1))
            .await
            .unwrap()
            .address;
        fs::remove_file(open_segment_path(directory.path(), address.segment_id)).unwrap();
        // A well-formed header for an unrelated segment id.
        let foreign = SegmentHeader::new(
            SegmentId::from_uuid(Uuid::from_u128(999)),
            session_id,
            TargetId::from_uuid(Uuid::from_u128(200)),
            SessionTime::from_nanos(0),
            ObservedTime::from_nanos(0),
            1,
            1,
        );
        fs::write(
            sealed_segment_path(directory.path(), address.segment_id),
            foreign.encode(),
        )
        .unwrap();

        let error = sink.flush_indexable(session_id).await.unwrap_err();
        assert_eq!(
            error.persistence.as_ref().unwrap().recoverability(),
            PersistenceRecoverability::WriterUsable
        );
    }

    struct BlockingDirectorySync {
        entered: Mutex<Option<std_mpsc::Sender<ThreadId>>>,
        release: Mutex<std_mpsc::Receiver<()>>,
    }

    impl DirectorySync for BlockingDirectorySync {
        fn sync(&self, _directory: &Path) -> std::io::Result<()> {
            if let Some(entered) = self.entered.lock().unwrap().take() {
                entered.send(std::thread::current().id()).unwrap();
                self.release
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(2))
                    .map_err(|_| std::io::Error::other("worker release timed out"))?;
            }
            Ok(())
        }
    }

    #[test]
    fn async_handoff_is_bounded_cancellable_and_runs_filesystem_work_off_thread() {
        let directory = TempDir::new().unwrap();
        let (entered_tx, entered_rx) = std_mpsc::channel();
        let (release_tx, release_rx) = std_mpsc::channel();
        let sink = SegmentWriter::open_with_worker(
            config(&directory),
            1,
            Arc::new(BlockingDirectorySync {
                entered: Mutex::new(Some(entered_tx)),
                release: Mutex::new(release_rx),
            }),
        )
        .unwrap();
        let session_id = SessionId::from_uuid(Uuid::from_u128(4));
        let mut first = Box::pin(sink.append_indexable(test_frame(session_id, 1)));
        let mut second = Box::pin(sink.append_indexable(test_frame(session_id, 2)));
        let mut cancelled = Box::pin(sink.append_indexable(test_frame(session_id, 3)));
        let mut context = Context::from_waker(std::task::Waker::noop());

        assert!(matches!(first.as_mut().poll(&mut context), Poll::Pending));
        let worker_thread = entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_ne!(worker_thread, std::thread::current().id());

        // Cancellation after acceptance drops only the response: the worker
        // still completes the first append. One more command fits in the queue;
        // the third remains pending at the bounded handoff and cancellation
        // before acceptance prevents it from being written.
        drop(first);
        assert!(matches!(second.as_mut().poll(&mut context), Poll::Pending));
        assert!(matches!(
            cancelled.as_mut().poll(&mut context),
            Poll::Pending
        ));
        drop(cancelled);
        release_tx.send(()).unwrap();

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            second.await.unwrap();
            sink.flush_indexable(session_id).await.unwrap();
        });

        let sealed = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.extension().is_some_and(|extension| extension == "kts"))
            .unwrap();
        let bytes = fs::read(sealed).unwrap();
        let footer =
            SealedFooter::decode(&bytes[bytes.len() - super::super::SEALED_FOOTER_LEN..]).unwrap();
        assert_eq!(footer.record_count, 2);
    }
}
