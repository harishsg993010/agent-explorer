//! Dynamic Script Loader - Fetches and executes dynamically added scripts
//!
//! When a <script> element with src attribute is added to the DOM,
//! this module fetches the script content and queues it for execution.

use std::collections::VecDeque;
use std::sync::RwLock;

use crate::cookies;

/// User-Agent string for script fetching
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// A script that has been fetched and is ready for execution
#[derive(Debug, Clone)]
pub struct PendingScript {
    pub url: String,
    pub content: String,
    pub is_module: bool,
    pub is_async: bool,
    pub is_defer: bool,
}

/// Global queue of pending scripts to execute
lazy_static::lazy_static! {
    static ref PENDING_SCRIPTS: RwLock<VecDeque<PendingScript>> = RwLock::new(VecDeque::new());
    static ref BASE_URL: RwLock<String> = RwLock::new(String::new());
}

/// Set the base URL for resolving relative script URLs
pub fn set_base_url(url: &str) {
    if let Ok(mut base) = BASE_URL.write() {
        *base = url.to_string();
    }
}

/// Get the base URL
pub fn get_base_url() -> String {
    BASE_URL.read().map(|b| b.clone()).unwrap_or_default()
}

/// Resolve a potentially relative URL against the base URL
pub fn resolve_url(src: &str) -> String {
    if src.starts_with("http://") || src.starts_with("https://") || src.starts_with("//") {
        if src.starts_with("//") {
            format!("https:{}", src)
        } else {
            src.to_string()
        }
    } else {
        // Relative URL - resolve against base
        let base = get_base_url();
        if let Ok(base_url) = url::Url::parse(&base) {
            if let Ok(resolved) = base_url.join(src) {
                return resolved.to_string();
            }
        }
        // Fallback: just return as-is
        src.to_string()
    }
}

/// Maximum script size in bytes (5MB) - prevents OOM on huge bundles
const MAX_SCRIPT_SIZE: u64 = 5 * 1024 * 1024;

/// Fetch a script from a URL
pub fn fetch_script(src: &str) -> Result<String, String> {
    let url = resolve_url(src);

    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(30))
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .build()
        .map_err(|e| e.to_string())?;

    let domain = cookies::extract_domain(&url);
    let path = cookies::extract_path(&url);

    let mut request = client.get(&url)
        .header("Accept", "*/*")
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Accept-Encoding", "gzip, deflate, br")
        .header("Sec-Ch-Ua", "\"Not_A Brand\";v=\"8\", \"Chromium\";v=\"120\", \"Google Chrome\";v=\"120\"")
        .header("Sec-Ch-Ua-Mobile", "?0")
        .header("Sec-Ch-Ua-Platform", "\"Windows\"")
        .header("Sec-Fetch-Dest", "script")
        .header("Sec-Fetch-Mode", "no-cors")
        .header("Sec-Fetch-Site", "same-origin");

    // Add cookies from shared store
    if let Some(cookie_header) = cookies::get_cookie_header(&domain, &path) {
        request = request.header("Cookie", cookie_header);
    }

    let response = request.send().map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    // Check content-length before downloading large scripts
    if let Some(content_length) = response.content_length() {
        if content_length > MAX_SCRIPT_SIZE {
            return Err(format!(
                "Script too large: {} bytes (max {} bytes)",
                content_length, MAX_SCRIPT_SIZE
            ));
        }
    }

    // Add Set-Cookie headers to cookie store
    for (name, value) in response.headers().iter() {
        if name.as_str().eq_ignore_ascii_case("set-cookie") {
            if let Ok(cookie_str) = value.to_str() {
                cookies::add_cookie_from_document(cookie_str, &domain);
            }
        }
    }

    let text = response.text().map_err(|e| e.to_string())?;

    // Double-check size after decompression (gzip can compress a lot)
    if text.len() > MAX_SCRIPT_SIZE as usize {
        return Err(format!(
            "Script too large after decompression: {} bytes (max {} bytes)",
            text.len(), MAX_SCRIPT_SIZE
        ));
    }

    Ok(text)
}

/// Queue a script for execution (called when appendChild adds a script with src)
pub fn queue_script(src: &str, is_module: bool, is_async: bool, is_defer: bool) {
    match fetch_script(src) {
        Ok(content) => {
            let script = PendingScript {
                url: resolve_url(src),
                content,
                is_module,
                is_async,
                is_defer,
            };
            if let Ok(mut queue) = PENDING_SCRIPTS.write() {
                queue.push_back(script);
            }
        }
        Err(e) => {
            log::warn!("Failed to fetch script {}: {}", src, e);
        }
    }
}

/// Get and remove the next pending script
pub fn pop_pending_script() -> Option<PendingScript> {
    PENDING_SCRIPTS.write().ok()?.pop_front()
}

/// Check if there are pending scripts
pub fn has_pending_scripts() -> bool {
    PENDING_SCRIPTS.read().map(|q| !q.is_empty()).unwrap_or(false)
}

/// Get all pending scripts (drains the queue)
pub fn drain_pending_scripts() -> Vec<PendingScript> {
    PENDING_SCRIPTS
        .write()
        .map(|mut q| q.drain(..).collect())
        .unwrap_or_default()
}

/// Clear all pending scripts
pub fn clear_pending_scripts() {
    if let Ok(mut queue) = PENDING_SCRIPTS.write() {
        queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_url() {
        set_base_url("https://example.com/page/index.html");

        assert_eq!(resolve_url("https://cdn.example.com/app.js"), "https://cdn.example.com/app.js");
        assert_eq!(resolve_url("//cdn.example.com/app.js"), "https://cdn.example.com/app.js");
        assert_eq!(resolve_url("/scripts/app.js"), "https://example.com/scripts/app.js");
        assert_eq!(resolve_url("./app.js"), "https://example.com/page/app.js");
        assert_eq!(resolve_url("../lib.js"), "https://example.com/lib.js");
    }
}
