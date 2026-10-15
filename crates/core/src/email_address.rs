//! A normalised, plausibly deliverable email address.

use std::fmt;

/// An email address, trimmed and lower-cased. Its `Debug` and `Display`
/// output is masked (`j***@example.com`) so it never reaches logs in full.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct EmailAddress(String);

impl EmailAddress {
    /// Validates and normalises an address. Deliberately simple: one `@`, a
    /// sane local part, and a domain with a dot. The confirmation email is
    /// the real check.
    pub fn parse(raw: &str) -> Option<Self> {
        let email = raw.trim().to_lowercase();
        if email.is_empty() || email.len() > 254 {
            return None;
        }
        let (local, domain) = email.split_once('@')?;
        if local.is_empty() || local.len() > 64 || domain.contains('@') {
            return None;
        }
        let local_ok = local.chars().all(|c| {
            c.is_alphanumeric() || "!#$%&'*+-/=?^_`{|}~.".contains(c)
        }) && !local.starts_with('.')
            && !local.ends_with('.')
            && !local.contains("..");
        if !local_ok {
            return None;
        }
        let labels: Vec<&str> = domain.split('.').collect();
        if labels.len() < 2 || domain.len() > 253 {
            return None;
        }
        let domain_ok = labels.iter().all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.chars().all(|c| c.is_alphanumeric() || c == '-')
        });
        let tld = labels.last().copied().unwrap_or_default();
        if !domain_ok || tld.len() < 2 || tld.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        Some(Self(email))
    }

    /// The full address. Use only to store it or to send mail to it.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The masked form for logs, e.g. `j***@example.com`.
    pub fn masked(&self) -> String {
        mask(&self.0)
    }
}

/// Masks any address-like string for logs: the first character of the local
/// part, then `***`, then the domain.
pub fn mask(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) => {
            let first: String = local.chars().take(1).collect();
            format!("{first}***@{domain}")
        }
        None => "***".to_owned(),
    }
}

impl fmt::Debug for EmailAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EmailAddress({})", self.masked())
    }
}

impl fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.masked())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_and_normalises() {
        let e = EmailAddress::parse("  Jane.Doe+Dijmo@Example.COM ").unwrap();
        assert_eq!(e.as_str(), "jane.doe+dijmo@example.com");
        assert_eq!(e.masked(), "j***@example.com");
        assert_eq!(format!("{e:?}"), "EmailAddress(j***@example.com)");
        assert!(EmailAddress::parse("ünïcode@bücher.de").is_some());
    }

    #[test]
    fn rejects_garbage() {
        for bad in [
            "",
            "plain",
            "@example.com",
            "a@",
            "a@b",
            "a@@b.com",
            "a b@c.com",
            "a@b.c",
            "a@1.2",
            ".a@b.com",
            "a..b@c.com",
            "a@-b.com",
            "<a>@b.com",
            "a@b.com\r\nBcc: x@y.com",
        ] {
            assert!(EmailAddress::parse(bad).is_none(), "{bad:?} should be rejected");
        }
        let long = format!("{}@example.com", "a".repeat(65));
        assert!(EmailAddress::parse(&long).is_none());
    }
}
