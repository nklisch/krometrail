use rusqlite::{Connection, TransactionBehavior};

use crate::persistence_error;

pub(crate) const CURRENT_SCHEMA_VERSION: u32 = 6;

pub(crate) const CURRENT_SCHEMA_SQL: &str = r#"
CREATE TABLE sessions (
    session_id BLOB PRIMARY KEY CHECK(length(session_id) = 16),
    record_json TEXT NULL
) STRICT;
CREATE TABLE targets (
    session_id BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id BLOB NOT NULL CHECK(length(target_id) = 16),
    record_json TEXT NULL,
    PRIMARY KEY(session_id, target_id),
    FOREIGN KEY(session_id) REFERENCES sessions(session_id) ON DELETE CASCADE
) STRICT;
CREATE TABLE segments (
    segment_id BLOB PRIMARY KEY CHECK(length(segment_id) = 16),
    session_id BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id BLOB NOT NULL CHECK(length(target_id) = 16),
    state TEXT NOT NULL CHECK(state IN ('open', 'sealed')),
    relative_path TEXT NOT NULL UNIQUE,
    start_time_be BLOB NOT NULL CHECK(length(start_time_be) = 8),
    end_time_be BLOB NULL CHECK(end_time_be IS NULL OR length(end_time_be) = 8),
    file_bytes_be BLOB NOT NULL CHECK(length(file_bytes_be) = 8),
    payload_bytes_be BLOB NOT NULL CHECK(length(payload_bytes_be) = 8),
    record_count_be BLOB NOT NULL CHECK(length(record_count_be) = 8),
    retention_sequence INTEGER,
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id)
) STRICT;
CREATE TABLE frames (
    frame_id BLOB PRIMARY KEY CHECK(length(frame_id) = 16),
    session_id BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id BLOB NOT NULL CHECK(length(target_id) = 16),
    segment_id BLOB NOT NULL CHECK(length(segment_id) = 16),
    byte_offset_be BLOB NOT NULL CHECK(length(byte_offset_be) = 8),
    session_time_be BLOB NOT NULL CHECK(length(session_time_be) = 8),
    source_time_be BLOB NULL CHECK(source_time_be IS NULL OR length(source_time_be) = 16),
    observed_time_be BLOB NOT NULL CHECK(length(observed_time_be) = 8),
    capture_ordinal_be BLOB NOT NULL CHECK(length(capture_ordinal_be) = 8),
    format TEXT NOT NULL CHECK(format IN ('jpeg', 'png')),
    image_width INTEGER NOT NULL CHECK(image_width > 0),
    image_height INTEGER NOT NULL CHECK(image_height > 0),
    viewport_width INTEGER NOT NULL CHECK(viewport_width > 0),
    viewport_height INTEGER NOT NULL CHECK(viewport_height > 0),
    device_scale REAL NOT NULL CHECK(device_scale > 0.0),
    warnings_json TEXT NOT NULL,
    UNIQUE(segment_id, byte_offset_be),
    UNIQUE(session_id, target_id, capture_ordinal_be),
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id),
    FOREIGN KEY(segment_id) REFERENCES segments(segment_id)
) STRICT;
CREATE TABLE timeline_observations (
    observation_id INTEGER PRIMARY KEY,
    session_id BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id BLOB NOT NULL CHECK(length(target_id) = 16),
    session_time_be BLOB NOT NULL CHECK(length(session_time_be) = 8),
    source_time_be BLOB NULL CHECK(source_time_be IS NULL OR length(source_time_be) = 16),
    observed_time_be BLOB NOT NULL CHECK(length(observed_time_be) = 8),
    capture_ordinal_be BLOB NULL CHECK(capture_ordinal_be IS NULL OR length(capture_ordinal_be) = 8),
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    payload_sort_key BLOB NOT NULL,
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id)
) STRICT;
CREATE TABLE capture_gaps (
    gap_id BLOB PRIMARY KEY CHECK(length(gap_id) = 16),
    session_id BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id BLOB NOT NULL CHECK(length(target_id) = 16),
    start_time_be BLOB NOT NULL CHECK(length(start_time_be) = 8),
    end_time_be BLOB NOT NULL CHECK(length(end_time_be) = 8),
    observed_time_be BLOB NOT NULL CHECK(length(observed_time_be) = 8),
    reason TEXT NOT NULL,
    estimated_missing_be BLOB NULL CHECK(estimated_missing_be IS NULL OR length(estimated_missing_be) = 8),
    detail TEXT NULL,
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id)
) STRICT;
CREATE TABLE pins (
    pin_id INTEGER PRIMARY KEY,
    session_id BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id BLOB NOT NULL CHECK(length(target_id) = 16),
    start_time_be BLOB NOT NULL CHECK(length(start_time_be) = 8),
    end_time_be BLOB NOT NULL CHECK(length(end_time_be) = 8),
    UNIQUE(session_id, target_id, start_time_be, end_time_be),
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id)
) STRICT;
CREATE TABLE pin_segments (
    pin_id INTEGER NOT NULL,
    segment_id BLOB NOT NULL CHECK(length(segment_id) = 16),
    PRIMARY KEY(pin_id, segment_id),
    FOREIGN KEY(pin_id) REFERENCES pins(pin_id) ON DELETE CASCADE,
    FOREIGN KEY(segment_id) REFERENCES segments(segment_id) ON DELETE CASCADE
) STRICT;
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
CREATE TABLE usage (
    class TEXT NOT NULL CHECK(class IN ('segment', 'index', 'browser_event', 'artifact')),
    object_key BLOB NOT NULL,
    session_id BLOB NULL CHECK(session_id IS NULL OR length(session_id) = 16),
    byte_len_be BLOB NOT NULL CHECK(length(byte_len_be) = 8),
    PRIMARY KEY(class, object_key)
) STRICT;
CREATE TABLE retention_sequence (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    next_value INTEGER NOT NULL CHECK(next_value > 0)
) STRICT;
INSERT INTO retention_sequence(singleton, next_value) VALUES (1, 1);
CREATE TABLE deletion_batches (
    batch_id BLOB PRIMARY KEY CHECK(length(batch_id) = 16),
    kind TEXT NOT NULL CHECK(kind IN ('eviction', 'session')),
    session_id BLOB NULL CHECK(session_id IS NULL OR length(session_id) = 16),
    state TEXT NOT NULL CHECK(state IN ('prepared', 'metadata_removed'))
) STRICT;
CREATE TABLE deletion_objects (
    batch_id BLOB NOT NULL CHECK(length(batch_id) = 16),
    position INTEGER NOT NULL CHECK(position >= 0),
    kind TEXT NOT NULL CHECK(kind IN ('segment', 'artifact')),
    object_key BLOB NOT NULL,
    relative_path TEXT NOT NULL CHECK(length(relative_path) > 0),
    byte_len_be BLOB NOT NULL CHECK(length(byte_len_be) = 8),
    usage_class TEXT NOT NULL CHECK(usage_class IN ('segment', 'artifact')),
    usage_key BLOB NOT NULL,
    session_id BLOB NOT NULL CHECK(length(session_id) = 16),
    PRIMARY KEY(batch_id, position),
    UNIQUE(batch_id, kind, object_key),
    FOREIGN KEY(batch_id) REFERENCES deletion_batches(batch_id) ON DELETE CASCADE
) STRICT;
CREATE TABLE interactions (
    interaction_id BLOB PRIMARY KEY CHECK(length(interaction_id) = 16),
    session_id BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id BLOB NOT NULL CHECK(length(target_id) = 16),
    operation TEXT NOT NULL,
    started_time_be BLOB NOT NULL CHECK(length(started_time_be) = 8),
    dispatched_time_be BLOB NOT NULL CHECK(length(dispatched_time_be) = 8),
    completed_time_be BLOB NOT NULL CHECK(length(completed_time_be) = 8),
    observed_time_be BLOB NULL CHECK(observed_time_be IS NULL OR length(observed_time_be) = 8),
    record_json TEXT NULL,
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id) ON DELETE CASCADE
) STRICT;
CREATE TABLE evicted_frame_ranges (
    eviction_id INTEGER PRIMARY KEY,
    session_id BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id BLOB NOT NULL CHECK(length(target_id) = 16),
    start_time_be BLOB NOT NULL CHECK(length(start_time_be) = 8),
    end_time_be BLOB NOT NULL CHECK(length(end_time_be) = 8),
    CHECK(start_time_be <= end_time_be),
    UNIQUE(session_id, target_id, start_time_be, end_time_be),
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id) ON DELETE CASCADE
) STRICT;
CREATE TABLE browser_events (
    event_id BLOB PRIMARY KEY CHECK(length(event_id)=16),
    session_id BLOB NOT NULL CHECK(length(session_id)=16),
    target_id BLOB NOT NULL CHECK(length(target_id)=16),
    event_ordinal_be BLOB NOT NULL CHECK(length(event_ordinal_be)=8),
    attachment_generation_be BLOB NOT NULL CHECK(length(attachment_generation_be)=8),
    session_time_be BLOB NOT NULL CHECK(length(session_time_be)=8),
    affected_start_time_be BLOB NOT NULL CHECK(length(affected_start_time_be)=8),
    affected_end_time_be BLOB NOT NULL CHECK(length(affected_end_time_be)=8),
    source_clock TEXT NULL CHECK(source_clock IS NULL OR source_clock IN ('cdp_monotonic','unix_epoch')),
    source_time_be BLOB NULL CHECK(source_time_be IS NULL OR length(source_time_be)=16),
    source_rounded INTEGER NOT NULL CHECK(source_rounded IN (0,1)),
    observed_time_be BLOB NOT NULL CHECK(length(observed_time_be)=8),
    kind TEXT NOT NULL,
    class TEXT NOT NULL,
    severity_rank INTEGER NOT NULL CHECK(severity_rank BETWEEN 0 AND 3),
    compact_priority INTEGER NOT NULL CHECK(compact_priority BETWEEN 0 AND 255),
    payload_json TEXT NOT NULL CHECK(length(CAST(payload_json AS BLOB)) BETWEEN 2 AND 8192),
    accounted_bytes_be BLOB NOT NULL CHECK(length(accounted_bytes_be)=8),
    retention_sequence INTEGER NOT NULL UNIQUE CHECK(retention_sequence>0),
    UNIQUE(session_id,target_id,event_ordinal_be),
    CHECK(affected_start_time_be<=affected_end_time_be),
    CHECK((source_clock IS NULL)=(source_time_be IS NULL)),
    CHECK(source_clock IS NOT NULL OR source_rounded=0),
    FOREIGN KEY(session_id,target_id) REFERENCES targets(session_id,target_id) ON DELETE CASCADE
) STRICT;
CREATE TABLE browser_event_unavailable_ranges (
    unavailable_id INTEGER PRIMARY KEY,
    session_id BLOB NOT NULL CHECK(length(session_id)=16),
    target_id BLOB NOT NULL CHECK(length(target_id)=16),
    start_time_be BLOB NOT NULL CHECK(length(start_time_be)=8),
    end_time_be BLOB NOT NULL CHECK(length(end_time_be)=8),
    first_ordinal_be BLOB NULL CHECK(first_ordinal_be IS NULL OR length(first_ordinal_be)=8),
    last_ordinal_be BLOB NULL CHECK(last_ordinal_be IS NULL OR length(last_ordinal_be)=8),
    event_count_be BLOB NOT NULL CHECK(length(event_count_be)=8),
    reason TEXT NOT NULL CHECK(reason IN ('retention_evicted','corrupt_discarded')),
    CHECK(start_time_be<=end_time_be),
    CHECK((first_ordinal_be IS NULL)=(last_ordinal_be IS NULL)),
    FOREIGN KEY(session_id,target_id) REFERENCES targets(session_id,target_id) ON DELETE CASCADE
) STRICT;
CREATE INDEX frame_range_idx ON frames(session_id, target_id, session_time_be, capture_ordinal_be);
CREATE INDEX timeline_range_idx ON timeline_observations(session_id, target_id, session_time_be, capture_ordinal_be, observed_time_be, kind, payload_sort_key);
CREATE INDEX gap_range_idx ON capture_gaps(session_id, target_id, start_time_be, end_time_be);
CREATE UNIQUE INDEX segment_retention_sequence_idx ON segments(retention_sequence);
CREATE INDEX segment_retention_idx ON segments(state, retention_sequence, segment_id);
CREATE INDEX deletion_batch_state_idx ON deletion_batches(state, batch_id);
CREATE INDEX interaction_latest_idx ON interactions(session_id, target_id, observed_time_be, completed_time_be, dispatched_time_be, started_time_be, interaction_id);
CREATE INDEX evicted_frame_range_idx ON evicted_frame_ranges(session_id, target_id, start_time_be, end_time_be, eviction_id);
CREATE UNIQUE INDEX navigation_anchor_id_idx ON timeline_observations(kind, payload_sort_key) WHERE kind='navigation';
CREATE UNIQUE INDEX marker_anchor_id_idx ON timeline_observations(kind, payload_sort_key) WHERE kind='marker';
CREATE UNIQUE INDEX interaction_boundary_point_idx ON timeline_observations(kind, payload_sort_key, session_time_be) WHERE kind='interaction_boundary';
CREATE INDEX artifact_range_idx ON artifacts(session_id,target_id,state,start_time_be,end_time_be);
CREATE INDEX artifact_ready_cache_idx ON artifacts(cache_key) WHERE state='ready';
CREATE INDEX artifact_source_frame_idx ON artifact_frames(frame_id,artifact_id,source_position);
CREATE INDEX browser_event_range_idx ON browser_events(session_id,target_id,session_time_be,event_ordinal_be,event_id);
CREATE INDEX browser_event_filter_idx ON browser_events(session_id,target_id,class,severity_rank,session_time_be,event_ordinal_be,event_id);
CREATE INDEX browser_event_priority_idx ON browser_events(session_id,target_id,compact_priority,session_time_be,event_ordinal_be,event_id);
CREATE INDEX browser_event_retention_idx ON browser_events(retention_sequence,event_id);
CREATE INDEX browser_event_unavailable_idx ON browser_event_unavailable_ranges(session_id,target_id,start_time_be,end_time_be,unavailable_id);
CREATE UNIQUE INDEX browser_event_timeline_ref_idx ON timeline_observations(kind,payload_sort_key) WHERE kind='browser_event';
CREATE TRIGGER segments_retention_sequence_required_insert
BEFORE INSERT ON segments WHEN NEW.retention_sequence IS NULL
BEGIN SELECT RAISE(ABORT, 'segment retention sequence is required'); END;
CREATE TRIGGER segments_retention_sequence_immutable_update
BEFORE UPDATE OF retention_sequence ON segments
WHEN NEW.retention_sequence IS NULL OR NEW.retention_sequence != OLD.retention_sequence
BEGIN SELECT RAISE(ABORT, 'segment retention sequence is immutable'); END;
"#;

pub(crate) fn initialize_or_validate(connection: &mut Connection) -> krometrail_core::Result<()> {
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|_| persistence_error("could not read the metadata schema version"))?;

    if version == CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    if version != 0 || has_user_tables(connection)? {
        return Err(persistence_error(format!(
            "metadata schema version {version} is incompatible with this Krometrail build; archive or remove the Krometrail data directory, then restart Krometrail"
        )));
    }

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Exclusive)
        .map_err(|_| persistence_error("could not begin metadata schema initialization"))?;
    transaction
        .execute_batch(CURRENT_SCHEMA_SQL)
        .map_err(|_| persistence_error("could not initialize the current metadata schema"))?;
    transaction
        .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
        .map_err(|_| persistence_error("could not record the metadata schema version"))?;
    transaction
        .commit()
        .map_err(|_| persistence_error("could not commit metadata schema initialization"))
}

fn has_user_tables(connection: &Connection) -> krometrail_core::Result<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%')",
            [],
            |row| row.get(0),
        )
        .map_err(|_| persistence_error("could not inspect the metadata schema"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(connection: &Connection) -> Vec<(String, String, String)> {
        let mut statement = connection
            .prepare(
                "SELECT type,name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name",
            )
            .unwrap();
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn empty_database_initializes_directly_to_current_schema() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_or_validate(&mut connection).unwrap();

        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        let actual_tables: Vec<_> = catalog(&connection)
            .into_iter()
            .filter_map(|(kind, name, _)| (kind == "table").then_some(name))
            .collect();
        assert_eq!(
            actual_tables,
            [
                "artifact_frames",
                "artifacts",
                "browser_event_unavailable_ranges",
                "browser_events",
                "capture_gaps",
                "deletion_batches",
                "deletion_objects",
                "evicted_frame_ranges",
                "frames",
                "interactions",
                "pin_segments",
                "pins",
                "retention_sequence",
                "segments",
                "sessions",
                "targets",
                "timeline_observations",
                "usage",
            ]
        );
        let expected_indexes = [
            "artifact_range_idx",
            "artifact_ready_cache_idx",
            "artifact_source_frame_idx",
            "browser_event_filter_idx",
            "browser_event_priority_idx",
            "browser_event_range_idx",
            "browser_event_retention_idx",
            "browser_event_timeline_ref_idx",
            "browser_event_unavailable_idx",
            "deletion_batch_state_idx",
            "evicted_frame_range_idx",
            "frame_range_idx",
            "gap_range_idx",
            "interaction_boundary_point_idx",
            "interaction_latest_idx",
            "marker_anchor_id_idx",
            "navigation_anchor_id_idx",
            "segment_retention_idx",
            "segment_retention_sequence_idx",
            "timeline_range_idx",
        ];
        let actual_indexes: Vec<_> = catalog(&connection)
            .into_iter()
            .filter_map(|(kind, name, _)| (kind == "index").then_some(name))
            .collect();
        assert_eq!(actual_indexes, expected_indexes);
        for name in [
            "segments_retention_sequence_required_insert",
            "segments_retention_sequence_immutable_update",
        ] {
            assert!(catalog(&connection).iter().any(|(_, item, _)| item == name));
        }
        assert!(
            catalog(&connection)
                .iter()
                .filter(|(kind, _, _)| kind == "table")
                .all(|(_, _, sql)| sql.ends_with("STRICT") || sql.ends_with("STRICT;"))
        );
        for (table, expected_columns) in [
            ("sessions", "session_id,record_json"),
            ("targets", "session_id,target_id,record_json"),
            (
                "segments",
                "segment_id,session_id,target_id,state,relative_path,start_time_be,end_time_be,file_bytes_be,payload_bytes_be,record_count_be,retention_sequence",
            ),
            (
                "frames",
                "frame_id,session_id,target_id,segment_id,byte_offset_be,session_time_be,source_time_be,observed_time_be,capture_ordinal_be,format,image_width,image_height,viewport_width,viewport_height,device_scale,warnings_json",
            ),
            (
                "timeline_observations",
                "observation_id,session_id,target_id,session_time_be,source_time_be,observed_time_be,capture_ordinal_be,kind,payload_json,payload_sort_key",
            ),
            (
                "capture_gaps",
                "gap_id,session_id,target_id,start_time_be,end_time_be,observed_time_be,reason,estimated_missing_be,detail",
            ),
            (
                "pins",
                "pin_id,session_id,target_id,start_time_be,end_time_be",
            ),
            ("pin_segments", "pin_id,segment_id"),
            (
                "artifacts",
                "artifact_id,session_id,target_id,state,kind,start_time_be,end_time_be,manifest_json,manifest_hash,media_type,output_hash,relative_path,byte_len_be,cache_key,source_fingerprint,parameter_hash,visual_epoch_hash,cache_schema_version,adapter_version,generator_name,generator_version",
            ),
            (
                "artifact_frames",
                "artifact_id,source_position,frame_id,encoded_hash",
            ),
            ("usage", "class,object_key,session_id,byte_len_be"),
            ("retention_sequence", "singleton,next_value"),
            ("deletion_batches", "batch_id,kind,session_id,state"),
            (
                "deletion_objects",
                "batch_id,position,kind,object_key,relative_path,byte_len_be,usage_class,usage_key,session_id",
            ),
            (
                "interactions",
                "interaction_id,session_id,target_id,operation,started_time_be,dispatched_time_be,completed_time_be,observed_time_be,record_json",
            ),
            (
                "evicted_frame_ranges",
                "eviction_id,session_id,target_id,start_time_be,end_time_be",
            ),
            (
                "browser_events",
                "event_id,session_id,target_id,event_ordinal_be,attachment_generation_be,session_time_be,affected_start_time_be,affected_end_time_be,source_clock,source_time_be,source_rounded,observed_time_be,kind,class,severity_rank,compact_priority,payload_json,accounted_bytes_be,retention_sequence",
            ),
            (
                "browser_event_unavailable_ranges",
                "unavailable_id,session_id,target_id,start_time_be,end_time_be,first_ordinal_be,last_ordinal_be,event_count_be,reason",
            ),
        ] {
            let actual: String = connection
                .query_row(
                    "SELECT group_concat(name, ',') FROM pragma_table_info(?1)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(actual, expected_columns, "column drift in {table}");
        }
        for (table, expected_references) in [
            ("targets", "sessions"),
            ("segments", "targets,targets"),
            ("frames", "segments,targets,targets"),
            ("timeline_observations", "targets,targets"),
            ("capture_gaps", "targets,targets"),
            ("pins", "targets,targets"),
            ("pin_segments", "segments,pins"),
            ("artifacts", "targets,targets"),
            ("artifact_frames", "frames,artifacts"),
            ("deletion_objects", "deletion_batches"),
            ("interactions", "targets,targets"),
            ("evicted_frame_ranges", "targets,targets"),
            ("browser_events", "targets,targets"),
            ("browser_event_unavailable_ranges", "targets,targets"),
        ] {
            let actual: String = connection
                .query_row(
                    "SELECT group_concat(\"table\", ',') FROM pragma_foreign_key_list(?1)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(actual, expected_references, "foreign-key drift in {table}");
        }
    }

    #[test]
    fn current_database_opens_without_mutation() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_or_validate(&mut connection).unwrap();
        connection
            .execute(
                "INSERT INTO sessions(session_id,record_json) VALUES (?1,?2)",
                rusqlite::params![vec![1_u8; 16], "retained"],
            )
            .unwrap();
        let before = catalog(&connection);

        initialize_or_validate(&mut connection).unwrap();

        assert_eq!(catalog(&connection), before);
        assert_eq!(
            connection
                .query_row::<String, _, _>("SELECT record_json FROM sessions", [], |row| row.get(0))
                .unwrap(),
            "retained"
        );
    }

    #[test]
    fn incompatible_versions_are_rejected_without_mutation() {
        for version in [1, 2, 3, 4, 5, 7, u32::MAX] {
            let mut connection = Connection::open_in_memory().unwrap();
            connection
                .execute("CREATE TABLE retained(value TEXT) STRICT", [])
                .unwrap();
            connection
                .execute("INSERT INTO retained VALUES ('unchanged')", [])
                .unwrap();
            connection
                .pragma_update(None, "user_version", version)
                .unwrap();
            let before = catalog(&connection);

            let error = initialize_or_validate(&mut connection).unwrap_err();

            assert_eq!(catalog(&connection), before);
            assert!(error.to_string().contains("archive or remove"));
            assert_eq!(
                connection
                    .query_row::<String, _, _>("SELECT value FROM retained", [], |row| row.get(0))
                    .unwrap(),
                "unchanged"
            );
        }
    }

    #[test]
    fn unversioned_non_empty_database_is_rejected_without_mutation() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute("CREATE TABLE retained(value TEXT) STRICT", [])
            .unwrap();
        let before = catalog(&connection);

        let error = initialize_or_validate(&mut connection).unwrap_err();

        assert_eq!(catalog(&connection), before);
        assert!(error.to_string().contains("archive or remove"));
    }
}
