//! Routes, per `docs/contract.md`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{RawQuery, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use dijmo_core::EmailAddress;
use dijmo_core::waitlist::{self, ConfirmOutcome, JoinOutcome};
use serde::Deserialize;
use serde_json::json;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::assets::ONE_DAY;
use crate::pages::{
    Caching, ConfirmPage, IndexPage, LeavePage, Meta, PrivacyPage, error_page, render,
};
use crate::security::{ClientIp, json_error, same_origin, wants_json};
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
        .route(
            "/waitlist",
            post(join).get(|| async { Redirect::to("/#join") }),
        )
        .route("/waitlist/confirm", get(confirm))
        .route("/waitlist/leave", get(leave).post(leave_one_click))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/robots.txt", get(robots))
        .route("/sitemap.xml", get(sitemap))
        .route(&css.url.clone(), get(move || async move { css.response() }))
        .route(&js.url.clone(), get(move || async move { js.response() }))
        .nest_service("/assets", static_files)
        .fallback(|| async { StatusCode::NOT_FOUND })
}

/// The first value of `name` in a raw query string.
fn query_param(raw: Option<&str>, name: &str) -> Option<String> {
    url::form_urlencoded::parse(raw?.as_bytes())
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

// ---------------------------------------------------------------- pages

async fn index(State(state): State<AppState>, RawQuery(q): RawQuery) -> Response {
    let status = query_param(q.as_deref(), "status")
        .filter(|s| matches!(s.as_str(), "sent" | "invalid" | "slow_down"))
        .unwrap_or_default();
    let email = if status == "invalid" {
        query_param(q.as_deref(), "email")
            .map(|e| e.chars().take(254).collect())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let meta = Meta::new(&state, "/", SITE_TITLE, SITE_DESCRIPTION);
    let page = IndexPage::new(meta, status, email, now_ms().to_string());
    render(&page, StatusCode::OK, Caching::Revalidate)
}

async fn privacy(State(state): State<AppState>) -> Response {
    let meta = Meta::new(
        &state,
        "/privacy",
        "Privacy · Dijmo",
        "What happens when you join the Dijmo waitlist, in plain words.",
    );
    let mut res = render(&PrivacyPage::new(meta), StatusCode::OK, Caching::Revalidate);
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    res
}

// ---------------------------------------------------------------- waitlist

/// The JSON body of the JavaScript path. `t` may be a string or a number.
#[derive(Debug, Default, Deserialize)]
struct JsonJoin {
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    website: Option<String>,
    #[serde(default)]
    t: Option<serde_json::Value>,
}

#[derive(Debug, Default)]
struct Submission {
    email: String,
    website: String,
    t: Option<u128>,
}

fn parse_submission(headers: &HeaderMap, body: &[u8]) -> Option<Submission> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if content_type.starts_with("application/json") {
        let j: JsonJoin = serde_json::from_slice(body).ok()?;
        let t = match j.t {
            Some(serde_json::Value::Number(n)) => n.as_u64().map(u128::from),
            Some(serde_json::Value::String(s)) => s.trim().parse().ok(),
            _ => None,
        };
        Some(Submission {
            email: j.email.unwrap_or_default(),
            website: j.website.unwrap_or_default(),
            t,
        })
    } else if content_type.starts_with("application/x-www-form-urlencoded")
        || content_type.is_empty()
    {
        let mut s = Submission::default();
        for (k, v) in url::form_urlencoded::parse(body) {
            match k.as_ref() {
                "email" if s.email.is_empty() => s.email = v.into_owned(),
                "website" if s.website.is_empty() => s.website = v.into_owned(),
                "t" if s.t.is_none() => s.t = v.trim().parse().ok(),
                _ => {}
            }
        }
        Some(s)
    } else {
        None
    }
}

enum Answer {
    /// Accepted, already known, or silently dropped: indistinguishable.
    Ok,
    Invalid(String),
    SlowDown,
}

fn answer(json: bool, a: Answer) -> Response {
    let no_store = [(header::CACHE_CONTROL, "no-store")];
    match (json, a) {
        (true, Answer::Ok) => {
            (StatusCode::OK, no_store, axum::Json(json!({"ok": true}))).into_response()
        }
        (true, Answer::Invalid(_)) => json_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid"),
        (true, Answer::SlowDown) => json_error(StatusCode::TOO_MANY_REQUESTS, "slow_down"),
        (false, Answer::Ok) => (no_store, Redirect::to("/?status=sent#join")).into_response(),
        (false, Answer::SlowDown) => {
            (no_store, Redirect::to("/?status=slow_down#join")).into_response()
        }
        (false, Answer::Invalid(email)) => {
            let refill: String = email.trim().chars().take(254).collect();
            let to = if refill.is_empty() {
                "/?status=invalid#join".to_owned()
            } else {
                let encoded: String =
                    url::form_urlencoded::byte_serialize(refill.as_bytes()).collect();
                format!("/?status=invalid&email={encoded}#join")
            };
            (no_store, Redirect::to(&to)).into_response()
        }
    }
}

/// UTM parameters from the page the form was on (the same-origin `Referer`
/// carries its full URL) and from this request's own query.
fn utm_source(headers: &HeaderMap, own_query: Option<&str>, origin: &str) -> serde_json::Value {
    let referer_query = headers
        .get(header::REFERER)
        .and_then(|r| r.to_str().ok())
        .and_then(|r| url::Url::parse(r).ok())
        .filter(|u| u.origin().ascii_serialization() == origin)
        .and_then(|u| u.query().map(str::to_owned));
    let mut pairs: Vec<(String, String)> = Vec::new();
    for q in [own_query.map(str::to_owned), referer_query]
        .into_iter()
        .flatten()
    {
        pairs.extend(url::form_urlencoded::parse(q.as_bytes()).into_owned());
    }
    waitlist::source_from_pairs(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
}

async fn join(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    RawQuery(own_query): RawQuery,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let json = wants_json(&headers);

    if !same_origin(&headers, state.base_url.origin()) {
        tracing::info!("waitlist: refused a cross-origin post");
        return if json {
            json_error(StatusCode::FORBIDDEN, "forbidden")
        } else {
            error_page(&state, StatusCode::FORBIDDEN)
        };
    }

    let Some(sub) = parse_submission(&headers, &body) else {
        return answer(json, Answer::Invalid(String::new()));
    };

    let ip_key = ip
        .map(|ip| state.keys.rate_limit_ip_key(ip))
        .unwrap_or([0u8; 16]);
    if !state.limits.ip.check(&ip_key) {
        tracing::info!("waitlist: rate limited (ip)");
        return answer(json, Answer::SlowDown);
    }

    // Bots: the hidden field is filled, or the form came back too fast (or
    // without its render time). They get the same answer as everyone else.
    let now = now_ms();
    let min_fill = state.min_fill_time.as_millis();
    let too_fast = match sub.t {
        None => true,
        Some(t) => t > now + 300_000 || now.saturating_sub(t) < min_fill,
    };
    if !sub.website.trim().is_empty() || too_fast {
        tracing::info!(
            honeypot = !sub.website.trim().is_empty(),
            too_fast,
            "waitlist: dropped a bot-like post"
        );
        return answer(json, Answer::Ok);
    }

    let Some(email) = EmailAddress::parse(&sub.email) else {
        return answer(json, Answer::Invalid(sub.email));
    };

    if !state
        .limits
        .email
        .check(&state.keys.rate_limit_email_key(email.as_str()))
    {
        tracing::info!("waitlist: rate limited (email)");
        return answer(json, Answer::SlowDown);
    }

    let source = utm_source(&headers, own_query.as_deref(), state.base_url.origin());
    let ip_hash = ip.map(|ip| state.keys.coarse_ip_hash(ip));
    match waitlist::join(
        &state.pool,
        &email,
        &source,
        ip_hash.as_deref(),
        &state.policy,
    )
    .await
    {
        Ok(outcome) => {
            if let JoinOutcome::Queued { .. } = outcome {
                state.outbox_wake.notify_one();
            }
            tracing::info!(outcome = ?outcome_label(outcome), "waitlist: join");
            answer(json, Answer::Ok)
        }
        Err(e) => {
            tracing::error!(error = %e, "waitlist: join failed");
            if json {
                json_error(StatusCode::INTERNAL_SERVER_ERROR, "server")
            } else {
                error_page(&state, StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

fn outcome_label(o: JoinOutcome) -> &'static str {
    match o {
        JoinOutcome::Queued { .. } => "queued",
        JoinOutcome::CoolingDown => "cooling_down",
        JoinOutcome::AlreadyConfirmed => "already_confirmed",
    }
}

async fn confirm(State(state): State<AppState>, RawQuery(q): RawQuery) -> Response {
    let token = query_param(q.as_deref(), "token").unwrap_or_default();
    let outcome = match waitlist::confirm(&state.pool, token.trim(), &state.policy).await {
        Ok(o) => o,
        Err(e) => {
            tracing::error!(error = %e, "waitlist: confirm failed");
            return error_page(&state, StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    tracing::info!(outcome = outcome.as_str(), "waitlist: confirm");
    let (title, status) = match outcome {
        ConfirmOutcome::Confirmed => ("You’re on the list · Dijmo", StatusCode::OK),
        ConfirmOutcome::Already => ("You’re already on the list · Dijmo", StatusCode::OK),
        ConfirmOutcome::Expired => ("This link has expired · Dijmo", StatusCode::GONE),
        ConfirmOutcome::Invalid => ("This link doesn’t work · Dijmo", StatusCode::BAD_REQUEST),
    };
    let meta = Meta::new(&state, "/waitlist/confirm", title, SITE_DESCRIPTION);
    render(
        &ConfirmPage::new(meta, outcome.as_str().to_owned()),
        status,
        Caching::NoStore,
    )
}

async fn leave_with(state: &AppState, q: Option<&str>) -> Response {
    let token = query_param(q, "token").unwrap_or_default();
    let left = match state.keys.verify_leave_token(token.trim()) {
        None => false,
        Some(id) => match waitlist::leave(&state.pool, id).await {
            Ok(_) => true,
            Err(e) => {
                tracing::error!(error = %e, "waitlist: leave failed");
                return error_page(state, StatusCode::INTERNAL_SERVER_ERROR);
            }
        },
    };
    tracing::info!(left, "waitlist: leave");
    let (outcome, title, status) = if left {
        ("left", "You’ve left the waitlist · Dijmo", StatusCode::OK)
    } else {
        (
            "invalid",
            "This link doesn’t work · Dijmo",
            StatusCode::BAD_REQUEST,
        )
    };
    let meta = Meta::new(state, "/waitlist/leave", title, SITE_DESCRIPTION);
    render(
        &LeavePage::new(meta, outcome.to_owned()),
        status,
        Caching::NoStore,
    )
}

async fn leave(State(state): State<AppState>, RawQuery(q): RawQuery) -> Response {
    leave_with(&state, q.as_deref()).await
}

/// RFC 8058 one-click unsubscribe: mail providers POST to the
/// `List-Unsubscribe` URL. The signed token is the authority, so there is no
/// Origin check here.
async fn leave_one_click(State(state): State<AppState>, RawQuery(q): RawQuery) -> Response {
    leave_with(&state, q.as_deref()).await
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
        "User-agent: *\nAllow: /\nDisallow: /waitlist/\n\nSitemap: {}\n",
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
