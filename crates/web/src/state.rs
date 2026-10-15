//! Shared application state.

use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use dijmo_core::config::BaseUrl;
use dijmo_core::waitlist::Policy;
use dijmo_core::{Config, Keys};
use sqlx::PgPool;
use tokio::sync::Notify;

use crate::assets::Assets;
use crate::ratelimit::Limits;

/// A form posted sooner than this after render is treated as a bot.
pub const MIN_FILL_TIME: Duration = Duration::from_millis(1500);

pub struct Inner {
    pub pool: PgPool,
    pub base_url: BaseUrl,
    pub keys: Keys,
    pub assets: Assets,
    pub policy: Policy,
    pub limits: Limits,
    pub proxy_hops: usize,
    pub min_fill_time: Duration,
    /// Poked after a signup queues mail, so the worker sends it at once.
    pub outbox_wake: Arc<Notify>,
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

impl AppState {
    pub fn new(inner: Inner) -> Self {
        Self(Arc::new(inner))
    }

    /// The production state for `config`.
    pub fn from_config(config: &Config, pool: PgPool, assets: Assets) -> Self {
        Self::new(Inner {
            pool,
            base_url: config.public_base_url.clone(),
            keys: Keys::derive(&config.secret_key),
            assets,
            policy: Policy::default(),
            limits: Limits::default(),
            proxy_hops: config.proxy_hops,
            min_fill_time: MIN_FILL_TIME,
            outbox_wake: Arc::new(Notify::new()),
        })
    }
}

impl Deref for AppState {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}
