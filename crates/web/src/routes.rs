//! Routes, per `docs/contract.md`.

use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::assets::ONE_DAY;
use crate::pages::{Caching, IndexPage, Meta, PrivacyPage, render};
use crate::security::json_error;
use crate::state::AppState;

const SITE_TITLE: &str = "Dijmo: say what you want, and it’s done";
const SITE_DESCRIPTION: &str = "Dijmo is a video editor you talk to. It runs on your computer, and your videos never leave it. Coming to Mac first, then Windows and Linux.";

pub fn router(state: AppState) -> Router<AppState> {
    let static_files = ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static(ONE_DAY),
        ))
        .service(ServeDir::new(&state.assets.dir).append_index_html_on_directories(false));

    let css = state.assets.css.clone();
    let js = state.assets.js.clone();

    Router::new()
        .route("/", get(index))
        .route("/privacy", get(privacy))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/robots.txt", get(robots))
        .route("/sitemap.xml", get(sitemap))
        .route(&css.url.clone(), get(move || async move { css.response() }))
        .route(&js.url.clone(), get(move || async move { js.response() }))
        .nest_service("/assets", static_files)
        .fallback(|| async { StatusCode::NOT_FOUND })
}

// ---------------------------------------------------------------- pages

async fn index(State(state): State<AppState>) -> Response {
    let meta = Meta::new(&state, "/", SITE_TITLE, SITE_DESCRIPTION);
    render(&IndexPage::new(meta), StatusCode::OK, Caching::Revalidate)
}

async fn privacy(State(state): State<AppState>) -> Response {
    let meta = Meta::new(
        &state,
        "/privacy",
        "Privacy · Dijmo",
        "What this site collects: nothing, right now. In plain words.",
    );
    let mut res = render(&PrivacyPage::new(meta), StatusCode::OK, Caching::Revalidate);
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    res
}

// ---------------------------------------------------------------- ops & SEO

async fn healthz() -> Response {
    (
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(json!({"ok": true})),
    )
        .into_response()
}

async fn readyz(State(state): State<AppState>) -> Response {
    let up = tokio::time::timeout(Duration::from_secs(2), dijmo_core::ping(&state.pool))
        .await
        .unwrap_or(false);
    if up {
        healthz().await
    } else {
        tracing::warn!("readyz: database is not answering");
        json_error(StatusCode::SERVICE_UNAVAILABLE, "database")
    }
}

async fn robots(State(state): State<AppState>) -> Response {
    let body = format!(
        "User-agent: *\nAllow: /\n\nSitemap: {}\n",
        state.base_url.join("/sitemap.xml")
    );
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, ONE_DAY),
        ],
        body,
    )
        .into_response()
}

async fn sitemap(State(state): State<AppState>) -> Response {
    let url = |path: &str| format!("  <url><loc>{}</loc></url>\n", state.base_url.join(path));
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n{}{}</urlset>\n",
        url("/"),
        url("/privacy"),
    );
    (
        [
            (header::CONTENT_TYPE, "application/xml; charset=utf-8"),
            (header::CACHE_CONTROL, ONE_DAY),
        ],
        body,
    )
        .into_response()
}
