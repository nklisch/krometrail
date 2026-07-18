use rusqlite::{Connection, TransactionBehavior};

use crate::persistence_error;

use super::{schema_v1, schema_v2, schema_v3, schema_v4, schema_v5, schema_v6};

pub(crate) struct Migration {
    pub version: u32,
    pub sql: &'static str,
}

pub(crate) const LATEST_SCHEMA_VERSION: u32 = 6;
pub(crate) const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: schema_v1::SQL,
    },
    Migration {
        version: 2,
        sql: schema_v2::SQL,
    },
    Migration {
        version: 3,
        sql: schema_v3::SQL,
    },
    Migration {
        version: 4,
        sql: schema_v4::SQL,
    },
    Migration {
        version: 5,
        sql: schema_v5::SQL,
    },
    Migration {
        version: 6,
        sql: schema_v6::SQL,
    },
];

pub(crate) fn migrate(connection: &mut Connection) -> krometrail_core::Result<()> {
    migrate_with(connection, MIGRATIONS, LATEST_SCHEMA_VERSION)
}

fn migrate_with(
    connection: &mut Connection,
    migrations: &[Migration],
    latest: u32,
) -> krometrail_core::Result<()> {
    let current: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|_| persistence_error("could not read the metadata schema version"))?;
    if current > latest {
        return Err(persistence_error(
            "metadata schema is newer than this Krometrail build",
        ));
    }
    if current == latest {
        return Ok(());
    }

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Exclusive)
        .map_err(|_| persistence_error("could not begin the metadata schema migration"))?;
    let mut expected = current + 1;
    for migration in migrations.iter().filter(|item| item.version > current) {
        if migration.version != expected || migration.version > latest {
            return Err(persistence_error(
                "metadata schema migrations are not contiguous",
            ));
        }
        transaction
            .execute_batch(migration.sql)
            .map_err(|_| persistence_error("could not apply the metadata schema migration"))?;
        transaction
            .pragma_update(None, "user_version", migration.version)
            .map_err(|_| persistence_error("could not record the metadata schema version"))?;
        expected += 1;
    }
    if expected != latest + 1 {
        return Err(persistence_error(
            "metadata schema migration step is missing",
        ));
    }
    transaction
        .commit()
        .map_err(|_| persistence_error("could not commit the metadata schema migration"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v3_database_upgrades_through_v6_in_one_transaction() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate_with(&mut connection, &MIGRATIONS[..3], 3).unwrap();
        let before: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(before, 3);

        migrate(&mut connection).unwrap();
        let after: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(after, 6);
        for table in [
            "interactions",
            "evicted_frame_ranges",
            "artifacts",
            "artifact_frames",
            "browser_events",
            "browser_event_unavailable_ranges",
        ] {
            let count: u32 = connection
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1);
        }
    }

    #[test]
    fn v4_purges_only_legacy_derived_artifact_state() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate_with(&mut connection, &MIGRATIONS[..3], 3).unwrap();
        let id = vec![1_u8; 16];
        let target = vec![2_u8; 16];
        connection
            .execute(
                "INSERT INTO sessions(session_id,record_json) VALUES (?1,NULL)",
                [&id],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO targets(session_id,target_id,record_json) VALUES (?1,?2,NULL)",
                rusqlite::params![&id, &target],
            )
            .unwrap();
        connection.execute(
            "INSERT INTO artifacts(artifact_id,session_id,target_id,kind,start_time_be,end_time_be,manifest_json,relative_path,byte_len_be)
             VALUES (?1,?2,?3,'storyboard',?4,?4,'{}','legacy.png',?4)",
            rusqlite::params![vec![3_u8; 16], &id, &target, vec![0_u8; 8]],
        ).unwrap();
        connection.execute(
            "INSERT INTO usage(class,object_key,session_id,byte_len_be) VALUES ('artifact',?1,?2,?3)",
            rusqlite::params![vec![3_u8; 16], &id, vec![0_u8; 8]],
        ).unwrap();

        migrate(&mut connection).unwrap();
        let retained: (u64, u64, u64, u64) = connection
            .query_row(
                "SELECT (SELECT count(*) FROM sessions),(SELECT count(*) FROM targets),
                    (SELECT count(*) FROM artifacts),
                    (SELECT count(*) FROM usage WHERE class='artifact')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(retained, (1, 1, 0, 0));
    }

    #[test]
    fn artifact_v4_upgrades_to_v5_without_rebuilding_v4_tables() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate_with(&mut connection, &MIGRATIONS[..4], 4).unwrap();
        connection
            .execute(
                "INSERT INTO sessions(session_id,record_json) VALUES (?1,NULL)",
                [vec![1_u8; 16]],
            )
            .unwrap();

        migrate(&mut connection).unwrap();
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        let sessions: u32 = connection
            .query_row("SELECT count(*) FROM sessions", [], |row| row.get(0))
            .unwrap();
        assert_eq!((version, sessions), (6, 1));
    }

    #[test]
    fn artifact_v5_rows_and_source_links_survive_v6_exactly() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate_with(&mut connection, &MIGRATIONS[..5], 5).unwrap();
        let session = vec![1_u8; 16];
        let target = vec![2_u8; 16];
        let segment = vec![3_u8; 16];
        let frame = vec![4_u8; 16];
        let artifact = vec![5_u8; 16];
        let zero = vec![0_u8; 8];
        connection
            .execute(
                "INSERT INTO sessions(session_id,record_json) VALUES (?1,NULL)",
                [&session],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO targets(session_id,target_id,record_json) VALUES (?1,?2,NULL)",
                rusqlite::params![&session, &target],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO segments(segment_id,session_id,target_id,state,relative_path,start_time_be,end_time_be,file_bytes_be,payload_bytes_be,record_count_be,retention_sequence)
                 VALUES (?1,?2,?3,'sealed','segment.kt',?4,?4,?4,?4,?4,1)",
                rusqlite::params![&segment, &session, &target, &zero],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO frames(frame_id,session_id,target_id,segment_id,byte_offset_be,session_time_be,source_time_be,observed_time_be,capture_ordinal_be,format,image_width,image_height,viewport_width,viewport_height,device_scale,warnings_json)
                 VALUES (?1,?2,?3,?4,?5,?5,NULL,?5,?5,'png',1,1,1,1,1.0,'[]')",
                rusqlite::params![&frame, &session, &target, &segment, &zero],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO artifacts(artifact_id,session_id,target_id,state,kind,start_time_be,end_time_be,manifest_json,manifest_hash,media_type,output_hash,relative_path,byte_len_be,cache_key,source_fingerprint,parameter_hash,visual_epoch_hash,cache_schema_version,adapter_version,generator_name,generator_version)
                 VALUES (?1,?2,?3,'ready','storyboard',?4,?4,'{\"stable\":true}',?5,'image/png',?6,'preserved.png',?4,?7,?8,?9,?10,1,'adapter','storyboard','1.0')",
                rusqlite::params![
                    &artifact,
                    &session,
                    &target,
                    &zero,
                    vec![11_u8; 32],
                    vec![12_u8; 32],
                    vec![13_u8; 32],
                    vec![14_u8; 32],
                    vec![15_u8; 32],
                    vec![16_u8; 32],
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO artifact_frames(artifact_id,source_position,frame_id,encoded_hash) VALUES (?1,0,?2,?3)",
                rusqlite::params![&artifact, &frame, vec![17_u8; 32]],
            )
            .unwrap();
        let before: (String, Vec<u8>, String, Vec<u8>, Vec<u8>) = connection
            .query_row(
                "SELECT manifest_json,manifest_hash,relative_path,cache_key,output_hash FROM artifacts WHERE artifact_id=?1",
                [&artifact],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .unwrap();

        migrate(&mut connection).unwrap();

        let after: (String, Vec<u8>, String, Vec<u8>, Vec<u8>) = connection
            .query_row(
                "SELECT manifest_json,manifest_hash,relative_path,cache_key,output_hash FROM artifacts WHERE artifact_id=?1",
                [&artifact],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .unwrap();
        let source: (i64, Vec<u8>, Vec<u8>) = connection
            .query_row(
                "SELECT source_position,frame_id,encoded_hash FROM artifact_frames WHERE artifact_id=?1",
                [&artifact],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(after, before);
        assert_eq!(source, (0, frame, vec![17_u8; 32]));
        assert_eq!(
            connection
                .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            6
        );
    }

    #[test]
    fn failed_v5_rolls_back_to_intact_v4() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate_with(&mut connection, &MIGRATIONS[..4], 4).unwrap();
        let broken = [Migration {
            version: 5,
            sql: "CREATE TABLE browser_events(value INTEGER) STRICT; THIS IS NOT SQL;",
        }];
        assert!(migrate_with(&mut connection, &broken, 5).is_err());
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        let table_count: u32 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='browser_events'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((version, table_count), (4, 0));
    }

    #[test]
    fn failed_migration_rolls_back_schema_and_version() {
        let mut connection = Connection::open_in_memory().unwrap();
        let migrations = [
            Migration {
                version: 1,
                sql: "CREATE TABLE first(value INTEGER) STRICT;",
            },
            Migration {
                version: 2,
                sql: "THIS IS NOT SQL;",
            },
        ];
        assert!(migrate_with(&mut connection, &migrations, 2).is_err());
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 0);
        let count: u32 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='first'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
