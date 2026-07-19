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
fn current_schema_reopens_and_has_the_declared_inventory() {
    let directory = TempDir::new().unwrap();
    let config = config(&directory);
    drop(SqliteIndex::open(config.clone()).unwrap());
    drop(SqliteIndex::open(config.clone()).unwrap());

    let connection = Connection::open(&config.database_path).unwrap();
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 7);
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
    for required in [
        "artifact_frames",
        "artifact_range_idx",
        "artifact_ready_cache_idx",
        "artifact_source_frame_idx",
        "artifacts",
        "browser_event_filter_idx",
        "browser_event_priority_idx",
        "browser_event_range_idx",
        "browser_event_retention_idx",
        "browser_event_timeline_ref_idx",
        "browser_event_unavailable_idx",
        "browser_event_unavailable_ranges",
        "browser_events",
        "frames",
        "segments",
        "sessions",
        "targets",
        "usage",
    ] {
        assert!(
            names.iter().any(|name| name == required),
            "missing {required}"
        );
    }

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
    assert!(sql.contains("'temporal_video'"));
    assert!(sql.contains("'video/mp4'"));
}

#[test]
fn future_schema_is_replaced_with_the_current_cache() {
    let directory = TempDir::new().unwrap();
    let config = config(&directory);
    let connection = Connection::open(&config.database_path).unwrap();
    connection.pragma_update(None, "user_version", 8).unwrap();
    drop(connection);

    drop(SqliteIndex::open(config.clone()).unwrap());
    let connection = Connection::open(&config.database_path).unwrap();
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 7);
}

#[test]
fn incompatible_recording_cache_is_cleared_without_touching_other_data() {
    let directory = TempDir::new().unwrap();
    let config = config(&directory);
    let connection = Connection::open(&config.database_path).unwrap();
    connection
        .execute("CREATE TABLE stale(value TEXT) STRICT", [])
        .unwrap();
    connection.pragma_update(None, "user_version", 6).unwrap();
    drop(connection);

    std::fs::create_dir_all(config.segments_directory.join("nested")).unwrap();
    std::fs::write(
        config.segments_directory.join("nested/stale.segment"),
        b"stale",
    )
    .unwrap();
    for cache in ["artifacts", ".trash"] {
        std::fs::create_dir_all(directory.path().join(cache)).unwrap();
        std::fs::write(directory.path().join(cache).join("stale"), b"stale").unwrap();
    }
    std::fs::create_dir_all(directory.path().join("browser-profiles/default")).unwrap();
    std::fs::write(
        directory.path().join("browser-profiles/default/profile"),
        b"preserve",
    )
    .unwrap();
    std::fs::write(directory.path().join("config.toml"), b"preserve").unwrap();

    drop(SqliteIndex::open(config.clone()).unwrap());

    let connection = Connection::open(&config.database_path).unwrap();
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 7);
    let stale_table: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='stale')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!stale_table);
    assert!(
        !config
            .segments_directory
            .join("nested/stale.segment")
            .exists()
    );
    assert!(!directory.path().join("artifacts").exists());
    assert!(!directory.path().join(".trash").exists());
    assert_eq!(
        std::fs::read(directory.path().join("browser-profiles/default/profile")).unwrap(),
        b"preserve"
    );
    assert_eq!(
        std::fs::read(directory.path().join("config.toml")).unwrap(),
        b"preserve"
    );
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
