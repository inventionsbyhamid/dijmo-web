//! Waitlist and outbox against a real Postgres (`dijmo_web_test` by default;
//! override with `TEST_DATABASE_URL`). Tests share the database and run in
//! parallel, so each uses its own addresses and looks only at its own rows.

use std::time::Duration;

use dijmo_core::outbox;
use dijmo_core::waitlist::{self, ConfirmOutcome, JoinOutcome, Policy};
use dijmo_core::{EmailAddress, keys};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

async fn pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/dijmo_web_test".to_owned());
    let pool = dijmo_core::connect_with(&url, 4)
        .await
        .expect("test database");
    dijmo_core::migrate(&pool).await.expect("migrations");
    pool
}

/// Claiming leases every due row, which would stop another test's unsent
/// row from being replaced; tests that claim or replace take this lock.
static OUTBOX_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn unique_email(tag: &str) -> EmailAddress {
    EmailAddress::parse(&format!("{tag}-{}@Core.Test.Example", Uuid::new_v4())).unwrap()
}

async fn unsent_jobs(pool: &PgPool, signup_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM email_outbox WHERE signup_id = $1 AND sent_at IS NULL AND failed_at IS NULL",
    )
    .bind(signup_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn join_confirm_leave_round_trip() {
    let pool = pool().await;
    let policy = Policy::default();
    let email = unique_email("trip");
    let source = json!({"utm_source": "test"});

    let JoinOutcome::Queued { signup_id } =
        waitlist::join(&pool, &email, &source, Some(&[1, 2, 3]), &policy)
            .await
            .unwrap()
    else {
        panic!("first join queues an email");
    };
    assert_eq!(unsent_jobs(&pool, signup_id).await, 1);

    // Stored lower-cased, with source and IP hash, and no token until sent.
    let (stored, src, token_hash): (String, serde_json::Value, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT email::text, source, token_hash FROM waitlist_signups WHERE id = $1",
    )
    .bind(signup_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, email.as_str());
    assert_eq!(stored, stored.to_lowercase());
    assert_eq!(src, source);
    assert!(token_hash.is_none());

    let mail = waitlist::issue_confirmation(&pool, signup_id)
        .await
        .unwrap()
        .expect("pending signup gets a token");
    assert_eq!(mail.email, email.as_str());
    let stored_hash: Vec<u8> =
        sqlx::query_scalar("SELECT token_hash FROM waitlist_signups WHERE id = $1")
            .bind(signup_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored_hash, keys::hash_token(&mail.token).to_vec());

    assert_eq!(
        waitlist::confirm(&pool, &mail.token, &policy)
            .await
            .unwrap(),
        ConfirmOutcome::Confirmed
    );
    assert_eq!(
        waitlist::confirm(&pool, &mail.token, &policy)
            .await
            .unwrap(),
        ConfirmOutcome::Already
    );
    // A confirmed address gets no more mail.
    assert!(
        waitlist::issue_confirmation(&pool, signup_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        waitlist::join(&pool, &email, &json!({}), None, &policy)
            .await
            .unwrap(),
        JoinOutcome::AlreadyConfirmed
    );

    assert!(waitlist::leave(&pool, signup_id).await.unwrap());
    assert!(!waitlist::leave(&pool, signup_id).await.unwrap());
    assert_eq!(unsent_jobs(&pool, signup_id).await, 0);
    assert_eq!(
        waitlist::confirm(&pool, &mail.token, &policy)
            .await
            .unwrap(),
        ConfirmOutcome::Invalid
    );
}

#[tokio::test]
async fn duplicate_join_respects_cooldown_and_replaces_unsent_mail() {
    let _guard = OUTBOX_LOCK.lock().await;
    let pool = pool().await;
    let policy = Policy::default();
    let email = unique_email("dup");

    let JoinOutcome::Queued { signup_id } =
        waitlist::join(&pool, &email, &json!({}), None, &policy)
            .await
            .unwrap()
    else {
        panic!("queued");
    };
    // Same address, different case: still one signup, and within the cooldown.
    let shouted = EmailAddress::parse(&email.as_str().to_uppercase()).unwrap();
    assert_eq!(
        waitlist::join(&pool, &shouted, &json!({}), None, &policy)
            .await
            .unwrap(),
        JoinOutcome::CoolingDown
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM waitlist_signups WHERE email = $1::citext")
            .bind(email.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);

    // After the cooldown, a new email replaces the unsent one.
    sqlx::query("UPDATE waitlist_signups SET last_enqueued_at = now() - interval '11 minutes' WHERE id = $1")
        .bind(signup_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        waitlist::join(&pool, &email, &json!({}), None, &policy)
            .await
            .unwrap(),
        JoinOutcome::Queued { signup_id }
    );
    assert_eq!(unsent_jobs(&pool, signup_id).await, 1);

    waitlist::leave(&pool, signup_id).await.unwrap();
}

#[tokio::test]
async fn expired_and_invalid_tokens() {
    let pool = pool().await;
    let policy = Policy::default();
    let email = unique_email("expired");
    let JoinOutcome::Queued { signup_id } =
        waitlist::join(&pool, &email, &json!({}), None, &policy)
            .await
            .unwrap()
    else {
        panic!("queued");
    };
    let mail = waitlist::issue_confirmation(&pool, signup_id)
        .await
        .unwrap()
        .unwrap();
    sqlx::query(
        "UPDATE waitlist_signups SET token_created_at = now() - interval '73 hours' WHERE id = $1",
    )
    .bind(signup_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        waitlist::confirm(&pool, &mail.token, &policy)
            .await
            .unwrap(),
        ConfirmOutcome::Expired
    );

    // A re-issued token (a new signup request) works again.
    let fresh = waitlist::issue_confirmation(&pool, signup_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        waitlist::confirm(&pool, &mail.token, &policy)
            .await
            .unwrap(),
        ConfirmOutcome::Invalid,
        "the old link is replaced"
    );
    assert_eq!(
        waitlist::confirm(&pool, &fresh.token, &policy)
            .await
            .unwrap(),
        ConfirmOutcome::Confirmed
    );

    for bad in ["", "short", &"A".repeat(43), "../../etc/passwd"] {
        assert_eq!(
            waitlist::confirm(&pool, bad, &policy).await.unwrap(),
            ConfirmOutcome::Invalid
        );
    }
    waitlist::leave(&pool, signup_id).await.unwrap();
}

#[tokio::test]
async fn outbox_claim_retry_send_and_prune() {
    let _guard = OUTBOX_LOCK.lock().await;
    let pool = pool().await;
    let policy = Policy::default();
    let email = unique_email("outbox");
    let JoinOutcome::Queued { signup_id } =
        waitlist::join(&pool, &email, &json!({}), None, &policy)
            .await
            .unwrap()
    else {
        panic!("queued");
    };
    let job_id: i64 = sqlx::query_scalar("SELECT id FROM email_outbox WHERE signup_id = $1")
        .bind(signup_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    let claimed = outbox::claim(&pool, 10_000, Duration::from_secs(60))
        .await
        .unwrap();
    let job = claimed
        .iter()
        .find(|j| j.id == job_id)
        .expect("our job is due");
    assert_eq!(job.kind, outbox::KIND_WAITLIST_CONFIRM);
    assert_eq!(job.signup_id, Some(signup_id));
    assert_eq!(job.attempts, 1);
    // Leased: a second claim does not see it.
    let again = outbox::claim(&pool, 10_000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(again.iter().all(|j| j.id != job_id));

    outbox::mark_retry(&pool, job_id, Duration::from_secs(3600), "provider down")
        .await
        .unwrap();
    let again = outbox::claim(&pool, 10_000, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(again.iter().all(|j| j.id != job_id), "not due for an hour");

    outbox::mark_sent(&pool, job_id).await.unwrap();
    assert_eq!(unsent_jobs(&pool, signup_id).await, 0);

    // Old finished mail and stale unconfirmed signups are pruned.
    sqlx::query("UPDATE email_outbox SET updated_at = now() - interval '40 days' WHERE id = $1")
        .bind(job_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE waitlist_signups SET last_enqueued_at = now() - interval '31 days' WHERE id = $1",
    )
    .bind(signup_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        outbox::prune_finished(&pool, Duration::from_secs(30 * 86400))
            .await
            .unwrap()
            >= 1
    );
    assert!(waitlist::prune_unconfirmed(&pool, &policy).await.unwrap() >= 1);
    assert!(
        !waitlist::leave(&pool, signup_id).await.unwrap(),
        "already pruned"
    );
}
