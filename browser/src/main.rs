//! Semantic Browser - CLI entrypoint
//!
//! A semantic browser that renders HTML/CSS/JS into Markdown.
//!
//! Usage:
//!   semantic-browser <url>

use anyhow::{Context, Result};
use std::env;
use std::process::ExitCode;
use std::rc::Rc;
use std::thread;

/// Extract domain from URL for cookie scoping
fn extract_domain(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| u.host_str().unwrap_or("").to_string())
        .unwrap_or_default()
}

fn main() -> ExitCode {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp(None)
        .init();

    // Parse arguments
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: semantic-browser <url>");
        eprintln!();
        eprintln!("A semantic browser that renders web pages as Markdown.");
        eprintln!();
        eprintln!("Example:");
        eprintln!("  semantic-browser https://example.com");
        return ExitCode::from(1);
    }

    let url = args[1].clone();

    // Run in a thread with larger stack size (64MB) to handle deeply recursive JS
    let handle = thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || run(&url))
        .expect("Failed to spawn thread");

    match handle.join() {
        Ok(Ok(markdown)) => {
            println!("{}", markdown);
            ExitCode::SUCCESS
        }
        Ok(Err(e)) => {
            eprintln!("Error: {:#}", e);
            ExitCode::from(1)
        }
        Err(_) => {
            eprintln!("Error: Thread panicked");
            ExitCode::from(1)
        }
    }
}

/// Main pipeline: fetch -> parse -> JS -> render
fn run(url: &str) -> Result<String> {
    log::info!("Fetching URL: {}", url);

    // Step 1: Fetch the URL
    let response = net::fetch(url).context("Failed to fetch URL")?;

    log::info!(
        "Received response: status={}, final_url={}",
        response.status,
        response.url
    );

    if response.status >= 400 {
        anyhow::bail!(
            "HTTP error: {} (status {})",
            response.url,
            response.status
        );
    }

    // Step 1.5: Sync cookies from response headers to JS cookie store
    js::set_current_url(&response.url);
    js::add_cookies_from_headers(&response.headers, &extract_domain(&response.url));

    // Step 2: Parse HTML into DOM
    log::info!("Parsing HTML...");
    let dom = Rc::new(dom::Dom::parse(&response.body_text).context("Failed to parse HTML")?);

    // Get scripts before JS execution
    let scripts = dom.get_scripts();

    log::info!(
        "Parsed DOM: title='{}', {} scripts",
        dom.get_title(),
        scripts.len()
    );

    // Step 3: Create StyleStore for CSSOM↔Layout integration
    let style_store = markdown::StyleStore::new();

    // Step 3.1: Initialize JS runtime with live DOM, URL, and StyleStore
    log::info!("Initializing JavaScript runtime...");
    let mut runtime = js::JsRuntime::new_with_url_and_styles(
        Rc::clone(&dom),
        &response.url,
        Some(style_store.clone())
    ).context("Failed to initialize JS runtime")?;

    // Step 3.5: Fire DOMContentLoaded event (HTML parsed, before scripts execute)
    log::debug!("Firing DOMContentLoaded event...");
    runtime.fire_dom_content_loaded();

    // Step 4: Execute scripts (both inline and external, non-fatal errors don't abort)
    // Limits to prevent OOM on sites with huge JS bundles
    const MAX_SCRIPTS: usize = 100;
    const MAX_DYNAMIC_SCRIPTS: usize = 50;
    const MAX_TOTAL_SCRIPT_BYTES: usize = 10 * 1024 * 1024; // 10MB total JS limit
    let mut dynamic_script_count = 0;
    let mut total_script_bytes = 0;

    for (i, script) in scripts.iter().enumerate() {
        // Limit total number of scripts to prevent runaway execution
        if i >= MAX_SCRIPTS {
            log::warn!("Reached script limit ({} scripts), skipping remaining", MAX_SCRIPTS);
            break;
        }

        // Determine script content - fetch if external
        let script_content = if let Some(src) = &script.src {
            // Skip polyfill scripts - they cause OOM in Boa parser (known issue)
            // Polyfills aren't essential for content extraction anyway
            if src.contains("polyfill") {
                log::debug!("Skipping polyfill script #{}: {}", i + 1, src);
                continue;
            }
            log::debug!("Fetching external script #{}: {}", i + 1, src);
            match js::fetch_script(src) {
                Ok(content) => {
                    // Skip known problematic polyfill by exact size (Next.js polyfill)
                    // This polyfill causes Boa to allocate 32GB due to parser bug
                    if content.len() == 112594 {
                        log::debug!(
                            "Skipping script #{} (matches problematic polyfill size): {}",
                            i + 1,
                            src
                        );
                        continue;
                    }
                    content
                }
                Err(e) => {
                    log::warn!("Failed to fetch script #{} ({}): {}", i + 1, src, e);
                    continue;
                }
            }
        } else {
            log::debug!("Executing inline script #{}", i + 1);
            script.content.clone()
        };

        if script_content.is_empty() {
            continue;
        }

        // Check total script bytes limit to prevent OOM from cumulative parsing
        total_script_bytes += script_content.len();
        if total_script_bytes > MAX_TOTAL_SCRIPT_BYTES {
            log::warn!(
                "Reached total script bytes limit ({} bytes), skipping remaining scripts",
                MAX_TOTAL_SCRIPT_BYTES
            );
            break;
        }

        // Log script size for debugging OOM issues
        log::debug!(
            "Script #{} size: {} bytes, total: {} bytes ({})",
            i + 1,
            script_content.len(),
            total_script_bytes,
            script.src.as_deref().unwrap_or("inline")
        );

        // Execute script with currentScript properly set
        let src_ref = script.src.as_deref();
        if let Some(error) = runtime.execute_safe_with_src(&script_content, src_ref) {
            log::warn!("Script #{} failed: {}", i + 1, error);
            // Continue with next script - non-fatal
        }

        // Execute any dynamically loaded scripts after each inline script (with limit)
        if dynamic_script_count < MAX_DYNAMIC_SCRIPTS {
            let dynamic_errors = runtime.execute_pending_scripts();
            dynamic_script_count += dynamic_errors.len();
            for err in dynamic_errors {
                log::warn!("Dynamic script failed: {}", err);
            }
        }
    }

    // Final pass: execute any remaining pending scripts (with iteration limit)
    const MAX_FINAL_PASSES: usize = 10;
    for pass in 0..MAX_FINAL_PASSES {
        let errors = runtime.execute_pending_scripts();
        if errors.is_empty() && !runtime.has_pending_scripts() {
            break;
        }
        for err in errors {
            log::warn!("Dynamic script failed: {}", err);
        }
        if pass == MAX_FINAL_PASSES - 1 {
            log::warn!("Reached final script pass limit, stopping execution");
        }
    }

    // Step 4.5: Fire load event (all resources loaded)
    log::debug!("Firing load event...");
    runtime.fire_load();

    // Log any console output
    let console_output = runtime.console_output();
    if !console_output.is_empty() {
        log::debug!("Console output:");
        for line in &console_output {
            log::debug!("  {}", line);
        }
    }

    // Step 5: Check if JS modified styles and needs relayout
    if style_store.needs_relayout() {
        log::debug!(
            "JS modified styles, triggering relayout for {} elements",
            style_store.take_dirty_elements().len()
        );
    }

    // Step 6: Render to Markdown using the Layout Engine with StyleStore
    log::info!("Rendering to Markdown...");

    // Use the full Layout Engine pipeline with StyleStore for CSSOM integration
    let markdown = markdown::render_with_style_store(&dom, 80, Some(&style_store));

    log::info!("Done! Generated {} bytes of Markdown", markdown.len());

    Ok(markdown)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_pipeline_with_simple_html() {
        // This would require mocking net::fetch
        // Components are tested individually in their crates
    }
}
