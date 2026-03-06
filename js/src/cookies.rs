//! Shared Cookie Store - Unified cookie management for document.cookie and HTTP requests
//!
//! This module provides a thread-safe cookie store that:
//! - Syncs between document.cookie and fetch() requests
//! - Parses Set-Cookie headers from HTTP responses
//! - Formats cookies for HTTP request headers

use std::collections::HashMap;
use std::sync::RwLock;

/// A parsed cookie with all its attributes
#[derive(Debug, Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub expires: Option<String>,
    pub max_age: Option<i64>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<String>,
}

impl Cookie {
    /// Parse a Set-Cookie header value
    pub fn parse(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split(';').collect();
        if parts.is_empty() {
            return None;
        }

        // First part is name=value
        let name_value: Vec<&str> = parts[0].splitn(2, '=').collect();
        if name_value.len() < 2 {
            return None;
        }

        let name = name_value[0].trim().to_string();
        let value = name_value[1].trim().to_string();

        let mut cookie = Cookie {
            name,
            value,
            domain: None,
            path: None,
            expires: None,
            max_age: None,
            secure: false,
            http_only: false,
            same_site: None,
        };

        // Parse attributes
        for part in parts.iter().skip(1) {
            let attr: Vec<&str> = part.splitn(2, '=').collect();
            let attr_name = attr[0].trim().to_lowercase();
            let attr_value = attr.get(1).map(|v| v.trim().to_string());

            match attr_name.as_str() {
                "domain" => cookie.domain = attr_value,
                "path" => cookie.path = attr_value,
                "expires" => cookie.expires = attr_value,
                "max-age" => cookie.max_age = attr_value.and_then(|v| v.parse().ok()),
                "secure" => cookie.secure = true,
                "httponly" => cookie.http_only = true,
                "samesite" => cookie.same_site = attr_value,
                _ => {}
            }
        }

        Some(cookie)
    }

    /// Format for document.cookie (name=value only, no HttpOnly cookies)
    pub fn to_document_string(&self) -> Option<String> {
        if self.http_only {
            None // HttpOnly cookies should not be visible to JS
        } else {
            Some(format!("{}={}", self.name, self.value))
        }
    }

    /// Format for Cookie header
    pub fn to_header_string(&self) -> String {
        format!("{}={}", self.name, self.value)
    }
}

/// Global cookie store
lazy_static::lazy_static! {
    static ref COOKIE_STORE: RwLock<CookieStore> = RwLock::new(CookieStore::new());
}

/// Cookie store that manages all cookies
#[derive(Debug, Default)]
pub struct CookieStore {
    /// Cookies keyed by domain -> path -> name
    cookies: HashMap<String, HashMap<String, HashMap<String, Cookie>>>,
    /// Current page URL for domain matching
    current_url: Option<String>,
}

impl CookieStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the current page URL for cookie scoping
    pub fn set_current_url(&mut self, url: &str) {
        self.current_url = Some(url.to_string());
    }

    /// Add or update a cookie
    pub fn set_cookie(&mut self, cookie: Cookie, domain: &str) {
        let domain_key = cookie.domain.clone().unwrap_or_else(|| domain.to_string());
        let path_key = cookie.path.clone().unwrap_or_else(|| "/".to_string());

        self.cookies
            .entry(domain_key)
            .or_insert_with(HashMap::new)
            .entry(path_key)
            .or_insert_with(HashMap::new)
            .insert(cookie.name.clone(), cookie);
    }

    /// Parse and add a Set-Cookie header
    pub fn add_from_header(&mut self, header: &str, domain: &str) {
        if let Some(cookie) = Cookie::parse(header) {
            self.set_cookie(cookie, domain);
        }
    }

    /// Parse and add a document.cookie assignment (e.g., "name=value; path=/")
    pub fn add_from_document(&mut self, cookie_str: &str, domain: &str) {
        if let Some(cookie) = Cookie::parse(cookie_str) {
            // Document cookies can't be HttpOnly
            let mut cookie = cookie;
            cookie.http_only = false;
            self.set_cookie(cookie, domain);
        }
    }

    /// Get all cookies for document.cookie (excludes HttpOnly)
    pub fn get_document_cookies(&self, domain: &str, path: &str) -> String {
        let mut result = Vec::new();

        for (cookie_domain, paths) in &self.cookies {
            // Check if domain matches (simplified matching)
            if domain.ends_with(cookie_domain) || cookie_domain == domain {
                for (cookie_path, cookies) in paths {
                    // Check if path matches
                    if path.starts_with(cookie_path) {
                        for cookie in cookies.values() {
                            if let Some(s) = cookie.to_document_string() {
                                result.push(s);
                            }
                        }
                    }
                }
            }
        }

        result.join("; ")
    }

    /// Get Cookie header value for HTTP requests
    pub fn get_cookie_header(&self, domain: &str, path: &str) -> Option<String> {
        let mut result = Vec::new();

        for (cookie_domain, paths) in &self.cookies {
            // Check if domain matches
            if domain.ends_with(cookie_domain) || cookie_domain == domain || cookie_domain.starts_with('.') && domain.ends_with(&cookie_domain[1..]) {
                for (cookie_path, cookies) in paths {
                    // Check if path matches
                    if path.starts_with(cookie_path) {
                        for cookie in cookies.values() {
                            result.push(cookie.to_header_string());
                        }
                    }
                }
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result.join("; "))
        }
    }

    /// Clear all cookies
    pub fn clear(&mut self) {
        self.cookies.clear();
    }
}

// ============ PUBLIC API FOR OTHER MODULES ============

/// Set the current URL (call after fetching a page)
pub fn set_current_url(url: &str) {
    if let Ok(mut store) = COOKIE_STORE.write() {
        store.set_current_url(url);
    }
}

/// Add cookies from Set-Cookie headers (call after HTTP response)
pub fn add_cookies_from_headers(headers: &HashMap<String, String>, domain: &str) {
    if let Ok(mut store) = COOKIE_STORE.write() {
        // Handle both "set-cookie" and multiple Set-Cookie headers
        if let Some(cookie_header) = headers.get("set-cookie") {
            // Multiple cookies might be comma-separated or in multiple headers
            for cookie_str in cookie_header.split(',') {
                store.add_from_header(cookie_str.trim(), domain);
            }
        }
    }
}

/// Add a cookie from document.cookie = "..." assignment
pub fn add_cookie_from_document(cookie_str: &str, domain: &str) {
    if let Ok(mut store) = COOKIE_STORE.write() {
        store.add_from_document(cookie_str, domain);
    }
}

/// Get cookies for document.cookie property
pub fn get_document_cookies(domain: &str, path: &str) -> String {
    if let Ok(store) = COOKIE_STORE.read() {
        store.get_document_cookies(domain, path)
    } else {
        String::new()
    }
}

/// Get Cookie header for HTTP requests
pub fn get_cookie_header(domain: &str, path: &str) -> Option<String> {
    if let Ok(store) = COOKIE_STORE.read() {
        store.get_cookie_header(domain, path)
    } else {
        None
    }
}

/// Clear all cookies
pub fn clear_cookies() {
    if let Ok(mut store) = COOKIE_STORE.write() {
        store.clear();
    }
}

/// Add a simple cookie with name, value, and domain
pub fn add_cookie(name: &str, value: &str, domain: &str) {
    let cookie_str = format!("{}={}", name, value);
    add_cookie_from_document(&cookie_str, domain);
}

/// Extract domain from URL
pub fn extract_domain(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| u.host_str().unwrap_or("").to_string())
        .unwrap_or_default()
}

/// Extract path from URL
pub fn extract_path(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| u.path().to_string())
        .unwrap_or_else(|_| "/".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cookie_parse() {
        let cookie = Cookie::parse("session=abc123; Path=/; HttpOnly; Secure").unwrap();
        assert_eq!(cookie.name, "session");
        assert_eq!(cookie.value, "abc123");
        assert_eq!(cookie.path, Some("/".to_string()));
        assert!(cookie.http_only);
        assert!(cookie.secure);
    }

    #[test]
    fn test_cookie_store() {
        let mut store = CookieStore::new();
        store.add_from_header("session=abc123; Path=/", "example.com");
        store.add_from_header("user=john; Path=/; HttpOnly", "example.com");

        let doc_cookies = store.get_document_cookies("example.com", "/");
        assert!(doc_cookies.contains("session=abc123"));
        assert!(!doc_cookies.contains("user=john")); // HttpOnly not visible

        let header = store.get_cookie_header("example.com", "/").unwrap();
        assert!(header.contains("session=abc123"));
        assert!(header.contains("user=john")); // HttpOnly sent in requests
    }
}
