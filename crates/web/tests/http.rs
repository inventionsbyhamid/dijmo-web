//! The site over HTTP (in-process), against a real Postgres
//! (`dijmo_web_test`, or `TEST_DATABASE_URL`).
//!
//! Tests run in parallel against one database: each uses its own addresses
//! and its own app (so its own rate limits). Outbox workers from different
//! tests may send each other's mail, so all of them share one in-memory
//! sender and each test looks for mail to its own address.

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use dijmo_core::Keys;
use dijmo_core::config::BaseUrl;
use dijmo_core::keys::SecretKey;
use dijmo_core::waitlist::Policy;
use dijmo_web::assets::Assets;
use dijmo_web::email::{Email, MemorySender};
use dijmo_web::ratelimit::Limits;
use dijmo_web::security::CSP;
use dijmo_web::state::{AppState, Inner, MIN_FILL_TIME};
use dijmo_web::worker::Worker;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::Notify;
use tower::ServiceExt;
use uuid::Uuid;

const BASE: &str = "http://dijmo.test";

fn sender() -> Arc<MemorySender> {
    static SENDER: OnceLock<Arc<MemorySender>> = OnceLock::new();
    SENDER
        .get_or_init(|| Arc::new(MemorySender::default()))
        .clone()
}

struct TestApp {
    app: Router,
    state: AppState,
    worker: Worker,
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
    let worker = Worker {
        pool,
        sender: sender(),
        base_url: state.base_url.clone(),
        keys: state.keys.clone(),
        policy: state.policy,
        wake: state.outbox_wake.clone(),
    };
    TestApp {
        app: dijmo_web::app(state.clone()),
        state,
        worker,
    }
}

fn unique_email(tag: &str) -> String {
    format!("{tag}-{}@web.test.example", Uuid::new_v4().simple())
}

fn random_ip() -> IpAddr {
    let b = Uuid::new_v4().into_bytes();
    IpAddr::from([203, b[0], b[1], b[2]])
}

/// The render time of a page loaded 5 s ago.
fn t_ok() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    (now - 5_000).to_string()
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

    async fn post_form(&self, form: &[(&str, &str)], ip: IpAddr) -> Res {
        let body: String = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(form)
            .finish();
        let req = Request::post("/waitlist")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::ORIGIN, BASE)
            .body(Body::from(body))
            .unwrap();
        self.send(req, ip).await
    }

    async fn post_json(&self, body: Value, ip: IpAddr) -> Res {
        let req = Request::post("/waitlist")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json")
            .header(header::ORIGIN, BASE)
            .body(Body::from(body.to_string()))
            .unwrap();
        self.send(req, ip).await
    }

    /// The leave page's form: POST /waitlist/leave with `token`.
    async fn post_leave(&self, token: &str, origin: Option<&str>) -> Res {
        let body: String = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("token", token)
            .finish();
        let mut req = Request::post("/waitlist/leave")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
        if let Some(o) = origin {
            req = req.header(header::ORIGIN, o);
        }
        self.send(req.body(Body::from(body)).unwrap(), random_ip())
            .await
    }

    async fn join(&self, email: &str) -> Res {
        self.post_json(
            json!({"email": email, "website": "", "t": t_ok()}),
            random_ip(),
        )
        .await
    }

    /// Runs the outbox until mail to `to` shows up.
    async fn mail_to(&self, to: &str, count: usize) -> Vec<Email> {
        for _ in 0..100 {
            self.worker.run_once().await.unwrap();
            let got = sender().messages_to(to);
            if got.len() >= count {
                return got;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("no mail to {to}");
    }

    async fn signup_row(&self, email: &str) -> Option<(Uuid, bool, Value, Option<Vec<u8>>)> {
        sqlx::query_as(
            "SELECT id, confirmed_at IS NOT NULL, source, ip_hash FROM waitlist_signups WHERE email = $1::citext",
        )
        .bind(email)
        .fetch_optional(&self.state.pool)
        .await
        .unwrap()
    }
}

/// The path and query of the first link in `text` that starts with `prefix`.
fn link(text: &str, prefix: &str) -> String {
    let start = text
        .find(prefix)
        .unwrap_or_else(|| panic!("no {prefix} in {text}"));
    let url: String = text[start..]
        .chars()
        .take_while(|c| !c.is_whitespace())
        .collect();
    url.strip_prefix(BASE).unwrap().to_owned()
}

// ------------------------------------------------------------------ flow

#[tokio::test]
async fn signup_outbox_confirm_leave() {
    let t = test_app().await;
    let email = unique_email("flow");

    // The no-JS form: 303 to the index with status=sent.
    let res = t
        .post_form(
            &[
                ("email", &email.to_uppercase()),
                ("website", ""),
                ("t", &t_ok()),
            ],
            random_ip(),
        )
        .await;
    assert_eq!(res.status, StatusCode::SEE_OTHER);
    assert_eq!(res.header("location"), "/?status=sent#join");

    let (id, confirmed, _, ip_hash) = t.signup_row(&email).await.expect("stored lower-cased");
    assert!(!confirmed);
    assert_eq!(
        ip_hash.map(|h| h.len()),
        Some(16),
        "a coarse IP hash, not the IP"
    );

    // The outbox sends one confirmation with a confirm and a leave link.
    let mail = t.mail_to(&email, 1).await;
    assert_eq!(mail.len(), 1);
    let m = &mail[0];
    assert!(m.subject.contains("Dijmo"));
    let confirm = link(&m.text, &format!("{BASE}/waitlist/confirm?token="));
    let leave = link(&m.text, &format!("{BASE}/waitlist/leave?token="));
    assert!(m.html.contains(&confirm) && m.html.contains(&leave));
    assert_eq!(
        m.unsubscribe_url.as_deref(),
        Some(format!("{BASE}{leave}").as_str())
    );
    assert!(!m.html.contains("<img"), "no tracking pixels");

    // The token itself is not stored, only its SHA-256.
    let token = confirm.split("token=").nth(1).unwrap();
    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT token_hash FROM waitlist_signups WHERE id = $1")
            .bind(id)
            .fetch_one(&t.state.pool)
            .await
            .unwrap();
    assert_ne!(stored, token.as_bytes());
    assert_eq!(stored, dijmo_core::keys::hash_token(token).to_vec());

    let res = t.get(&confirm).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.body.contains("You’re on the list."), "{}", res.body);
    assert_eq!(res.header("cache-control"), "no-store");
    assert!(t.signup_row(&email).await.unwrap().1, "confirmed_at set");

    // Idempotent.
    let res = t.get(&confirm).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.body.contains("You’re already on the list."));

    // The leave link only asks; the button's POST removes the signup.
    let res = t.get(&leave).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(t.signup_row(&email).await.is_some(), "GET deletes nothing");
    let token = leave.split("token=").nth(1).unwrap();
    let res = t.post_leave(token, Some(BASE)).await;
    assert_eq!(res.status, StatusCode::SEE_OTHER);
    assert_eq!(res.header("location"), "/waitlist/leave?done=1");
    assert!(t.signup_row(&email).await.is_none(), "removed");
    let res = t.get("/waitlist/leave?done=1").await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.body.contains("You’ve left the waitlist."));

    // Leaving twice still lands on "left"; the old confirm link is dead.
    let res = t.post_leave(token, Some(BASE)).await;
    assert_eq!(res.header("location"), "/waitlist/leave?done=1");
    let res = t.get(&confirm).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert!(res.body.contains("This link doesn’t work."));
}

#[tokio::test]
async fn duplicates_get_the_same_answer_and_one_email() {
    let t = test_app().await;
    let email = unique_email("dup");

    let first = t.join(&email).await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.json(), json!({"ok": true}));
    t.mail_to(&email, 1).await;

    // Again, within the cooldown: same answer, no second email.
    let second = t.join(&email).await;
    assert_eq!((second.status, second.json()), (first.status, first.json()));
    t.worker.run_once().await.unwrap();
    assert_eq!(sender().messages_to(&email).len(), 1);

    // Confirmed addresses get the same answer too, and no mail.
    let confirm = link(
        &sender().messages_to(&email)[0].text,
        &format!("{BASE}/waitlist/confirm?token="),
    );
    assert_eq!(t.get(&confirm).await.status, StatusCode::OK);
    sqlx::query("UPDATE waitlist_signups SET last_enqueued_at = now() - interval '1 hour' WHERE email = $1::citext")
        .bind(&email)
        .execute(&t.state.pool)
        .await
        .unwrap();
    let third = t.join(&email).await;
    assert_eq!((third.status, third.json()), (first.status, first.json()));
    t.worker.run_once().await.unwrap();
    assert_eq!(sender().messages_to(&email).len(), 1);

    // An unknown address answers exactly the same.
    let fresh = t.join(&unique_email("fresh")).await;
    assert_eq!((fresh.status, fresh.body), (first.status, first.body));
}

#[tokio::test]
async fn resend_after_cooldown_replaces_the_link() {
    let t = test_app().await;
    let email = unique_email("resend");
    t.join(&email).await;
    let first = t.mail_to(&email, 1).await;
    sqlx::query("UPDATE waitlist_signups SET last_enqueued_at = now() - interval '11 minutes' WHERE email = $1::citext")
        .bind(&email)
        .execute(&t.state.pool)
        .await
        .unwrap();
    t.join(&email).await;
    let both = t.mail_to(&email, 2).await;
    let old = link(&first[0].text, &format!("{BASE}/waitlist/confirm?token="));
    let new = link(&both[1].text, &format!("{BASE}/waitlist/confirm?token="));
    assert_ne!(old, new);
    assert_eq!(t.get(&old).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(t.get(&new).await.status, StatusCode::OK);
}

#[tokio::test]
async fn expired_token() {
    let t = test_app().await;
    let email = unique_email("expired");
    t.join(&email).await;
    let mail = t.mail_to(&email, 1).await;
    let confirm = link(&mail[0].text, &format!("{BASE}/waitlist/confirm?token="));
    sqlx::query("UPDATE waitlist_signups SET token_created_at = now() - interval '73 hours' WHERE email = $1::citext")
        .bind(&email)
        .execute(&t.state.pool)
        .await
        .unwrap();
    let res = t.get(&confirm).await;
    assert_eq!(res.status, StatusCode::GONE);
    assert!(res.body.contains("This link has expired."));
    assert!(!t.signup_row(&email).await.unwrap().1, "still unconfirmed");
}

#[tokio::test]
async fn bad_tokens() {
    let t = test_app().await;
    for uri in [
        "/waitlist/confirm",
        "/waitlist/confirm?token=",
        "/waitlist/confirm?token=nope",
        "/waitlist/confirm?token=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    ] {
        let res = t.get(uri).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{uri}");
        assert!(res.body.contains("This link doesn’t work."));
    }
    for uri in ["/waitlist/leave", "/waitlist/leave?token=forged"] {
        let res = t.get(uri).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{uri}");
        assert!(res.body.contains("This link doesn’t work."));
    }
}

#[tokio::test]
async fn leave_asks_first_and_only_the_post_deletes() {
    let t = test_app().await;
    let email = unique_email("leave");
    t.join(&email).await;
    let mail = t.mail_to(&email, 1).await;
    let leave = link(&mail[0].text, &format!("{BASE}/waitlist/leave?token="));
    let token = leave.split("token=").nth(1).unwrap().to_owned();

    // A scanner opening the link (any number of times) deletes nothing.
    for _ in 0..3 {
        let res = t.get(&leave).await;
        assert_eq!(res.status, StatusCode::OK);
        assert_eq!(res.header("cache-control"), "no-store");
    }
    assert!(t.signup_row(&email).await.is_some());

    // The page's form posts the token; it must come from our origin.
    let res = t.post_leave(&token, Some("https://evil.example")).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert!(t.signup_row(&email).await.is_some());
    let res = t.post_leave(&token, None).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert!(t.signup_row(&email).await.is_some());

    let res = t.post_leave(&token, Some(BASE)).await;
    assert_eq!(res.status, StatusCode::SEE_OTHER);
    assert_eq!(res.header("location"), "/waitlist/leave?done=1");
    assert!(t.signup_row(&email).await.is_none());
}

#[tokio::test]
async fn leave_with_a_bad_token_on_either_path() {
    let t = test_app().await;
    for bad in ["", "forged", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"] {
        let res = t.get(&format!("/waitlist/leave?token={bad}")).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "GET {bad:?}");
        assert!(res.body.contains("This link doesn’t work."));

        let res = t.post_leave(bad, Some(BASE)).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "form POST {bad:?}");
        assert!(res.body.contains("This link doesn’t work."));

        let req = Request::post(format!("/waitlist/leave?token={bad}").as_str())
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("List-Unsubscribe=One-Click"))
            .unwrap();
        let res = t.send(req, random_ip()).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "one-click {bad:?}");
    }
}

#[tokio::test]
async fn one_click_unsubscribe_needs_no_origin() {
    let t = test_app().await;
    let email = unique_email("oneclick");
    t.join(&email).await;
    let mail = t.mail_to(&email, 1).await;
    let leave = link(&mail[0].text, &format!("{BASE}/waitlist/leave?token="));
    assert_eq!(
        mail[0].unsubscribe_url.as_deref(),
        Some(format!("{BASE}{leave}").as_str()),
        "List-Unsubscribe points at the same URL"
    );
    let req = Request::post(leave.as_str())
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("List-Unsubscribe=One-Click"))
        .unwrap();
    let res = t.send(req, random_ip()).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.body.contains("You’ve left the waitlist."));
    assert!(t.signup_row(&email).await.is_none());
}

#[tokio::test]
async fn invalid_email() {
    let t = test_app().await;
    let res = t
        .post_json(
            json!({"email": "not-an-email", "website": "", "t": t_ok()}),
            random_ip(),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res.json(), json!({"ok": false, "error": "invalid"}));
    assert_eq!(
        res.body, r#"{"ok":false,"error":"invalid"}"#,
        "the contract's key order"
    );

    let res = t
        .post_form(
            &[("email", "nope@"), ("website", ""), ("t", &t_ok())],
            random_ip(),
        )
        .await;
    assert_eq!(res.status, StatusCode::SEE_OTHER);
    assert_eq!(
        res.header("location"),
        "/?status=invalid&email=nope%40#join"
    );

    // The index re-fills it, escaped.
    let res = t
        .get("/?status=invalid&email=%22%3E%3Cscript%3Ex%3C%2Fscript%3E")
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(!res.body.contains("<script>x"), "user data is escaped");
    assert!(res.body.contains("aria-invalid=\"true\""));

    // Malformed JSON is invalid too.
    let req = Request::post("/waitlist")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, BASE)
        .body(Body::from("{nope"))
        .unwrap();
    assert_eq!(
        t.send(req, random_ip()).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn honeypot_and_timing_are_silent() {
    let t = test_app().await;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .to_string();
    let honey = unique_email("honey");
    let fast = unique_email("fast");
    let untimed = unique_email("untimed");

    let res = t
        .post_json(
            json!({"email": honey, "website": "https://spam.example", "t": t_ok()}),
            random_ip(),
        )
        .await;
    assert_eq!(
        (res.status, res.json()),
        (StatusCode::OK, json!({"ok": true}))
    );

    let res = t
        .post_json(json!({"email": fast, "website": "", "t": now}), random_ip())
        .await;
    assert_eq!(
        (res.status, res.json()),
        (StatusCode::OK, json!({"ok": true}))
    );

    let res = t
        .post_form(&[("email", &untimed), ("website", "")], random_ip())
        .await;
    assert_eq!(res.header("location"), "/?status=sent#join");

    for e in [&honey, &fast, &untimed] {
        assert!(t.signup_row(e).await.is_none(), "{e} was not stored");
    }
}

#[tokio::test]
async fn rate_limit_per_ip() {
    let t = test_app().await;
    let ip = random_ip();
    for i in 0..8 {
        let res = t
            .post_json(
                json!({"email": unique_email("ip"), "website": "", "t": t_ok()}),
                ip,
            )
            .await;
        assert_eq!(res.status, StatusCode::OK, "request {i}");
    }
    let res = t
        .post_json(
            json!({"email": unique_email("ip"), "website": "", "t": t_ok()}),
            ip,
        )
        .await;
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(res.json(), json!({"ok": false, "error": "slow_down"}));

    let res = t
        .post_form(
            &[
                ("email", &unique_email("ip")),
                ("website", ""),
                ("t", &t_ok()),
            ],
            ip,
        )
        .await;
    assert_eq!(res.status, StatusCode::SEE_OTHER);
    assert_eq!(res.header("location"), "/?status=slow_down#join");

    // Another address is unaffected.
    assert_eq!(t.join(&unique_email("ip")).await.status, StatusCode::OK);
}

#[tokio::test]
async fn rate_limit_per_email() {
    let t = test_app().await;
    let email = unique_email("limit");
    for _ in 0..3 {
        assert_eq!(t.join(&email).await.status, StatusCode::OK);
    }
    let res = t.join(&email).await;
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
    // Case does not get around it.
    assert_eq!(
        t.join(&email.to_uppercase()).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn cross_origin_posts_are_refused() {
    let t = test_app().await;
    let body = json!({"email": unique_email("cors"), "website": "", "t": t_ok()}).to_string();

    let no_origin = Request::post("/waitlist")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json")
        .body(Body::from(body.clone()))
        .unwrap();
    let res = t.send(no_origin, random_ip()).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(res.json(), json!({"ok": false, "error": "forbidden"}));

    let evil = Request::post("/waitlist")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::from("email=a%40b.example&t=1"))
        .unwrap();
    let res = t.send(evil, random_ip()).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert!(res.header("content-type").starts_with("text/html"));

    // A same-origin Referer is enough, and its UTM parameters are kept.
    let email = unique_email("utm");
    let by_referer = Request::post("/waitlist")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json")
        .header(
            header::REFERER,
            format!("{BASE}/?utm_source=newsletter&utm_campaign=launch&x=1"),
        )
        .body(Body::from(
            json!({"email": email, "website": "", "t": t_ok()}).to_string(),
        ))
        .unwrap();
    assert_eq!(t.send(by_referer, random_ip()).await.status, StatusCode::OK);
    let (_, _, source, _) = t.signup_row(&email).await.unwrap();
    assert_eq!(
        source,
        json!({"utm_source": "newsletter", "utm_campaign": "launch"})
    );
}

#[tokio::test]
async fn body_limit() {
    let t = test_app().await;
    let big = format!("email={}&t=1", "a".repeat(20 * 1024));
    let req = Request::post("/waitlist")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ORIGIN, BASE)
        .body(Body::from(big))
        .unwrap();
    let res = t.send(req, random_ip()).await;
    assert_eq!(res.status, StatusCode::PAYLOAD_TOO_LARGE);
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
    let res = t.join(&unique_email("hdr")).await;
    assert_security_headers(&res, "POST /waitlist");
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
        ("/?status=sent", StatusCode::OK),
        ("/?status=invalid&email=x", StatusCode::OK),
        ("/?status=slow_down", StatusCode::OK),
        ("/?status=bogus", StatusCode::OK),
        ("/privacy", StatusCode::OK),
        ("/waitlist/confirm?token=x", StatusCode::BAD_REQUEST),
        ("/waitlist/leave?token=x", StatusCode::BAD_REQUEST),
        ("/waitlist/leave?done=1", StatusCode::OK),
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

    // The index carries the form fields and a fresh render time.
    let res = t.get("/").await;
    assert!(res.body.contains(r#"action="/waitlist""#));
    assert!(res.body.contains(r#"name="website""#));
    assert!(
        res.body
            .contains(&format!(r#"<link rel="canonical" href="{BASE}/">"#))
    );
    let t_field = res.body.split(r#"name="t" value=""#).nth(1).unwrap();
    let rendered: u128 = t_field[..t_field.find('"').unwrap()].parse().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    assert!(now.abs_diff(rendered) < 10_000);
    assert_eq!(res.header("cache-control"), "no-cache");
    assert!(
        t.get("/?status=sent")
            .await
            .body
            .contains("Check your inbox.")
    );
    assert!(
        t.get("/?status=slow_down")
            .await
            .body
            .contains("Too many tries")
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

    // GET /waitlist sends people to the form.
    let res = t.get("/waitlist").await;
    assert_eq!(res.status, StatusCode::SEE_OTHER);
    assert_eq!(res.header("location"), "/#join");
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
    assert!(res.body.contains("Disallow: /waitlist/"));

    let res = t.get("/sitemap.xml").await;
    assert!(res.header("content-type").starts_with("application/xml"));
    assert!(res.body.contains(&format!("<loc>{BASE}/</loc>")));
    assert!(res.body.contains(&format!("<loc>{BASE}/privacy</loc>")));

    // HEAD works on pages.
    let req = Request::head("/").body(Body::empty()).unwrap();
    assert_eq!(t.send(req, random_ip()).await.status, StatusCode::OK);
}
