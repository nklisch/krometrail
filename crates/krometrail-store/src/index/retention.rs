use std::collections::{BTreeMap, BTreeSet};

use krometrail_core::{
    ArtifactId, PinChange, RetainedPoint, RetentionRange, SegmentId, SessionId, SessionTime,
    StorageUsage, TargetId,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::persistence_error;

use super::{SqliteIndex, codec};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SegmentCandidate {
    pub segment_id: SegmentId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub relative_path: String,
    pub file_bytes: u64,
    pub retention_sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ArtifactCandidate {
    pub artifact_id: ArtifactId,
    pub session_id: SessionId,
    pub relative_path: String,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UsageSnapshot {
    pub usage: StorageUsage,
    pub pinned_usage_bytes: u64,
    pub oldest_retained: Option<RetainedPoint>,
    pub newest_retained: Option<RetainedPoint>,
    pub open_segment_count: u64,
}

impl SqliteIndex {
    pub(crate) fn pin_range(&self, request: RetentionRange) -> krometrail_core::Result<PinChange> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin range pin transaction"))?;
        let pin_id = ensure_pin(&transaction, request)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO pin_segments(pin_id, segment_id) \
                 SELECT ?1, segment_id FROM segments \
                 WHERE session_id=?2 AND target_id=?3 AND state='sealed' \
                   AND start_time_be<=?4 AND end_time_be>=?5",
                params![
                    pin_id,
                    codec::id(request.session_id.as_uuid()).to_vec(),
                    codec::id(request.target_id.as_uuid()).to_vec(),
                    codec::u64_blob(request.range.end().as_nanos()).to_vec(),
                    codec::u64_blob(request.range.start().as_nanos()).to_vec(),
                ],
            )
            .map_err(|_| persistence_error("could not protect range segments"))?;
        let protected_segments = pin_segments(&transaction, pin_id)?;
        let pinned_usage_bytes = pinned_usage(&transaction)?;
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit range pin transaction"))?;
        Ok(PinChange {
            request,
            protected_segments,
            pinned_usage_bytes,
        })
    }

    pub(crate) fn unpin_range(
        &self,
        request: RetentionRange,
    ) -> krometrail_core::Result<PinChange> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin range unpin transaction"))?;
        let pin_id: Option<i64> = transaction
            .query_row(
                "SELECT pin_id FROM pins WHERE session_id=?1 AND target_id=?2 \
                 AND start_time_be=?3 AND end_time_be=?4",
                params![
                    codec::id(request.session_id.as_uuid()).to_vec(),
                    codec::id(request.target_id.as_uuid()).to_vec(),
                    codec::u64_blob(request.range.start().as_nanos()).to_vec(),
                    codec::u64_blob(request.range.end().as_nanos()).to_vec(),
                ],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| persistence_error("could not query range pin"))?;
        let protected_segments = pin_id
            .map(|id| pin_segments(&transaction, id))
            .transpose()?
            .unwrap_or_default();
        if let Some(pin_id) = pin_id {
            transaction
                .execute("DELETE FROM pins WHERE pin_id=?1", params![pin_id])
                .map_err(|_| persistence_error("could not remove range pin"))?;
        }
        let pinned_usage_bytes = pinned_usage(&transaction)?;
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit range unpin transaction"))?;
        Ok(PinChange {
            request,
            protected_segments,
            pinned_usage_bytes,
        })
    }

    pub(crate) fn oldest_unpinned_segment(
        &self,
    ) -> krometrail_core::Result<Option<SegmentCandidate>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT s.segment_id, s.session_id, s.target_id, s.relative_path, \
                        s.file_bytes_be, s.retention_sequence \
                 FROM segments s WHERE s.state='sealed' \
                   AND NOT EXISTS (SELECT 1 FROM pin_segments p WHERE p.segment_id=s.segment_id) \
                 ORDER BY s.retention_sequence ASC, s.segment_id ASC LIMIT 1",
                [],
                decode_segment_candidate,
            )
            .optional()
            .map_err(|_| persistence_error("could not select oldest unpinned segment"))?
            .map(decode_segment_candidate_parts)
            .transpose()
    }

    pub(crate) fn artifacts_for_segment(
        &self,
        segment_id: SegmentId,
    ) -> krometrail_core::Result<Vec<ArtifactCandidate>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT a.artifact_id, a.session_id, a.relative_path, a.byte_len_be \
                 FROM artifacts a JOIN artifact_frames af USING(artifact_id) \
                 JOIN frames f USING(frame_id) WHERE f.segment_id=?1 ORDER BY a.artifact_id",
            )
            .map_err(|_| persistence_error("could not prepare artifact provenance lookup"))?;
        let rows = statement
            .query_map(params![codec::id(segment_id.as_uuid()).to_vec()], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            })
            .map_err(|_| persistence_error("could not query artifact provenance"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| persistence_error("could not read artifact provenance"))?
            .into_iter()
            .map(|(artifact, session, relative_path, bytes)| {
                validate_file_name(&relative_path)?;
                Ok(ArtifactCandidate {
                    artifact_id: ArtifactId::from_uuid(codec::decode_id(&artifact)?),
                    session_id: SessionId::from_uuid(codec::decode_id(&session)?),
                    relative_path,
                    byte_len: codec::decode_u64(&bytes)?,
                })
            })
            .collect()
    }

    pub(crate) fn oldest_artifact(&self) -> krometrail_core::Result<Option<ArtifactCandidate>> {
        let connection = self.connection()?;
        let raw = connection
            .query_row(
                "SELECT artifact_id, session_id, relative_path, byte_len_be FROM artifacts \
                 ORDER BY start_time_be ASC, artifact_id ASC LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| persistence_error("could not select oldest artifact"))?;
        raw.map(|(artifact, session, relative_path, bytes)| {
            validate_file_name(&relative_path)?;
            Ok(ArtifactCandidate {
                artifact_id: ArtifactId::from_uuid(codec::decode_id(&artifact)?),
                session_id: SessionId::from_uuid(codec::decode_id(&session)?),
                relative_path,
                byte_len: codec::decode_u64(&bytes)?,
            })
        })
        .transpose()
    }

    pub(crate) fn usage_snapshot(&self) -> krometrail_core::Result<UsageSnapshot> {
        let connection = self.connection()?;
        usage_snapshot(&connection)
    }

    pub(crate) fn session_segments(
        &self,
        session_id: SessionId,
    ) -> krometrail_core::Result<Vec<SegmentCandidate>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT segment_id, session_id, target_id, relative_path, file_bytes_be, \
                        retention_sequence FROM segments WHERE session_id=?1 \
                 ORDER BY retention_sequence, segment_id",
            )
            .map_err(|_| persistence_error("could not prepare session segment lookup"))?;
        let rows = statement
            .query_map(
                params![codec::id(session_id.as_uuid()).to_vec()],
                decode_segment_candidate,
            )
            .map_err(|_| persistence_error("could not query session segments"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| persistence_error("could not read session segments"))?
            .into_iter()
            .map(decode_segment_candidate_parts)
            .collect()
    }

    pub(crate) fn session_artifacts(
        &self,
        session_id: SessionId,
    ) -> krometrail_core::Result<Vec<ArtifactCandidate>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT artifact_id, session_id, relative_path, byte_len_be FROM artifacts \
                 WHERE session_id=?1 ORDER BY artifact_id",
            )
            .map_err(|_| persistence_error("could not prepare session artifact lookup"))?;
        let rows = statement
            .query_map(params![codec::id(session_id.as_uuid()).to_vec()], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            })
            .map_err(|_| persistence_error("could not query session artifacts"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| persistence_error("could not read session artifacts"))?
            .into_iter()
            .map(|(artifact, session, relative_path, bytes)| {
                validate_file_name(&relative_path)?;
                Ok(ArtifactCandidate {
                    artifact_id: ArtifactId::from_uuid(codec::decode_id(&artifact)?),
                    session_id: SessionId::from_uuid(codec::decode_id(&session)?),
                    relative_path,
                    byte_len: codec::decode_u64(&bytes)?,
                })
            })
            .collect()
    }
}

type RawSegment = (Vec<u8>, Vec<u8>, Vec<u8>, String, Vec<u8>, i64);

fn decode_segment_candidate(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawSegment> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn decode_segment_candidate_parts(raw: RawSegment) -> krometrail_core::Result<SegmentCandidate> {
    validate_file_name(&raw.3)?;
    Ok(SegmentCandidate {
        segment_id: SegmentId::from_uuid(codec::decode_id(&raw.0)?),
        session_id: SessionId::from_uuid(codec::decode_id(&raw.1)?),
        target_id: TargetId::from_uuid(codec::decode_id(&raw.2)?),
        relative_path: raw.3,
        file_bytes: codec::decode_u64(&raw.4)?,
        retention_sequence: u64::try_from(raw.5)
            .map_err(|_| persistence_error("stored retention sequence is malformed"))?,
    })
}

fn ensure_pin(
    transaction: &Transaction<'_>,
    request: RetentionRange,
) -> krometrail_core::Result<i64> {
    transaction
        .execute(
            "INSERT OR IGNORE INTO pins(session_id, target_id, start_time_be, end_time_be) \
             VALUES (?1, ?2, ?3, ?4)",
            params![
                codec::id(request.session_id.as_uuid()).to_vec(),
                codec::id(request.target_id.as_uuid()).to_vec(),
                codec::u64_blob(request.range.start().as_nanos()).to_vec(),
                codec::u64_blob(request.range.end().as_nanos()).to_vec(),
            ],
        )
        .map_err(|_| persistence_error("could not create range pin"))?;
    transaction
        .query_row(
            "SELECT pin_id FROM pins WHERE session_id=?1 AND target_id=?2 \
             AND start_time_be=?3 AND end_time_be=?4",
            params![
                codec::id(request.session_id.as_uuid()).to_vec(),
                codec::id(request.target_id.as_uuid()).to_vec(),
                codec::u64_blob(request.range.start().as_nanos()).to_vec(),
                codec::u64_blob(request.range.end().as_nanos()).to_vec(),
            ],
            |row| row.get(0),
        )
        .map_err(|_| persistence_error("could not read range pin"))
}

fn pin_segments(connection: &Connection, pin_id: i64) -> krometrail_core::Result<Vec<SegmentId>> {
    let mut statement = connection
        .prepare("SELECT segment_id FROM pin_segments WHERE pin_id=?1 ORDER BY segment_id")
        .map_err(|_| persistence_error("could not prepare protected segment lookup"))?;
    let rows = statement
        .query_map(params![pin_id], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|_| persistence_error("could not query protected segments"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read protected segments"))?
        .into_iter()
        .map(|raw| codec::decode_id(&raw).map(SegmentId::from_uuid))
        .collect()
}

fn pinned_usage(connection: &Connection) -> krometrail_core::Result<u64> {
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT s.segment_id, s.file_bytes_be FROM segments s \
             JOIN pin_segments p USING(segment_id)",
        )
        .map_err(|_| persistence_error("could not prepare pinned usage lookup"))?;
    let rows = statement
        .query_map([], |row| row.get::<_, Vec<u8>>(1))
        .map_err(|_| persistence_error("could not query pinned usage"))?;
    checked_sum(
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| persistence_error("could not read pinned usage"))?,
    )
}

fn usage_snapshot(connection: &Connection) -> krometrail_core::Result<UsageSnapshot> {
    let mut classes: BTreeMap<String, u64> = BTreeMap::new();
    let mut statement = connection
        .prepare("SELECT class, byte_len_be FROM usage ORDER BY class, object_key")
        .map_err(|_| persistence_error("could not prepare usage snapshot"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(|_| persistence_error("could not query usage snapshot"))?;
    for row in rows {
        let (class, raw) = row.map_err(|_| persistence_error("could not read usage snapshot"))?;
        let bytes = codec::decode_u64(&raw)?;
        *classes.entry(class).or_default() = classes
            .get(&class)
            .copied()
            .unwrap_or(0)
            .checked_add(bytes)
            .ok_or_else(|| persistence_error("usage snapshot overflow"))?;
    }
    let pending = {
        let mut statement = connection
            .prepare("SELECT batch_id, kind, object_key, byte_len_be FROM deletion_objects")
            .map_err(|_| persistence_error("could not prepare pending deletion usage"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            })
            .map_err(|_| persistence_error("could not query pending deletion usage"))?;
        let mut seen = BTreeSet::new();
        let mut total = 0_u64;
        for row in rows {
            let (_batch, kind, key, raw) =
                row.map_err(|_| persistence_error("could not read pending deletion usage"))?;
            if seen.insert((kind, key)) {
                total = total
                    .checked_add(codec::decode_u64(&raw)?)
                    .ok_or_else(|| persistence_error("pending deletion usage overflow"))?;
            }
        }
        total
    };
    let (open_count, open_bytes) = segment_state_usage(connection, "open")?;
    let (oldest, newest) = retained_bounds(connection)?;
    let usage = StorageUsage::new(
        classes.get("segment").copied().unwrap_or(0),
        classes.get("index").copied().unwrap_or(0),
        classes.get("browser_event").copied().unwrap_or(0),
        classes.get("artifact").copied().unwrap_or(0),
        pending,
        open_bytes,
        0,
    )?;
    Ok(UsageSnapshot {
        usage,
        pinned_usage_bytes: pinned_usage(connection)?,
        oldest_retained: oldest,
        newest_retained: newest,
        open_segment_count: open_count,
    })
}

fn segment_state_usage(
    connection: &Connection,
    state: &str,
) -> krometrail_core::Result<(u64, u64)> {
    let mut statement = connection
        .prepare("SELECT file_bytes_be FROM segments WHERE state=?1")
        .map_err(|_| persistence_error("could not prepare segment state usage"))?;
    let rows = statement
        .query_map(params![state], |row| row.get::<_, Vec<u8>>(0))
        .map_err(|_| persistence_error("could not query segment state usage"))?;
    let values = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read segment state usage"))?;
    Ok((values.len() as u64, checked_sum(values)?))
}

fn retained_bounds(
    connection: &Connection,
) -> krometrail_core::Result<(Option<RetainedPoint>, Option<RetainedPoint>)> {
    fn point(
        connection: &Connection,
        ascending: bool,
    ) -> krometrail_core::Result<Option<RetainedPoint>> {
        let sql = if ascending {
            "SELECT session_id, target_id, session_time_be FROM frames ORDER BY rowid ASC LIMIT 1"
        } else {
            "SELECT session_id, target_id, session_time_be FROM frames ORDER BY rowid DESC LIMIT 1"
        };
        let raw = connection
            .query_row(sql, [], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })
            .optional()
            .map_err(|_| persistence_error("could not query retained bounds"))?;
        raw.map(|(session, target, time)| {
            Ok(RetainedPoint {
                session_id: SessionId::from_uuid(codec::decode_id(&session)?),
                target_id: TargetId::from_uuid(codec::decode_id(&target)?),
                session_time: SessionTime::from_nanos(codec::decode_u64(&time)?),
            })
        })
        .transpose()
    }
    Ok((point(connection, true)?, point(connection, false)?))
}

fn checked_sum(values: Vec<Vec<u8>>) -> krometrail_core::Result<u64> {
    values.into_iter().try_fold(0_u64, |total, raw| {
        total
            .checked_add(codec::decode_u64(&raw)?)
            .ok_or_else(|| persistence_error("usage sum overflow"))
    })
}

pub(crate) fn validate_file_name(value: &str) -> krometrail_core::Result<()> {
    if value.is_empty() || value.contains(['/', '\\']) || value == "." || value == ".." {
        return Err(persistence_error("stored retention path is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::Duration};

    use krometrail_core::{RetentionRange, SessionRange};
    use tempfile::TempDir;
    use uuid::Uuid;

    use crate::{IndexStoreConfig, SegmentRegistration, SegmentState};

    use super::*;

    fn registration(id: u128, session: u128, target: u128, start: u64) -> SegmentRegistration {
        SegmentRegistration {
            segment_id: SegmentId::from_uuid(Uuid::from_u128(id)),
            session_id: SessionId::from_uuid(Uuid::from_u128(session)),
            target_id: TargetId::from_uuid(Uuid::from_u128(target)),
            state: SegmentState::Sealed,
            relative_path: PathBuf::from(format!("{}.kts", Uuid::from_u128(id))),
            start_time: SessionTime::from_nanos(start),
            end_time: Some(SessionTime::from_nanos(start + 9)),
            file_bytes: 100,
            payload_bytes: 10,
            record_count: 1,
        }
    }

    fn index(directory: &TempDir) -> SqliteIndex {
        SqliteIndex::open(IndexStoreConfig {
            database_path: directory.path().join("index.sqlite3"),
            segments_directory: directory.path().join("segments"),
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap()
    }

    #[test]
    fn sequence_usage_and_overlapping_pins_are_deterministic() {
        let directory = TempDir::new().unwrap();
        let index = index(&directory);
        let first = registration(1, 10, 20, 0);
        let second = registration(2, 11, 21, 0);
        {
            let mut connection = index.connection().unwrap();
            let tx = connection.transaction().unwrap();
            super::super::segments::register_segment_tx(&tx, &first).unwrap();
            super::super::segments::register_segment_tx(&tx, &second).unwrap();
            // An open->sealed style upsert must not allocate a new age.
            super::super::segments::register_segment_tx(&tx, &first).unwrap();
            tx.commit().unwrap();
        }
        let range = RetentionRange {
            session_id: first.session_id,
            target_id: first.target_id,
            range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(5)).unwrap(),
        };
        let pinned = index.pin_range(range).unwrap();
        assert_eq!(pinned.protected_segments, [first.segment_id]);
        assert_eq!(pinned.pinned_usage_bytes, 100);
        assert_eq!(index.pin_range(range).unwrap(), pinned);
        let candidate = index.oldest_unpinned_segment().unwrap().unwrap();
        assert_eq!(candidate.segment_id, second.segment_id);
        assert_eq!(candidate.retention_sequence, 2);
        let snapshot = index.usage_snapshot().unwrap();
        assert_eq!(snapshot.usage.segment_bytes, 200);
        assert_eq!(snapshot.pinned_usage_bytes, 100);
        assert_eq!(index.unpin_range(range).unwrap().pinned_usage_bytes, 0);
        assert_eq!(
            index.oldest_unpinned_segment().unwrap().unwrap().segment_id,
            first.segment_id
        );
    }
}
