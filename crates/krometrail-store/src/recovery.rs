use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use krometrail_core::{
    ByteOffset, EncodedFrame, ErrorCode, FrameAddress, KrometrailError, NonEmptyText, SegmentId,
};

use crate::{
    FrameWriteCommit, SegmentRegistration, SegmentState, SqliteIndex,
    index::{
        maintenance::{UsageClass, UsageEntry},
        reconcile,
    },
    permissions, persistence_error,
    segments::{
        OPEN_SEGMENT_EXTENSION, RecordSpan, SEALED_FOOTER_LEN, SEALED_SEGMENT_EXTENSION,
        SEGMENT_HEADER_LEN, SealedFooter, SegmentHeader, Trailing, read_frame_at,
        scan_complete_records, sealed_segment_path,
    },
};

pub const QUARANTINED_SEGMENT_EXTENSION: &str = "corrupt";

/// Observable work performed by one startup recovery pass.
///
/// A second pass over a recovered store returns the default report.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecoveryReport {
    pub open_segments_sealed: u64,
    pub segments_repaired: u64,
    pub segments_quarantined: u64,
    pub segments_removed: u64,
    pub bytes_truncated: u64,
    pub frames_recovered: u64,
    pub frames_removed: u64,
    pub usage_rows_reconciled: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CandidateKind {
    Open,
    Sealed,
}

#[derive(Clone, Debug)]
struct Candidate {
    segment_id: SegmentId,
    path: PathBuf,
    kind: CandidateKind,
}

#[derive(Clone, Debug)]
struct RecoveredRecord {
    span: RecordSpan,
    frame: EncodedFrame,
}

struct Analysis {
    header: SegmentHeader,
    records: Vec<RecoveredRecord>,
    footer: SealedFooter,
    record_end: u64,
    footer_is_exact: bool,
    repair_at: Option<ByteOffset>,
    original_len: u64,
}

struct NormalizedSegment {
    registration: SegmentRegistration,
    records: Vec<RecoveredRecord>,
    repaired: bool,
    repair_at: Option<ByteOffset>,
    bytes_truncated: u64,
}

/// Restores the segment/index consistency invariant before capture or retention starts.
///
/// Segment files are the byte-level authority. SQLite metadata is repaired to
/// describe only complete records, while complete orphan records are inserted
/// into the index after their files have been sealed and synced.
pub fn recover(index: &SqliteIndex) -> krometrail_core::Result<RecoveryReport> {
    let directory = index.segments_directory();
    permissions::tighten_existing_directory(directory)
        .map_err(|_| persistence_error("could not protect recording segments"))?;
    let initial = discover(directory)?;
    let mut report = RecoveryReport::default();
    let mut quarantined = BTreeSet::new();
    let mut sealed_tail_repairs = BTreeMap::new();

    // Phase B is filesystem-only. No SQLite lock or transaction spans sealing.
    for candidate in initial
        .values()
        .filter(|candidate| candidate.kind == CandidateKind::Open)
    {
        match analyze_file(candidate) {
            Ok(analysis) => {
                let normalized = normalize_file(directory, candidate, analysis)?;
                report.open_segments_sealed += 1;
                if let Some(repair_at) = normalized.repair_at {
                    sealed_tail_repairs.insert(candidate.segment_id, repair_at);
                }
                report.bytes_truncated = report
                    .bytes_truncated
                    .checked_add(normalized.bytes_truncated)
                    .ok_or_else(|| persistence_error("recovery byte count overflow"))?;
            }
            Err(FileAnalysisError::CorruptHeader) => {
                quarantine(directory, candidate)?;
                quarantined.insert(candidate.segment_id);
                report.segments_quarantined += 1;
            }
            Err(FileAnalysisError::Operational(error)) => return Err(error),
        }
    }

    // Phase C reconciles the union of disk and index identities. Discovery is
    // repeated because Phase B changed every `.open` publication to `.kts`.
    let disk: BTreeMap<_, _> = discover(directory)?
        .into_iter()
        .filter(|(_, candidate)| candidate.kind == CandidateKind::Sealed)
        .collect();
    let stored = {
        let connection = index.connection()?;
        reconcile::list_segments(&connection)?
    };
    let mut stored_by_id: BTreeMap<_, _> = stored
        .into_iter()
        .map(|stored| (stored.registration.segment_id, stored.registration))
        .collect();
    let mut segment_ids: BTreeSet<_> = disk.keys().copied().collect();
    segment_ids.extend(stored_by_id.keys().copied());
    segment_ids.extend(quarantined.iter().copied());

    for segment_id in segment_ids {
        let Some(candidate) = disk.get(&segment_id) else {
            let removed = remove_index_segment(index, segment_id)?;
            report.frames_removed += removed;
            if stored_by_id.remove(&segment_id).is_some() && !quarantined.contains(&segment_id) {
                report.segments_removed += 1;
            }
            continue;
        };

        let analysis = match analyze_file(candidate) {
            Ok(analysis) => analysis,
            Err(FileAnalysisError::CorruptHeader) => {
                quarantine(directory, candidate)?;
                report.segments_quarantined += 1;
                report.frames_removed += remove_index_segment(index, segment_id)?;
                stored_by_id.remove(&segment_id);
                continue;
            }
            Err(FileAnalysisError::Operational(error)) => return Err(error),
        };
        let mut normalized = normalize_file(directory, candidate, analysis)?;
        if normalized.repair_at.is_none() {
            normalized.repair_at = sealed_tail_repairs.get(&segment_id).copied();
        }
        if normalized.repaired {
            report.segments_repaired += 1;
            report.bytes_truncated = report
                .bytes_truncated
                .checked_add(normalized.bytes_truncated)
                .ok_or_else(|| persistence_error("recovery byte count overflow"))?;
        }
        reconcile_segment(
            index,
            stored_by_id.get(&segment_id),
            normalized,
            &mut report,
        )?;
    }

    // Phase D touches only segment-class usage. Pins are intentionally never
    // read or reconstructed; FK cascades remove only links to lost segments.
    reconcile_usage(index, &mut report)?;
    Ok(report)
}

fn discover(directory: &Path) -> krometrail_core::Result<BTreeMap<SegmentId, Candidate>> {
    let entries = fs::read_dir(directory)
        .map_err(|_| shutdown_error("recording segments cannot be enumerated during recovery"))?;
    let mut candidates = BTreeMap::new();
    for entry in entries {
        let entry = entry.map_err(|_| {
            shutdown_error("recording segments cannot be enumerated during recovery")
        })?;
        let path = entry.path();
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        let kind = match extension {
            OPEN_SEGMENT_EXTENSION => CandidateKind::Open,
            SEALED_SEGMENT_EXTENSION => CandidateKind::Sealed,
            QUARANTINED_SEGMENT_EXTENSION => continue,
            _ => continue,
        };
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let Ok(segment_id) = stem.parse::<SegmentId>() else {
            continue;
        };
        let candidate = Candidate {
            segment_id,
            path,
            kind,
        };
        if candidates.insert(segment_id, candidate).is_some() {
            return Err(persistence_error(
                "multiple segment publications use the same identifier",
            ));
        }
    }
    Ok(candidates)
}

enum FileAnalysisError {
    CorruptHeader,
    Operational(KrometrailError),
}

fn analyze_file(candidate: &Candidate) -> Result<Analysis, FileAnalysisError> {
    permissions::tighten_existing_file(&candidate.path).map_err(|error| {
        FileAnalysisError::Operational(io_error("protect a segment during recovery", error))
    })?;
    let bytes = fs::read(&candidate.path).map_err(|error| {
        FileAnalysisError::Operational(io_error("read a segment during recovery", error))
    })?;
    let header_bytes = bytes
        .get(..SEGMENT_HEADER_LEN)
        .ok_or(FileAnalysisError::CorruptHeader)?;
    let header =
        SegmentHeader::decode(header_bytes).map_err(|_| FileAnalysisError::CorruptHeader)?;
    if header.segment_id != candidate.segment_id {
        return Err(FileAnalysisError::CorruptHeader);
    }
    let scan = scan_complete_records(&bytes).map_err(|_| FileAnalysisError::CorruptHeader)?;

    let mut records = Vec::with_capacity(scan.records.len());
    let mut semantic_corruption = None;
    for span in scan.records {
        let address = FrameAddress::new(header.segment_id, span.byte_offset);
        match read_frame_at(&bytes, address) {
            Ok(frame) => records.push(RecoveredRecord { span, frame }),
            Err(_) => {
                semantic_corruption = Some(span.byte_offset);
                break;
            }
        }
    }
    let repair_at = semantic_corruption.or(match scan.trailing {
        Trailing::Clean => None,
        Trailing::Incomplete { at } | Trailing::Corrupt { at } => Some(at),
    });
    let record_end = records.last().map_or(SEGMENT_HEADER_LEN as u64, |record| {
        record.span.byte_offset.get() + record.span.encoded_len
    });
    let total_payload = records
        .iter()
        .try_fold(0_u64, |total, record| {
            total
                .checked_add(record.span.payload_len)
                .ok_or_else(|| persistence_error("recovered segment payload count overflow"))
        })
        .map_err(FileAnalysisError::Operational)?;
    let record_count = u64::try_from(records.len()).map_err(|_| {
        FileAnalysisError::Operational(persistence_error("recovered segment record count overflow"))
    })?;
    let first_session_time = records.first().map_or(header.start_session_time, |record| {
        record.frame.metadata().session_time()
    });
    let last_session_time = records.last().map_or(header.start_session_time, |record| {
        record.frame.metadata().session_time()
    });
    let sealed_observed = records.last().map_or(header.created_observed, |record| {
        record.frame.metadata().observed_time()
    });
    let footer = SealedFooter::new(
        header.segment_id,
        record_count,
        total_payload,
        first_session_time,
        last_session_time,
        sealed_observed,
    );
    let record_end_usize = usize::try_from(record_end).map_err(|_| {
        FileAnalysisError::Operational(persistence_error("segment length exceeds this platform"))
    })?;
    let footer_is_exact = repair_at.is_none()
        && bytes.len() == record_end_usize + SEALED_FOOTER_LEN
        && SealedFooter::decode(&bytes[record_end_usize..]).is_ok_and(|stored| stored == footer);

    Ok(Analysis {
        header,
        records,
        footer,
        record_end,
        footer_is_exact,
        repair_at,
        original_len: bytes.len() as u64,
    })
}

fn normalize_file(
    directory: &Path,
    candidate: &Candidate,
    analysis: Analysis,
) -> krometrail_core::Result<NormalizedSegment> {
    let must_write_footer = !analysis.footer_is_exact;
    let bytes_truncated = if must_write_footer {
        analysis.original_len.saturating_sub(analysis.record_end)
    } else {
        0
    };
    if must_write_footer {
        let mut file = OpenOptions::new()
            .write(true)
            .open(&candidate.path)
            .map_err(|error| io_error("open a segment for recovery", error))?;
        file.set_len(analysis.record_end)
            .map_err(|error| io_error("truncate a segment during recovery", error))?;
        file.seek(SeekFrom::Start(analysis.record_end))
            .map_err(|error| io_error("seek a segment during recovery", error))?;
        file.write_all(&analysis.footer.encode())
            .map_err(|error| io_error("write a recovered segment footer", error))?;
        file.flush()
            .map_err(|error| io_error("flush a recovered segment", error))?;
        file.sync_data()
            .map_err(|error| io_error("sync a recovered segment", error))?;
    }

    let sealed_path = sealed_segment_path(directory, candidate.segment_id);
    if candidate.kind == CandidateKind::Open {
        fs::rename(&candidate.path, &sealed_path)
            .map_err(|error| io_error("publish a recovered segment", error))?;
        sync_directory(directory)
            .map_err(|error| io_error("sync a recovered segment publication", error))?;
    } else if must_write_footer {
        // No directory entry changed, but syncing the directory keeps the
        // durability boundary uniform with startup repair and quarantine.
        sync_directory(directory)
            .map_err(|error| io_error("sync a repaired segment publication", error))?;
    }

    let file_bytes = analysis
        .record_end
        .checked_add(SEALED_FOOTER_LEN as u64)
        .ok_or_else(|| persistence_error("recovered segment length overflow"))?;
    Ok(NormalizedSegment {
        registration: SegmentRegistration {
            segment_id: analysis.header.segment_id,
            session_id: analysis.header.session_id,
            target_id: analysis.header.target_id,
            state: SegmentState::Sealed,
            relative_path: PathBuf::from(format!(
                "{}.{}",
                analysis.header.segment_id, SEALED_SEGMENT_EXTENSION
            )),
            start_time: analysis.header.start_session_time,
            end_time: Some(analysis.footer.last_session_time),
            file_bytes,
            payload_bytes: analysis.footer.total_payload,
            record_count: analysis.footer.record_count,
        },
        records: analysis.records,
        repaired: candidate.kind == CandidateKind::Sealed && must_write_footer,
        repair_at: analysis.repair_at,
        bytes_truncated,
    })
}

fn reconcile_segment(
    index: &SqliteIndex,
    stored_registration: Option<&SegmentRegistration>,
    normalized: NormalizedSegment,
    report: &mut RecoveryReport,
) -> krometrail_core::Result<()> {
    let segment_id = normalized.registration.segment_id;
    let indexed = {
        let connection = index.connection()?;
        reconcile::indexed_frames(&connection, segment_id)?
    };
    let valid: BTreeMap<_, _> = normalized
        .records
        .iter()
        .map(|record| (record.span.byte_offset, record))
        .collect();
    let immutable_registration_mismatch = stored_registration.is_some_and(|stored| {
        stored.segment_id != normalized.registration.segment_id
            || stored.session_id != normalized.registration.session_id
            || stored.target_id != normalized.registration.target_id
            || stored.start_time != normalized.registration.start_time
    });
    let row_mismatch = indexed.iter().any(|row| {
        valid.get(&row.byte_offset).is_none_or(|record| {
            row.frame_id != record.frame.metadata().id()
                || row.session_id != record.frame.metadata().session_id()
                || row.target_id != record.frame.metadata().target_id()
        })
    });

    let mut retained_offsets: BTreeSet<_> = indexed.iter().map(|row| row.byte_offset).collect();
    if immutable_registration_mismatch || row_mismatch {
        let can_remove_tail_only = !immutable_registration_mismatch
            && normalized.repair_at.is_some_and(|at| {
                indexed.iter().all(|row| {
                    row.byte_offset >= at
                        || valid.get(&row.byte_offset).is_some_and(|record| {
                            row.frame_id == record.frame.metadata().id()
                                && row.session_id == record.frame.metadata().session_id()
                                && row.target_id == record.frame.metadata().target_id()
                        })
                })
            });
        let removed = if can_remove_tail_only {
            let at = normalized
                .repair_at
                .expect("tail-only removal has a repair offset");
            retained_offsets.retain(|offset| *offset < at);
            index.remove_frame_rows(segment_id, Some(at))?
        } else {
            retained_offsets.clear();
            let removed = index.remove_frame_rows(segment_id, None)?;
            if immutable_registration_mismatch {
                index.remove_segment(segment_id)?;
            }
            removed
        };
        report.frames_removed += removed.len() as u64;
    }

    let mut missing: Vec<_> = normalized
        .records
        .iter()
        .filter(|record| !retained_offsets.contains(&record.span.byte_offset))
        .collect();
    if !missing.is_empty() {
        let connection = index.connection()?;
        let mut insertable = Vec::with_capacity(missing.len());
        for record in missing {
            if !reconcile::frame_exists(&connection, record.frame.metadata().id())? {
                insertable.push(record);
            }
        }
        missing = insertable;
    }
    let registration_matches =
        stored_registration == Some(&normalized.registration) && !immutable_registration_mismatch;
    if registration_matches && missing.is_empty() {
        return Ok(());
    }

    let mut connection = index.connection()?;
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| persistence_error("could not begin segment recovery transaction"))?;
    reconcile::register_segment_tx(&transaction, &normalized.registration)?;
    for record in missing {
        let commit = FrameWriteCommit {
            address: FrameAddress::new(segment_id, record.span.byte_offset),
            active_segment: normalized.registration.clone(),
            sealed_segment: None,
        };
        if reconcile::upsert_recovered_frame_tx(&transaction, &record.frame, &commit)? {
            report.frames_recovered += 1;
        }
    }
    transaction
        .commit()
        .map_err(|_| persistence_error("could not commit segment recovery transaction"))
}

fn remove_index_segment(
    index: &SqliteIndex,
    segment_id: SegmentId,
) -> krometrail_core::Result<u64> {
    let removed = index.remove_frame_rows(segment_id, None)?;
    index.remove_segment(segment_id)?;
    Ok(removed.len() as u64)
}

fn reconcile_usage(
    index: &SqliteIndex,
    report: &mut RecoveryReport,
) -> krometrail_core::Result<()> {
    let (segments, usage) = {
        let connection = index.connection()?;
        (
            reconcile::list_segments(&connection)?,
            reconcile::list_segment_usage(&connection)?,
        )
    };
    let mut existing: BTreeMap<_, _> = usage
        .into_iter()
        .map(|entry| (entry.object_key.clone(), entry))
        .collect();
    for stored in segments {
        let registration = stored.registration;
        let key = reconcile::segment_usage_key(registration.segment_id);
        let matches = existing.get(&key).is_some_and(|entry| {
            entry.session_id == Some(registration.session_id)
                && entry.byte_len == registration.file_bytes
        });
        existing.remove(&key);
        if !matches {
            index.update_usage(UsageEntry {
                class: UsageClass::Segment,
                object_key: key,
                session_id: Some(registration.session_id),
                byte_len: registration.file_bytes,
            })?;
            report.usage_rows_reconciled += 1;
        }
    }
    for key in existing.into_keys() {
        index.remove_usage(UsageClass::Segment, &key)?;
        report.usage_rows_reconciled += 1;
    }
    Ok(())
}

fn quarantine(directory: &Path, candidate: &Candidate) -> krometrail_core::Result<()> {
    let destination = directory.join(format!(
        "{}.{}",
        candidate.segment_id, QUARANTINED_SEGMENT_EXTENSION
    ));
    fs::rename(&candidate.path, destination)
        .map_err(|error| io_error("quarantine a corrupt segment", error))?;
    sync_directory(directory)
        .map_err(|error| io_error("sync a quarantined segment publication", error))
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> std::io::Result<()> {
    std::fs::File::open(directory)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> std::io::Result<()> {
    Ok(())
}

fn io_error(action: &str, error: std::io::Error) -> KrometrailError {
    persistence_error(format!("could not {action}: {}", error.kind()))
}

fn shutdown_error(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ShutdownIncomplete,
        NonEmptyText::new(message).expect("static recovery error is non-empty"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segments::open_segment_path;
    use krometrail_core::{ObservedTime, SessionId, SessionTime, TargetId};
    use tempfile::TempDir;
    use uuid::Uuid;

    fn header(segment_id: SegmentId) -> SegmentHeader {
        SegmentHeader::new(
            segment_id,
            SessionId::from_uuid(Uuid::from_u128(2)),
            TargetId::from_uuid(Uuid::from_u128(3)),
            SessionTime::from_nanos(4),
            ObservedTime::from_nanos(5),
            6,
            7,
        )
    }

    #[test]
    fn discovery_accepts_only_named_segment_publications() {
        let directory = TempDir::new().unwrap();
        let segment_id = SegmentId::from_uuid(Uuid::from_u128(1));
        fs::write(
            open_segment_path(directory.path(), segment_id),
            header(segment_id).encode(),
        )
        .unwrap();
        fs::write(directory.path().join("not-a-segment.open"), b"ignored").unwrap();
        fs::write(directory.path().join("probe"), b"ignored").unwrap();
        fs::write(
            directory.path().join(format!("{segment_id}.corrupt")),
            b"ignored",
        )
        .unwrap();

        let found = discover(directory.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[&segment_id].kind, CandidateKind::Open);
    }

    #[test]
    fn header_only_segment_derives_a_valid_empty_footer() {
        let directory = TempDir::new().unwrap();
        let segment_id = SegmentId::from_uuid(Uuid::from_u128(10));
        let path = open_segment_path(directory.path(), segment_id);
        fs::write(&path, header(segment_id).encode()).unwrap();
        let candidate = Candidate {
            segment_id,
            path,
            kind: CandidateKind::Open,
        };
        let analysis = match analyze_file(&candidate) {
            Ok(analysis) => analysis,
            Err(_) => panic!("valid header-only segment must analyze"),
        };
        assert_eq!(analysis.footer.record_count, 0);
        assert_eq!(analysis.footer.total_payload, 0);
        assert_eq!(
            analysis.footer.first_session_time,
            SessionTime::from_nanos(4)
        );
        assert_eq!(
            analysis.footer.last_session_time,
            SessionTime::from_nanos(4)
        );
        assert!(!analysis.footer_is_exact);
    }
}
