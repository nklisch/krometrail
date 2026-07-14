use krometrail_core::{ArtifactId, ByteOffset, FrameId, SegmentId, SessionId};
use rusqlite::params;

use super::{SqliteIndex, codec};
use crate::persistence_error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UsageClass {
    Segment,
    Index,
    BrowserEvent,
    Artifact,
}

impl UsageClass {
    const fn as_str(self) -> &'static str {
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
    pub(crate) fn remove_frame_rows(
        &self,
        segment_id: SegmentId,
        from_offset: Option<ByteOffset>,
    ) -> krometrail_core::Result<Vec<FrameId>> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin frame-index maintenance"))?;
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

    pub(crate) fn remove_artifact(&self, artifact_id: ArtifactId) -> krometrail_core::Result<()> {
        let connection = self.connection()?;
        connection
            .execute(
                "DELETE FROM artifacts WHERE artifact_id=?1",
                params![codec::id(artifact_id.as_uuid()).to_vec()],
            )
            .map_err(|_| persistence_error("could not remove artifact metadata"))?;
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
        IndexStoreConfig, IndexedRecordingSink, RotationConfig, SegmentStoreConfig, SegmentWriter,
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
        let sink = IndexedRecordingSink::new(
            Arc::new(
                SegmentWriter::open(SegmentStoreConfig {
                    directory: segments,
                    rotation: RotationConfig::suggested(),
                })
                .unwrap(),
            ),
            Arc::clone(&index),
        );
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

        let artifact_id = ArtifactId::from_uuid(Uuid::from_u128(10));
        {
            let connection = index.connection().unwrap();
            connection.execute(
                "INSERT INTO artifacts(artifact_id, session_id, target_id, kind, start_time_be, \
                 end_time_be, manifest_json, relative_path, byte_len_be) \
                 VALUES (?1, ?2, ?3, 'storyboard', ?4, ?4, '{}', 'artifact.png', ?5)",
                params![
                    codec::id(artifact_id.as_uuid()).to_vec(),
                    codec::id(session.as_uuid()).to_vec(),
                    codec::id(target.as_uuid()).to_vec(),
                    codec::u64_blob(1).to_vec(),
                    codec::u64_blob(u64::MAX).to_vec(),
                ],
            ).unwrap();
        }
        index.remove_artifact(artifact_id).unwrap();
        index.remove_artifact(artifact_id).unwrap();

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
