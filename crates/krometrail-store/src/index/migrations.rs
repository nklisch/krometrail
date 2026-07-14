use rusqlite::{Connection, TransactionBehavior};

use crate::persistence_error;

use super::{schema_v1, schema_v2};

pub(crate) struct Migration {
    pub version: u32,
    pub sql: &'static str,
}

pub(crate) const LATEST_SCHEMA_VERSION: u32 = 2;
pub(crate) const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: schema_v1::SQL,
    },
    Migration {
        version: 2,
        sql: schema_v2::SQL,
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
