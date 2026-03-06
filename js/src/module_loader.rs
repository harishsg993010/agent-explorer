//! ES Module Loader - Implements Boa's ModuleLoader trait for ES module support
//!
//! This module provides:
//! - Fetching ES modules from URLs (both absolute and relative)
//! - Resolving import specifiers against the referrer module's URL
//! - Support for `import.meta.url`
//! - Module caching to prevent re-fetching

use boa_engine::{
    js_string, module::ModuleLoader, module::Referrer, JsNativeError, JsResult, JsString,
    JsValue, Module, Source, Context,
};
use boa_engine::object::JsObject;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::RwLock;

use crate::cookies;
use crate::script_loader::resolve_url;

/// User-Agent string for module fetching
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// Maximum module size in bytes (5MB)
const MAX_MODULE_SIZE: u64 = 5 * 1024 * 1024;

lazy_static::lazy_static! {
    /// Cache for fetched module source code
    static ref MODULE_CACHE: RwLock<HashMap<String, String>> = RwLock::new(HashMap::new());
}

/// Clear the module cache
pub fn clear_module_cache() {
    if let Ok(mut cache) = MODULE_CACHE.write() {
        cache.clear();
    }
}

/// HTTP Module Loader for ES modules
///
/// Fetches modules from URLs and resolves relative imports.
pub struct HttpModuleLoader {
    /// Base URL for resolving relative module specifiers in the root module
    base_url: String,
}

impl HttpModuleLoader {
    /// Create a new HTTP module loader with the given base URL
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
        }
    }

    /// Resolve a module specifier against a referrer URL
    fn resolve_specifier(&self, specifier: &str, referrer: Option<&str>) -> String {
        // If it's already an absolute URL, return as-is
        if specifier.starts_with("http://") || specifier.starts_with("https://") {
            return specifier.to_string();
        }

        // Handle protocol-relative URLs
        if specifier.starts_with("//") {
            return format!("https:{}", specifier);
        }

        // Resolve relative to referrer or base URL
        let base = referrer.unwrap_or(&self.base_url);

        if let Ok(base_url) = url::Url::parse(base) {
            if let Ok(resolved) = base_url.join(specifier) {
                return resolved.to_string();
            }
        }

        // Fallback: use script_loader's resolve_url
        resolve_url(specifier)
    }

    /// Fetch a module from a URL (blocking)
    fn fetch_module_sync(url: &str) -> Result<String, String> {
        // Check cache first
        if let Ok(cache) = MODULE_CACHE.read() {
            if let Some(cached) = cache.get(url) {
                log::debug!("Module cache hit: {}", url);
                return Ok(cached.clone());
            }
        }

        log::debug!("Fetching ES module: {}", url);

        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .redirect(reqwest::redirect::Policy::limited(10))
            .timeout(std::time::Duration::from_secs(30))
            .gzip(true)
            .brotli(true)
            .deflate(true)
            .build()
            .map_err(|e| e.to_string())?;

        let domain = cookies::extract_domain(url);
        let path = cookies::extract_path(url);

        let mut request = client.get(url)
            .header("Accept", "application/javascript, text/javascript, */*")
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("Accept-Encoding", "gzip, deflate, br")
            .header("Sec-Ch-Ua", "\"Not_A Brand\";v=\"8\", \"Chromium\";v=\"120\", \"Google Chrome\";v=\"120\"")
            .header("Sec-Ch-Ua-Mobile", "?0")
            .header("Sec-Ch-Ua-Platform", "\"Windows\"")
            .header("Sec-Fetch-Dest", "script")
            .header("Sec-Fetch-Mode", "cors")
            .header("Sec-Fetch-Site", "same-origin");

        // Add cookies from shared store
        if let Some(cookie_header) = cookies::get_cookie_header(&domain, &path) {
            request = request.header("Cookie", cookie_header);
        }

        let response = request.send().map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            return Err(format!("HTTP error fetching module: {}", response.status()));
        }

        // Check content-length before downloading
        if let Some(content_length) = response.content_length() {
            if content_length > MAX_MODULE_SIZE {
                return Err(format!(
                    "Module too large: {} bytes (max {} bytes)",
                    content_length, MAX_MODULE_SIZE
                ));
            }
        }

        // Process Set-Cookie headers
        for (name, value) in response.headers().iter() {
            if name.as_str().eq_ignore_ascii_case("set-cookie") {
                if let Ok(cookie_str) = value.to_str() {
                    cookies::add_cookie_from_document(cookie_str, &domain);
                }
            }
        }

        let text = response.text().map_err(|e| e.to_string())?;

        // Check size after decompression
        if text.len() > MAX_MODULE_SIZE as usize {
            return Err(format!(
                "Module too large after decompression: {} bytes (max {} bytes)",
                text.len(), MAX_MODULE_SIZE
            ));
        }

        // Cache the result
        if let Ok(mut cache) = MODULE_CACHE.write() {
            cache.insert(url.to_string(), text.clone());
        }

        Ok(text)
    }
}

impl ModuleLoader for HttpModuleLoader {
    async fn load_imported_module(
        self: Rc<Self>,
        referrer: Referrer,
        specifier: JsString,
        context: &RefCell<&mut Context>,
    ) -> JsResult<Module> {
        let specifier_str = specifier.to_std_string_escaped();

        // Get the referrer URL for resolution
        let referrer_url: Option<&str> = match &referrer {
            Referrer::Module(_module) => {
                // In a full implementation, we'd track module URLs
                // For now, use base URL
                None
            }
            Referrer::Realm(_) | Referrer::Script(_) => None,
        };

        // Resolve the module specifier to a full URL
        let module_url = self.resolve_specifier(&specifier_str, referrer_url);

        log::debug!("Loading ES module: {} (specifier: {})", module_url, specifier_str);

        // Fetch the module synchronously (we're in an async context but using blocking HTTP)
        let source_text = Self::fetch_module_sync(&module_url)
            .map_err(|e| {
                JsNativeError::typ()
                    .with_message(format!("Failed to fetch module {}: {}", module_url, e))
            })?;

        let source = Source::from_bytes(&source_text);

        // Parse and return the module
        Module::parse(source, None, &mut context.borrow_mut())
            .map_err(|e| {
                JsNativeError::syntax()
                    .with_message(format!("Failed to parse module {}: {}", module_url, e))
                    .into()
            })
    }

    fn init_import_meta(
        self: Rc<Self>,
        import_meta: &JsObject,
        _module: &Module,
        context: &mut Context,
    ) {
        // Set import.meta.url to the module's URL
        // For now, we use the base URL since we don't have per-module URL tracking yet
        let url = self.base_url.clone();

        if let Err(e) = import_meta.set(
            js_string!("url"),
            JsValue::from(js_string!(url.clone())),
            false,
            context,
        ) {
            log::warn!("Failed to set import.meta.url: {}", e);
        }

        // Set import.meta.resolve function
        // Use from_closure with unsafe since we need to capture a String
        let base_url = url;
        let resolve_fn = unsafe {
            boa_engine::object::FunctionObjectBuilder::new(
                context.realm(),
                boa_engine::native_function::NativeFunction::from_closure(move |_this, args, ctx| {
                    let specifier = args
                        .get(0)
                        .cloned()
                        .unwrap_or(JsValue::undefined())
                        .to_string(ctx)?
                        .to_std_string_escaped();

                    // Resolve relative to the module's URL
                    let resolved = if specifier.starts_with("http://") || specifier.starts_with("https://") {
                        specifier
                    } else if specifier.starts_with("//") {
                        format!("https:{}", specifier)
                    } else if let Ok(base) = url::Url::parse(&base_url) {
                        base.join(&specifier)
                            .map(|u| u.to_string())
                            .unwrap_or(specifier)
                    } else {
                        specifier
                    };

                    Ok(JsValue::from(js_string!(resolved)))
                }),
            )
            .name(js_string!("resolve"))
            .length(1)
            .build()
        };

        if let Err(e) = import_meta.set(
            js_string!("resolve"),
            JsValue::from(resolve_fn),
            false,
            context,
        ) {
            log::warn!("Failed to set import.meta.resolve: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_specifier() {
        let loader = HttpModuleLoader::new("https://example.com/app/main.js");

        // Absolute URLs
        assert_eq!(
            loader.resolve_specifier("https://cdn.example.com/lib.js", None),
            "https://cdn.example.com/lib.js"
        );

        // Protocol-relative
        assert_eq!(
            loader.resolve_specifier("//cdn.example.com/lib.js", None),
            "https://cdn.example.com/lib.js"
        );

        // Relative URLs
        assert_eq!(
            loader.resolve_specifier("./utils.js", Some("https://example.com/app/main.js")),
            "https://example.com/app/utils.js"
        );

        assert_eq!(
            loader.resolve_specifier("../lib.js", Some("https://example.com/app/main.js")),
            "https://example.com/lib.js"
        );

        assert_eq!(
            loader.resolve_specifier("/vendor/lodash.js", Some("https://example.com/app/main.js")),
            "https://example.com/vendor/lodash.js"
        );
    }
}
