//! Static assets.
//!
//! `assets/css/site.css` and `assets/js/site.js` are hashed at startup and
//! served from memory at `/assets/site.<hash>.<ext>`, cached for a year.
//! Everything else under `assets/` is served from disk at `/assets/<path>`,
//! cached for a day.

use std::io;
use std::path::{Path, PathBuf};

use axum::body::Bytes;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

pub const IMMUTABLE: &str = "public, max-age=31536000, immutable";
pub const ONE_DAY: &str = "public, max-age=86400";

/// One file served from memory under a content hash.
#[derive(Clone, Debug)]
pub struct HashedAsset {
    /// e.g. `/assets/site.3f2a9c1e.css`
    pub url: String,
    pub bytes: Bytes,
    pub content_type: &'static str,
    pub etag: String,
}

impl HashedAsset {
    fn load(dir: &Path, rel: &str, ext: &str, content_type: &'static str) -> io::Result<Self> {
        let path = dir.join(rel);
        let bytes = std::fs::read(&path).map_err(|e| {
            io::Error::new(e.kind(), format!("cannot read {}: {e}", path.display()))
        })?;
        let digest = Sha256::digest(&bytes);
        let hash: String = digest[..4].iter().map(|b| format!("{b:02x}")).collect();
        Ok(Self {
            url: format!("/assets/site.{hash}.{ext}"),
            bytes: Bytes::from(bytes),
            content_type,
            etag: format!("\"{hash}\""),
        })
    }

    pub fn response(&self) -> Response {
        (
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static(self.content_type),
                ),
                (header::CACHE_CONTROL, HeaderValue::from_static(IMMUTABLE)),
                (
                    header::ETAG,
                    HeaderValue::from_str(&self.etag).expect("hex etag is a valid header"),
                ),
            ],
            self.bytes.clone(),
        )
            .into_response()
    }
}

#[derive(Clone, Debug)]
pub struct Assets {
    pub dir: PathBuf,
    pub css: HashedAsset,
    pub js: HashedAsset,
}

impl Assets {
    /// Reads and hashes the two source files. Fails if either is missing:
    /// the page would be broken without them.
    pub fn load(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        let css = HashedAsset::load(&dir, "css/site.css", "css", "text/css; charset=utf-8")?;
        let js = HashedAsset::load(&dir, "js/site.js", "js", "text/javascript; charset=utf-8")?;
        Ok(Self { dir, css, js })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_change_with_content() {
        let dir = std::env::temp_dir().join(format!("dijmo-assets-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("css")).unwrap();
        std::fs::create_dir_all(dir.join("js")).unwrap();
        std::fs::write(dir.join("css/site.css"), "body{}").unwrap();
        std::fs::write(dir.join("js/site.js"), "1").unwrap();
        let a = Assets::load(&dir).unwrap();
        assert!(a.css.url.starts_with("/assets/site.") && a.css.url.ends_with(".css"));
        assert_eq!(a.css.url.len(), "/assets/site.12345678.css".len());
        std::fs::write(dir.join("css/site.css"), "body{color:red}").unwrap();
        let b = Assets::load(&dir).unwrap();
        assert_ne!(a.css.url, b.css.url);
        assert_eq!(a.js.url, b.js.url);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_files_fail() {
        assert!(Assets::load("/nonexistent/dijmo-assets").is_err());
    }
}
