pub(crate) const SQL: &str = r#"
-- The typed browser-event row supersedes the never-production external event
-- timeline shapes. Explicit navigation and every source/artifact row remain intact.
DELETE FROM timeline_observations
WHERE kind IN ('console_message','javascript_exception','network_lifecycle');

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

CREATE INDEX browser_event_range_idx ON browser_events(
    session_id,target_id,session_time_be,event_ordinal_be,event_id
);
CREATE INDEX browser_event_filter_idx ON browser_events(
    session_id,target_id,class,severity_rank,session_time_be,event_ordinal_be,event_id
);
CREATE INDEX browser_event_priority_idx ON browser_events(
    session_id,target_id,compact_priority,session_time_be,event_ordinal_be,event_id
);
CREATE INDEX browser_event_retention_idx ON browser_events(retention_sequence,event_id);
CREATE INDEX browser_event_unavailable_idx ON browser_event_unavailable_ranges(
    session_id,target_id,start_time_be,end_time_be,unavailable_id
);
CREATE UNIQUE INDEX browser_event_timeline_ref_idx
ON timeline_observations(kind,payload_sort_key) WHERE kind='browser_event';
"#;
