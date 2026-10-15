//! Outgoing email: the [`EmailSender`] trait, with [`LogSender`] for
//! development and [`ResendSender`] for production, plus the messages.

use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Duration;

use dijmo_core::email_address::mask;
use serde_json::json;

/// One message, as both HTML and plain text.
#[derive(Clone, Debug)]
pub struct Email {
    pub to: String,
    pub subject: String,
    pub html: String,
    pub text: String,
    /// The one-click unsubscribe URL (RFC 8058), sent as `List-Unsubscribe`.
    pub unsubscribe_url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SendError {
    /// Worth retrying: network trouble, rate limits, provider outages,
    /// or a configuration problem the owner can fix.
    #[error("transient: {0}")]
    Transient(String),
    /// Retrying will not help, e.g. the provider rejects the address.
    #[error("permanent: {0}")]
    Permanent(String),
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait EmailSender: Send + Sync + 'static {
    fn send<'a>(&'a self, email: &'a Email) -> BoxFuture<'a, Result<(), SendError>>;
}

/// Development: writes the message to the log. The recipient is masked;
/// the text body (with its links) is logged in full so you can click them.
#[derive(Debug, Default)]
pub struct LogSender;

impl EmailSender for LogSender {
    fn send<'a>(&'a self, email: &'a Email) -> BoxFuture<'a, Result<(), SendError>> {
        Box::pin(async move {
            tracing::info!(
                target: "dijmo_web::email",
                to = %mask(&email.to),
                subject = %email.subject,
                body = %email.text,
                "email (log sender, not delivered)"
            );
            Ok(())
        })
    }
}

/// Keeps messages in memory. For tests.
#[derive(Debug, Default)]
pub struct MemorySender {
    pub sent: Mutex<Vec<Email>>,
}

impl MemorySender {
    pub fn messages_to(&self, to: &str) -> Vec<Email> {
        self.sent
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .filter(|e| e.to == to)
            .cloned()
            .collect()
    }
}

impl EmailSender for MemorySender {
    fn send<'a>(&'a self, email: &'a Email) -> BoxFuture<'a, Result<(), SendError>> {
        Box::pin(async move {
            self.sent
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(email.clone());
            Ok(())
        })
    }
}

/// Production: Resend's HTTP API.
pub struct ResendSender {
    client: reqwest::Client,
    api_key: String,
    from: String,
    endpoint: String,
}

impl ResendSender {
    pub fn new(api_key: String, from: String) -> Self {
        Self::with_endpoint(api_key, from, "https://api.resend.com/emails".to_owned())
    }

    pub fn with_endpoint(api_key: String, from: String, endpoint: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .user_agent("dijmo-web")
            .build()
            .expect("the HTTP client builds");
        Self {
            client,
            api_key,
            from,
            endpoint,
        }
    }
}

impl EmailSender for ResendSender {
    fn send<'a>(&'a self, email: &'a Email) -> BoxFuture<'a, Result<(), SendError>> {
        Box::pin(async move {
            let mut body = json!({
                "from": self.from,
                "to": [email.to],
                "subject": email.subject,
                "html": email.html,
                "text": email.text,
            });
            if let Some(url) = &email.unsubscribe_url {
                body["headers"] = json!({
                    "List-Unsubscribe": format!("<{url}>"),
                    "List-Unsubscribe-Post": "List-Unsubscribe=One-Click",
                });
            }
            let res = self
                .client
                .post(&self.endpoint)
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    SendError::Transient(format!("request failed: {}", e.without_url()))
                })?;
            let status = res.status();
            if status.is_success() {
                return Ok(());
            }
            // Resend's error body names the problem, never our secrets.
            let detail: String = res
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(300)
                .collect();
            let msg = format!("resend answered {status}: {detail}");
            match status.as_u16() {
                // Bad key, unverified domain: fixable by the owner, so keep retrying.
                401 | 403 | 408 | 409 | 429 => Err(SendError::Transient(msg)),
                s if s >= 500 => Err(SendError::Transient(msg)),
                _ => Err(SendError::Permanent(msg)),
            }
        })
    }
}

/// Escapes text for HTML.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// The confirmation email: one link to confirm, one to leave. No images, no
/// tracking pixels.
pub fn confirmation_email(to: &str, confirm_url: &str, leave_url: &str, base_url: &str) -> Email {
    let subject = "Confirm your place on the Dijmo waitlist".to_owned();
    let text = format!(
        "You asked to join the Dijmo waitlist.\n\
         \n\
         One click confirms it:\n\
         {confirm_url}\n\
         \n\
         The link works for 72 hours. If you didn’t ask, ignore this email and nothing happens.\n\
         \n\
         Dijmo is a video editor you talk to. Say what you want, and it’s done.\n\
         Coming to Mac first, then Windows and Linux.\n\
         \n\
         —\n\
         Leave the waitlist: {leave_url}\n\
         {base_url}\n"
    );
    let (c, l, b) = (esc(confirm_url), esc(leave_url), esc(base_url));
    let html = format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<title>{subject}</title>
</head>
<body style="margin:0;padding:0;background:#F2F3F1;color:#17181A;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#F2F3F1;">
<tr><td align="center" style="padding:48px 20px;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="max-width:480px;font-family:'Schibsted Grotesk',-apple-system,BlinkMacSystemFont,'Segoe UI',Helvetica,Arial,sans-serif;">
<tr><td style="padding:0 0 40px;font-size:17px;font-weight:700;letter-spacing:-0.01em;">dijmo<span style="color:#E0301A;">|</span></td></tr>
<tr><td style="padding:0 0 16px;font-size:26px;line-height:1.25;font-weight:600;letter-spacing:-0.02em;">One click, and you’re in.</td></tr>
<tr><td style="padding:0 0 32px;font-size:16px;line-height:1.6;color:#5E6166;">You asked to join the Dijmo waitlist. Confirm it, and we’ll write when Dijmo is ready for you to try.</td></tr>
<tr><td style="padding:0 0 32px;">
<a href="{c}" style="display:inline-block;background:#17181A;color:#F2F3F1;text-decoration:none;font-size:16px;font-weight:600;padding:14px 24px;border-radius:999px;">Confirm my email</a>
</td></tr>
<tr><td style="padding:0 0 40px;font-size:14px;line-height:1.6;color:#5E6166;">The link works for 72 hours. If you didn’t ask, ignore this email and nothing happens.<br><br>Or paste this into your browser:<br><a href="{c}" style="color:#17181A;word-break:break-all;">{c}</a></td></tr>
<tr><td style="padding:24px 0 0;border-top:1px solid #D9DBD8;font-size:13px;line-height:1.6;color:#5E6166;">Dijmo is a video editor you talk to. Say what you want, and it’s done.<br><a href="{l}" style="color:#5E6166;">Leave the waitlist</a> · <a href="{b}" style="color:#5E6166;">{b}</a></td></tr>
</table>
</td></tr>
</table>
</body>
</html>
"#
    );
    Email {
        to: to.to_owned(),
        subject,
        html,
        text,
        unsubscribe_url: Some(leave_url.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_email_has_both_links_and_no_tracking() {
        let e = confirmation_email(
            "a@b.example",
            "https://x.example/waitlist/confirm?token=abc",
            "https://x.example/waitlist/leave?token=def",
            "https://x.example",
        );
        for body in [&e.text, &e.html] {
            assert!(body.contains("/waitlist/confirm?token=abc"));
            assert!(body.contains("/waitlist/leave?token=def"));
        }
        assert!(!e.html.contains("<img"));
        assert_eq!(
            e.unsubscribe_url.as_deref(),
            Some("https://x.example/waitlist/leave?token=def")
        );
    }

    #[test]
    fn html_is_escaped() {
        assert_eq!(
            esc(r#"<a href="x">&'"#),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }
}
