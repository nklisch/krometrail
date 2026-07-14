use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::Arc,
};

use krometrail_core::ArtifactId;

use crate::{
    SqliteIndex,
    artifacts::{source_fingerprints, validate_stored_artifact},
    index::{
        artifacts::{ArtifactRow, ArtifactState},
        codec,
        maintenance::{UsageClass, UsageEntry},
        retention::ArtifactCandidate,
    },
    persistence_error,
};

use super::files::{ArtifactFiles, sync_directory};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ArtifactRecoveryReport {
    pub staging_finalized: u64,
    pub staging_removed: u64,
    pub ready_invalidated: u64,
    pub orphan_files_removed: u64,
    pub usage_rows_reconciled: u64,
}

pub(crate) struct ArtifactRecoveryPlan {
    pub invalid: Vec<ArtifactCandidate>,
    pub report: ArtifactRecoveryReport,
}

/// Reconcile durable publication states after deletion journals have resumed.
/// Invalid indexed artifacts are returned to `RecordingStore`, which removes them
/// through the existing deletion journal rather than inventing another recovery log.
pub(crate) fn plan(
    index: &SqliteIndex,
    files: &ArtifactFiles,
) -> krometrail_core::Result<ArtifactRecoveryPlan> {
    let rows = index.artifact_rows()?;
    let known: BTreeSet<_> = rows.iter().map(|row| row.artifact_id).collect();
    let mut invalid = Vec::new();
    let mut report = ArtifactRecoveryReport::default();

    for row in rows {
        let final_path = files.final_path(row.artifact_id);
        let temp_path = files.temp_path(row.artifact_id);
        let sources = index.artifact_sources(row.artifact_id)?;
        let valid = fs::read(&final_path)
            .ok()
            .and_then(|bytes| {
                let mut ready = row.clone();
                ready.state = ArtifactState::Ready;
                validate_stored_artifact(&ready, &sources, Arc::from(bytes), None).ok()
            })
            .is_some();
        match (row.state, valid) {
            (ArtifactState::Staging, true) => {
                let source_fingerprints = source_fingerprints(&sources);
                if index.finalize_artifact(
                    row.artifact_id,
                    row.cache.cache_key,
                    row.session_id,
                    row.target_id,
                    &source_fingerprints,
                )? {
                    report.staging_finalized += 1;
                }
            }
            (ArtifactState::Ready, true) => {}
            (ArtifactState::Staging, false) => {
                invalid.push(candidate(&row));
                report.staging_removed += 1;
            }
            (ArtifactState::Ready, false) => {
                invalid.push(candidate(&row));
                report.ready_invalidated += 1;
            }
        }
        if temp_path.exists() {
            fs::remove_file(&temp_path).map_err(|_| {
                persistence_error("could not remove artifact recovery temporary file")
            })?;
            report.orphan_files_removed += 1;
        }
    }

    for entry in fs::read_dir(files.directory())
        .map_err(|_| persistence_error("could not enumerate artifact recovery files"))?
    {
        let entry =
            entry.map_err(|_| persistence_error("could not read artifact recovery file"))?;
        let path = entry.path();
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        if !matches!(extension, "png" | "tmp") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let Ok(id) = stem.parse::<ArtifactId>() else {
            continue;
        };
        if !known.contains(&id) {
            fs::remove_file(path)
                .map_err(|_| persistence_error("could not remove orphan artifact file"))?;
            report.orphan_files_removed += 1;
        }
    }
    sync_directory(files.directory())
        .map_err(|_| persistence_error("could not sync artifact recovery cleanup"))?;
    Ok(ArtifactRecoveryPlan { invalid, report })
}

pub(crate) fn reconcile_usage(index: &SqliteIndex) -> krometrail_core::Result<u64> {
    let rows = index.artifact_rows()?;
    let expected: BTreeMap<Vec<u8>, (krometrail_core::SessionId, u64)> = rows
        .into_iter()
        .map(|row| {
            (
                codec::id(row.artifact_id.as_uuid()).to_vec(),
                (row.session_id, row.byte_len),
            )
        })
        .collect();
    let stored = {
        let connection = index.connection()?;
        let mut statement = connection
            .prepare("SELECT object_key,session_id,byte_len_be FROM usage WHERE class='artifact'")
            .map_err(|_| persistence_error("could not prepare artifact usage recovery"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(|_| persistence_error("could not query artifact usage recovery"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| persistence_error("could not read artifact usage recovery"))?
    };
    let mut stored_map = BTreeMap::new();
    for (key, session, bytes) in stored {
        stored_map.insert(
            key,
            (
                session
                    .as_deref()
                    .map(codec::decode_id)
                    .transpose()?
                    .map(krometrail_core::SessionId::from_uuid),
                codec::decode_u64(&bytes)?,
            ),
        );
    }
    let mut changed = 0_u64;
    for (key, (session, bytes)) in &expected {
        if stored_map.remove(key) != Some((Some(*session), *bytes)) {
            index.update_usage(UsageEntry {
                class: UsageClass::Artifact,
                object_key: key.clone().into_boxed_slice(),
                session_id: Some(*session),
                byte_len: *bytes,
            })?;
            changed += 1;
        }
    }
    for key in stored_map.into_keys() {
        index.remove_usage(UsageClass::Artifact, &key)?;
        changed += 1;
    }
    Ok(changed)
}

fn candidate(row: &ArtifactRow) -> ArtifactCandidate {
    ArtifactCandidate {
        artifact_id: row.artifact_id,
        session_id: row.session_id,
        relative_path: row.relative_path.clone(),
        byte_len: row.byte_len,
    }
}
