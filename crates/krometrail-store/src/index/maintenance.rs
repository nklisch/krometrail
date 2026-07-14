use krometrail_core::{ByteOffset, FrameId, SegmentId, SessionId};
use rusqlite::params;

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

impl SqliteIndex {
    pub(crate) fn refresh_index_usage(&self) -> krometrail_core::Result<u64> {
        // A checkpoint makes SQLite pages, rather than transient WAL length, the
        // accounting authority. Browser-event usage is a classified subset of
        // those live pages and is subtracted once from the index class.
        let mut connection = self.connection()?;
        let checkpoint_busy: i64 = connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
            .map_err(|_| persistence_error("could not checkpoint index usage"))?;
        if checkpoint_busy != 0 {
            return Err(persistence_error("index usage checkpoint is busy"));
        }
        let (live_bytes, _) = sqlite_page_usage(&connection)?;
        let browser_event_bytes = class_usage(&connection, UsageClass::BrowserEvent)?;
        let index_bytes = live_bytes
            .checked_sub(browser_event_bytes)
            .ok_or_else(|| persistence_error("browser event usage exceeds live index pages"))?;
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin index usage refresh"))?;
        transaction
            .execute(
                "DELETE FROM usage WHERE class='index' AND object_key!=?1",
                params![b"live-pages".as_slice()],
            )
            .map_err(|_| persistence_error("could not remove legacy index usage"))?;
        transaction
            .execute(
                "INSERT INTO usage(class, object_key, session_id, byte_len_be) \
                 VALUES ('index', ?1, NULL, ?2) \
                 ON CONFLICT(class, object_key) DO UPDATE SET byte_len_be=excluded.byte_len_be \
                 WHERE usage.byte_len_be != excluded.byte_len_be",
                params![
                    b"live-pages".as_slice(),
                    codec::u64_blob(index_bytes).to_vec()
                ],
            )
            .map_err(|_| persistence_error("could not refresh index usage"))?;
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit index usage refresh"))?;
        Ok(live_bytes)
    }

    pub(crate) fn session_usage_bytes(
        &self,
        session_id: SessionId,
    ) -> krometrail_core::Result<u64> {
        let connection = self.connection()?;
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
        for frame_id in &ids {
            let payload_json =
                serde_json::to_string(&krometrail_core::ObservationPayloadRef::Frame(*frame_id))
                    .map_err(|_| persistence_error("could not encode frame timeline reference"))?;
            transaction
                .execute(
                    "DELETE FROM timeline_observations WHERE kind='frame' AND payload_json=?1",
                    params![payload_json],
                )
                .map_err(|_| persistence_error("could not remove frame timeline metadata"))?;
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

fn class_usage(
    connection: &rusqlite::Connection,
    class: UsageClass,
) -> krometrail_core::Result<u64> {
    let mut statement = connection
        .prepare("SELECT byte_len_be FROM usage WHERE class=?1 ORDER BY object_key")
        .map_err(|_| persistence_error("could not prepare class usage lookup"))?;
    let rows = statement
        .query_map(params![class.as_str()], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|_| persistence_error("could not query class usage"))?;
    let mut total = 0_u64;
    for row in rows {
        let raw = row.map_err(|_| persistence_error("could not read class usage"))?;
        total = total
            .checked_add(codec::decode_u64(&raw)?)
            .ok_or_else(|| persistence_error("class usage overflow"))?;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
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
