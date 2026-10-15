//! Dijmo website: domain and persistence.
//!
//! The waitlist lives here today. Licences, accounts and purchases arrive
//! later as sibling modules over the same pool and migrations.

pub mod config;
pub mod email_address;
pub mod keys;
pub mod outbox;
pub mod waitlist;

use std::str::FromStr;
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

pub use config::Config;
pub use email_address::EmailAddress;
pub use keys::Keys;

/// Opens the Postgres pool with timeouts on acquiring a connection and on
/// every statement.
pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    connect_with(database_url, 10).await
}

pub async fn connect_with(database_url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    let options = PgConnectOptions::from_str(database_url)?
        .application_name("dijmo-web")
        .options([("statement_timeout", "10s"), ("lock_timeout", "5s")]);
    PgPoolOptions::new()
        .max_connections(max_connections)
        .min_connections(0)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(300))
        .connect_with(options)
        .await
}

/// Runs pending migrations from `migrations/` (embedded at compile time).
/// Concurrent instances are serialised by sqlx's advisory lock.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(pool).await
}

/// Whether the database answers.
pub async fn ping(pool: &PgPool) -> bool {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
        .is_ok()
}
