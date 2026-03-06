//! Semantic Browser Protocol Server
//!
//! A browser automation server similar to Chrome DevTools Protocol.
//! Each action returns the rendered Markdown page.
//!
//! Usage:
//!   semantic-protocol [--port PORT] [--host HOST]
//!
//! Connect with netcat or any TCP client:
//!   nc localhost 9222
//!
//! Send JSON commands:
//!   {"id":1,"method":"Page.navigate","params":{"url":"https://example.com"}}

use std::env;

fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let mut config = protocol::server::ServerConfig::default();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                if i + 1 < args.len() {
                    config.port = args[i + 1].parse().unwrap_or(9222);
                    i += 1;
                }
            }
            "--host" | "-h" => {
                if i + 1 < args.len() {
                    config.host = args[i + 1].clone();
                    i += 1;
                }
            }
            "--help" => {
                print_help();
                return;
            }
            _ => {}
        }
        i += 1;
    }

    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║       Semantic Browser Protocol Server                      ║");
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║                                                             ║");
    println!("║  A CDP-like protocol for browser automation.                ║");
    println!("║  Each action returns the rendered Markdown page.            ║");
    println!("║                                                             ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();
    println!("Available methods:");
    println!("  Page.navigate      - Navigate to URL");
    println!("  Page.reload        - Reload current page");
    println!("  Page.getContent    - Get page HTML");
    println!("  DOM.querySelector  - Find element");
    println!("  DOM.querySelectorAll - Find all elements");
    println!("  DOM.click          - Click element");
    println!("  DOM.setValue       - Set input value");
    println!("  DOM.getAttribute   - Get attribute");
    println!("  DOM.setText        - Set text content");
    println!("  DOM.getText        - Get text content");
    println!("  Input.type         - Type text into input");
    println!("  Input.pressKey     - Press keyboard key");
    println!("  Input.selectOption - Select dropdown option");
    println!("  Input.check        - Check/uncheck checkbox");
    println!("  Runtime.evaluate   - Execute JavaScript");
    println!("  Network.setCookie  - Set cookie");
    println!("  Network.getCookies - Get all cookies");
    println!("  Network.clearCookies - Clear cookies");
    println!("  Emulation.setViewport - Set viewport size");
    println!("  Screenshot.capture - Capture page content");
    println!();
    println!("Example:");
    println!(r#"  {{"id":1,"method":"Page.navigate","params":{{"url":"https://example.com"}}}}"#);
    println!();

    // Start server
    if let Err(e) = protocol::server::start_server(config) {
        eprintln!("Server error: {}", e);
        std::process::exit(1);
    }
}

fn print_help() {
    println!("Semantic Browser Protocol Server");
    println!();
    println!("Usage: semantic-protocol [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -p, --port PORT    Port to listen on (default: 9222)");
    println!("  -h, --host HOST    Host to bind to (default: 127.0.0.1)");
    println!("      --help         Show this help message");
    println!();
    println!("Example:");
    println!("  semantic-protocol --port 9222");
    println!("  nc localhost 9222");
    println!(r#"  {{"id":1,"method":"Page.navigate","params":{{"url":"https://example.com"}}}}"#);
}
