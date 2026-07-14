use krometrail_core::{
    ArtifactCacheKey, ArtifactCacheMetadata, ArtifactId, ArtifactPublication,
    ArtifactSourceFingerprint, FrameId, NonEmptyText, SessionId, TargetId,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use crate::persistence_error;

use super::{SqliteIndex, codec};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArtifactState {
    Staging,
    Ready,
}

impl ArtifactState {
    fn decode(value: &str) -> krometrail_core::Result<Self> {
        match value {
            "staging" => Ok(Self::Staging),
            "ready" => Ok(Self::Ready),
            _ => Err(persistence_error("stored artifact state is malformed")),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ArtifactRow {
    pub artifact_id: ArtifactId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub state: ArtifactState,
    pub kind: temporal_vision::ArtifactKind,
    pub start_time_nanos: u64,
    pub end_time_nanos: u64,
    pub manifest_json: String,
    pub manifest_hash: [u8; 32],
    pub media_type: NonEmptyText,
    pub output_hash: [u8; 32],
    pub relative_path: String,
    pub byte_len: u64,
    pub cache: ArtifactCacheMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ArtifactSourceRow {
    pub source_position: usize,
    pub frame_id: FrameId,
    pub encoded_hash: [u8; 32],
}

pub(crate) enum StageArtifact {
    Staged(ArtifactRow),
    Existing(ArtifactRow),
}

impl SqliteIndex {
    pub(crate) fn stage_artifact(
        &self,
        publication: &ArtifactPublication,
    ) -> krometrail_core::Result<StageArtifact> {
        let artifact_id = *publication.manifest.artifact_id();
        let manifest_json = serde_json::to_string(&publication.manifest)
            .map_err(|_| persistence_error("could not serialize artifact provenance"))?;
        let manifest_hash: [u8; 32] = Sha256::digest(manifest_json.as_bytes()).into();
        let relative_path = format!("{artifact_id}.png");
        let byte_len = u64::try_from(publication.encoded_bytes.len())
            .map_err(|_| persistence_error("artifact byte length exceeds storage limits"))?;

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin artifact staging"))?;
        if let Some(existing) =
            artifact_by_cache_connection(&transaction, publication.cache.cache_key, false)?
        {
            transaction
                .commit()
                .map_err(|_| persistence_error("could not close artifact cache lookup"))?;
            return Ok(StageArtifact::Existing(existing));
        }
        validate_source_rows(&transaction, publication)?;
        transaction.execute(
            "INSERT INTO artifacts(
                artifact_id,session_id,target_id,state,kind,start_time_be,end_time_be,
                manifest_json,manifest_hash,media_type,output_hash,relative_path,byte_len_be,
                cache_key,source_fingerprint,parameter_hash,visual_epoch_hash,
                cache_schema_version,adapter_version,generator_name,generator_version
             ) VALUES (?1,?2,?3,'staging',?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
            params![
                codec::id(artifact_id.as_uuid()).to_vec(),
                codec::id(publication.session_id.as_uuid()).to_vec(),
                codec::id(publication.target_id.as_uuid()).to_vec(),
                publication.manifest.artifact_kind().as_str(),
                codec::u64_blob(publication.manifest.range().start().as_nanos()).to_vec(),
                codec::u64_blob(publication.manifest.range().end().as_nanos()).to_vec(),
                &manifest_json,
                manifest_hash.to_vec(),
                publication.media_type.as_str(),
                publication.manifest.output_hash().as_bytes().to_vec(),
                &relative_path,
                codec::u64_blob(byte_len).to_vec(),
                publication.cache.cache_key.as_bytes().to_vec(),
                publication.cache.source_fingerprint.to_vec(),
                publication.cache.parameter_hash.to_vec(),
                publication.cache.visual_epoch_hash.to_vec(),
                i64::from(publication.cache.cache_schema_version),
                publication.cache.adapter_version.as_str(),
                publication.cache.generator_name.as_str(),
                publication.cache.generator_version.as_str(),
            ],
        ).map_err(|_| persistence_error("could not create artifact staging metadata"))?;
        for (position, source) in publication.sources.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO artifact_frames(artifact_id,source_position,frame_id,encoded_hash)
                 VALUES (?1,?2,?3,?4)",
                    params![
                        codec::id(artifact_id.as_uuid()).to_vec(),
                        i64::try_from(position)
                            .map_err(|_| persistence_error("too many artifact source frames"))?,
                        codec::id(source.frame_id.as_uuid()).to_vec(),
                        source.encoded_sha256.to_vec(),
                    ],
                )
                .map_err(|_| persistence_error("could not link artifact source frames"))?;
        }
        transaction
            .execute(
                "INSERT INTO usage(class,object_key,session_id,byte_len_be)
             VALUES ('artifact',?1,?2,?3)",
                params![
                    codec::id(artifact_id.as_uuid()).to_vec(),
                    codec::id(publication.session_id.as_uuid()).to_vec(),
                    codec::u64_blob(byte_len).to_vec(),
                ],
            )
            .map_err(|_| persistence_error("could not reserve artifact usage"))?;
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit artifact staging"))?;
        drop(connection);
        let row = self
            .artifact_row(artifact_id)?
            .ok_or_else(|| persistence_error("staged artifact metadata disappeared"))?;
        Ok(StageArtifact::Staged(row))
    }

    pub(crate) fn finalize_artifact(
        &self,
        artifact_id: ArtifactId,
        key: ArtifactCacheKey,
        session_id: SessionId,
        target_id: TargetId,
        sources: &[ArtifactSourceFingerprint],
    ) -> krometrail_core::Result<bool> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| persistence_error("could not begin artifact finalization"))?;
        let row = artifact_by_id_connection(&transaction, artifact_id)?
            .ok_or_else(|| persistence_error("artifact staging metadata disappeared"))?;
        if row.state != ArtifactState::Staging
            || row.cache.cache_key != key
            || row.session_id != session_id
            || row.target_id != target_id
        {
            return Err(persistence_error(
                "artifact staging metadata changed before finalization",
            ));
        }
        validate_source_ids(&transaction, session_id, target_id, sources)?;
        let changed = transaction.execute(
            "UPDATE artifacts SET state='ready' WHERE artifact_id=?1 AND state='staging' AND cache_key=?2",
            params![codec::id(artifact_id.as_uuid()).to_vec(), key.as_bytes().to_vec()],
        ).map_err(|_| persistence_error("could not publish ready artifact metadata"))?;
        transaction
            .commit()
            .map_err(|_| persistence_error("could not commit ready artifact metadata"))?;
        Ok(changed == 1)
    }

    pub(crate) fn artifact_by_cache(
        &self,
        key: ArtifactCacheKey,
        ready_only: bool,
    ) -> krometrail_core::Result<Option<ArtifactRow>> {
        let connection = self.connection()?;
        artifact_by_cache_connection(&connection, key, ready_only)
    }

    pub(crate) fn artifact_row(
        &self,
        artifact_id: ArtifactId,
    ) -> krometrail_core::Result<Option<ArtifactRow>> {
        let connection = self.connection()?;
        artifact_by_id_connection(&connection, artifact_id)
    }

    pub(crate) fn artifact_sources(
        &self,
        artifact_id: ArtifactId,
    ) -> krometrail_core::Result<Vec<ArtifactSourceRow>> {
        let connection = self.connection()?;
        artifact_sources_connection(&connection, artifact_id)
    }

    pub(crate) fn artifact_rows(&self) -> krometrail_core::Result<Vec<ArtifactRow>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!("{} ORDER BY artifact_id", artifact_select("1=1")))
            .map_err(|_| persistence_error("could not prepare artifact recovery scan"))?;
        let rows = statement
            .query_map([], raw_artifact)
            .map_err(|_| persistence_error("could not query artifact recovery rows"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| persistence_error("could not read artifact recovery rows"))?
            .into_iter()
            .map(decode_artifact)
            .collect()
    }
}

fn validate_source_rows(
    transaction: &Transaction<'_>,
    publication: &ArtifactPublication,
) -> krometrail_core::Result<()> {
    validate_source_ids(
        transaction,
        publication.session_id,
        publication.target_id,
        &publication.sources,
    )
}

fn validate_source_ids(
    connection: &Connection,
    session_id: SessionId,
    target_id: TargetId,
    sources: &[ArtifactSourceFingerprint],
) -> krometrail_core::Result<()> {
    if sources.is_empty() {
        return Err(persistence_error("artifact source links are empty"));
    }
    for source in sources {
        let retained: Option<(Vec<u8>, Vec<u8>)> = connection
            .query_row(
                "SELECT session_id,target_id FROM frames WHERE frame_id=?1",
                params![codec::id(source.frame_id.as_uuid()).to_vec()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|_| persistence_error("could not validate artifact source retention"))?;
        let Some((stored_session, stored_target)) = retained else {
            return Err(source_lost_error(session_id, target_id));
        };
        if codec::decode_id(&stored_session)? != *session_id.as_uuid()
            || codec::decode_id(&stored_target)? != *target_id.as_uuid()
        {
            return Err(persistence_error(
                "artifact source belongs to another session or target",
            ));
        }
    }
    Ok(())
}

pub(crate) fn purge_artifacts_for_segment_tx(
    transaction: &Transaction<'_>,
    segment_id: krometrail_core::SegmentId,
) -> krometrail_core::Result<u64> {
    let mut statement = transaction
        .prepare(
            "SELECT DISTINCT a.artifact_id FROM artifacts a
         JOIN artifact_frames af USING(artifact_id)
         JOIN frames f USING(frame_id) WHERE f.segment_id=?1",
        )
        .map_err(|_| persistence_error("could not prepare artifact source invalidation"))?;
    let ids = statement
        .query_map(params![codec::id(segment_id.as_uuid()).to_vec()], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .map_err(|_| persistence_error("could not query artifacts linked to lost sources"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read artifacts linked to lost sources"))?;
    drop(statement);
    for id in &ids {
        transaction
            .execute(
                "DELETE FROM usage WHERE class='artifact' AND object_key=?1",
                params![id],
            )
            .map_err(|_| persistence_error("could not invalidate artifact usage"))?;
        transaction
            .execute("DELETE FROM artifacts WHERE artifact_id=?1", params![id])
            .map_err(|_| persistence_error("could not invalidate artifact metadata"))?;
    }
    Ok(ids.len() as u64)
}

fn artifact_by_cache_connection(
    connection: &Connection,
    key: ArtifactCacheKey,
    ready_only: bool,
) -> krometrail_core::Result<Option<ArtifactRow>> {
    let predicate = if ready_only {
        "cache_key=?1 AND state='ready'"
    } else {
        "cache_key=?1"
    };
    connection
        .query_row(
            &artifact_select(predicate),
            params![key.as_bytes().to_vec()],
            raw_artifact,
        )
        .optional()
        .map_err(|_| persistence_error("could not query artifact cache metadata"))?
        .map(decode_artifact)
        .transpose()
}

fn artifact_by_id_connection(
    connection: &Connection,
    artifact_id: ArtifactId,
) -> krometrail_core::Result<Option<ArtifactRow>> {
    connection
        .query_row(
            &artifact_select("artifact_id=?1"),
            params![codec::id(artifact_id.as_uuid()).to_vec()],
            raw_artifact,
        )
        .optional()
        .map_err(|_| persistence_error("could not query artifact metadata"))?
        .map(decode_artifact)
        .transpose()
}

fn artifact_sources_connection(
    connection: &Connection,
    artifact_id: ArtifactId,
) -> krometrail_core::Result<Vec<ArtifactSourceRow>> {
    let mut statement = connection
        .prepare(
            "SELECT source_position,frame_id,encoded_hash FROM artifact_frames
         WHERE artifact_id=?1 ORDER BY source_position",
        )
        .map_err(|_| persistence_error("could not prepare artifact source lookup"))?;
    let rows = statement
        .query_map(params![codec::id(artifact_id.as_uuid()).to_vec()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(|_| persistence_error("could not query artifact source links"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| persistence_error("could not read artifact source links"))?
        .into_iter()
        .map(|(position, frame, hash)| {
            Ok(ArtifactSourceRow {
                source_position: usize::try_from(position).map_err(|_| {
                    persistence_error("stored artifact source position is malformed")
                })?,
                frame_id: FrameId::from_uuid(codec::decode_id(&frame)?),
                encoded_hash: decode_hash(&hash)?,
            })
        })
        .collect()
}

fn artifact_select(predicate: &str) -> String {
    format!(
        "SELECT artifact_id,session_id,target_id,state,kind,start_time_be,end_time_be,
                manifest_json,manifest_hash,media_type,output_hash,relative_path,byte_len_be,
                cache_key,source_fingerprint,parameter_hash,visual_epoch_hash,
                cache_schema_version,adapter_version,generator_name,generator_version
         FROM artifacts WHERE {predicate}"
    )
}

struct RawArtifact {
    id: Vec<u8>,
    session: Vec<u8>,
    target: Vec<u8>,
    state: String,
    kind: String,
    start: Vec<u8>,
    end: Vec<u8>,
    manifest: String,
    manifest_hash: Vec<u8>,
    media: String,
    output_hash: Vec<u8>,
    relative_path: String,
    byte_len: Vec<u8>,
    cache_key: Vec<u8>,
    source_fingerprint: Vec<u8>,
    parameter_hash: Vec<u8>,
    epoch_hash: Vec<u8>,
    cache_version: i64,
    adapter: String,
    generator_name: String,
    generator_version: String,
}

fn raw_artifact(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawArtifact> {
    Ok(RawArtifact {
        id: row.get(0)?,
        session: row.get(1)?,
        target: row.get(2)?,
        state: row.get(3)?,
        kind: row.get(4)?,
        start: row.get(5)?,
        end: row.get(6)?,
        manifest: row.get(7)?,
        manifest_hash: row.get(8)?,
        media: row.get(9)?,
        output_hash: row.get(10)?,
        relative_path: row.get(11)?,
        byte_len: row.get(12)?,
        cache_key: row.get(13)?,
        source_fingerprint: row.get(14)?,
        parameter_hash: row.get(15)?,
        epoch_hash: row.get(16)?,
        cache_version: row.get(17)?,
        adapter: row.get(18)?,
        generator_name: row.get(19)?,
        generator_version: row.get(20)?,
    })
}

fn decode_artifact(raw: RawArtifact) -> krometrail_core::Result<ArtifactRow> {
    super::retention::validate_file_name(&raw.relative_path)?;
    let cache_version = u32::try_from(raw.cache_version)
        .ok()
        .filter(|version| *version > 0)
        .ok_or_else(|| persistence_error("stored artifact cache version is malformed"))?;
    Ok(ArtifactRow {
        artifact_id: ArtifactId::from_uuid(codec::decode_id(&raw.id)?),
        session_id: SessionId::from_uuid(codec::decode_id(&raw.session)?),
        target_id: TargetId::from_uuid(codec::decode_id(&raw.target)?),
        state: ArtifactState::decode(&raw.state)?,
        kind: decode_kind(&raw.kind)?,
        start_time_nanos: codec::decode_u64(&raw.start)?,
        end_time_nanos: codec::decode_u64(&raw.end)?,
        manifest_json: raw.manifest,
        manifest_hash: decode_hash(&raw.manifest_hash)?,
        media_type: NonEmptyText::new(raw.media)
            .map_err(|_| persistence_error("stored artifact media type is empty"))?,
        output_hash: decode_hash(&raw.output_hash)?,
        relative_path: raw.relative_path,
        byte_len: codec::decode_u64(&raw.byte_len)?,
        cache: ArtifactCacheMetadata {
            cache_key: ArtifactCacheKey::from_bytes(decode_hash(&raw.cache_key)?),
            source_fingerprint: decode_hash(&raw.source_fingerprint)?,
            parameter_hash: decode_hash(&raw.parameter_hash)?,
            visual_epoch_hash: decode_hash(&raw.epoch_hash)?,
            cache_schema_version: cache_version,
            adapter_version: NonEmptyText::new(raw.adapter)
                .map_err(|_| persistence_error("stored artifact adapter version is empty"))?,
            generator_name: NonEmptyText::new(raw.generator_name)
                .map_err(|_| persistence_error("stored artifact generator name is empty"))?,
            generator_version: NonEmptyText::new(raw.generator_version)
                .map_err(|_| persistence_error("stored artifact generator version is empty"))?,
        },
    })
}

fn decode_hash(value: &[u8]) -> krometrail_core::Result<[u8; 32]> {
    value
        .try_into()
        .map_err(|_| persistence_error("stored artifact hash is malformed"))
}

fn decode_kind(value: &str) -> krometrail_core::Result<temporal_vision::ArtifactKind> {
    temporal_vision::ArtifactKind::ALL
        .iter()
        .copied()
        .find(|kind| kind.as_str() == value)
        .ok_or_else(|| persistence_error("stored artifact kind is malformed"))
}

fn source_lost_error(
    session_id: SessionId,
    target_id: TargetId,
) -> krometrail_core::KrometrailError {
    krometrail_core::KrometrailError::new(
        krometrail_core::ErrorCode::NotFound,
        NonEmptyText::new("artifact source evidence is no longer retained")
            .expect("static source error is non-empty"),
    )
    .with_context(krometrail_core::ErrorContext {
        session_id: Some(session_id),
        target_id: Some(target_id),
        ..Default::default()
    })
}
