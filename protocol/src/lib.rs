//! Browser Automation Protocol
//!
//! A protocol similar to Chrome DevTools Protocol (CDP) for browser automation.
//! Key difference: Each action returns the rendered Markdown page.
//!
//! ## Protocol Format
//!
//! Requests are JSON objects with the following structure:
//! ```json
//! {
//!   "id": 1,
//!   "method": "Page.navigate",
//!   "params": { "url": "https://example.com" }
//! }
//! ```
//!
//! Responses include the markdown-rendered page:
//! ```json
//! {
//!   "id": 1,
//!   "result": {
//!     "markdown": "# Example Domain\n...",
//!     "url": "https://example.com",
//!     "title": "Example Domain"
//!   }
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub mod server;
pub mod session;

// ============================================================================
// Protocol Types
// ============================================================================

/// Protocol error types
#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Method not found: {0}")]
    MethodNotFound(String),

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("Navigation failed: {0}")]
    NavigationFailed(String),

    #[error("Element not found: {0}")]
    ElementNotFound(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Session error: {0}")]
    SessionError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

/// JSON-RPC style request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// JSON-RPC style response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorResponse>,
}

/// Error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub code: i32,
    pub message: String,
}

/// Page result returned with every action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageResult {
    /// Rendered markdown content
    pub markdown: String,
    /// Current URL
    pub url: String,
    /// Page title
    pub title: String,
    /// Console output (if any)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub console: Vec<String>,
}

// ============================================================================
// Protocol Commands
// ============================================================================

/// Page domain - navigation and page lifecycle
pub mod page {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct NavigateParams {
        pub url: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct NavigateResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub status: u16,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReloadParams {
        #[serde(default)]
        pub ignore_cache: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetContentResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub html: String,
    }
}

/// DOM domain - element interaction
pub mod dom {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct QuerySelectorParams {
        pub selector: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct QuerySelectorResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub found: bool,
        pub node_id: Option<u64>,
        pub text: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ClickParams {
        pub selector: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ClickResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub clicked: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SetValueParams {
        pub selector: String,
        pub value: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SetValueResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub success: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetAttributeParams {
        pub selector: String,
        pub name: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetAttributeResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub value: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SetAttributeParams {
        pub selector: String,
        pub name: String,
        pub value: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetTextParams {
        pub selector: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetTextResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub text: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct QuerySelectorAllParams {
        pub selector: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct QuerySelectorAllResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub count: usize,
        pub node_ids: Vec<u64>,
        pub texts: Vec<String>,
    }
}

/// Input domain - keyboard and form input
pub mod input {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct TypeTextParams {
        pub selector: String,
        pub text: String,
        #[serde(default)]
        pub clear_first: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct TypeTextResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub success: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PressKeyParams {
        pub key: String,
        #[serde(default)]
        pub modifiers: Vec<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PressKeyResult {
        #[serde(flatten)]
        pub page: PageResult,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SelectOptionParams {
        pub selector: String,
        pub value: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SelectOptionResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub success: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CheckParams {
        pub selector: String,
        pub checked: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CheckResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub success: bool,
    }
}

/// Runtime domain - JavaScript execution
pub mod runtime {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EvaluateParams {
        pub expression: String,
        #[serde(default)]
        pub return_by_value: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EvaluateResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub result: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub exception: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CallFunctionParams {
        pub function_declaration: String,
        #[serde(default)]
        pub arguments: Vec<serde_json::Value>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CallFunctionResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub result: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub exception: Option<String>,
    }
}

/// Network domain - request/response handling
pub mod network {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SetCookieParams {
        pub name: String,
        pub value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub domain: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SetCookieResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub success: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetCookiesResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub cookies: Vec<Cookie>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Cookie {
        pub name: String,
        pub value: String,
        pub domain: String,
        pub path: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ClearCookiesResult {
        #[serde(flatten)]
        pub page: PageResult,
    }
}

/// Emulation domain - viewport and device emulation
pub mod emulation {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SetViewportParams {
        pub width: u32,
        pub height: u32,
        #[serde(default = "default_device_scale")]
        pub device_scale_factor: f64,
    }

    fn default_device_scale() -> f64 {
        1.0
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SetViewportResult {
        #[serde(flatten)]
        pub page: PageResult,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SetUserAgentParams {
        pub user_agent: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SetUserAgentResult {
        #[serde(flatten)]
        pub page: PageResult,
    }
}

/// Screenshot domain - page capture
pub mod screenshot {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CaptureParams {
        #[serde(default)]
        pub format: String, // "markdown" (default), "html", "text"
        #[serde(skip_serializing_if = "Option::is_none")]
        pub selector: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CaptureResult {
        #[serde(flatten)]
        pub page: PageResult,
        pub content: String,
        pub format: String,
    }
}

// ============================================================================
// Protocol Handler
// ============================================================================

impl Response {
    pub fn success(id: u64, result: serde_json::Value) -> Self {
        Response {
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: u64, code: i32, message: String) -> Self {
        Response {
            id,
            result: None,
            error: Some(ErrorResponse { code, message }),
        }
    }
}

// Error codes similar to JSON-RPC
pub const ERROR_PARSE: i32 = -32700;
pub const ERROR_INVALID_REQUEST: i32 = -32600;
pub const ERROR_METHOD_NOT_FOUND: i32 = -32601;
pub const ERROR_INVALID_PARAMS: i32 = -32602;
pub const ERROR_INTERNAL: i32 = -32603;
pub const ERROR_NAVIGATION_FAILED: i32 = -32000;
pub const ERROR_ELEMENT_NOT_FOUND: i32 = -32001;
pub const ERROR_EXECUTION_FAILED: i32 = -32002;

impl From<ProtocolError> for ErrorResponse {
    fn from(err: ProtocolError) -> Self {
        let (code, message) = match &err {
            ProtocolError::InvalidRequest(msg) => (ERROR_INVALID_REQUEST, msg.clone()),
            ProtocolError::MethodNotFound(msg) => (ERROR_METHOD_NOT_FOUND, msg.clone()),
            ProtocolError::InvalidParams(msg) => (ERROR_INVALID_PARAMS, msg.clone()),
            ProtocolError::NavigationFailed(msg) => (ERROR_NAVIGATION_FAILED, msg.clone()),
            ProtocolError::ElementNotFound(msg) => (ERROR_ELEMENT_NOT_FOUND, msg.clone()),
            ProtocolError::ExecutionFailed(msg) => (ERROR_EXECUTION_FAILED, msg.clone()),
            ProtocolError::SessionError(msg) => (ERROR_INTERNAL, msg.clone()),
            ProtocolError::InternalError(msg) => (ERROR_INTERNAL, msg.clone()),
        };
        ErrorResponse { code, message }
    }
}

/// Parse a JSON request string
pub fn parse_request(json: &str) -> Result<Request, ProtocolError> {
    serde_json::from_str(json)
        .map_err(|e| ProtocolError::InvalidRequest(format!("JSON parse error: {}", e)))
}

/// Serialize a response to JSON
pub fn serialize_response(response: &Response) -> String {
    serde_json::to_string(response).unwrap_or_else(|_| {
        r#"{"id":0,"error":{"code":-32603,"message":"Serialization failed"}}"#.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_request() {
        let json = r#"{"id":1,"method":"Page.navigate","params":{"url":"https://example.com"}}"#;
        let req = parse_request(json).unwrap();
        assert_eq!(req.id, 1);
        assert_eq!(req.method, "Page.navigate");
    }

    #[test]
    fn test_serialize_response() {
        let response = Response::success(1, serde_json::json!({"markdown": "# Test"}));
        let json = serialize_response(&response);
        assert!(json.contains("markdown"));
    }
}
