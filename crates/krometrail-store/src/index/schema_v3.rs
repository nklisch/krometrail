pub(crate) const SQL: &str = r#"
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

CREATE INDEX interaction_latest_idx ON interactions(
    session_id, target_id, observed_time_be, completed_time_be,
    dispatched_time_be, started_time_be, interaction_id
);

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

CREATE INDEX evicted_frame_range_idx ON evicted_frame_ranges(
    session_id, target_id, start_time_be, end_time_be, eviction_id
);

-- Older generic writes were not required to be idempotent. Keep the earliest exact
-- anchor row before installing the durable uniqueness contracts.
DELETE FROM timeline_observations
WHERE kind IN ('navigation', 'marker')
  AND observation_id NOT IN (
    SELECT min(observation_id)
    FROM timeline_observations
    WHERE kind IN ('navigation', 'marker')
    GROUP BY kind, payload_sort_key
  );
DELETE FROM timeline_observations
WHERE kind = 'interaction_boundary'
  AND observation_id NOT IN (
    SELECT min(observation_id)
    FROM timeline_observations
    WHERE kind = 'interaction_boundary'
    GROUP BY kind, payload_sort_key, session_time_be
  );

CREATE UNIQUE INDEX navigation_anchor_id_idx
ON timeline_observations(kind, payload_sort_key) WHERE kind='navigation';
CREATE UNIQUE INDEX marker_anchor_id_idx
ON timeline_observations(kind, payload_sort_key) WHERE kind='marker';
CREATE UNIQUE INDEX interaction_boundary_point_idx
ON timeline_observations(kind, payload_sort_key, session_time_be)
WHERE kind='interaction_boundary';
"#;
