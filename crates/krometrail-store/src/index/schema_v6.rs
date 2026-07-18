pub(crate) const SQL: &str = r#"
DROP INDEX artifact_range_idx;
DROP INDEX artifact_ready_cache_idx;
DROP INDEX artifact_source_frame_idx;

ALTER TABLE artifact_frames RENAME TO artifact_frames_v5;
ALTER TABLE artifacts RENAME TO artifacts_v5;

CREATE TABLE artifacts (
    artifact_id BLOB PRIMARY KEY CHECK(length(artifact_id)=16),
    session_id BLOB NOT NULL CHECK(length(session_id)=16),
    target_id BLOB NOT NULL CHECK(length(target_id)=16),
    state TEXT NOT NULL CHECK(state IN ('staging','ready')),
    kind TEXT NOT NULL CHECK(kind IN (
        'before_during_after','storyboard','difference_map','region_filmstrip','motion_history',
        'temporal_video'
    )),
    start_time_be BLOB NOT NULL CHECK(length(start_time_be)=8),
    end_time_be BLOB NOT NULL CHECK(length(end_time_be)=8),
    manifest_json TEXT NOT NULL,
    manifest_hash BLOB NOT NULL CHECK(length(manifest_hash)=32),
    media_type TEXT NOT NULL CHECK(
        (kind='temporal_video' AND media_type='video/mp4') OR
        (kind!='temporal_video' AND media_type='image/png')
    ),
    output_hash BLOB NOT NULL CHECK(length(output_hash)=32),
    relative_path TEXT NOT NULL UNIQUE,
    byte_len_be BLOB NOT NULL CHECK(length(byte_len_be)=8),
    cache_key BLOB NOT NULL UNIQUE CHECK(length(cache_key)=32),
    source_fingerprint BLOB NOT NULL CHECK(length(source_fingerprint)=32),
    parameter_hash BLOB NOT NULL CHECK(length(parameter_hash)=32),
    visual_epoch_hash BLOB NOT NULL CHECK(length(visual_epoch_hash)=32),
    cache_schema_version INTEGER NOT NULL CHECK(cache_schema_version>0),
    adapter_version TEXT NOT NULL CHECK(length(adapter_version)>0),
    generator_name TEXT NOT NULL CHECK(length(generator_name)>0),
    generator_version TEXT NOT NULL CHECK(length(generator_version)>0),
    FOREIGN KEY(session_id,target_id) REFERENCES targets(session_id,target_id) ON DELETE CASCADE
) STRICT;

INSERT INTO artifacts SELECT * FROM artifacts_v5;

CREATE TABLE artifact_frames (
    artifact_id BLOB NOT NULL CHECK(length(artifact_id)=16),
    source_position INTEGER NOT NULL CHECK(source_position>=0),
    frame_id BLOB NOT NULL CHECK(length(frame_id)=16),
    encoded_hash BLOB NOT NULL CHECK(length(encoded_hash)=32),
    PRIMARY KEY(artifact_id,source_position),
    UNIQUE(artifact_id,frame_id),
    FOREIGN KEY(artifact_id) REFERENCES artifacts(artifact_id) ON DELETE CASCADE,
    FOREIGN KEY(frame_id) REFERENCES frames(frame_id)
) STRICT;

INSERT INTO artifact_frames SELECT * FROM artifact_frames_v5;

DROP TABLE artifact_frames_v5;
DROP TABLE artifacts_v5;

CREATE INDEX artifact_range_idx ON artifacts(session_id,target_id,state,start_time_be,end_time_be);
CREATE INDEX artifact_ready_cache_idx ON artifacts(cache_key) WHERE state='ready';
CREATE INDEX artifact_source_frame_idx ON artifact_frames(frame_id,artifact_id,source_position);
"#;
