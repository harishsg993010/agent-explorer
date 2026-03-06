//! Net crate - HTTP fetching for the semantic browser.
//!
//! Provides a simple interface for fetching web pages with proper
//! redirect handling, cookie persistence, and realistic browser headers.

use std::collections::HashMap;
use std::sync::Arc;
use reqwest::cookie::Jar;
use thiserror::Error;

/// User-Agent string mimicking Chrome 120
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// Global cookie jar for session persistence
lazy_static::lazy_static! {
    static ref COOKIE_JAR: Arc<Jar> = Arc::new(Jar::default());
}

/// Errors that can occur during HTTP operations
#[derive(Error, Debug)]
pub enum NetError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Failed to read response body: {0}")]
    BodyReadError(String),
}

/// Result type for net operations
pub type Result<T> = std::result::Result<T, NetError>;

/// HTTP response containing fetched page data
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// Final URL after any redirects
    pub url: String,
    /// HTTP status code
    pub status: u16,
    /// Response headers
    pub headers: HashMap<String, String>,
    /// Response body as text
    pub body_text: String,
}

/// Fetches a URL and returns the HTTP response.
///
/// # Arguments
/// * `url` - The URL to fetch
///
/// # Returns
/// * `Ok(HttpResponse)` - The fetched response
/// * `Err(NetError)` - If the fetch failed
///
/// # Features
/// - Automatically follows redirects (up to 10)
/// - Sets realistic browser headers (Chrome 120)
/// - Persists cookies across requests
/// - Handles HTTPS with modern TLS
pub fn fetch(url: &str) -> Result<HttpResponse> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .cookie_store(true)
        .cookie_provider(Arc::clone(&COOKIE_JAR))
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(30))
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .build()?;

    let response = client
        .get(url)
        // Standard browser headers
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Accept-Encoding", "gzip, deflate, br")
        .header("Cache-Control", "max-age=0")
        .header("Sec-Ch-Ua", "\"Not_A Brand\";v=\"8\", \"Chromium\";v=\"120\", \"Google Chrome\";v=\"120\"")
        .header("Sec-Ch-Ua-Mobile", "?0")
        .header("Sec-Ch-Ua-Platform", "\"Windows\"")
        .header("Sec-Fetch-Dest", "document")
        .header("Sec-Fetch-Mode", "navigate")
        .header("Sec-Fetch-Site", "none")
        .header("Sec-Fetch-User", "?1")
        .header("Upgrade-Insecure-Requests", "1")
        .send()?;

    let final_url = response.url().to_string();
    let status = response.status().as_u16();

    let headers: HashMap<String, String> = response
        .headers()
        .iter()
        .filter_map(|(k, v)| {
            match v.to_str() {
                Ok(val) => Some((k.as_str().to_string(), val.to_string())),
                Err(_) => {
                    // Header value contains non-ASCII bytes, use lossy conversion
                    let lossy_val = String::from_utf8_lossy(v.as_bytes()).to_string();
                    log::debug!(
                        "Header '{}' contains non-ASCII bytes, using lossy conversion: {:?}",
                        k.as_str(),
                        lossy_val
                    );
                    Some((k.as_str().to_string(), lossy_val))
                }
            }
        })
        .collect();

    let body_text = response
        .text()
        .map_err(|e| NetError::BodyReadError(e.to_string()))?;

    Ok(HttpResponse {
        url: final_url,
        status,
        headers,
        body_text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetch_example_com() {
        // This test requires network access
        let result = fetch("https://example.com");
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.status, 200);
        assert!(response.body_text.contains("Example Domain"));
    }

    #[test]
    fn test_invalid_url() {
        let result = fetch("not-a-valid-url");
        assert!(result.is_err());
    }
}
