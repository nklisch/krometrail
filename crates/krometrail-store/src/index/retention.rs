use std::collections::{BTreeMap, BTreeSet};

use krometrail_core::{
    ArtifactId, FrameId, PinChange, ProtectedSegment, RangeEvidenceAvailability, RetainedPoint,
    RetentionPinRequest, RetentionRange, SegmentId, SessionId, SessionRange, SessionTime,
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
    pub file_bytes: u64,
    pub retention_sequence: u64,
}

/// Narrows the segment reclaim candidate set without changing its ordering.
///
/// `None` in either field means "no restriction", which is exactly the
/// budget-pressure behaviour. A future dead-instance tier narrows the same way.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SegmentReclaimFilter {
    /// Only consider segments stamped before this instant.
    pub created_before_unix_ms: Option<i64>,
    /// Skip segments that any artifact published at or after this instant derives
    /// from, so a fresh evidence link is not cascade-evicted out from under the
    /// agent that was just handed it.
    pub artifact_grace_since_unix_ms: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ArtifactCandidate {
    pub artifact_id: ArtifactId,
    pub session_id: SessionId,
    pub relative_path: String,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProgressivePinSnapshot {
    pub exact_pin_active: bool,
    pub evidence: RangeEvidenceAvailability,
    pub protected_segments: Vec<ProtectedSegment>,
    pub pinned_usage_bytes: u64,
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
    pub(crate) fn pin_resolved_range(
        &self,
        request: &RetentionPinRequest,
    ) -> krometrail_core::Result<bool> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin resolved range pin transaction"))?;
        validate_expected_range_frames(&transaction, request)?;
        let changed = transaction
            .execute(
                "INSERT OR IGNORE INTO pins(session_id,target_id,start_time_be,end_time_be) \
                 VALUES (?1,?2,?3,?4)",
                rusqlite::params_from_iter(pin_values(request.request)),
            )
            .map_err(|_| persistence_error("could not create exact resolved range pin"))?
            == 1;
        let pin_id = exact_pin_id(&transaction, request.request)?.ok_or_else(|| {
            persistence_error("exact resolved range pin disappeared during mutation")
        })?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO pin_segments(pin_id,segment_id) \
                 SELECT ?1,segment_id FROM segments \
                 WHERE session_id=?2 AND target_id=?3 AND state='sealed' \
                   AND start_time_be<=?4 AND end_time_be>=?5",
                params![
                    pin_id,
                    codec::id(request.request.session_id.as_uuid()).to_vec(),
                    codec::id(request.request.target_id.as_uuid()).to_vec(),
                    codec::u64_blob(request.request.range.end().as_nanos()).to_vec(),
                    codec::u64_blob(request.request.range.start().as_nanos()).to_vec(),
                ],
            )
            .map_err(|_| persistence_error("could not link resolved range source segments"))?;
        let protected: u64 = transaction
            .query_row(
                "SELECT count(*) FROM pin_segments WHERE pin_id=?1",
                params![pin_id],
                |row| row.get(0),
            )
            .map_err(|_| persistence_error("could not verify resolved range segment links"))?;
        if protected == 0 {
            return Err(persistence_error(
                "resolved range pin did not protect a sealed source segment",
            ));
        }
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit resolved range pin"))?;
        Ok(changed)
    }

    pub(crate) fn unpin_resolved_range(
        &self,
        request: RetentionRange,
    ) -> krometrail_core::Result<bool> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin exact range unpin transaction"))?;
        let changed = transaction
            .execute(
                "DELETE FROM pins WHERE session_id=?1 AND target_id=?2 \
                 AND start_time_be=?3 AND end_time_be=?4",
                rusqlite::params_from_iter(pin_values(request)),
            )
            .map_err(|_| persistence_error("could not remove exact resolved range pin"))?
            == 1;
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit exact range unpin"))?;
        Ok(changed)
    }

    pub(crate) fn progressive_pin_snapshot(
        &self,
        request: &RetentionPinRequest,
    ) -> krometrail_core::Result<ProgressivePinSnapshot> {
        let connection = self.connection()?;
        let session_exists = connection
            .query_row(
                "SELECT 1 FROM sessions WHERE session_id=?1",
                params![codec::id(request.request.session_id.as_uuid()).to_vec()],
                |_| Ok(()),
            )
            .optional()
            .map_err(|_| persistence_error("could not query pin session lifetime"))?
            .is_some();
        if !session_exists {
            return Err(pin_not_found(request.request));
        }
        let exact_pin_active = exact_pin_id(&connection, request.request)?.is_some();
        let evidence = range_evidence_availability(&connection, request)?;
        let protected_segments = protected_segments_for_request(&connection, request.request)?;
        Ok(ProgressivePinSnapshot {
            exact_pin_active,
            evidence,
            protected_segments,
            pinned_usage_bytes: pinned_usage(&connection)?,
        })
    }

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
        self.oldest_reclaimable_segment(SegmentReclaimFilter::default())
    }

    /// Selects the next sealed, unpinned segment in retention order.
    ///
    /// One query serves both budget pressure and age-out: the filter narrows the
    /// candidate set, but the ordering and the pin exclusion are identical, so
    /// age-out cannot drift into a second eviction policy. Pins are excluded
    /// unconditionally — pinned evidence survives age-out exactly as it survives
    /// budget pressure.
    pub(crate) fn oldest_reclaimable_segment(
        &self,
        filter: SegmentReclaimFilter,
    ) -> krometrail_core::Result<Option<SegmentCandidate>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT s.segment_id, s.session_id, s.target_id, \
                        s.file_bytes_be, s.retention_sequence \
                 FROM segments s WHERE s.state='sealed' \
                   AND NOT EXISTS (SELECT 1 FROM pin_segments p WHERE p.segment_id=s.segment_id) \
                   AND (?1 IS NULL OR s.created_unix_ms < ?1) \
                   AND (?2 IS NULL OR NOT EXISTS ( \
                         SELECT 1 FROM artifacts a \
                         JOIN artifact_frames af USING(artifact_id) \
                         JOIN frames f USING(frame_id) \
                         WHERE f.segment_id=s.segment_id AND a.created_unix_ms >= ?2)) \
                 ORDER BY s.retention_sequence ASC, s.segment_id ASC LIMIT 1",
                params![
                    filter.created_before_unix_ms,
                    filter.artifact_grace_since_unix_ms
                ],
                decode_segment_candidate,
            )
            .optional()
            .map_err(|_| persistence_error("could not select oldest unpinned segment"))?
            .map(decode_segment_candidate_parts)
            .transpose()
    }

    /// Reports the number of segments and artifacts already older than `cutoff`.
    ///
    /// Age-out needs to know whether expired evidence exists *before* deciding to
    /// walk, so that a store inside its byte budget still reclaims on time.
    pub(crate) fn expired_object_count(&self, cutoff_unix_ms: i64) -> krometrail_core::Result<u64> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT (SELECT count(*) FROM segments s \
                          WHERE s.state='sealed' AND s.created_unix_ms < ?1 \
                            AND NOT EXISTS ( \
                                  SELECT 1 FROM pin_segments p \
                                  WHERE p.segment_id=s.segment_id)) \
                      + (SELECT count(*) FROM artifacts WHERE created_unix_ms < ?1)",
                params![cutoff_unix_ms],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| persistence_error("could not count expired retained objects"))
            .and_then(|value| {
                u64::try_from(value)
                    .map_err(|_| persistence_error("expired object count is malformed"))
            })
    }

    /// The index's own wall clock, in Unix milliseconds.
    ///
    /// Age comparisons read the same clock that stamped `created_unix_ms`, so a
    /// cutoff can never be computed against a different time source than the rows
    /// it is compared with.
    pub(crate) fn now_unix_ms(&self) -> krometrail_core::Result<i64> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT CAST(unixepoch('subsec') * 1000 AS INTEGER)",
                [],
                |row| row.get(0),
            )
            .map_err(|_| persistence_error("could not read the retention clock"))
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
        self.oldest_artifact_excluding(None)
    }

    pub(crate) fn oldest_artifact_excluding(
        &self,
        excluded: Option<ArtifactId>,
    ) -> krometrail_core::Result<Option<ArtifactCandidate>> {
        self.oldest_reclaimable_artifact(excluded, None)
    }

    /// Selects the next evictable artifact, optionally restricted to expired ones.
    ///
    /// As with segments, budget pressure and age-out share one query so their
    /// ordering can never diverge.
    pub(crate) fn oldest_reclaimable_artifact(
        &self,
        excluded: Option<ArtifactId>,
        created_before_unix_ms: Option<i64>,
    ) -> krometrail_core::Result<Option<ArtifactCandidate>> {
        let connection = self.connection()?;
        let raw = connection
            .query_row(
                "SELECT artifact_id, session_id, relative_path, byte_len_be FROM artifacts \
                 WHERE (?1 IS NULL OR artifact_id!=?1) \
                   AND (?2 IS NULL OR created_unix_ms < ?2) \
                 ORDER BY start_time_be ASC, artifact_id ASC LIMIT 1",
                params![
                    excluded.map(|id| codec::id(id.as_uuid()).to_vec()),
                    created_before_unix_ms
                ],
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

    /// Counts segment rows still in the `open` state.
    ///
    /// Used as the construction-time proof that recovery already ran.
    pub(crate) fn open_segment_count(&self) -> krometrail_core::Result<u64> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT count(*) FROM segments WHERE state='open'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| persistence_error("could not count open segments"))
            .and_then(|value| {
                u64::try_from(value)
                    .map_err(|_| persistence_error("open segment count is malformed"))
            })
    }

    pub(crate) fn usage_snapshot(&self) -> krometrail_core::Result<UsageSnapshot> {
        let connection = self.connection()?;
        usage_snapshot(&connection)
    }

    /// Usage for read-only status, computed without writing or checkpointing.
    ///
    /// The stored `index` usage row is only refreshed by mutating paths, so a
    /// status read that trusted it would report zero on a store that has not
    /// mutated yet. Deriving the index class from live pages instead keeps status
    /// honest while staying a pure read. The one accepted imprecision is that
    /// pages still sitting in an un-checkpointed WAL may not be counted yet.
    pub(crate) fn live_usage_snapshot(&self) -> krometrail_core::Result<UsageSnapshot> {
        let connection = self.connection()?;
        let mut snapshot = usage_snapshot(&connection)?;
        let (live_pages, _) = super::maintenance::sqlite_page_usage(&connection)?;
        let index_bytes = live_pages.saturating_sub(snapshot.usage.browser_event_bytes);
        snapshot.usage = StorageUsage::new(
            snapshot.usage.segment_bytes,
            index_bytes,
            snapshot.usage.browser_event_bytes,
            snapshot.usage.artifact_bytes,
            snapshot.usage.pending_deletion_bytes,
            snapshot.usage.open_segment_bytes,
            snapshot.usage.accounting_slack_bytes,
        )?;
        Ok(snapshot)
    }

    pub(crate) fn session_segments(
        &self,
        session_id: SessionId,
    ) -> krometrail_core::Result<Vec<SegmentCandidate>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                // Sealed only. Session deletion runs after `flush_session`, so every
                // segment of this session is already published; an `open` row here
                // would belong to a live writer and must never become a deletion
                // object.
                "SELECT segment_id, session_id, target_id, file_bytes_be, \
                        retention_sequence FROM segments \
                 WHERE session_id=?1 AND state='sealed' \
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

type RawSegment = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, i64);

fn decode_segment_candidate(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawSegment> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
    ))
}

fn decode_segment_candidate_parts(raw: RawSegment) -> krometrail_core::Result<SegmentCandidate> {
    Ok(SegmentCandidate {
        segment_id: SegmentId::from_uuid(codec::decode_id(&raw.0)?),
        session_id: SessionId::from_uuid(codec::decode_id(&raw.1)?),
        target_id: TargetId::from_uuid(codec::decode_id(&raw.2)?),
        file_bytes: codec::decode_u64(&raw.3)?,
        retention_sequence: u64::try_from(raw.4)
            .map_err(|_| persistence_error("stored retention sequence is malformed"))?,
    })
}

fn validate_expected_range_frames(
    connection: &Connection,
    request: &RetentionPinRequest,
) -> krometrail_core::Result<()> {
    let mut statement = connection
        .prepare(
            "SELECT f.frame_id FROM frames f JOIN segments s USING(segment_id) \
             WHERE f.session_id=?1 AND f.target_id=?2 \
               AND f.session_time_be>=?3 AND f.session_time_be<=?4 AND s.state='sealed' \
             ORDER BY f.capture_ordinal_be ASC,f.session_time_be ASC,f.frame_id ASC",
        )
        .map_err(|_| persistence_error("could not prepare resolved pin frame validation"))?;
    let rows = statement
        .query_map(
            rusqlite::params_from_iter(pin_values(request.request)),
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(|_| persistence_error("could not query resolved pin frames"))?;
    let retained = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read resolved pin frames"))?
        .into_iter()
        .map(|raw| codec::decode_id(&raw).map(FrameId::from_uuid))
        .collect::<krometrail_core::Result<Vec<_>>>()?;
    if retained != request.expected_frame_ids {
        return Err(pin_not_found(request.request));
    }
    Ok(())
}

fn exact_pin_id(
    connection: &Connection,
    request: RetentionRange,
) -> krometrail_core::Result<Option<i64>> {
    connection
        .query_row(
            "SELECT pin_id FROM pins WHERE session_id=?1 AND target_id=?2 \
             AND start_time_be=?3 AND end_time_be=?4",
            rusqlite::params_from_iter(pin_values(request)),
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| persistence_error("could not query exact resolved range pin"))
}

fn range_evidence_availability(
    connection: &Connection,
    request: &RetentionPinRequest,
) -> krometrail_core::Result<RangeEvidenceAvailability> {
    let mut statement = connection
        .prepare(
            "SELECT 1 FROM frames WHERE frame_id=?1 AND session_id=?2 AND target_id=?3 \
               AND session_time_be>=?4 AND session_time_be<=?5",
        )
        .map_err(|_| persistence_error("could not prepare expected frame availability"))?;
    let mut retained_frame_ids = Vec::new();
    let mut missing_frame_ids = Vec::new();
    for frame_id in &request.expected_frame_ids {
        let retained = statement
            .query_row(
                params![
                    codec::id(frame_id.as_uuid()).to_vec(),
                    codec::id(request.request.session_id.as_uuid()).to_vec(),
                    codec::id(request.request.target_id.as_uuid()).to_vec(),
                    codec::u64_blob(request.request.range.start().as_nanos()).to_vec(),
                    codec::u64_blob(request.request.range.end().as_nanos()).to_vec(),
                ],
                |_| Ok(()),
            )
            .optional()
            .map_err(|_| persistence_error("could not query expected frame availability"))?
            .is_some();
        if retained {
            retained_frame_ids.push(*frame_id);
        } else {
            missing_frame_ids.push(*frame_id);
        }
    }
    Ok(if missing_frame_ids.is_empty() {
        RangeEvidenceAvailability::Complete
    } else if retained_frame_ids.is_empty() {
        RangeEvidenceAvailability::Unavailable { missing_frame_ids }
    } else {
        RangeEvidenceAvailability::PartiallyUnavailable {
            retained_frame_ids,
            missing_frame_ids,
        }
    })
}

fn protected_segments_for_request(
    connection: &Connection,
    request: RetentionRange,
) -> krometrail_core::Result<Vec<ProtectedSegment>> {
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT s.segment_id,s.start_time_be,s.end_time_be,s.file_bytes_be \
             FROM segments s WHERE s.session_id=?1 AND s.target_id=?2 AND s.state='sealed' \
               AND s.start_time_be<=?3 AND s.end_time_be>=?4 \
               AND EXISTS (SELECT 1 FROM pin_segments p WHERE p.segment_id=s.segment_id) \
             ORDER BY s.start_time_be,s.end_time_be,s.segment_id",
        )
        .map_err(|_| persistence_error("could not prepare overlapping pin segment state"))?;
    let rows = statement
        .query_map(
            params![
                codec::id(request.session_id.as_uuid()).to_vec(),
                codec::id(request.target_id.as_uuid()).to_vec(),
                codec::u64_blob(request.range.end().as_nanos()).to_vec(),
                codec::u64_blob(request.range.start().as_nanos()).to_vec(),
            ],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            },
        )
        .map_err(|_| persistence_error("could not query overlapping pin segments"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read overlapping pin segments"))?
        .into_iter()
        .map(|(segment, start, end, bytes)| {
            ProtectedSegment::new(
                SegmentId::from_uuid(codec::decode_id(&segment)?),
                SessionRange::new(
                    SessionTime::from_nanos(codec::decode_u64(&start)?),
                    SessionTime::from_nanos(codec::decode_u64(&end)?),
                )
                .map_err(|_| persistence_error("stored protected segment range is invalid"))?,
                codec::decode_u64(&bytes)?,
            )
            .map_err(|_| persistence_error("stored protected segment state is invalid"))
        })
        .collect()
}

fn pin_values(request: RetentionRange) -> [Vec<u8>; 4] {
    [
        codec::id(request.session_id.as_uuid()).to_vec(),
        codec::id(request.target_id.as_uuid()).to_vec(),
        codec::u64_blob(request.range.start().as_nanos()).to_vec(),
        codec::u64_blob(request.range.end().as_nanos()).to_vec(),
    ]
}

fn pin_not_found(request: RetentionRange) -> krometrail_core::KrometrailError {
    krometrail_core::KrometrailError::new(
        krometrail_core::ErrorCode::NotFound,
        krometrail_core::NonEmptyText::new(
            "resolved range source frames are not completely retained",
        )
        .expect("static pin range error is non-empty"),
    )
    .with_context(krometrail_core::ErrorContext {
        session_id: Some(request.session_id),
        target_id: Some(request.target_id),
        range: Some(request.range),
        ..Default::default()
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
        super::maintenance::sqlite_page_usage(connection)?.1,
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

/// Oldest and newest retained evidence, ordered by wall clock.
///
/// **Ordering authority: `segments.created_unix_ms`.** These bounds are global —
/// they range over every retained session — so they need an ordering key that is
/// meaningful across sessions. `session_time` is not one: it is measured from
/// each session's own start, so comparing two sessions' session times is
/// meaningless by construction. The previous implementation ordered by `rowid`,
/// which is global insertion order and answers a different question entirely
/// ("which row was written first"). That is why a live store could report an
/// `oldest_retained` session time *greater* than its `newest_retained`: the two
/// endpoints came from different sessions, and their session-relative values were
/// never comparable in the first place.
///
/// `created_unix_ms` is the right authority because it is one wall clock shared
/// by every session, so "oldest" and "newest" genuinely order the retained
/// evidence. Ties break on `session_time` then `frame_id` for determinism.
///
/// **What the returned `session_time` does and does not mean.** Each endpoint
/// still carries its own session-relative time, because that is the coordinate a
/// caller needs to address that frame within its session. Those two values are
/// only comparable — and a span between them only meaningful — when both
/// endpoints share a session and target. The MCP surface is responsible for not
/// implying otherwise; this query guarantees only that the endpoints are ordered,
/// not that their session times can be subtracted.
fn retained_bounds(
    connection: &Connection,
) -> krometrail_core::Result<(Option<RetainedPoint>, Option<RetainedPoint>)> {
    fn point(
        connection: &Connection,
        ascending: bool,
    ) -> krometrail_core::Result<Option<RetainedPoint>> {
        let sql = if ascending {
            "SELECT f.session_id, f.target_id, f.session_time_be \
             FROM frames f JOIN segments s USING(segment_id) \
             ORDER BY s.created_unix_ms ASC, f.session_time_be ASC, f.frame_id ASC LIMIT 1"
        } else {
            "SELECT f.session_id, f.target_id, f.session_time_be \
             FROM frames f JOIN segments s USING(segment_id) \
             ORDER BY s.created_unix_ms DESC, f.session_time_be DESC, f.frame_id DESC LIMIT 1"
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
    use std::time::Duration;

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
