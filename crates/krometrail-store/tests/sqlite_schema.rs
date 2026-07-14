use std::time::Duration;

use krometrail_core::ErrorCode;
use krometrail_store::{IndexStoreConfig, SqliteIndex};
use rusqlite::Connection;
use tempfile::TempDir;

fn config(directory: &TempDir) -> IndexStoreConfig {
    IndexStoreConfig {
        database_path: directory.path().join("index.sqlite3"),
        segments_directory: directory.path().join("segments"),
        busy_timeout: Duration::from_millis(250),
    }
}

#[test]
fn schema_migrates_reopens_and_has_the_declared_inventory() {
    let directory = TempDir::new().unwrap();
    let config = config(&directory);
    drop(SqliteIndex::open(config.clone()).unwrap());
    drop(SqliteIndex::open(config.clone()).unwrap());

    let connection = Connection::open(&config.database_path).unwrap();
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 2);
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_master \
             WHERE name NOT LIKE 'sqlite_%' AND type IN ('table', 'index') ORDER BY name",
        )
        .unwrap();
    let names: Vec<String> = statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        names,
        [
            "artifact_frames",
            "artifact_range_idx",
            "artifacts",
            "capture_gaps",
            "deletion_batch_state_idx",
            "deletion_batches",
            "deletion_objects",
            "frame_range_idx",
            "frames",
            "gap_range_idx",
            "pin_segments",
            "pins",
            "retention_sequence",
            "segment_retention_idx",
            "segment_retention_sequence_idx",
            "segments",
            "sessions",
            "targets",
            "timeline_observations",
            "timeline_range_idx",
            "usage",
        ]
    );

    let sql: String = connection
        .query_row(
            "SELECT group_concat(sql, ' ') FROM sqlite_master WHERE sql IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    for forbidden in [
        "request_body",
        "response_body",
        "cookie",
        "authorization",
        "raw_payload",
    ] {
        assert!(!sql.to_ascii_lowercase().contains(forbidden));
    }
}

#[test]
fn future_schema_is_refused_without_mutation() {
    let directory = TempDir::new().unwrap();
    let config = config(&directory);
    let connection = Connection::open(&config.database_path).unwrap();
    connection.pragma_update(None, "user_version", 3).unwrap();
    drop(connection);

    let error = SqliteIndex::open(config.clone()).err().unwrap();
    assert_eq!(error.code, ErrorCode::PersistenceFailed);
    let connection = Connection::open(&config.database_path).unwrap();
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 3);
}

#[test]
fn startup_errors_do_not_expose_database_paths_or_driver_text() {
    let directory = TempDir::new().unwrap();
    let occupied = directory.path().join("occupied");
    std::fs::write(&occupied, b"not a database directory").unwrap();
    let error = SqliteIndex::open(IndexStoreConfig {
        database_path: occupied.join("private.sqlite3"),
        segments_directory: directory.path().join("segments"),
        busy_timeout: Duration::from_secs(1),
    })
    .err()
    .unwrap();
    assert_eq!(error.code, ErrorCode::PersistenceFailed);
    assert!(!error.message.as_str().contains("private.sqlite3"));
    assert!(
        !error
            .message
            .as_str()
            .contains(directory.path().to_string_lossy().as_ref())
    );
}
