//! The Dijmo website: an Axum app over `dijmo-core`.

pub mod assets;
pub mod email;
pub mod pages;
pub mod ratelimit;
pub mod routes;
pub mod security;
pub mod state;
pub mod worker;

use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{Request, StatusCode};
use axum::middleware;
use tower::ServiceBuilder;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::predicate::{NotForContentType, Predicate};
use tower_http::compression::{CompressionLayer, DefaultPredicate};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::{DefaultOnFailure, DefaultOnResponse, TraceLayer};
use tracing::Level;

pub use state::AppState;

/// Request bodies are capped at 16 KiB.
pub const BODY_LIMIT: usize = 16 * 1024;
/// Every handler must answer within this.
pub const HANDLER_TIMEOUT: Duration = Duration::from_secs(10);

/// The whole app: routes plus middleware.
pub fn app(state: AppState) -> Router {
    let compress_when = DefaultPredicate::new()
        .and(NotForContentType::const_new("audio/"))
        .and(NotForContentType::const_new("video/"))
        .and(NotForContentType::const_new("font/"));

    let layers = ServiceBuilder::new()
        // Spans carry the method and path only: never the query (tokens) or the IP.
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|req: &Request<_>| {
                    tracing::info_span!("http", method = %req.method(), path = %req.uri().path())
                })
                .on_request(())
                .on_response(DefaultOnResponse::new().level(Level::INFO))
                .on_failure(DefaultOnFailure::new().level(Level::ERROR)),
        )
        .layer(middleware::from_fn(security::security_headers))
        .layer(CompressionLayer::new().br(true).gzip(true).compress_when(compress_when))
        .layer(middleware::from_fn_with_state(state.clone(), security::error_pages))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::SERVICE_UNAVAILABLE,
            HANDLER_TIMEOUT,
        ))
        .layer(CatchPanicLayer::new())
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT))
        .layer(DefaultBodyLimit::max(BODY_LIMIT));

    routes::router(state.clone())
        .layer(layers)
        .with_state(state)
}
