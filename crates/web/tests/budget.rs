//! The real page against the spec's weight budget, and every `/assets/…`
//! reference resolving. Uses the repo's real `templates/` and `assets/`;
//! skipped (with a note) until `assets/css/site.css` exists.
//!
//! Budget (spec): under 150 KB before any optional video, JavaScript
//! ≤ 25 KB, sounds ≤ 30 KB in total. Sizes here are uncompressed, which is
//! stricter than transfer size.

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use dijmo_core::Keys;
use dijmo_core::config::BaseUrl;
use dijmo_core::keys::SecretKey;
use dijmo_core::waitlist::Policy;
use dijmo_web::assets::Assets;
use dijmo_web::ratelimit::Limits;
use dijmo_web::state::{AppState, Inner, MIN_FILL_TIME};
use sqlx::postgres::PgPoolOptions;
use tokio::sync::Notify;
use tower::ServiceExt;

const KB: usize = 1024;

fn assets_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, Vec<u8>) {
    let mut req = Request::get(uri).body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 1))));
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let body = axum::body::to_bytes(res.into_body(), 8 << 20)
        .await
        .unwrap();
    (status, body.to_vec())
}

/// Every `/assets/…` path mentioned in `text`.
fn asset_refs(text: &str) -> BTreeSet<String> {
    text.match_indices("/assets/")
        .map(|(i, _)| {
            text[i..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || "/._-".contains(*c))
                .collect::<String>()
        })
        .filter(|p| p.len() > "/assets/".len() && !p.ends_with('/'))
        .collect()
}

#[tokio::test]
async fn page_weight_budget_and_asset_references() {
    let dir = assets_dir();
    if !dir.join("css/site.css").exists() || !dir.join("js/site.js").exists() {
        eprintln!("skipped: the page's assets are not in assets/ yet");
        return;
    }
    // Pages never touch the database; a lazy pool is never opened.
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://localhost/dijmo_web_test")
        .unwrap();
    let assets = Assets::load(&dir).unwrap();
    let state = AppState::new(Inner {
        pool,
        base_url: BaseUrl::parse("https://dijmo.test").unwrap(),
        keys: Keys::derive(&SecretKey::new(vec![1; 64])),
        assets: assets.clone(),
        policy: Policy::default(),
        limits: Limits::default(),
        proxy_hops: 0,
        min_fill_time: MIN_FILL_TIME,
        outbox_wake: Arc::new(Notify::new()),
    });
    let app = dijmo_web::app(state);

    let (status, html) = get(&app, "/").await;
    assert_eq!(status, StatusCode::OK);
    let html = String::from_utf8(html).unwrap();
    let css = String::from_utf8(assets.css.bytes.to_vec()).unwrap();
    let js = String::from_utf8(assets.js.bytes.to_vec()).unwrap();

    // Every referenced asset exists and is served.
    let mut refs = asset_refs(&html);
    refs.extend(asset_refs(&css));
    refs.extend(asset_refs(&js));
    for r in [
        "/privacy",
        "/waitlist/confirm?token=x",
        "/waitlist/leave?token=x",
    ] {
        let (_, page) = get(&app, r).await;
        refs.extend(asset_refs(&String::from_utf8(page).unwrap()));
    }
    for r in &refs {
        let (status, _) = get(&app, r).await;
        assert_eq!(status, StatusCode::OK, "{r} is referenced but not served");
    }

    // What a first visit loads: the HTML, CSS, JS, the fonts the CSS uses,
    // and the favicon. Sounds load only when the visitor turns sound on;
    // og.png and the touch icon are not fetched by the page.
    let fonts: usize = refs
        .iter()
        .filter(|r| r.ends_with(".woff2"))
        .map(|r| {
            std::fs::metadata(dir.join(r.trim_start_matches("/assets/")))
                .unwrap()
                .len() as usize
        })
        .sum();
    let icon = std::fs::metadata(dir.join("img/icon.svg"))
        .map(|m| m.len() as usize)
        .unwrap_or(0);
    let total = html.len() + css.len() + js.len() + fonts + icon;

    let sounds: usize = std::fs::read_dir(dir.join("sounds"))
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter_map(|e| e.metadata().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len() as usize)
                .sum()
        })
        .unwrap_or(0);

    eprintln!(
        "page weight (uncompressed): html {} B, css {} B, js {} B, fonts {} B, icon {} B = {:.1} KB; sounds {:.1} KB",
        html.len(),
        css.len(),
        js.len(),
        fonts,
        icon,
        total as f64 / KB as f64,
        sounds as f64 / KB as f64
    );
    assert!(total < 150 * KB, "first visit is {total} B, budget 150 KB");
    assert!(
        js.len() <= 25 * KB,
        "site.js is {} B, budget 25 KB",
        js.len()
    );
    assert!(sounds <= 30 * KB, "sounds are {sounds} B, budget 30 KB");

    // With compression the page itself shrinks further.
    let mut req = Request::get("/")
        .header(header::ACCEPT_ENCODING, "br")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 1))));
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.headers()[header::CONTENT_ENCODING], "br");
}
