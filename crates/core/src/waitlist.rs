//! The waitlist, with double opt-in.
//!
//! 1. [`join`] stores a pending signup and queues a confirmation email in the
//!    outbox, in one transaction. Known addresses are handled silently.
//! 2. The outbox worker calls [`issue_confirmation`] right before sending:
//!    it mints the token and stores only its SHA-256, so the raw token exists
//!    only in the email.
//! 3. [`confirm`] marks the signup confirmed (idempotent).
//! 4. [`leave`] deletes the signup.

use std::time::Duration;

use serde_json::{Map, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::email_address::EmailAddress;
use crate::keys::{ConfirmationToken, hash_token, is_well_formed_token};
use crate::outbox;

/// Timings of the waitlist.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    /// A confirmation token is valid for this long after it is sent.
    pub token_ttl: Duration,
    /// At most one confirmation email per address in this window.
    pub resend_cooldown: Duration,
    /// Unconfirmed signups are deleted after this long without a new request.
    pub prune_unconfirmed_after: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            token_ttl: Duration::from_secs(72 * 3600),
            resend_cooldown: Duration::from_secs(10 * 60),
            prune_unconfirmed_after: Duration::from_secs(30 * 24 * 3600),
        }
    }
}

/// What [`join`] did. The HTTP answer is the same for all of them, so nobody
/// can probe whether an address is on the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinOutcome {
    /// A confirmation email is queued.
    Queued { signup_id: Uuid },
    /// A confirmation email went out less than the cooldown ago.
    CoolingDown,
    /// The address is already confirmed; nothing to send.
    AlreadyConfirmed,
}

/// The UTM parameters kept with a signup.
pub const UTM_KEYS: [&str; 5] = [
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "utm_term",
    "utm_content",
];

/// Builds the stored `source` from query pairs: known UTM keys only, each
/// value trimmed and capped at 100 characters.
pub fn source_from_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Value {
    let mut map = Map::new();
    for (k, v) in pairs {
        let v = v.trim();
        if UTM_KEYS.contains(&k) && !v.is_empty() && !map.contains_key(k) {
            let capped: String = v.chars().filter(|c| !c.is_control()).take(100).collect();
            map.insert(k.to_owned(), Value::String(capped));
        }
    }
    Value::Object(map)
}

fn secs(d: Duration) -> f64 {
    d.as_secs_f64()
}

/// Adds `email` to the waitlist (pending) and queues a confirmation email,
/// unless one went out within the cooldown or the address is confirmed.
pub async fn join(
    pool: &PgPool,
    email: &EmailAddress,
    source: &Value,
    ip_hash: Option<&[u8]>,
    policy: &Policy,
) -> Result<JoinOutcome, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let inserted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO waitlist_signups (email, source, ip_hash)
         VALUES ($1::citext, $2, $3)
         ON CONFLICT (email) DO NOTHING
         RETURNING id",
    )
    .bind(email.as_str())
    .bind(source)
    .bind(ip_hash)
    .fetch_optional(&mut *tx)
    .await?;

    let signup_id = match inserted {
        Some(id) => Some(id),
        None => {
            sqlx::query_scalar(
                "UPDATE waitlist_signups
                    SET last_enqueued_at = now(), updated_at = now()
                  WHERE email = $1::citext
                    AND confirmed_at IS NULL
                    AND last_enqueued_at <= now() - make_interval(secs => $2)
                  RETURNING id",
            )
            .bind(email.as_str())
            .bind(secs(policy.resend_cooldown))
            .fetch_optional(&mut *tx)
            .await?
        }
    };

    let outcome = match signup_id {
        Some(signup_id) => {
            outbox::enqueue_replacing(
                &mut tx,
                outbox::KIND_WAITLIST_CONFIRM,
                signup_id,
                &Value::Object(Map::new()),
            )
            .await?;
            JoinOutcome::Queued { signup_id }
        }
        None => {
            let confirmed: Option<bool> = sqlx::query_scalar(
                "SELECT confirmed_at IS NOT NULL FROM waitlist_signups WHERE email = $1::citext",
            )
            .bind(email.as_str())
            .fetch_optional(&mut *tx)
            .await?;
            if confirmed == Some(true) {
                JoinOutcome::AlreadyConfirmed
            } else {
                JoinOutcome::CoolingDown
            }
        }
    };

    tx.commit().await?;
    Ok(outcome)
}

/// What the confirmation email needs.
#[derive(Debug)]
pub struct ConfirmationMail {
    pub signup_id: Uuid,
    pub email: String,
    /// The raw token for the link. It is not stored anywhere.
    pub token: String,
}

/// Mints a fresh confirmation token for a pending signup and stores its hash.
/// Returns `None` if the signup is gone (left) or already confirmed.
pub async fn issue_confirmation(
    pool: &PgPool,
    signup_id: Uuid,
) -> Result<Option<ConfirmationMail>, sqlx::Error> {
    let token = ConfirmationToken::generate();
    let email: Option<String> = sqlx::query_scalar(
        "UPDATE waitlist_signups
            SET token_hash = $2, token_created_at = now(), updated_at = now()
          WHERE id = $1 AND confirmed_at IS NULL
          RETURNING email::text",
    )
    .bind(signup_id)
    .bind(&token.hash[..])
    .fetch_optional(pool)
    .await?;
    Ok(email.map(|email| ConfirmationMail {
        signup_id,
        email,
        token: token.token,
    }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmOutcome {
    /// Confirmed just now.
    Confirmed,
    /// Was already confirmed.
    Already,
    /// The token is real but older than the TTL; sign up again.
    Expired,
    /// No such token.
    Invalid,
}

impl ConfirmOutcome {
    /// The `outcome` value for `confirm.html`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Already => "already",
            Self::Expired => "expired",
            Self::Invalid => "invalid",
        }
    }
}

/// Confirms the signup holding `token`. Idempotent.
pub async fn confirm(
    pool: &PgPool,
    token: &str,
    policy: &Policy,
) -> Result<ConfirmOutcome, sqlx::Error> {
    if !is_well_formed_token(token) {
        return Ok(ConfirmOutcome::Invalid);
    }
    let hash = hash_token(token);
    let row = sqlx::query(
        "SELECT id,
                confirmed_at IS NOT NULL AS confirmed,
                token_created_at > now() - make_interval(secs => $2) AS fresh
           FROM waitlist_signups
          WHERE token_hash = $1",
    )
    .bind(&hash[..])
    .bind(secs(policy.token_ttl))
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(ConfirmOutcome::Invalid);
    };
    let id: Uuid = row.try_get("id")?;
    let confirmed: bool = row.try_get("confirmed")?;
    let fresh: Option<bool> = row.try_get("fresh")?;
    if confirmed {
        return Ok(ConfirmOutcome::Already);
    }
    if fresh != Some(true) {
        return Ok(ConfirmOutcome::Expired);
    }

    let updated = sqlx::query(
        "UPDATE waitlist_signups
            SET confirmed_at = now(), updated_at = now()
          WHERE id = $1 AND confirmed_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(if updated.rows_affected() == 1 {
        ConfirmOutcome::Confirmed
    } else {
        ConfirmOutcome::Already
    })
}

/// Removes a signup (and any unsent mail for it). Returns whether it existed.
pub async fn leave(pool: &PgPool, signup_id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM waitlist_signups WHERE id = $1")
        .bind(signup_id)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Deletes unconfirmed signups with no request for `policy.prune_unconfirmed_after`.
pub async fn prune_unconfirmed(pool: &PgPool, policy: &Policy) -> Result<u64, sqlx::Error> {
    let done = sqlx::query(
        "DELETE FROM waitlist_signups
          WHERE confirmed_at IS NULL
            AND last_enqueued_at < now() - make_interval(secs => $1)",
    )
    .bind(secs(policy.prune_unconfirmed_after))
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_keeps_only_utm_and_caps_values() {
        let long = "x".repeat(300);
        let s = source_from_pairs([
            ("utm_source", "newsletter"),
            ("utm_source", "second"),
            ("utm_medium", "  "),
            ("email", "a@b.com"),
            ("utm_campaign", long.as_str()),
        ]);
        let m = s.as_object().unwrap();
        assert_eq!(m["utm_source"], "newsletter");
        assert!(!m.contains_key("utm_medium"));
        assert!(!m.contains_key("email"));
        assert_eq!(m["utm_campaign"].as_str().unwrap().len(), 100);
    }
}
