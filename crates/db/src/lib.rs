use std::{str::FromStr, sync::Arc, time::Duration};

use sqlx::{
    ConnectOptions, Error, Pool, Sqlite,
    migrate::MigrateError,
    sqlite::{
        SqliteConnectOptions, SqliteConnection, SqliteJournalMode, SqlitePoolOptions,
        SqliteSynchronous,
    },
};
use utils::assets::asset_dir;

pub mod models;

/// How long a statement waits for the write lock before giving up. Long enough
/// to sit out a WAL checkpoint on a busy runner; the alternative is
/// `SQLITE_BUSY` reaching a caller that may not be able to retry.
const BUSY_TIMEOUT: Duration = Duration::from_secs(30);

/// Connection options shared by every pool.
///
/// WAL matters more than it looks: under the default rollback journal a single
/// writer takes an exclusive lock on the whole database, so every read queues
/// behind every write. On a runner driving dozens of workspaces that produced
/// multi-second `acquire` waits, 500s on ordinary reads, and — because
/// `SQLITE_BUSY` surfaced as a plain error — a lost "process finished" write
/// that left a workspace permanently stuck in `running` (2026-08-06). WAL lets
/// readers proceed during a write; `synchronous = NORMAL` trades an fsync per
/// commit for throughput, risking only the last commits on a host crash, which
/// for local workspace bookkeeping is the right trade.
/// Connections the app may hold open at once.
///
/// This has to be set explicitly on every pool: sqlx defaults to 10, and a pool
/// built from `SqlitePoolOptions::new()` without it silently runs on ten
/// connections however wide the intent.
///
/// Ten is not enough here. In WAL mode readers do not block one another, but a
/// writer waiting on SQLite's single write lock holds its connection for the
/// whole wait — so a handful of concurrent writers can occupy the pool and
/// leave every reader queueing behind them for seconds. Raising the cap does
/// not make writes concurrent; it stops them monopolising the pool.
const MAX_CONNECTIONS: u32 = 64;

fn connect_options() -> Result<SqliteConnectOptions, Error> {
    let database_url = format!(
        "sqlite://{}",
        asset_dir().join("db.v2.sqlite").to_string_lossy()
    );
    Ok(SqliteConnectOptions::from_str(&database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(BUSY_TIMEOUT))
}

async fn run_migrations(pool: &Pool<Sqlite>) -> Result<(), Error> {
    use std::collections::HashSet;

    let migrator = sqlx::migrate!("./migrations");
    let mut processed_versions: HashSet<i64> = HashSet::new();

    loop {
        match migrator.run(pool).await {
            Ok(()) => return Ok(()),
            Err(MigrateError::VersionMismatch(version)) => {
                if cfg!(debug_assertions) {
                    // return the error in debug mode to catch migration issues early
                    return Err(sqlx::Error::Migrate(Box::new(
                        MigrateError::VersionMismatch(version),
                    )));
                }

                if !cfg!(windows) {
                    // On non-Windows platforms, we do not attempt to auto-fix checksum mismatches
                    return Err(sqlx::Error::Migrate(Box::new(
                        MigrateError::VersionMismatch(version),
                    )));
                }

                // Guard against infinite loop
                if !processed_versions.insert(version) {
                    return Err(sqlx::Error::Migrate(Box::new(
                        MigrateError::VersionMismatch(version),
                    )));
                }

                // On Windows, there can be checksum mismatches due to line ending differences
                // or other platform-specific issues. Update the stored checksum and retry.
                tracing::warn!(
                    "Migration version {} has checksum mismatch, updating stored checksum (likely platform-specific difference)",
                    version
                );

                // Find the migration with the mismatched version and get its current checksum
                if let Some(migration) = migrator.iter().find(|m| m.version == version) {
                    // Update the checksum in _sqlx_migrations to match the current file
                    sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?")
                        .bind(&*migration.checksum)
                        .bind(version)
                        .execute(pool)
                        .await?;
                } else {
                    // Migration not found in current set, can't fix
                    return Err(sqlx::Error::Migrate(Box::new(
                        MigrateError::VersionMismatch(version),
                    )));
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
}

#[derive(Clone)]
pub struct DBService {
    pub pool: Pool<Sqlite>,
}

impl DBService {
    pub async fn new() -> Result<DBService, Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(MAX_CONNECTIONS)
            .connect_with(connect_options()?)
            .await?;
        run_migrations(&pool).await?;
        Ok(DBService { pool })
    }

    pub async fn new_migration_pool() -> Result<Pool<Sqlite>, Error> {
        SqlitePoolOptions::new()
            .max_connections(MAX_CONNECTIONS)
            .connect_with(connect_options()?.disable_statement_logging())
            .await
    }

    pub async fn new_with_after_connect<F>(after_connect: F) -> Result<DBService, Error>
    where
        F: for<'a> Fn(
                &'a mut SqliteConnection,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), Error>> + Send + 'a>,
            > + Send
            + Sync
            + 'static,
    {
        let pool = Self::create_pool(Some(Arc::new(after_connect))).await?;
        Ok(DBService { pool })
    }

    async fn create_pool<F>(after_connect: Option<Arc<F>>) -> Result<Pool<Sqlite>, Error>
    where
        F: for<'a> Fn(
                &'a mut SqliteConnection,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), Error>> + Send + 'a>,
            > + Send
            + Sync
            + 'static,
    {
        let options = connect_options()?;

        let pool = if let Some(hook) = after_connect {
            SqlitePoolOptions::new()
                .max_connections(MAX_CONNECTIONS)
                .after_connect(move |conn, _meta| {
                    let hook = hook.clone();
                    Box::pin(async move {
                        hook(conn).await?;
                        Ok(())
                    })
                })
                .connect_with(options)
                .await?
        } else {
            SqlitePoolOptions::new()
                .max_connections(MAX_CONNECTIONS)
                .connect_with(options)
                .await?
        };

        run_migrations(&pool).await?;
        Ok(pool)
    }
}
