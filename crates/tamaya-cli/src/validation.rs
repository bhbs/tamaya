use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Deserialize, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum PublishType {
    #[default]
    Static,
    Spa,
}

impl std::fmt::Display for PublishType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Static => f.write_str("static"),
            Self::Spa => f.write_str("spa"),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DomainIdentity {
    value: String,
}

impl DomainIdentity {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_domain(&value)?;
        Ok(Self { value })
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }

    #[allow(dead_code)]
    pub fn key(&self) -> String {
        let value = self
            .value
            .strip_prefix("http://")
            .map_or(self.value.as_str(), |host| {
                // Keep http://example.com distinct from example.com while
                // matching the worker-side domain_key shell helper.
                host
            });
        let prefix = if self.value.starts_with("http://") {
            "http_"
        } else {
            ""
        };
        let safe: String = value
            .bytes()
            .map(|b| match b {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'-' => b as char,
                _ => '-',
            })
            .collect();
        format!("{prefix}{safe}")
    }
}

pub fn validate_app_name(value: &str) -> Result<()> {
    crate::ssh::validate_name("app", value)
}

pub fn validate_domain(value: &str) -> Result<()> {
    let host = value.strip_prefix("http://").unwrap_or(value);
    if host.is_empty() || host.len() > 253 {
        bail!("domain contains unsupported characters: {value:?}");
    }
    for label in host.split('.') {
        if label.is_empty()
            || label.len() > 63
            || !label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || !label
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            || !label
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric)
        {
            bail!("domain contains an invalid hostname label: {value:?}");
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RouteKind {
    None,
    Root,
    Path,
}

impl RouteKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Root => "root",
            Self::Path => "path",
        }
    }
}

/// Resolved public route for deploy/publish.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ResolvedRoute {
    pub kind: RouteKind,
    /// Script/metadata path: empty when no domain, `"/"` for root, `/api` for path routes.
    pub path: String,
}

pub fn resolve_route(domain: Option<&str>, path: Option<&str>) -> Result<ResolvedRoute> {
    let domain = domain.filter(|d| !d.is_empty());
    let Some(_domain) = domain.filter(|d| !d.is_empty()) else {
        if path.is_some_and(|p| !p.is_empty()) {
            bail!("path deploys require domain; set domain in .tamaya.toml or pass --domain");
        }
        return Ok(ResolvedRoute {
            kind: RouteKind::None,
            path: String::new(),
        });
    };
    let path = path.map(str::trim).filter(|p| !p.is_empty());
    match path {
        None | Some("/") => Ok(ResolvedRoute {
            kind: RouteKind::Root,
            path: "/".into(),
        }),
        Some(value) => Ok(ResolvedRoute {
            kind: RouteKind::Path,
            path: normalize_path_prefix(value)?,
        }),
    }
}

pub fn normalize_path_prefix(value: &str) -> Result<String> {
    if value == "/" {
        bail!("path / is only valid as a root fallback route");
    }
    if !value.starts_with('/') {
        bail!("path must start with /");
    }
    if value.contains("//") || value.contains("..") || value.contains('?') || value.contains('#') {
        bail!("path contains unsupported characters: {value:?}");
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-' | b'~' | b'%'))
    {
        bail!("path contains unsupported characters: {value:?}");
    }
    Ok(value.trim_end_matches('/').to_owned())
}

#[allow(dead_code)]
pub fn path_prefix_matches(prefix: &str, request_path: &str) -> bool {
    request_path == prefix
        || request_path
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

pub fn resolve_project_relative(
    path: PathBuf,
    project_config_path: Option<&std::path::Path>,
) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    project_config_path
        .and_then(std::path::Path::parent)
        .map_or(path.clone(), |parent| parent.join(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_key_preserves_http_scheme_distinction() {
        let plain = DomainIdentity::parse("example.com").unwrap();
        let http = DomainIdentity::parse("http://example.com").unwrap();
        assert_eq!(plain.as_str(), "example.com");
        assert_eq!(http.as_str(), "http://example.com");
        assert_eq!(plain.key(), "example.com");
        assert_eq!(http.key(), "http_example.com");
        assert_ne!(plain.key(), http.key());
    }

    #[test]
    fn domains_require_valid_hostname_labels() {
        for domain in [
            "localhost",
            "example.com",
            "http://example.com",
            "a-b.example",
        ] {
            assert!(validate_domain(domain).is_ok(), "{domain}");
        }
        for domain in [
            "",
            ".",
            "..",
            ".example.com",
            "example.com.",
            "example..com",
            "-example.com",
            "example-.com",
        ] {
            assert!(validate_domain(domain).is_err(), "{domain}");
        }
    }

    #[test]
    fn resolve_route_kinds() {
        let none = resolve_route(None, None).unwrap();
        assert_eq!(none.kind, RouteKind::None);
        assert!(none.path.is_empty());

        let root = resolve_route(Some("example.com"), None).unwrap();
        assert_eq!(root.kind, RouteKind::Root);
        assert_eq!(root.path, "/");

        let root_slash = resolve_route(Some("example.com"), Some("/")).unwrap();
        assert_eq!(root_slash.kind, RouteKind::Root);
        assert_eq!(root_slash.path, "/");

        let path = resolve_route(Some("example.com"), Some("/api/")).unwrap();
        assert_eq!(path.kind, RouteKind::Path);
        assert_eq!(path.path, "/api");

        assert!(resolve_route(None, Some("/api")).is_err());
    }

    #[test]
    fn normalizes_path_prefixes() {
        assert!(normalize_path_prefix("/").is_err());
        assert_eq!(normalize_path_prefix("/api/").unwrap(), "/api");
        assert_eq!(normalize_path_prefix("/api").unwrap(), "/api");
        assert!(normalize_path_prefix("api").is_err());
        assert!(normalize_path_prefix("/api//v1").is_err());
        assert!(normalize_path_prefix("/../api").is_err());
        assert!(normalize_path_prefix("/api?x=1").is_err());
    }

    #[test]
    fn path_prefix_matching_uses_segment_boundaries() {
        assert!(path_prefix_matches("/api", "/api"));
        assert!(path_prefix_matches("/api", "/api/"));
        assert!(path_prefix_matches("/api", "/api/users"));
        assert!(!path_prefix_matches("/api", "/api-v2"));
        assert!(!path_prefix_matches("/api", "/"));
    }
}
