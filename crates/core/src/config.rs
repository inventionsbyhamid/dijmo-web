//! Configuration, read only from the environment (12-factor).

use std::fmt;
use std::path::PathBuf;

use base64::Engine as _;
use url::Url;

use crate::keys::SecretKey;

/// Minimum decoded length of `SECRET_KEY`.
pub const SECRET_KEY_MIN_BYTES: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0} is not set")]
    Missing(&'static str),
    #[error("{name} is invalid: {reason}")]
    Invalid { name: &'static str, reason: String },
}

fn invalid(name: &'static str, reason: impl Into<String>) -> ConfigError {
    ConfigError::Invalid {
        name,
        reason: reason.into(),
    }
}

/// Where outgoing email goes.
#[derive(Clone)]
pub enum EmailProvider {
    /// Development: the message is written to the log.
    Log,
    /// Production: Resend's HTTP API.
    Resend { api_key: String },
}

impl fmt::Debug for EmailProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Log => f.write_str("Log"),
            Self::Resend { .. } => f.write_str("Resend { api_key: <redacted> }"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogFormat {
    Text,
    Json,
}

/// `PUBLIC_BASE_URL`, normalised: no trailing slash, plus its origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseUrl {
    url: String,
    origin: String,
}

impl BaseUrl {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let parsed = Url::parse(raw.trim()).map_err(|e| e.to_string())?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("must start with http:// or https://".into());
        }
        if parsed.host_str().is_none() {
            return Err("has no host".into());
        }
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return Err("must not have a query or fragment".into());
        }
        let origin = parsed.origin().ascii_serialization();
        let url = parsed.as_str().trim_end_matches('/').to_owned();
        Ok(Self { url, origin })
    }

    /// The base URL with no trailing slash, e.g. `https://dijmo.com`.
    pub fn as_str(&self) -> &str {
        &self.url
    }

    /// The origin, e.g. `https://dijmo.com`, as browsers send it in `Origin`.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// An absolute URL for `path` (which starts with `/`).
    pub fn join(&self, path: &str) -> String {
        format!("{}{}", self.url, path)
    }

    pub fn is_https(&self) -> bool {
        self.url.starts_with("https://")
    }
}

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub public_base_url: BaseUrl,
    pub secret_key: SecretKey,
    pub port: u16,
    pub email_provider: EmailProvider,
    /// The `From:` address, e.g. `Dijmo <hello@example.com>`.
    pub email_from: String,
    /// How many reverse proxies sit in front of the server. 0 means the peer
    /// address is the client; N means the client is the Nth address from the
    /// right of `X-Forwarded-For` (Coolify's proxy appends one).
    pub proxy_hops: usize,
    pub assets_dir: PathBuf,
    pub log_format: LogFormat,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"<redacted>")
            .field("public_base_url", &self.public_base_url.as_str())
            .field("secret_key", &"<redacted>")
            .field("port", &self.port)
            .field("email_provider", &self.email_provider)
            .field("email_from", &self.email_from)
            .field("proxy_hops", &self.proxy_hops)
            .field("assets_dir", &self.assets_dir)
            .field("log_format", &self.log_format)
            .finish()
    }
}

impl Config {
    /// Reads the configuration from the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Reads the configuration through `lookup` (the environment, or a map in tests).
    /// Empty values count as unset.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let get = |name: &str| lookup(name).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        let require = |name: &'static str| get(name).ok_or(ConfigError::Missing(name));

        let database_url = require("DATABASE_URL")?;

        let public_base_url = BaseUrl::parse(&require("PUBLIC_BASE_URL")?)
            .map_err(|reason| invalid("PUBLIC_BASE_URL", reason))?;

        let secret_key = parse_secret_key(&require("SECRET_KEY")?)?;

        let port = match get("PORT") {
            None => 8080,
            Some(p) => p
                .parse::<u16>()
                .map_err(|_| invalid("PORT", "must be a port number"))?,
        };

        let resend_key = get("RESEND_API_KEY");
        let email_provider = match get("EMAIL_PROVIDER").as_deref().map(str::to_ascii_lowercase) {
            Some(p) if p == "log" => EmailProvider::Log,
            Some(p) if p == "resend" => EmailProvider::Resend {
                api_key: resend_key.ok_or(ConfigError::Missing("RESEND_API_KEY"))?,
            },
            Some(_) => return Err(invalid("EMAIL_PROVIDER", "must be `log` or `resend`")),
            None => match resend_key {
                Some(api_key) => EmailProvider::Resend { api_key },
                None => EmailProvider::Log,
            },
        };

        let email_from = match (&email_provider, get("EMAIL_FROM")) {
            (_, Some(from)) => from,
            (EmailProvider::Log, None) => "Dijmo <waitlist@localhost>".to_owned(),
            (EmailProvider::Resend { .. }, None) => return Err(ConfigError::Missing("EMAIL_FROM")),
        };
        if email_from.contains(['\r', '\n']) || !email_from.contains('@') {
            return Err(invalid("EMAIL_FROM", "must be an address like `Dijmo <hello@example.com>`"));
        }

        let proxy_hops = match get("PROXY_HOPS") {
            None => 0,
            Some(h) => h
                .parse::<usize>()
                .ok()
                .filter(|h| *h <= 8)
                .ok_or_else(|| invalid("PROXY_HOPS", "must be a number from 0 to 8"))?,
        };

        let assets_dir = PathBuf::from(get("ASSETS_DIR").unwrap_or_else(|| "assets".to_owned()));

        let log_format = match get("LOG_FORMAT").as_deref().map(str::to_ascii_lowercase) {
            None => LogFormat::Text,
            Some(f) if f == "text" => LogFormat::Text,
            Some(f) if f == "json" => LogFormat::Json,
            Some(_) => return Err(invalid("LOG_FORMAT", "must be `text` or `json`")),
        };

        Ok(Self {
            database_url,
            public_base_url,
            secret_key,
            port,
            email_provider,
            email_from,
            proxy_hops,
            assets_dir,
            log_format,
        })
    }
}

/// Decodes `SECRET_KEY`: base64 (standard or URL-safe, padding optional,
/// whitespace ignored so `openssl rand -base64 64` output works as is).
fn parse_secret_key(raw: &str) -> Result<SecretKey, ConfigError> {
    use base64::engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD};
    let compact: String = raw
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .trim_end_matches('=')
        .to_owned();
    let bytes = STANDARD_NO_PAD
        .decode(&compact)
        .or_else(|_| URL_SAFE_NO_PAD.decode(&compact))
        .map_err(|_| invalid("SECRET_KEY", "must be base64"))?;
    if bytes.len() < SECRET_KEY_MIN_BYTES {
        return Err(invalid(
            "SECRET_KEY",
            format!(
                "must decode to at least {SECRET_KEY_MIN_BYTES} bytes (got {}); \
                 generate one with `openssl rand -base64 64 | tr -d '\\n'`",
                bytes.len()
            ),
        ));
    }
    Ok(SecretKey::new(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn key() -> String {
        base64::engine::general_purpose::STANDARD.encode([7u8; 64])
    }

    fn cfg(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(|k| map.get(k).cloned())
    }

    #[test]
    fn minimal_config_defaults_to_log_and_port_8080() {
        let k = key();
        let c = cfg(&[
            ("DATABASE_URL", "postgres://localhost/x"),
            ("PUBLIC_BASE_URL", "https://dijmo.example/"),
            ("SECRET_KEY", &k),
        ])
        .unwrap();
        assert_eq!(c.port, 8080);
        assert!(matches!(c.email_provider, EmailProvider::Log));
        assert_eq!(c.public_base_url.as_str(), "https://dijmo.example");
        assert_eq!(c.public_base_url.origin(), "https://dijmo.example");
        assert_eq!(c.proxy_hops, 0);
    }

    #[test]
    fn resend_needs_from_and_key_is_redacted() {
        let k = key();
        let base = [
            ("DATABASE_URL", "postgres://u:pw@localhost/x"),
            ("PUBLIC_BASE_URL", "http://localhost:8080"),
            ("SECRET_KEY", k.as_str()),
            ("RESEND_API_KEY", "re_secret"),
        ];
        assert!(matches!(cfg(&base), Err(ConfigError::Missing("EMAIL_FROM"))));
        let mut with_from = base.to_vec();
        with_from.push(("EMAIL_FROM", "Dijmo <hello@dijmo.example>"));
        let c = cfg(&with_from).unwrap();
        let shown = format!("{c:?}");
        assert!(!shown.contains("re_secret"));
        assert!(!shown.contains("pw"));
        assert_eq!(c.public_base_url.origin(), "http://localhost:8080");
    }

    #[test]
    fn short_or_bad_secret_is_rejected() {
        let short = base64::engine::general_purpose::STANDARD.encode([1u8; 32]);
        for bad in [short.as_str(), "not base64 !!"] {
            let r = cfg(&[
                ("DATABASE_URL", "postgres://localhost/x"),
                ("PUBLIC_BASE_URL", "https://dijmo.example"),
                ("SECRET_KEY", bad),
            ]);
            assert!(matches!(r, Err(ConfigError::Invalid { name: "SECRET_KEY", .. })));
        }
    }

    #[test]
    fn wrapped_openssl_output_is_accepted() {
        let k = key();
        let wrapped = format!("{}\n{}", &k[..40], &k[40..]);
        assert!(parse_secret_key(&wrapped).is_ok());
    }

    #[test]
    fn base_url_must_be_http() {
        assert!(BaseUrl::parse("ftp://x.example").is_err());
        assert!(BaseUrl::parse("https://x.example/?q=1").is_err());
        assert_eq!(
            BaseUrl::parse("https://x.example/").unwrap().join("/privacy"),
            "https://x.example/privacy"
        );
    }
}
