//! The site over HTTP (in-process), against a real Postgres
//! (`dijmo_web_test`, or `TEST_DATABASE_URL`).

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use dijmo_core::Keys;
use dijmo_core::config::BaseUrl;
use dijmo_core::keys::SecretKey;
use dijmo_core::waitlist::Policy;
use dijmo_web::assets::Assets;
use dijmo_web::ratelimit::Limits;
use dijmo_web::security::CSP;
use dijmo_web::state::{AppState, Inner, MIN_FILL_TIME};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::Notify;
use tower::ServiceExt;
use uuid::Uuid;

const BASE: &str = "http://dijmo.test";

struct TestApp {
    app: Router,
    state: AppState,
}

async fn pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/dijmo_web_test".to_owned());
    let pool = dijmo_core::connect_with(&url, 4)
        .await
        .expect("test database");
    dijmo_core::migrate(&pool).await.expect("migrations");
    pool
}

async fn test_app() -> TestApp {
    let pool = pool().await;
    let assets = Assets::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/assets"
    ))
    .expect("fixture assets");
    let state = AppState::new(Inner {
        pool: pool.clone(),
        base_url: BaseUrl::parse(BASE).unwrap(),
        keys: Keys::derive(&SecretKey::new(vec![42; 64])),
        assets,
        policy: Policy::default(),
        limits: Limits::default(),
        proxy_hops: 0,
        min_fill_time: MIN_FILL_TIME,
        outbox_wake: Arc::new(Notify::new()),
    });
    TestApp {
        app: dijmo_web::app(state.clone()),
        state,
    }
}

fn random_ip() -> IpAddr {
    let b = Uuid::new_v4().into_bytes();
    IpAddr::from([203, b[0], b[1], b[2]])
}

struct Res {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Res {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|_| panic!("not JSON: {}", self.body))
    }
    fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .map(|v| v.to_str().unwrap())
            .unwrap_or_else(|| panic!("no {name} header"))
    }
}

impl TestApp {
    async fn send(&self, req: Request<Body>, ip: IpAddr) -> Res {
        let mut req = req;
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::new(ip, 40000)));
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = axum::body::to_bytes(res.into_body(), 4 << 20)
            .await
            .unwrap();
        Res {
            status,
            headers,
            body: String::from_utf8_lossy(&bytes).into_owned(),
        }
    }

    async fn get(&self, uri: &str) -> Res {
        self.send(Request::get(uri).body(Body::empty()).unwrap(), random_ip())
            .await
    }
}

// ------------------------------------------------------------------ the waitlist is off

#[tokio::test]
async fn body_limit() {
    let t = test_app().await;
    let big = "a".repeat(20 * 1024);
    let req = Request::post("/")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::CONTENT_LENGTH, big.len())
        .header(header::ORIGIN, BASE)
        .body(Body::from(big))
        .unwrap();
    let res = t.send(req, random_ip()).await;
    assert_eq!(res.status, StatusCode::PAYLOAD_TOO_LARGE);
}

/// The waitlist is off: none of its routes answer, so nothing can post to them.
#[tokio::test]
async fn waitlist_routes_are_gone() {
    let t = test_app().await;
    for (method, uri) in [
        (Method::POST, "/waitlist"),
        (Method::GET, "/waitlist"),
        (
            Method::GET,
            "/waitlist/confirm?token=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ),
        (Method::GET, "/waitlist/leave?token=x"),
        (Method::POST, "/waitlist/leave?token=x"),
    ] {
        let req = Request::builder()
            .method(method.clone())
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::ORIGIN, BASE)
            .body(Body::from("email=a%40b.example&List-Unsubscribe=One-Click"))
            .unwrap();
        let res = t.send(req, random_ip()).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{method} {uri}");
    }
    let res = t.get("/").await;
    assert!(
        !res.body.contains("/waitlist"),
        "the page links nowhere near it"
    );
    assert!(!res.body.contains("<form"), "no form on the page");
}

// ------------------------------------------------------------------ headers, pages, assets

fn assert_security_headers(res: &Res, what: &str) {
    assert_eq!(res.header("content-security-policy"), CSP, "{what}");
    assert_eq!(
        CSP,
        "default-src 'self'; img-src 'self' data:; media-src 'self'; font-src 'self'; style-src 'self'; script-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"
    );
    assert!(
        res.header("strict-transport-security")
            .starts_with("max-age="),
        "{what}"
    );
    assert_eq!(res.header("x-content-type-options"), "nosniff", "{what}");
    assert_eq!(
        res.header("referrer-policy"),
        "strict-origin-when-cross-origin",
        "{what}"
    );
    let pp = res.header("permissions-policy");
    for f in ["camera=()", "microphone=()", "geolocation=()"] {
        assert!(pp.contains(f), "{what}: {pp}");
    }
}

#[tokio::test]
async fn security_headers_everywhere() {
    let t = test_app().await;
    let css = t.state.assets.css.url.clone();
    for uri in [
        "/",
        "/privacy",
        "/nope",
        "/healthz",
        "/robots.txt",
        css.as_str(),
        "/assets/img/icon.svg",
    ] {
        let res = t.get(uri).await;
        assert_security_headers(&res, uri);
    }
}

/// Rendered pages must obey the CSP: no inline scripts, styles or handlers.
fn assert_csp_clean(html: &str, what: &str) {
    let lower = html.to_ascii_lowercase();
    for (i, _) in lower.match_indices("<script") {
        let tag = &lower[i..i + lower[i..].find('>').unwrap()];
        assert!(tag.contains(" src="), "{what}: inline <script>");
    }
    assert!(!lower.contains("<style"), "{what}: inline <style>");
    assert!(!lower.contains(" style="), "{what}: style attribute");
    for handler in [
        " onclick=",
        " onload=",
        " onerror=",
        " onsubmit=",
        " oninput=",
    ] {
        assert!(!lower.contains(handler), "{what}: inline handler {handler}");
    }
    assert!(!lower.contains("javascript:"), "{what}: javascript: URL");
}

#[tokio::test]
async fn every_page_renders() {
    let t = test_app().await;
    let css = t.state.assets.css.url.clone();
    let js = t.state.assets.js.url.clone();

    for (uri, status) in [
        ("/", StatusCode::OK),
        ("/privacy", StatusCode::OK),
        ("/does-not-exist", StatusCode::NOT_FOUND),
    ] {
        let res = t.get(uri).await;
        assert_eq!(res.status, status, "{uri}");
        assert!(res.header("content-type").starts_with("text/html"), "{uri}");
        assert!(
            res.body.contains(&format!("href=\"{css}\"")),
            "{uri}: hashed css"
        );
        assert!(
            res.body.contains(&format!("src=\"{js}\"")),
            "{uri}: hashed js"
        );
        assert!(
            res.body.contains(&format!("{BASE}/assets/img/og.png")),
            "{uri}: og image"
        );
        assert_csp_clean(&res.body, uri);
    }

    // The index closes on where Dijmo is coming, and says it only once.
    let res = t.get("/").await;
    assert!(
        res.body
            .contains(&format!(r#"<link rel="canonical" href="{BASE}/">"#))
    );
    assert_eq!(res.header("cache-control"), "no-cache");
    assert!(res.body.contains(
        r#"Coming to Mac first, <span class="close__after">then Windows and Linux.</span>"#
    ));
    assert!(
        !res.body.contains("foot__ship"),
        "the footer doesn't repeat it"
    );
    let privacy = t.get("/privacy").await;
    assert!(privacy.body.contains("foot__ship"), "other pages keep it");
    assert!(
        t.get("/privacy")
            .await
            .body
            .contains("this site collects nothing about you")
    );

    // Every outcome and every error code renders through the real templates.
    use askama::Template;
    use dijmo_web::pages::*;
    let meta = Meta::new(&t.state, "/", "T", "D");
    for o in ["confirmed", "already", "expired", "invalid"] {
        let html = ConfirmPage::new(meta.clone(), o.into()).render().unwrap();
        assert_csp_clean(&html, o);
    }
    for (o, token) in [("confirm", "t0k3n"), ("left", ""), ("invalid", "")] {
        let html = LeavePage::new(meta.clone(), o.into(), token.into())
            .render()
            .unwrap();
        assert_csp_clean(&html, o);
    }
    for code in [400u16, 403, 404, 405, 408, 413, 422, 429, 500, 503] {
        let (_, heading, body) = error_copy(StatusCode::from_u16(code).unwrap());
        let html = ErrorPage::new(meta.clone(), code, heading.into(), body.into())
            .render()
            .unwrap();
        assert!(html.contains(&code.to_string()));
        assert_csp_clean(&html, "error");
    }
}

#[tokio::test]
async fn errors_render_the_error_page_or_json() {
    let t = test_app().await;
    let res = t.get("/nope").await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert!(res.body.contains("There’s nothing here."));
    assert_eq!(res.header("cache-control"), "no-store");

    let req = Request::get("/api/v1/nope")
        .header(header::ACCEPT, "application/json")
        .body(Body::empty())
        .unwrap();
    let res = t.send(req, random_ip()).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(res.json(), json!({"ok": false, "error": "not_found"}));

    // Wrong method on a real route: the error page, not an empty body.
    let req = Request::builder()
        .method(Method::DELETE)
        .uri("/privacy")
        .body(Body::empty())
        .unwrap();
    let res = t.send(req, random_ip()).await;
    assert_eq!(res.status, StatusCode::METHOD_NOT_ALLOWED);
    assert!(res.header("content-type").starts_with("text/html"));
}

#[tokio::test]
async fn assets_are_hashed_and_cached() {
    let t = test_app().await;
    let css = t.state.assets.css.url.clone();
    let js = t.state.assets.js.url.clone();
    assert!(css.starts_with("/assets/site.") && css.ends_with(".css"));

    let res = t.get(&css).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(
        res.header("cache-control"),
        "public, max-age=31536000, immutable"
    );
    assert!(res.header("content-type").starts_with("text/css"));
    assert!(res.body.contains("test fixture"));

    let res = t.get(&js).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.header("content-type").starts_with("text/javascript"));
    assert_eq!(
        res.header("cache-control"),
        "public, max-age=31536000, immutable"
    );

    let res = t.get("/assets/img/icon.svg").await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.header("cache-control"), "public, max-age=86400");
    assert!(res.header("content-type").starts_with("image/svg+xml"));

    for uri in [
        "/assets/img/missing.png",
        "/assets/../Cargo.toml",
        "/assets/%2e%2e/Cargo.toml",
        "/assets/site.00000000.css",
    ] {
        let res = t.get(uri).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{uri}");
        assert!(res.header("content-type").starts_with("text/html"), "{uri}");
    }

    // Compressed when asked.
    let req = Request::get("/")
        .header(header::ACCEPT_ENCODING, "br")
        .body(Body::empty())
        .unwrap();
    let res = t.send(req, random_ip()).await;
    assert_eq!(res.header("content-encoding"), "br");
}

#[tokio::test]
async fn health_robots_sitemap() {
    let t = test_app().await;
    let res = t.get("/healthz").await;
    assert_eq!(
        (res.status, res.json()),
        (StatusCode::OK, json!({"ok": true}))
    );
    let res = t.get("/readyz").await;
    assert_eq!(
        (res.status, res.json()),
        (StatusCode::OK, json!({"ok": true}))
    );

    let res = t.get("/robots.txt").await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.header("content-type").starts_with("text/plain"));
    assert!(res.body.contains(&format!("Sitemap: {BASE}/sitemap.xml")));
    assert!(!res.body.contains("/waitlist"));

    let res = t.get("/sitemap.xml").await;
    assert!(res.header("content-type").starts_with("application/xml"));
    assert!(res.body.contains(&format!("<loc>{BASE}/</loc>")));
    assert!(res.body.contains(&format!("<loc>{BASE}/privacy</loc>")));

    // HEAD works on pages.
    let req = Request::head("/").body(Body::empty()).unwrap();
    assert_eq!(t.send(req, random_ip()).await.status, StatusCode::OK);
}
