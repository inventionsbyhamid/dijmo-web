//! The outbox worker: sends queued email outside the request, with retries.
//!
//! A signup is stored before any email is attempted, so a provider outage
//! only delays mail. Rows are leased, so a crash mid-send is retried after
//! the lease expires, and several instances can share the table.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dijmo_core::Keys;
use dijmo_core::config::BaseUrl;
use dijmo_core::email_address::mask;
use dijmo_core::outbox::{self, Job};
use dijmo_core::waitlist::{self, Policy};
use sqlx::PgPool;
use tokio::sync::{Notify, watch};

use crate::email::{EmailSender, SendError, confirmation_email};

const BATCH: i64 = 20;
const LEASE: Duration = Duration::from_secs(120);
const IDLE_POLL: Duration = Duration::from_secs(5);
const PRUNE_EVERY: Duration = Duration::from_secs(3600);
const KEEP_FINISHED: Duration = Duration::from_secs(30 * 24 * 3600);

#[derive(Clone)]
pub struct Worker {
    pub pool: PgPool,
    pub sender: Arc<dyn EmailSender>,
    pub base_url: BaseUrl,
    pub keys: Keys,
    pub policy: Policy,
    pub wake: Arc<Notify>,
}

impl Worker {
    /// Runs until `shutdown` turns true. The batch in hand is finished first.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        tracing::info!("outbox worker started");
        let mut last_prune: Option<Instant> = None;
        let mut failures: u32 = 0;
        loop {
            if *shutdown.borrow() {
                break;
            }
            let pause = match self.run_once().await {
                Ok(_) => {
                    failures = 0;
                    IDLE_POLL
                }
                Err(e) => {
                    failures = failures.saturating_add(1);
                    tracing::warn!(error = %e, "outbox: database error, backing off");
                    Duration::from_secs(2u64.saturating_pow(failures.min(5)))
                }
            };
            if last_prune.is_none_or(|t| t.elapsed() >= PRUNE_EVERY) {
                last_prune = Some(Instant::now());
                self.prune().await;
            }
            tokio::select! {
                _ = self.wake.notified() => {}
                _ = tokio::time::sleep(pause) => {}
                _ = shutdown.changed() => {}
            }
        }
        tracing::info!("outbox worker stopped");
    }

    /// Sends everything that is due. Returns how many messages were handled.
    pub async fn run_once(&self) -> Result<usize, sqlx::Error> {
        let mut handled = 0;
        loop {
            let jobs = outbox::claim(&self.pool, BATCH, LEASE).await?;
            if jobs.is_empty() {
                return Ok(handled);
            }
            for job in jobs {
                self.process(job).await?;
                handled += 1;
            }
        }
    }

    async fn process(&self, job: Job) -> Result<(), sqlx::Error> {
        let result = match (job.kind.as_str(), job.signup_id) {
            (outbox::KIND_WAITLIST_CONFIRM, Some(signup_id)) => {
                match waitlist::issue_confirmation(&self.pool, signup_id).await? {
                    // Left or confirmed since: nothing to send.
                    None => Ok(false),
                    Some(mail) => {
                        let confirm = self
                            .base_url
                            .join(&format!("/waitlist/confirm?token={}", mail.token));
                        let leave = self.base_url.join(&format!(
                            "/waitlist/leave?token={}",
                            self.keys.leave_token(signup_id)
                        ));
                        let email = confirmation_email(
                            &mail.email,
                            &confirm,
                            &leave,
                            self.base_url.as_str(),
                        );
                        self.sender.send(&email).await.map(|()| true)
                    }
                }
            }
            (kind, _) => Err(SendError::Permanent(format!(
                "unknown outbox kind {kind:?}"
            ))),
        };

        match result {
            Ok(sent) => {
                outbox::mark_sent(&self.pool, job.id).await?;
                tracing::info!(job = job.id, kind = %job.kind, sent, "outbox: done");
            }
            Err(SendError::Transient(e)) if job.attempts < outbox::MAX_ATTEMPTS => {
                let delay = outbox::backoff(job.attempts);
                tracing::warn!(job = job.id, attempts = job.attempts, retry_in = ?delay, error = %redact(&e), "outbox: send failed, will retry");
                outbox::mark_retry(&self.pool, job.id, delay, &redact(&e)).await?;
            }
            Err(e) => {
                let e = redact(&e.to_string());
                tracing::error!(job = job.id, attempts = job.attempts, error = %e, "outbox: giving up");
                outbox::mark_failed(&self.pool, job.id, &e).await?;
            }
        }
        Ok(())
    }

    async fn prune(&self) {
        match outbox::prune_finished(&self.pool, KEEP_FINISHED).await {
            Ok(n) if n > 0 => tracing::info!(rows = n, "outbox: pruned finished mail"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "outbox: prune failed"),
        }
        match waitlist::prune_unconfirmed(&self.pool, &self.policy).await {
            Ok(n) if n > 0 => {
                tracing::info!(rows = n, "waitlist: pruned stale unconfirmed signups")
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "waitlist: prune failed"),
        }
    }
}

/// Provider errors may echo the recipient; mask anything address-like.
fn redact(s: &str) -> String {
    s.split_inclusive(|c: char| c.is_whitespace() || c == '"' || c == ',')
        .map(|w| {
            let core = w.trim_end_matches(|c: char| c.is_whitespace() || c == '"' || c == ',');
            if core.contains('@') {
                format!("{}{}", mask(core), &w[core.len()..])
            } else {
                w.to_owned()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn redacts_addresses_in_errors() {
        assert_eq!(
            redact(r#"422: {"message":"Invalid `to` field: jane@x.example"}"#),
            r#"422: {"message":"Invalid `to` field: j***@x.example"}"#
        );
    }
}
