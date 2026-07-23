use std::{fs, path::PathBuf, thread, time::Duration};

use krometrail_core::{ByteOffset, FrameId, SegmentId, SessionId};
use rusqlite::{params, params_from_iter};

use super::{SqliteIndex, codec};
use crate::persistence_error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)] // Browser-event and artifact writers use the same authoritative ledger as they land.
pub(crate) enum UsageClass {
    Segment,
    Index,
    BrowserEvent,
    Artifact,
}

impl UsageClass {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Segment => "segment",
            Self::Index => "index",
            Self::BrowserEvent => "browser_event",
            Self::Artifact => "artifact",
        }
    }
}

pub(crate) struct UsageEntry {
    pub class: UsageClass,
    pub object_key: Box<[u8]>,
    pub session_id: Option<SessionId>,
    pub byte_len: u64,
}

pub(crate) const WAL_CHECKPOINT_PAGE_LIMIT: u64 = 2_000;
const CHECKPOINT_BUSY_RETRIES: usize = 8;
const WAL_HEADER_BYTES: u64 = 32;
const WAL_FRAME_HEADER_BYTES: u64 = 24;

impl SqliteIndex {
    pub(crate) fn live_index_page_bytes(&self) -> krometrail_core::Result<u64> {
        let connection = self.read_connection()?;
        sqlite_page_usage(&connection).map(|(live, _)| live)
    }

    /// Checkpoint the metadata WAL when its on-disk frame count exceeds the
    /// bounded policy. The common path performs only a sidecar metadata probe;
    /// a concurrent reader may make the truncate busy, in which case the next
    /// mutation retries it. This housekeeping must never fail the mutation that
    /// already committed.
    pub(crate) fn checkpoint_if_wal_exceeds(
        &self,
        max_wal_pages: u64,
    ) -> krometrail_core::Result<u64> {
        if max_wal_pages == 0 {
            return Err(persistence_error(
                "metadata WAL checkpoint threshold must be greater than zero",
            ));
        }
        let wal_pages = match self.wal_frame_count() {
            Ok(pages) => pages,
            Err(error) => {
                tracing::warn!(error = %error.message, "could not inspect metadata WAL for periodic checkpoint");
                return Ok(0);
            }
        };
        if wal_pages <= max_wal_pages {
            return Ok(0);
        }
        match self.try_checkpoint_truncate() {
            Ok(Some(bytes)) => Ok(bytes),
            Ok(None) => {
                tracing::debug!(wal_pages, "metadata WAL checkpoint deferred while busy");
                Ok(0)
            }
            Err(error) => {
                tracing::warn!(error = %error.message, "periodic metadata WAL checkpoint deferred");
                Ok(0)
            }
        }
    }

    /// Force the metadata WAL through SQLite's durability barrier. A reader
    /// snapshot can make TRUNCATE return busy, so required barriers retry briefly
    /// before reporting that durability could not be completed.
    pub(crate) fn checkpoint_truncate(&self) -> krometrail_core::Result<u64> {
        for attempt in 0..CHECKPOINT_BUSY_RETRIES {
            match self.try_checkpoint_truncate()? {
                Some(bytes) => return Ok(bytes),
                None if attempt + 1 < CHECKPOINT_BUSY_RETRIES => {
                    thread::sleep(Duration::from_millis(2));
                }
                None => break,
            }
        }
        Err(persistence_error("metadata WAL checkpoint remained busy"))
    }

    fn try_checkpoint_truncate(&self) -> krometrail_core::Result<Option<u64>> {
        let connection = self.connection()?;
        let (busy, _log_pages, checkpointed_pages): (i64, i64, i64) = connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|_| persistence_error("could not checkpoint metadata WAL"))?;
        if busy != 0 {
            return Ok(None);
        }
        u64::try_from(checkpointed_pages)
            .ok()
            .and_then(|pages| pages.checked_mul(self.wal_page_size))
            .ok_or_else(|| persistence_error("metadata WAL checkpoint size overflow"))
            .map(Some)
    }

    fn wal_frame_count(&self) -> krometrail_core::Result<u64> {
        let length = match fs::metadata(self.wal_path()) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(_) => return Err(persistence_error("could not inspect metadata WAL")),
        };
        if length <= WAL_HEADER_BYTES {
            return Ok(0);
        }
        let frame_bytes = self
            .wal_page_size
            .checked_add(WAL_FRAME_HEADER_BYTES)
            .ok_or_else(|| persistence_error("metadata WAL frame size overflow"))?;
        Ok((length - WAL_HEADER_BYTES) / frame_bytes)
    }

    fn wal_path(&self) -> PathBuf {
        let name = self
            .database_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("index.sqlite3");
        self.database_path.with_file_name(format!("{name}-wal"))
    }

    pub(crate) fn session_usage_bytes(
        &self,
        session_id: SessionId,
    ) -> krometrail_core::Result<u64> {
        let connection = self.read_connection()?;
        let mut statement = connection
            .prepare("SELECT byte_len_be FROM usage WHERE session_id=?1")
            .map_err(|_| persistence_error("could not prepare session usage lookup"))?;
        let rows = statement
            .query_map(params![codec::id(session_id.as_uuid()).to_vec()], |row| {
                row.get::<_, Vec<u8>>(0)
            })
            .map_err(|_| persistence_error("could not query session usage"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| persistence_error("could not read session usage"))?
            .into_iter()
            .try_fold(0_u64, |total, raw| {
                total
                    .checked_add(codec::decode_u64(&raw)?)
                    .ok_or_else(|| persistence_error("session usage overflow"))
            })
    }

    pub(crate) fn remove_frame_rows(
        &self,
        segment_id: SegmentId,
        from_offset: Option<ByteOffset>,
    ) -> krometrail_core::Result<Vec<FrameId>> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin frame-index maintenance"))?;
        // Derived artifacts are invalidated before authoritative frame rows. Their managed
        // files are removed by artifact recovery if this maintenance runs during startup.
        super::artifacts::purge_artifacts_for_segment_tx(&transaction, segment_id)?;
        let mut statement = transaction
            .prepare(
                "SELECT frame_id FROM frames WHERE segment_id=?1 \
                 AND (?2 IS NULL OR byte_offset_be>=?2) ORDER BY byte_offset_be, frame_id",
            )
            .map_err(|_| persistence_error("could not prepare frame-index maintenance"))?;
        let rows = statement
            .query_map(
                params![
                    codec::id(segment_id.as_uuid()).to_vec(),
                    from_offset.map(|value| codec::u64_blob(value.get()).to_vec())
                ],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .map_err(|_| persistence_error("could not query frame-index maintenance"))?;
        let ids: Vec<FrameId> = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| persistence_error("could not read frame-index maintenance"))?
            .into_iter()
            .map(|value| codec::decode_id(&value).map(FrameId::from_uuid))
            .collect::<krometrail_core::Result<_>>()?;
        drop(statement);
        for chunk in ids.chunks(900) {
            let keys: Vec<Vec<u8>> = chunk
                .iter()
                .map(|frame_id| codec::id(frame_id.as_uuid()).to_vec())
                .collect();
            let placeholders = std::iter::repeat_n("?", keys.len())
                .collect::<Vec<_>>()
                .join(",");
            transaction
                .execute(
                    &format!(
                        "DELETE FROM timeline_observations WHERE kind='frame' AND payload_sort_key IN ({placeholders})"
                    ),
                    params_from_iter(keys.iter()),
                )
                .map_err(|_| persistence_error("could not remove frame timeline metadata"))?;
        }
        for frame_id in &ids {
            transaction
                .execute(
                    "DELETE FROM frames WHERE frame_id=?1",
                    params![codec::id(frame_id.as_uuid()).to_vec()],
                )
                .map_err(|_| persistence_error("could not remove frame metadata"))?;
        }
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit frame-index maintenance"))?;
        Ok(ids)
    }

    pub(crate) fn remove_segment(&self, segment_id: SegmentId) -> krometrail_core::Result<()> {
        let connection = self.connection()?;
        connection
            .execute(
                "DELETE FROM segments WHERE segment_id=?1",
                params![codec::id(segment_id.as_uuid()).to_vec()],
            )
            .map_err(|_| persistence_error("could not remove segment metadata"))?;
        Ok(())
    }

    pub(crate) fn update_usage(&self, entry: UsageEntry) -> krometrail_core::Result<()> {
        if entry.object_key.is_empty() {
            return Err(persistence_error("usage object key must not be empty"));
        }
        let connection = self.connection()?;
        connection
            .execute(
                "INSERT INTO usage(class, object_key, session_id, byte_len_be) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(class, object_key) DO UPDATE SET \
                    session_id=excluded.session_id, byte_len_be=excluded.byte_len_be",
                params![
                    entry.class.as_str(),
                    entry.object_key,
                    entry
                        .session_id
                        .map(|value| codec::id(value.as_uuid()).to_vec()),
                    codec::u64_blob(entry.byte_len).to_vec(),
                ],
            )
            .map_err(|_| persistence_error("could not update usage metadata"))?;
        Ok(())
    }

    pub(crate) fn remove_usage(
        &self,
        class: UsageClass,
        object_key: &[u8],
    ) -> krometrail_core::Result<()> {
        let connection = self.connection()?;
        connection
            .execute(
                "DELETE FROM usage WHERE class=?1 AND object_key=?2",
                params![class.as_str(), object_key],
            )
            .map_err(|_| persistence_error("could not remove usage metadata"))?;
        Ok(())
    }

    #[cfg(test)]
    fn usage_bytes(
        &self,
        class: UsageClass,
        object_key: &[u8],
    ) -> krometrail_core::Result<Option<u64>> {
        use rusqlite::OptionalExtension as _;

        let connection = self.connection()?;
        let value: Option<Vec<u8>> = connection
            .query_row(
                "SELECT byte_len_be FROM usage WHERE class=?1 AND object_key=?2",
                params![class.as_str(), object_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| persistence_error("could not query usage metadata"))?;
        value.as_deref().map(codec::decode_u64).transpose()
    }
}

pub(crate) fn sqlite_page_usage(
    connection: &rusqlite::Connection,
) -> krometrail_core::Result<(u64, u64)> {
    let page_count: u64 = connection
        .pragma_query_value(None, "page_count", |row| row.get(0))
        .map_err(|_| persistence_error("could not read index page count"))?;
    let freelist_count: u64 = connection
        .pragma_query_value(None, "freelist_count", |row| row.get(0))
        .map_err(|_| persistence_error("could not read index freelist count"))?;
    let page_size: u64 = connection
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .map_err(|_| persistence_error("could not read index page size"))?;
    let live = page_count
        .checked_sub(freelist_count)
        .and_then(|pages| pages.checked_mul(page_size))
        .ok_or_else(|| persistence_error("index live-page usage overflow"))?;
    let reusable = freelist_count
        .checked_mul(page_size)
        .ok_or_else(|| persistence_error("index freelist usage overflow"))?;
    Ok((live, reusable))
}

#[cfg(test)]
mod tests {

    fn recording_test_clock() -> std::sync::Arc<dyn krometrail_core::MonotonicClock> {
        struct Fixed;
        impl krometrail_core::MonotonicClock for Fixed {
            fn now(&self) -> krometrail_core::ObservedTime {
                krometrail_core::ObservedTime::from_nanos(0)
            }
        }
        std::sync::Arc::new(Fixed)
    }
    use std::{sync::Arc, time::Duration};

    use krometrail_core::{
        CaptureOrdinal, CapturedFrame, DeviceScaleFactor, EncodedFrame, FrameSource, ImageFormat,
        ObservedTime, PixelDimensions, RecordingSink, SessionTime, TargetId,
    };
    use tempfile::TempDir;
    use uuid::Uuid;

    use crate::{
        IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    };

    use super::*;

    fn frame(session: SessionId, target: TargetId, id: u128, ordinal: u64) -> EncodedFrame {
        EncodedFrame::new(
            CapturedFrame::new(
                FrameId::from_uuid(Uuid::from_u128(id)),
                session,
                target,
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
            vec![ordinal as u8],
        )
        .unwrap()
    }

    #[tokio::test]
    async fn frame_segment_and_usage_maintenance_is_composable_and_idempotent() {
        let directory = TempDir::new().unwrap();
        let segments = directory.path().join("segments");
        let index = Arc::new(
            SqliteIndex::open(IndexStoreConfig {
                database_path: directory.path().join("index.sqlite3"),
                segments_directory: segments.clone(),
                busy_timeout: Duration::from_secs(1),
            })
            .unwrap(),
        );
        let sink = RecordingStore::new(
            Arc::new(
                SegmentWriter::open(SegmentStoreConfig {
                    directory: segments,
                    rotation: RotationConfig::suggested(),
                })
                .unwrap(),
            ),
            Arc::clone(&index),
            recording_test_clock(),
        )
        .unwrap();
        let session = SessionId::from_uuid(Uuid::from_u128(1));
        let target = TargetId::from_uuid(Uuid::from_u128(2));
        let first = frame(session, target, 3, 1);
        let second = frame(session, target, 4, 2);
        let first_address = sink.append_frame(first.clone()).await.unwrap();
        let second_address = sink.append_frame(second.clone()).await.unwrap();
        assert!(index.remove_segment(first_address.segment_id).is_err());
        assert_eq!(
            index
                .remove_frame_rows(first_address.segment_id, Some(second_address.byte_offset))
                .unwrap(),
            [second.metadata().id()]
        );
        assert!(
            index
                .frames_by_id(vec![second.metadata().id()])
                .await
                .is_err()
        );
        assert_eq!(
            index
                .remove_frame_rows(first_address.segment_id, None)
                .unwrap(),
            [first.metadata().id()]
        );
        index.remove_segment(first_address.segment_id).unwrap();
        index.remove_segment(first_address.segment_id).unwrap();
        let connection = index.connection().unwrap();
        let remaining: (u32, u32) = connection
            .query_row(
                "SELECT (SELECT count(*) FROM frames), \
                        (SELECT count(*) FROM timeline_observations WHERE kind='frame')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(remaining, (0, 0));
        drop(connection);

        for class in [
            UsageClass::Segment,
            UsageClass::Index,
            UsageClass::BrowserEvent,
            UsageClass::Artifact,
        ] {
            index
                .update_usage(UsageEntry {
                    class,
                    object_key: vec![class as u8 + 1].into_boxed_slice(),
                    session_id: Some(session),
                    byte_len: u64::MAX,
                })
                .unwrap();
            assert_eq!(
                index.usage_bytes(class, &[class as u8 + 1]).unwrap(),
                Some(u64::MAX)
            );
            index.remove_usage(class, &[class as u8 + 1]).unwrap();
            index.remove_usage(class, &[class as u8 + 1]).unwrap();
        }
    }
}
