use krometrail_core::{ArtifactId, SegmentId, SessionId, SessionRange, SessionTime, TargetId};
use rusqlite::{OptionalExtension, TransactionBehavior, params, params_from_iter};
use uuid::Uuid;

use crate::persistence_error;

use super::{
    SqliteIndex, codec, range::record_evicted_frame_range_tx, retention::validate_file_name,
};

type RawEvictedFrameRange = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeletionKind {
    Eviction,
    Session,
}

impl DeletionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Eviction => "eviction",
            Self::Session => "session",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeletionState {
    Prepared,
    MetadataRemoved,
}

impl DeletionState {
    fn decode(value: &str) -> krometrail_core::Result<Self> {
        match value {
            "prepared" => Ok(Self::Prepared),
            "metadata_removed" => Ok(Self::MetadataRemoved),
            _ => Err(persistence_error("stored deletion state is malformed")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeletionObjectKind {
    Segment(SegmentId),
    Artifact(ArtifactId),
}

impl DeletionObjectKind {
    fn parts(self) -> (&'static str, Vec<u8>, &'static str, Vec<u8>) {
        match self {
            Self::Segment(id) => {
                let key = codec::id(id.as_uuid()).to_vec();
                ("segment", key.clone(), "segment", key)
            }
            Self::Artifact(id) => {
                let key = codec::id(id.as_uuid()).to_vec();
                ("artifact", key.clone(), "artifact", key)
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DeletionObject {
    pub kind: DeletionObjectKind,
    pub relative_path: String,
    pub byte_len: u64,
    pub session_id: SessionId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DeletionBatch {
    pub batch_id: Uuid,
    pub kind: DeletionKind,
    pub session_id: Option<SessionId>,
    pub state: DeletionState,
    pub objects: Vec<DeletionObject>,
}

impl SqliteIndex {
    pub(crate) fn prepare_deletion(
        &self,
        kind: DeletionKind,
        session_id: Option<SessionId>,
        objects: Vec<DeletionObject>,
    ) -> krometrail_core::Result<DeletionBatch> {
        if objects.is_empty() && kind != DeletionKind::Session {
            return Err(persistence_error(
                "eviction deletion batch must not be empty",
            ));
        }
        if (kind == DeletionKind::Session) != session_id.is_some() {
            return Err(persistence_error(
                "session deletion batch identity is inconsistent",
            ));
        }
        for object in &objects {
            validate_file_name(&object.relative_path)?;
        }
        let batch_id = Uuid::new_v4();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin deletion preparation"))?;
        transaction
            .execute(
                "INSERT INTO deletion_batches(batch_id, kind, session_id, state) \
                 VALUES (?1, ?2, ?3, 'prepared')",
                params![
                    codec::id(&batch_id).to_vec(),
                    kind.as_str(),
                    session_id.map(|id| codec::id(id.as_uuid()).to_vec()),
                ],
            )
            .map_err(|_| persistence_error("could not create deletion batch"))?;
        for (position, object) in objects.iter().enumerate() {
            let (object_kind, object_key, usage_class, usage_key) = object.kind.parts();
            transaction
                .execute(
                    "INSERT INTO deletion_objects(\
                        batch_id, position, kind, object_key, relative_path, byte_len_be, \
                        usage_class, usage_key, session_id\
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        codec::id(&batch_id).to_vec(),
                        i64::try_from(position)
                            .map_err(|_| persistence_error("too many deletion objects"))?,
                        object_kind,
                        object_key,
                        object.relative_path,
                        codec::u64_blob(object.byte_len).to_vec(),
                        usage_class,
                        usage_key,
                        codec::id(object.session_id.as_uuid()).to_vec(),
                    ],
                )
                .map_err(|_| persistence_error("could not record deletion object"))?;
        }
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit deletion preparation"))?;
        Ok(DeletionBatch {
            batch_id,
            kind,
            session_id,
            state: DeletionState::Prepared,
            objects,
        })
    }

    pub(crate) fn deletion_batches(&self) -> krometrail_core::Result<Vec<DeletionBatch>> {
        let connection = self.read_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT batch_id, kind, session_id, state FROM deletion_batches \
                 ORDER BY batch_id",
            )
            .map_err(|_| persistence_error("could not prepare deletion batch lookup"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|_| persistence_error("could not query deletion batches"))?;
        let raw = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| persistence_error("could not read deletion batches"))?;
        raw.into_iter()
            .map(|(batch, kind, session, state)| {
                let batch_id = codec::decode_id(&batch)?;
                let kind = match kind.as_str() {
                    "eviction" => DeletionKind::Eviction,
                    "session" => DeletionKind::Session,
                    _ => return Err(persistence_error("stored deletion kind is malformed")),
                };
                let session_id = session
                    .as_deref()
                    .map(codec::decode_id)
                    .transpose()?
                    .map(SessionId::from_uuid);
                let objects = deletion_objects(&connection, batch_id)?;
                Ok(DeletionBatch {
                    batch_id,
                    kind,
                    session_id,
                    state: DeletionState::decode(&state)?,
                    objects,
                })
            })
            .collect()
    }

    pub(crate) fn remove_deletion_metadata(
        &self,
        batch: &DeletionBatch,
    ) -> krometrail_core::Result<(u64, u64, u64)> {
        if batch.state == DeletionState::MetadataRemoved {
            return Ok((0, 0, 0));
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin deletion metadata removal"))?;
        let mut segments = 0_u64;
        let mut frames = 0_u64;
        let mut artifacts = 0_u64;
        for object in &batch.objects {
            match object.kind {
                DeletionObjectKind::Artifact(id) => {
                    artifacts += transaction
                        .execute(
                            "DELETE FROM artifacts WHERE artifact_id=?1",
                            params![codec::id(id.as_uuid()).to_vec()],
                        )
                        .map_err(|_| persistence_error("could not remove artifact metadata"))?
                        as u64;
                }
                DeletionObjectKind::Segment(id) => {
                    let segment_key = codec::id(id.as_uuid()).to_vec();
                    if batch.kind == DeletionKind::Eviction {
                        let removed_range: Option<RawEvictedFrameRange> = transaction
                            .query_row(
                                "SELECT session_id, target_id, min(session_time_be), \
                                            max(session_time_be) \
                                     FROM frames WHERE segment_id=?1 \
                                     GROUP BY session_id, target_id",
                                params![&segment_key],
                                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                            )
                            .optional()
                            .map_err(|_| {
                                persistence_error("could not query evicted frame interval")
                            })?;
                        if let Some((session, target, start, end)) = removed_range {
                            record_evicted_frame_range_tx(
                                &transaction,
                                SessionId::from_uuid(codec::decode_id(&session)?),
                                TargetId::from_uuid(codec::decode_id(&target)?),
                                SessionRange::new(
                                    SessionTime::from_nanos(codec::decode_u64(&start)?),
                                    SessionTime::from_nanos(codec::decode_u64(&end)?),
                                )
                                .map_err(|_| {
                                    persistence_error("evicted frame interval is invalid")
                                })?,
                            )?;
                        }
                    }
                    let frame_count: u64 = transaction
                        .query_row(
                            "SELECT count(*) FROM frames WHERE segment_id=?1",
                            params![&segment_key],
                            |row| row.get(0),
                        )
                        .map_err(|_| persistence_error("could not count deleted frames"))?;
                    // Frame timeline references use the frame id as their binary
                    // sort key. Delete them in bounded sets before frame rows.
                    let mut refs = transaction
                        .prepare("SELECT frame_id FROM frames WHERE segment_id=?1")
                        .map_err(|_| {
                            persistence_error("could not prepare deleted frame references")
                        })?;
                    let ids = refs
                        .query_map(params![&segment_key], |row| row.get::<_, Vec<u8>>(0))
                        .map_err(|_| persistence_error("could not query deleted frame references"))?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| {
                            persistence_error("could not read deleted frame references")
                        })?;
                    drop(refs);
                    for chunk in ids.chunks(900) {
                        let placeholders = std::iter::repeat_n("?", chunk.len())
                            .collect::<Vec<_>>()
                            .join(",");
                        transaction
                            .execute(
                                &format!(
                                    "DELETE FROM timeline_observations WHERE kind='frame' AND payload_sort_key IN ({placeholders})"
                                ),
                                params_from_iter(chunk.iter()),
                            )
                            .map_err(|_| persistence_error("could not remove frame timeline metadata"))?;
                    }
                    transaction
                        .execute(
                            "DELETE FROM frames WHERE segment_id=?1",
                            params![&segment_key],
                        )
                        .map_err(|_| persistence_error("could not remove frame metadata"))?;
                    segments += transaction
                        .execute(
                            "DELETE FROM segments WHERE segment_id=?1",
                            params![segment_key],
                        )
                        .map_err(|_| persistence_error("could not remove segment metadata"))?
                        as u64;
                    frames = frames
                        .checked_add(frame_count)
                        .ok_or_else(|| persistence_error("deleted frame count overflow"))?;
                }
            }
        }
        if let Some(session_id) = batch.session_id {
            let key = codec::id(session_id.as_uuid()).to_vec();
            transaction
                .execute("DELETE FROM artifacts WHERE session_id=?1", params![&key])
                .map_err(|_| persistence_error("could not remove session artifacts"))?;
            transaction
                .execute(
                    "DELETE FROM capture_gaps WHERE session_id=?1",
                    params![&key],
                )
                .map_err(|_| persistence_error("could not remove session gaps"))?;
            transaction
                .execute(
                    "DELETE FROM timeline_observations WHERE session_id=?1",
                    params![&key],
                )
                .map_err(|_| persistence_error("could not remove session timeline"))?;
            transaction
                .execute("DELETE FROM pins WHERE session_id=?1", params![&key])
                .map_err(|_| persistence_error("could not remove session pins"))?;
            transaction
                .execute("DELETE FROM targets WHERE session_id=?1", params![&key])
                .map_err(|_| persistence_error("could not remove session targets"))?;
            transaction
                .execute("DELETE FROM sessions WHERE session_id=?1", params![key])
                .map_err(|_| persistence_error("could not remove session catalog"))?;
        }
        transaction
            .execute(
                "UPDATE deletion_batches SET state='metadata_removed' WHERE batch_id=?1",
                params![codec::id(&batch.batch_id).to_vec()],
            )
            .map_err(|_| persistence_error("could not advance deletion batch"))?;
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit deletion metadata removal"))?;
        Ok((segments, frames, artifacts))
    }

    pub(crate) fn finalize_deletion(&self, batch: &DeletionBatch) -> krometrail_core::Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin deletion finalization"))?;
        for object in &batch.objects {
            let (_, _, usage_class, usage_key) = object.kind.parts();
            transaction
                .execute(
                    "DELETE FROM usage WHERE class=?1 AND object_key=?2",
                    params![usage_class, usage_key],
                )
                .map_err(|_| persistence_error("could not finalize deleted usage"))?;
        }
        if let Some(session_id) = batch.session_id {
            transaction
                .execute(
                    "DELETE FROM usage WHERE session_id=?1",
                    params![codec::id(session_id.as_uuid()).to_vec()],
                )
                .map_err(|_| persistence_error("could not finalize session usage"))?;
        }
        transaction
            .execute(
                "DELETE FROM deletion_batches WHERE batch_id=?1",
                params![codec::id(&batch.batch_id).to_vec()],
            )
            .map_err(|_| persistence_error("could not remove deletion journal"))?;
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit deletion finalization"))
    }
}

fn deletion_objects(
    connection: &rusqlite::Connection,
    batch_id: Uuid,
) -> krometrail_core::Result<Vec<DeletionObject>> {
    let mut statement = connection
        .prepare(
            "SELECT kind, object_key, relative_path, byte_len_be, session_id \
             FROM deletion_objects WHERE batch_id=?1 ORDER BY position",
        )
        .map_err(|_| persistence_error("could not prepare deletion object lookup"))?;
    let rows = statement
        .query_map(params![codec::id(&batch_id).to_vec()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, Vec<u8>>(4)?,
            ))
        })
        .map_err(|_| persistence_error("could not query deletion objects"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read deletion objects"))?
        .into_iter()
        .map(|(kind, key, relative_path, bytes, session)| {
            validate_file_name(&relative_path)?;
            let id = codec::decode_id(&key)?;
            let kind = match kind.as_str() {
                "segment" => DeletionObjectKind::Segment(SegmentId::from_uuid(id)),
                "artifact" => DeletionObjectKind::Artifact(ArtifactId::from_uuid(id)),
                _ => {
                    return Err(persistence_error(
                        "stored deletion object kind is malformed",
                    ));
                }
            };
            Ok(DeletionObject {
                kind,
                relative_path,
                byte_len: codec::decode_u64(&bytes)?,
                session_id: SessionId::from_uuid(codec::decode_id(&session)?),
            })
        })
        .collect()
}
