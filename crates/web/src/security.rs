//! Security headers, the Origin/Referer check, the client IP, and turning
//! bare error responses into the error page.

use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::pages::error_page;
use crate::state::AppState;

/// The contract's Content-Security-Policy, sent exactly.
pub const CSP: &str = "default-src 'self'; img-src 'self' data:; media-src 'self'; font-src 'self'; style-src 'self'; script-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'";

pub const PERMISSIONS_POLICY: &str = "camera=(), microphone=(), geolocation=(), browsing-topics=()";

pub const HSTS: &str = "max-age=31536000";

/// Adds the security headers to every response, and a default
/// `Cache-Control` where a handler set none.
pub async fn security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    let set = |h: &mut HeaderMap, name: HeaderName, value: &'static str| {
        h.insert(name, HeaderValue::from_static(value));
    };
    set(h, header::CONTENT_SECURITY_POLICY, CSP);
    set(h, header::STRICT_TRANSPORT_SECURITY, HSTS);
    set(h, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    set(
        h,
        header::REFERRER_POLICY,
        "strict-origin-when-cross-origin",
    );
    set(
        h,
        HeaderName::from_static("permissions-policy"),
        PERMISSIONS_POLICY,
    );
    set(h, header::X_FRAME_OPTIONS, "DENY");
    set(
        h,
        HeaderName::from_static("cross-origin-opener-policy"),
        "same-origin",
    );
    if !h.contains_key(header::CACHE_CONTROL) {
        set(h, header::CACHE_CONTROL, "no-cache");
    }
    res
}

/// Whether the client wants JSON back (the JavaScript path).
pub fn wants_json(headers: &HeaderMap) -> bool {
    let has = |name: HeaderName| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.to_ascii_lowercase().contains("application/json"))
    };
    has(header::ACCEPT) || has(header::CONTENT_TYPE)
}

/// Replaces bare error responses (from the router, a layer, a rejected
/// extractor or a panic) with the error page, or JSON for the JS path.
/// Responses that already carry HTML or JSON are left alone.
pub async fn error_pages(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let json = wants_json(req.headers());
    let res = next.run(req).await;
    let status = res.status();
    if !(status.is_client_error() || status.is_server_error()) {
        return res;
    }
    let own = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/html") || ct.starts_with("application/json"));
    if own {
        return res;
    }
    if status.is_server_error() {
        tracing::error!(status = status.as_u16(), "request failed");
    }
    if json {
        json_error(status, error_code(status))
    } else {
        error_page(&state, status)
    }
}

fn error_code(status: StatusCode) -> &'static str {
    match status.as_u16() {
        400 | 415 | 422 => "invalid",
        403 => "forbidden",
        404 | 405 => "not_found",
        413 => "too_large",
        429 => "slow_down",
        408 | 503 | 504 => "unavailable",
        _ => "server",
    }
}

/// The JSON error body, with keys in the contract's order.
#[derive(serde::Serialize)]
struct ApiError<'a> {
    ok: bool,
    error: &'a str,
}

/// `{"ok":false,"error":…}` with `status`.
pub fn json_error(status: StatusCode, error: &str) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(ApiError { ok: false, error }),
    )
        .into_response()
}

/// Checks a POST's `Origin` (or, failing that, `Referer`) against
/// `PUBLIC_BASE_URL`. A request with neither is refused.
pub fn same_origin(headers: &HeaderMap, expected_origin: &str) -> bool {
    if let Some(origin) = headers.get(header::ORIGIN) {
        return origin.to_str().is_ok_and(|o| o == expected_origin);
    }
    headers
        .get(header::REFERER)
        .and_then(|r| r.to_str().ok())
        .and_then(|r| url::Url::parse(r).ok())
        .is_some_and(|u| u.origin().ascii_serialization() == expected_origin)
}

/// The client's IP: the peer address, or with `PROXY_HOPS = N` the Nth
/// address from the right of `X-Forwarded-For` (each trusted proxy appends
/// the address it saw, so entries further left can be forged).
#[derive(Clone, Copy, Debug)]
pub struct ClientIp(pub Option<IpAddr>);

pub fn client_ip(headers: &HeaderMap, peer: Option<IpAddr>, proxy_hops: usize) -> Option<IpAddr> {
    if proxy_hops == 0 {
        return peer;
    }
    let chain: Vec<&str> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if chain.len() < proxy_hops {
        return peer;
    }
    parse_ip(chain[chain.len() - proxy_hops]).or(peer)
}

fn parse_ip(s: &str) -> Option<IpAddr> {
    s.parse::<IpAddr>()
        .ok()
        .or_else(|| s.parse::<SocketAddr>().ok().map(|a| a.ip()))
}

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Infallible> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip());
        Ok(Self(client_ip(&parts.headers, peer, state.proxy_hops)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    #[test]
    fn origin_check() {
        let o = "https://dijmo.example";
        assert!(same_origin(&headers(&[("origin", o)]), o));
        assert!(!same_origin(
            &headers(&[("origin", "https://evil.example")]),
            o
        ));
        assert!(!same_origin(&headers(&[("origin", "null")]), o));
        assert!(same_origin(
            &headers(&[("referer", "https://dijmo.example/?utm_source=x")]),
            o
        ));
        assert!(!same_origin(
            &headers(&[("referer", "https://dijmo.example.evil/")]),
            o
        ));
        assert!(!same_origin(&HeaderMap::new(), o));
    }

    #[test]
    fn client_ip_from_forwarded_chain() {
        let peer: Option<IpAddr> = Some("10.0.0.2".parse().unwrap());
        let h = headers(&[("x-forwarded-for", "1.1.1.1, 203.0.113.9")]);
        assert_eq!(client_ip(&h, peer, 0), peer);
        assert_eq!(client_ip(&h, peer, 1), Some("203.0.113.9".parse().unwrap()));
        assert_eq!(client_ip(&h, peer, 2), Some("1.1.1.1".parse().unwrap()));
        assert_eq!(client_ip(&h, peer, 3), peer, "chain too short");
        assert_eq!(client_ip(&HeaderMap::new(), peer, 1), peer);
    }

    #[test]
    fn json_detection() {
        assert!(wants_json(&headers(&[("accept", "application/json")])));
        assert!(wants_json(&headers(&[(
            "content-type",
            "application/json; charset=utf-8"
        )])));
        assert!(!wants_json(&headers(&[("accept", "text/html,*/*")])));
    }
}
