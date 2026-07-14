#[allow(dead_code)] // The next checkpoint consumes these schema boundary codecs.
mod codec;
mod migrations;
mod schema_v1;

use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use rusqlite::{Connection, OpenFlags};

use crate::persistence_error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexStoreConfig {
    pub database_path: PathBuf,
    pub segments_directory: PathBuf,
    pub busy_timeout: Duration,
}

/// File-backed searchable metadata authority.
pub struct SqliteIndex {
    #[allow(dead_code)] // Timeline/index adapters land in the next checkpoint.
    connection: Mutex<Connection>,
    #[allow(dead_code)] // Address-backed frame reads land after timeline indexing.
    segments_directory: PathBuf,
}

impl SqliteIndex {
    pub fn open(config: IndexStoreConfig) -> krometrail_core::Result<Self> {
        if config.busy_timeout.is_zero() {
            return Err(persistence_error(
                "metadata busy timeout must be greater than zero",
            ));
        }
        let parent = config
            .database_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or_else(|| persistence_error("metadata database requires a parent directory"))?;
        fs::create_dir_all(parent)
            .map_err(|_| persistence_error("could not create the metadata directory"))?;
        fs::create_dir_all(&config.segments_directory)
            .map_err(|_| persistence_error("could not create the segment directory"))?;

        let mut connection = Connection::open_with_flags(
            &config.database_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| persistence_error("could not open the metadata database"))?;
        connection
            .busy_timeout(config.busy_timeout)
            .map_err(|_| persistence_error("could not configure metadata lock contention"))?;
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(|_| persistence_error("could not enable metadata foreign keys"))?;
        let journal: String = connection
            .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
            .map_err(|_| persistence_error("could not enable metadata write-ahead logging"))?;
        if !journal.eq_ignore_ascii_case("wal") {
            return Err(persistence_error(
                "metadata database did not enable write-ahead logging",
            ));
        }
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(|_| persistence_error("could not configure metadata durability"))?;
        let foreign_keys: u32 = connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .map_err(|_| persistence_error("could not verify metadata foreign keys"))?;
        let synchronous: u32 = connection
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .map_err(|_| persistence_error("could not verify metadata durability"))?;
        if foreign_keys != 1 || synchronous != 2 {
            return Err(persistence_error(
                "metadata database safety settings were not retained",
            ));
        }
        migrations::migrate(&mut connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
            segments_directory: config.segments_directory,
        })
    }

    #[allow(dead_code)]
    pub(crate) fn connection(&self) -> krometrail_core::Result<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| persistence_error("metadata connection is unavailable"))
    }

    #[allow(dead_code)]
    pub(crate) fn segments_directory(&self) -> &std::path::Path {
        &self.segments_directory
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn open_applies_and_retains_required_connection_settings() {
        let directory = TempDir::new().unwrap();
        let index = SqliteIndex::open(IndexStoreConfig {
            database_path: directory.path().join("index.sqlite3"),
            segments_directory: directory.path().join("segments"),
            busy_timeout: Duration::from_millis(321),
        })
        .unwrap();
        let connection = index.connection().unwrap();
        let journal: String = connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        let foreign_keys: u32 = connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        let synchronous: u32 = connection
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();
        let timeout: u32 = connection
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(journal, "wal");
        assert_eq!(foreign_keys, 1);
        assert_eq!(synchronous, 2);
        assert_eq!(timeout, 321);
        assert_eq!(version, migrations::LATEST_SCHEMA_VERSION);
        assert_eq!(
            index.segments_directory(),
            directory.path().join("segments")
        );
    }
}
