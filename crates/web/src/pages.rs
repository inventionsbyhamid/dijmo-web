//! One struct per page, with the contract's exact field names
//! (`docs/contract.md`). Askama checks the templates at compile time and
//! escapes every field.

use askama::Template;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};

use crate::state::AppState;

/// The fields every page has.
#[derive(Clone, Debug)]
pub struct Meta {
    pub title: String,
    pub description: String,
    pub canonical: String,
    pub base_url: String,
    pub css: String,
    pub js: String,
    pub og_image: String,
}

impl Meta {
    /// Metadata for the page at `path` (the canonical URL never carries a query).
    pub fn new(state: &AppState, path: &str, title: &str, description: &str) -> Self {
        Self {
            title: title.to_owned(),
            description: description.to_owned(),
            canonical: state.base_url.join(path),
            base_url: state.base_url.as_str().to_owned(),
            css: state.assets.css.url.clone(),
            js: state.assets.js.url.clone(),
            og_image: state.base_url.join("/assets/img/og.png"),
        }
    }
}

macro_rules! page {
    ($(#[$doc:meta])* $name:ident, $path:literal { $($(#[$fdoc:meta])* $field:ident : $ty:ty),* $(,)? }) => {
        $(#[$doc])*
        #[derive(Template, Clone, Debug)]
        #[template(path = $path)]
        pub struct $name {
            pub title: String,
            pub description: String,
            pub canonical: String,
            pub base_url: String,
            pub css: String,
            pub js: String,
            pub og_image: String,
            $($(#[$fdoc])* pub $field: $ty,)*
        }

        impl $name {
            #[allow(clippy::too_many_arguments)]
            pub fn new(meta: Meta $(, $field: $ty)*) -> Self {
                Self {
                    title: meta.title,
                    description: meta.description,
                    canonical: meta.canonical,
                    base_url: meta.base_url,
                    css: meta.css,
                    js: meta.js,
                    og_image: meta.og_image,
                    $($field,)*
                }
            }
        }
    };
}

page!(
    /// `/`: the story and the waitlist.
    IndexPage, "index.html" {
        /// `""`, `"sent"`, `"invalid"` or `"slow_down"`.
        status: String,
        /// Re-filled on `invalid`.
        email: String,
        /// Render time in Unix ms, for the form's timing field.
        t: String,
    }
);

page!(
    /// `/waitlist/confirm`.
    ConfirmPage, "confirm.html" {
        /// `"confirmed"`, `"already"`, `"expired"` or `"invalid"`.
        outcome: String,
    }
);

page!(
    /// `/waitlist/leave`.
    LeavePage, "leave.html" {
        /// `"confirm"` (a valid link: ask first), `"left"` or `"invalid"`.
        outcome: String,
        /// The leave token on `"confirm"`, for the form's `token` field; else `""`.
        token: String,
    }
);

page!(
    /// `/privacy`.
    PrivacyPage, "privacy.html" {}
);

page!(
    /// 404, 500 and the other error pages.
    ErrorPage, "error.html" {
        code: u16,
        heading: String,
        body: String,
    }
);

/// Cache policy for HTML: pages carry a render time or an outcome, so the
/// browser revalidates; outcome pages are never stored.
#[derive(Clone, Copy, Debug)]
pub enum Caching {
    Revalidate,
    NoStore,
}

/// Renders a page into a response with `status`.
pub fn render<T: Template>(page: &T, status: StatusCode, caching: Caching) -> Response {
    match page.render() {
        Ok(html) => {
            let cache = match caching {
                Caching::Revalidate => "no-cache",
                Caching::NoStore => "no-store",
            };
            let mut res = (status, Html(html)).into_response();
            res.headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
            res
        }
        Err(e) => {
            tracing::error!(error = %e, "template failed to render");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CACHE_CONTROL, "no-store")],
                Html("<!doctype html><title>Dijmo</title><p>Something went wrong. Try again in a moment.</p>"),
            )
                .into_response()
        }
    }
}

/// The error page for `status`, in Dijmo's voice.
pub fn error_page(state: &AppState, status: StatusCode) -> Response {
    let (title, heading, body) = error_copy(status);
    let meta = Meta::new(state, "/", title, "Dijmo is a video editor you talk to.");
    let page = ErrorPage::new(meta, status.as_u16(), heading.to_owned(), body.to_owned());
    render(&page, status, Caching::NoStore)
}

pub fn error_copy(status: StatusCode) -> (&'static str, &'static str, &'static str) {
    match status.as_u16() {
        400 | 422 => (
            "That didn’t work · Dijmo",
            "That request didn’t make sense to us.",
            "Go back and try again.",
        ),
        403 => (
            "Not allowed · Dijmo",
            "That isn’t allowed from here.",
            "If you were joining the waitlist, use the form on the home page.",
        ),
        404 => (
            "Not found · Dijmo",
            "There’s nothing here.",
            "The link may be old, or cut short.",
        ),
        405 => (
            "Not found · Dijmo",
            "There’s nothing here.",
            "That address doesn’t take this kind of request.",
        ),
        408 | 503 | 504 => (
            "Taking too long · Dijmo",
            "That took too long.",
            "We’re a little busy. Try again in a moment.",
        ),
        413 => (
            "Too much · Dijmo",
            "That was more than we can take.",
            "Send something shorter.",
        ),
        429 => (
            "Slow down · Dijmo",
            "Too many tries in a short time.",
            "Wait a few minutes, then try again.",
        ),
        _ => (
            "Something went wrong · Dijmo",
            "Something went wrong on our side.",
            "It isn’t you. Try again in a moment.",
        ),
    }
}
