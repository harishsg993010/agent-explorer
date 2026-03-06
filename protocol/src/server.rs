//! Protocol Server
//!
//! TCP server that accepts JSON protocol commands.

use crate::session::Session;
use crate::{parse_request, serialize_response, Response};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

/// Server configuration
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 9222,
        }
    }
}

/// Start the protocol server (blocking)
pub fn start_server(config: ServerConfig) -> std::io::Result<()> {
    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr)?;

    log::info!("Protocol server listening on {}", addr);
    println!("Semantic Browser Protocol Server");
    println!("Listening on {}", addr);
    println!("Send JSON commands, one per line");
    println!();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                // Spawn with large stack for JS execution
                thread::Builder::new()
                    .stack_size(64 * 1024 * 1024)
                    .spawn(move || {
                        if let Err(e) = handle_client(stream) {
                            log::error!("Client error: {}", e);
                        }
                    })
                    .expect("Failed to spawn client thread");
            }
            Err(e) => {
                log::error!("Connection error: {}", e);
            }
        }
    }

    Ok(())
}

/// Handle a single client connection
fn handle_client(stream: TcpStream) -> std::io::Result<()> {
    let peer_addr = stream.peer_addr()?;
    log::info!("Client connected: {}", peer_addr);

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;

    // Each client gets its own session
    let mut session = Session::new();

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {
                log::info!("Client disconnected: {}", peer_addr);
                break;
            }
            Ok(_) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                let response = match parse_request(line) {
                    Ok(request) => session.handle_request(&request),
                    Err(e) => Response::error(0, crate::ERROR_PARSE, e.to_string()),
                };

                let response_json = serialize_response(&response);
                writeln!(writer, "{}", response_json)?;
                writer.flush()?;
            }
            Err(e) => {
                log::error!("Read error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

/// Simple synchronous client for testing
pub struct Client {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    next_id: u64,
}

impl Client {
    /// Connect to the protocol server
    pub fn connect(host: &str, port: u16) -> std::io::Result<Self> {
        let stream = TcpStream::connect(format!("{}:{}", host, port))?;
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Client {
            stream,
            reader,
            next_id: 1,
        })
    }

    /// Send a command and receive the response
    pub fn send(&mut self, method: &str, params: serde_json::Value) -> std::io::Result<Response> {
        let request = crate::Request {
            id: self.next_id,
            method: method.to_string(),
            params,
        };
        self.next_id += 1;

        let request_json = serde_json::to_string(&request)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        writeln!(self.stream, "{}", request_json)?;
        self.stream.flush()?;

        let mut line = String::new();
        self.reader.read_line(&mut line)?;

        serde_json::from_str(&line)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Navigate to a URL
    pub fn navigate(&mut self, url: &str) -> std::io::Result<Response> {
        self.send("Page.navigate", serde_json::json!({ "url": url }))
    }

    /// Click an element
    pub fn click(&mut self, selector: &str) -> std::io::Result<Response> {
        self.send("DOM.click", serde_json::json!({ "selector": selector }))
    }

    /// Type text into an element
    pub fn type_text(&mut self, selector: &str, text: &str) -> std::io::Result<Response> {
        self.send("Input.type", serde_json::json!({
            "selector": selector,
            "text": text
        }))
    }

    /// Get element text
    pub fn get_text(&mut self, selector: &str) -> std::io::Result<Response> {
        self.send("DOM.getText", serde_json::json!({ "selector": selector }))
    }

    /// Execute JavaScript
    pub fn evaluate(&mut self, expression: &str) -> std::io::Result<Response> {
        self.send("Runtime.evaluate", serde_json::json!({
            "expression": expression
        }))
    }

    /// Get current page as markdown
    pub fn capture(&mut self) -> std::io::Result<Response> {
        self.send("Screenshot.capture", serde_json::json!({}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 9222);
    }
}
