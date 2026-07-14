pub(crate) const SQL: &str = r#"
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
    artifact_id BLOB PRIMARY KEY CHECK(length(artifact_id) = 16),
    session_id BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id BLOB NOT NULL CHECK(length(target_id) = 16),
    kind TEXT NOT NULL,
    start_time_be BLOB NOT NULL CHECK(length(start_time_be) = 8),
    end_time_be BLOB NOT NULL CHECK(length(end_time_be) = 8),
    manifest_json TEXT NOT NULL,
    relative_path TEXT NOT NULL UNIQUE,
    byte_len_be BLOB NOT NULL CHECK(length(byte_len_be) = 8),
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id)
) STRICT;
CREATE TABLE artifact_frames (
    artifact_id BLOB NOT NULL CHECK(length(artifact_id) = 16),
    source_position INTEGER NOT NULL CHECK(source_position >= 0),
    frame_id BLOB NOT NULL CHECK(length(frame_id) = 16),
    PRIMARY KEY(artifact_id, source_position),
    UNIQUE(artifact_id, frame_id),
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
CREATE INDEX frame_range_idx ON frames(session_id, target_id, session_time_be, capture_ordinal_be);
CREATE INDEX timeline_range_idx ON timeline_observations(session_id, target_id, session_time_be, capture_ordinal_be, observed_time_be, kind, payload_sort_key);
CREATE INDEX gap_range_idx ON capture_gaps(session_id, target_id, start_time_be, end_time_be);
CREATE INDEX segment_retention_idx ON segments(state, start_time_be, segment_id);
CREATE INDEX artifact_range_idx ON artifacts(session_id, target_id, start_time_be, end_time_be);
"#;
