pub(crate) const SQL: &str = r#"
ALTER TABLE segments ADD COLUMN retention_sequence INTEGER;

UPDATE segments
SET retention_sequence = (
    SELECT count(*)
    FROM segments AS older
    WHERE older.rowid <= segments.rowid
);

CREATE TABLE retention_sequence (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    next_value INTEGER NOT NULL CHECK(next_value > 0)
) STRICT;
INSERT INTO retention_sequence(singleton, next_value)
VALUES (1, (SELECT coalesce(max(retention_sequence), 0) + 1 FROM segments));

CREATE UNIQUE INDEX segment_retention_sequence_idx
ON segments(retention_sequence);

CREATE TRIGGER segments_retention_sequence_required_insert
BEFORE INSERT ON segments
WHEN NEW.retention_sequence IS NULL
BEGIN
    SELECT RAISE(ABORT, 'segment retention sequence is required');
END;

CREATE TRIGGER segments_retention_sequence_immutable_update
BEFORE UPDATE OF retention_sequence ON segments
WHEN NEW.retention_sequence IS NULL OR NEW.retention_sequence != OLD.retention_sequence
BEGIN
    SELECT RAISE(ABORT, 'segment retention sequence is immutable');
END;

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

CREATE INDEX deletion_batch_state_idx ON deletion_batches(state, batch_id);
DROP INDEX segment_retention_idx;
CREATE INDEX segment_retention_idx ON segments(state, retention_sequence, segment_id);
"#;
