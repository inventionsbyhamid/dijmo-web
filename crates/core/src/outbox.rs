//! The email outbox: rows written inside the request's transaction, sent
//! later by a worker with retries and leases (`FOR UPDATE SKIP LOCKED`), so
//! several instances can share one table.

use std::time::Duration;

use serde_json::Value;
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

pub const KIND_WAITLIST_CONFIRM: &str = "waitlist_confirm";

/// After this many attempts a message is marked failed.
pub const MAX_ATTEMPTS: i32 = 20;

#[derive(Clone, Debug, FromRow)]
pub struct Job {
    pub id: i64,
    pub kind: String,
    pub signup_id: Option<Uuid>,
    pub payload: Value,
    /// Including the current one.
    pub attempts: i32,
}

/// Queues a message, dropping any not-yet-claimed message of the same kind
/// for the same signup (only the newest confirmation link should go out).
pub async fn enqueue_replacing(
    conn: &mut PgConnection,
    kind: &str,
    signup_id: Uuid,
    payload: &Value,
) -> Result<i64, sqlx::Error> {
    sqlx::query(
        "DELETE FROM email_outbox
          WHERE signup_id = $1 AND kind = $2
            AND sent_at IS NULL AND failed_at IS NULL
            AND (locked_until IS NULL OR locked_until < now())",
    )
    .bind(signup_id)
    .bind(kind)
    .execute(&mut *conn)
    .await?;

    sqlx::query_scalar(
        "INSERT INTO email_outbox (kind, signup_id, payload) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(kind)
    .bind(signup_id)
    .bind(payload)
    .fetch_one(&mut *conn)
    .await
}

/// Claims up to `limit` due messages for `lease`, counting the attempt.
pub async fn claim(pool: &PgPool, limit: i64, lease: Duration) -> Result<Vec<Job>, sqlx::Error> {
    sqlx::query_as::<_, Job>(
        "UPDATE email_outbox o
            SET locked_until = now() + make_interval(secs => $2),
                attempts = o.attempts + 1,
                updated_at = now()
          WHERE o.id IN (
                SELECT id FROM email_outbox
                 WHERE sent_at IS NULL AND failed_at IS NULL
                   AND next_attempt_at <= now()
                   AND (locked_until IS NULL OR locked_until < now())
                 ORDER BY next_attempt_at, id
                 LIMIT $1
                 FOR UPDATE SKIP LOCKED)
          RETURNING o.id, o.kind, o.signup_id, o.payload, o.attempts",
    )
    .bind(limit)
    .bind(lease.as_secs_f64())
    .fetch_all(pool)
    .await
}

pub async fn mark_sent(pool: &PgPool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE email_outbox
            SET sent_at = now(), locked_until = NULL, last_error = NULL, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Schedules another attempt after `delay`.
pub async fn mark_retry(
    pool: &PgPool,
    id: i64,
    delay: Duration,
    error: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE email_outbox
            SET next_attempt_at = now() + make_interval(secs => $2),
                locked_until = NULL, last_error = $3, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(delay.as_secs_f64())
    .bind(truncate(error))
    .execute(pool)
    .await?;
    Ok(())
}

/// Gives up on a message (a permanent error, or too many attempts).
pub async fn mark_failed(pool: &PgPool, id: i64, error: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE email_outbox
            SET failed_at = now(), locked_until = NULL, last_error = $2, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(truncate(error))
    .execute(pool)
    .await?;
    Ok(())
}

/// Deletes sent and failed messages older than `older_than`.
pub async fn prune_finished(pool: &PgPool, older_than: Duration) -> Result<u64, sqlx::Error> {
    let done = sqlx::query(
        "DELETE FROM email_outbox
          WHERE (sent_at IS NOT NULL OR failed_at IS NOT NULL)
            AND updated_at < now() - make_interval(secs => $1)",
    )
    .bind(older_than.as_secs_f64())
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Exponential backoff: 15 s, 30 s, 1 min … capped at 1 hour.
pub fn backoff(attempts: i32) -> Duration {
    let exp = attempts.clamp(1, 16) as u32 - 1;
    Duration::from_secs((15u64 << exp).min(3600))
}

fn truncate(s: &str) -> String {
    s.chars().take(500).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff(1), Duration::from_secs(15));
        assert_eq!(backoff(2), Duration::from_secs(30));
        assert_eq!(backoff(3), Duration::from_secs(60));
        assert_eq!(backoff(10), Duration::from_secs(3600));
        assert_eq!(backoff(MAX_ATTEMPTS), Duration::from_secs(3600));
    }
}
