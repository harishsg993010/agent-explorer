//! Browser Session
//!
//! Manages browser state and handles protocol commands.

use crate::*;
use ::dom::Dom;
use std::rc::Rc;

/// Browser session state
pub struct Session {
    /// Current URL
    url: Option<String>,
    /// Current DOM
    dom: Option<Rc<Dom>>,
    /// JavaScript runtime
    runtime: Option<js::JsRuntime>,
    /// Style store for CSSOM
    style_store: Option<markdown::StyleStore>,
    /// Viewport width for markdown rendering
    viewport_width: usize,
    /// Console output
    console_output: Vec<String>,
    /// Page title
    title: String,
}

impl Session {
    /// Create a new browser session
    pub fn new() -> Self {
        Session {
            url: None,
            dom: None,
            runtime: None,
            style_store: None,
            viewport_width: 80,
            console_output: Vec::new(),
            title: String::new(),
        }
    }

    /// Handle a protocol request
    pub fn handle_request(&mut self, request: &Request) -> Response {
        let result = self.dispatch(&request.method, &request.params);

        match result {
            Ok(value) => Response::success(request.id, value),
            Err(err) => {
                let error: ErrorResponse = err.into();
                Response::error(request.id, error.code, error.message)
            }
        }
    }

    /// Dispatch a method call
    fn dispatch(
        &mut self,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, ProtocolError> {
        match method {
            // Page domain
            "Page.navigate" => self.page_navigate(params),
            "Page.reload" => self.page_reload(params),
            "Page.getContent" => self.page_get_content(params),

            // DOM domain
            "DOM.querySelector" => self.dom_query_selector(params),
            "DOM.querySelectorAll" => self.dom_query_selector_all(params),
            "DOM.click" => self.dom_click(params),
            "DOM.setValue" => self.dom_set_value(params),
            "DOM.getAttribute" => self.dom_get_attribute(params),
            "DOM.setAttribute" => self.dom_set_attribute(params),
            "DOM.getText" => self.dom_get_text(params),

            // Input domain
            "Input.type" => self.input_type(params),
            "Input.pressKey" => self.input_press_key(params),
            "Input.selectOption" => self.input_select_option(params),
            "Input.check" => self.input_check(params),

            // Runtime domain
            "Runtime.evaluate" => self.runtime_evaluate(params),
            "Runtime.callFunction" => self.runtime_call_function(params),

            // Network domain
            "Network.setCookie" => self.network_set_cookie(params),
            "Network.getCookies" => self.network_get_cookies(params),
            "Network.clearCookies" => self.network_clear_cookies(params),

            // Emulation domain
            "Emulation.setViewport" => self.emulation_set_viewport(params),

            // Screenshot domain
            "Screenshot.capture" => self.screenshot_capture(params),

            _ => Err(ProtocolError::MethodNotFound(method.to_string())),
        }
    }

    /// Get current page result (markdown + metadata)
    fn get_page_result(&self) -> PageResult {
        let markdown = self.render_markdown();
        PageResult {
            markdown,
            url: self.url.clone().unwrap_or_default(),
            title: self.title.clone(),
            console: self.console_output.clone(),
        }
    }

    /// Render current page to markdown
    fn render_markdown(&self) -> String {
        if let Some(dom) = &self.dom {
            markdown::render_with_style_store(dom, self.viewport_width, self.style_store.as_ref())
        } else {
            String::new()
        }
    }

    /// Extract domain from URL
    fn extract_domain(url: &str) -> String {
        url::Url::parse(url)
            .map(|u| u.host_str().unwrap_or("").to_string())
            .unwrap_or_default()
    }

    // ========================================================================
    // Page Domain
    // ========================================================================

    fn page_navigate(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let nav_params: page::NavigateParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        // Navigate directly (server threads already have large stacks)
        let result = navigate_internal(&nav_params.url, self.viewport_width);

        match result {
            Ok((dom, runtime, style_store, title, console, status)) => {
                self.url = Some(nav_params.url);
                self.dom = Some(dom);
                self.runtime = Some(runtime);
                self.style_store = Some(style_store);
                self.title = title;
                self.console_output = console;

                let result = page::NavigateResult {
                    page: self.get_page_result(),
                    status,
                };
                serde_json::to_value(result)
                    .map_err(|e| ProtocolError::InternalError(e.to_string()))
            }
            Err(e) => Err(ProtocolError::NavigationFailed(e)),
        }
    }

    fn page_reload(&mut self, _params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        if let Some(url) = self.url.clone() {
            let nav_params = serde_json::json!({"url": url});
            self.page_navigate(&nav_params)
        } else {
            Err(ProtocolError::NavigationFailed("No page loaded".to_string()))
        }
    }

    fn page_get_content(&mut self, _params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let html = if let Some(dom) = &self.dom {
            dom.serialize_to_html()
        } else {
            String::new()
        };

        let result = page::GetContentResult {
            page: self.get_page_result(),
            html,
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    // ========================================================================
    // DOM Domain
    // ========================================================================

    fn dom_query_selector(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let qs_params: dom::QuerySelectorParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        let (found, node_id, text) = if let Some(dom) = &self.dom {
            if let Some(elem) = dom.query_selector(&qs_params.selector) {
                (true, Some(elem.unique_id()), Some(elem.inner_text()))
            } else {
                (false, None, None)
            }
        } else {
            (false, None, None)
        };

        let result = dom::QuerySelectorResult {
            page: self.get_page_result(),
            found,
            node_id,
            text,
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    fn dom_query_selector_all(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let qs_params: dom::QuerySelectorAllParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        let (count, node_ids, texts) = if let Some(dom) = &self.dom {
            let elements = dom.query_selector_all(&qs_params.selector);
            let count = elements.len();
            let node_ids: Vec<u64> = elements.iter().map(|e| e.unique_id()).collect();
            let texts: Vec<String> = elements.iter().map(|e| e.inner_text()).collect();
            (count, node_ids, texts)
        } else {
            (0, vec![], vec![])
        };

        let result = dom::QuerySelectorAllResult {
            page: self.get_page_result(),
            count,
            node_ids,
            texts,
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    fn dom_click(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let click_params: dom::ClickParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        // First, check if the element is an anchor with href
        let href_to_follow: Option<String> = if let Some(dom) = &self.dom {
            if let Some(elem) = dom.query_selector(&click_params.selector) {
                // Check if it's an anchor or find parent anchor
                let tag = elem.tag_name().to_lowercase();
                if tag == "a" {
                    elem.get_attribute("href")
                } else {
                    // Check for parent anchor
                    dom.query_selector(&format!("{} a, a {}", &click_params.selector, &click_params.selector))
                        .or_else(|| {
                            // Try to find if selector matches something inside an anchor
                            dom.query_selector(&format!("a:has({})", &click_params.selector))
                        })
                        .and_then(|a| a.get_attribute("href"))
                }
            } else {
                None
            }
        } else {
            None
        };

        let clicked = if let Some(dom) = &self.dom {
            if let Some(_elem) = dom.query_selector(&click_params.selector) {
                // Simulate click by firing click event via JS
                if let Some(runtime) = &mut self.runtime {
                    let js = format!(
                        r#"(function() {{
                            var el = document.querySelector('{}');
                            if (el) {{
                                var event = new MouseEvent('click', {{ bubbles: true, cancelable: true }});
                                el.dispatchEvent(event);
                                // Check if onclick property is a function, call it
                                if (typeof el.onclick === 'function') {{
                                    el.onclick(event);
                                }} else {{
                                    // Fallback: check for onclick attribute string and evaluate it
                                    var onclickAttr = el.getAttribute('onclick');
                                    if (onclickAttr) {{
                                        // Execute the onclick attribute code in the context of the element
                                        (new Function('event', onclickAttr)).call(el, event);
                                    }}
                                }}
                                return true;
                            }}
                            return false;
                        }})()"#,
                        click_params.selector.replace('\'', "\\'")
                    );
                    runtime.execute_safe(&js);
                    self.console_output = runtime.console_output().clone();
                }
                true
            } else {
                false
            }
        } else {
            false
        };

        // If we have an href to follow, navigate to it
        if let Some(href) = href_to_follow {
            if !href.is_empty() && !href.starts_with('#') && !href.starts_with("javascript:") {
                // Resolve relative URLs
                let full_url = if href.starts_with("http://") || href.starts_with("https://") {
                    href
                } else if let Some(base_url) = &self.url {
                    if let Ok(base) = url::Url::parse(base_url) {
                        base.join(&href).map(|u| u.to_string()).unwrap_or(href)
                    } else {
                        href
                    }
                } else {
                    href
                };

                // Navigate to the URL
                let nav_params = serde_json::json!({"url": full_url});
                return self.page_navigate(&nav_params);
            }
        }

        let result = dom::ClickResult {
            page: self.get_page_result(),
            clicked,
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    fn dom_set_value(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let sv_params: dom::SetValueParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        let success = if let Some(dom) = &self.dom {
            if let Some(elem) = dom.query_selector(&sv_params.selector) {
                elem.set_attribute("value", &sv_params.value);
                // Also update via JS for reactive frameworks
                if let Some(runtime) = &mut self.runtime {
                    let js = format!(
                        r#"(function() {{
                            var el = document.querySelector('{}');
                            if (el) {{
                                el.value = '{}';
                                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                                el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                            }}
                        }})()"#,
                        sv_params.selector.replace('\'', "\\'"),
                        sv_params.value.replace('\'', "\\'")
                    );
                    runtime.execute_safe(&js);
                    self.console_output = runtime.console_output().clone();
                }
                true
            } else {
                false
            }
        } else {
            false
        };

        let result = dom::SetValueResult {
            page: self.get_page_result(),
            success,
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    fn dom_get_attribute(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let ga_params: dom::GetAttributeParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        let value = if let Some(dom) = &self.dom {
            if let Some(elem) = dom.query_selector(&ga_params.selector) {
                elem.get_attribute(&ga_params.name)
            } else {
                None
            }
        } else {
            None
        };

        let result = dom::GetAttributeResult {
            page: self.get_page_result(),
            value,
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    fn dom_set_attribute(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let sa_params: dom::SetAttributeParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        if let Some(dom) = &self.dom {
            if let Some(elem) = dom.query_selector(&sa_params.selector) {
                elem.set_attribute(&sa_params.name, &sa_params.value);
            }
        }

        let result = dom::SetValueResult {
            page: self.get_page_result(),
            success: true,
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    fn dom_get_text(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let gt_params: dom::GetTextParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        let text = if let Some(dom) = &self.dom {
            if let Some(elem) = dom.query_selector(&gt_params.selector) {
                Some(elem.inner_text())
            } else {
                None
            }
        } else {
            None
        };

        let result = dom::GetTextResult {
            page: self.get_page_result(),
            text,
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    // ========================================================================
    // Input Domain
    // ========================================================================

    fn input_type(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let type_params: input::TypeTextParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        // Use DOM.setValue internally
        let sv_params = serde_json::json!({
            "selector": type_params.selector,
            "value": type_params.text
        });
        self.dom_set_value(&sv_params)
    }

    fn input_press_key(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let key_params: input::PressKeyParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        // Dispatch keyboard event via JS
        if let Some(runtime) = &mut self.runtime {
            let js = format!(
                r#"(function() {{
                    var event = new KeyboardEvent('keydown', {{
                        key: '{}',
                        bubbles: true,
                        cancelable: true
                    }});
                    document.activeElement.dispatchEvent(event);
                }})()"#,
                key_params.key.replace('\'', "\\'")
            );
            runtime.execute_safe(&js);
            self.console_output = runtime.console_output().clone();
        }

        let result = input::PressKeyResult {
            page: self.get_page_result(),
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    fn input_select_option(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let select_params: input::SelectOptionParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        let success = if let Some(runtime) = &mut self.runtime {
            let js = format!(
                r#"(function() {{
                    var el = document.querySelector('{}');
                    if (el && el.tagName === 'SELECT') {{
                        el.value = '{}';
                        el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                        return true;
                    }}
                    return false;
                }})()"#,
                select_params.selector.replace('\'', "\\'"),
                select_params.value.replace('\'', "\\'")
            );
            runtime.execute_safe(&js);
            self.console_output = runtime.console_output().clone();
            true
        } else {
            false
        };

        let result = input::SelectOptionResult {
            page: self.get_page_result(),
            success,
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    fn input_check(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let check_params: input::CheckParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        let success = if let Some(runtime) = &mut self.runtime {
            let js = format!(
                r#"(function() {{
                    var el = document.querySelector('{}');
                    if (el && (el.type === 'checkbox' || el.type === 'radio')) {{
                        el.checked = {};
                        el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                        return true;
                    }}
                    return false;
                }})()"#,
                check_params.selector.replace('\'', "\\'"),
                check_params.checked
            );
            runtime.execute_safe(&js);
            self.console_output = runtime.console_output().clone();
            true
        } else {
            false
        };

        let result = input::CheckResult {
            page: self.get_page_result(),
            success,
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    // ========================================================================
    // Runtime Domain
    // ========================================================================

    fn runtime_evaluate(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let eval_params: runtime::EvaluateParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        let (result_value, exception) = if let Some(runtime) = &mut self.runtime {
            match runtime.evaluate_to_string(&eval_params.expression) {
                Ok(value) => {
                    self.console_output = runtime.console_output().clone();
                    // Return the actual evaluated value as a JSON string
                    (serde_json::Value::String(value), None)
                }
                Err(e) => (serde_json::Value::Null, Some(e.to_string())),
            }
        } else {
            (serde_json::Value::Null, Some("No runtime available".to_string()))
        };

        let result = runtime::EvaluateResult {
            page: self.get_page_result(),
            result: result_value,
            exception,
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    fn runtime_call_function(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let call_params: runtime::CallFunctionParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        // Wrap function declaration and call it
        let args_json = serde_json::to_string(&call_params.arguments).unwrap_or("[]".to_string());
        let js = format!(
            "({}).apply(null, {})",
            call_params.function_declaration,
            args_json
        );

        let eval_params = serde_json::json!({
            "expression": js,
            "return_by_value": true
        });
        self.runtime_evaluate(&eval_params)
    }

    // ========================================================================
    // Network Domain
    // ========================================================================

    fn network_set_cookie(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let cookie_params: network::SetCookieParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        let domain = cookie_params.domain.unwrap_or_else(|| {
            self.url.as_ref()
                .and_then(|u| url::Url::parse(u).ok())
                .and_then(|u| u.host_str().map(String::from))
                .unwrap_or_default()
        });

        js::add_cookie(&cookie_params.name, &cookie_params.value, &domain);

        let result = network::SetCookieResult {
            page: self.get_page_result(),
            success: true,
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    fn network_get_cookies(&mut self, _params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let domain = self.url.as_ref()
            .and_then(|u| url::Url::parse(u).ok())
            .and_then(|u| u.host_str().map(String::from))
            .unwrap_or_default();
        let cookies_str = js::get_document_cookies(&domain, "/");
        let cookies: Vec<network::Cookie> = cookies_str
            .split(';')
            .filter_map(|s| {
                let parts: Vec<&str> = s.trim().splitn(2, '=').collect();
                if parts.len() == 2 {
                    Some(network::Cookie {
                        name: parts[0].to_string(),
                        value: parts[1].to_string(),
                        domain: self.url.as_ref()
                            .and_then(|u| url::Url::parse(u).ok())
                            .and_then(|u| u.host_str().map(String::from))
                            .unwrap_or_default(),
                        path: "/".to_string(),
                    })
                } else {
                    None
                }
            })
            .collect();

        let result = network::GetCookiesResult {
            page: self.get_page_result(),
            cookies,
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    fn network_clear_cookies(&mut self, _params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        js::clear_cookies();

        let result = network::ClearCookiesResult {
            page: self.get_page_result(),
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    // ========================================================================
    // Emulation Domain
    // ========================================================================

    fn emulation_set_viewport(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let vp_params: emulation::SetViewportParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        self.viewport_width = vp_params.width as usize;

        let result = emulation::SetViewportResult {
            page: self.get_page_result(),
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }

    // ========================================================================
    // Screenshot Domain
    // ========================================================================

    fn screenshot_capture(&mut self, params: &serde_json::Value) -> Result<serde_json::Value, ProtocolError> {
        let cap_params: screenshot::CaptureParams = serde_json::from_value(params.clone())
            .map_err(|e| ProtocolError::InvalidParams(e.to_string()))?;

        let format = if cap_params.format.is_empty() {
            "markdown".to_string()
        } else {
            cap_params.format
        };

        let content = match format.as_str() {
            "html" => {
                if let Some(dom) = &self.dom {
                    dom.serialize_to_html()
                } else {
                    String::new()
                }
            }
            "text" => {
                if let Some(dom) = &self.dom {
                    if let Some(body) = dom.query_selector("body") {
                        body.inner_text()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            }
            _ => self.render_markdown(),
        };

        let result = screenshot::CaptureResult {
            page: self.get_page_result(),
            content,
            format,
        };
        serde_json::to_value(result)
            .map_err(|e| ProtocolError::InternalError(e.to_string()))
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Internal Navigation (runs in separate thread)
// ============================================================================

fn navigate_internal(
    url: &str,
    viewport_width: usize,
) -> Result<(Rc<Dom>, js::JsRuntime, markdown::StyleStore, String, Vec<String>, u16), String> {
    // Fetch the URL
    let response = net::fetch(url).map_err(|e| e.to_string())?;

    if response.status >= 400 {
        return Err(format!("HTTP error: {} (status {})", response.url, response.status));
    }

    // Set up JS environment
    js::set_current_url(&response.url);
    let domain = url::Url::parse(&response.url)
        .map(|u| u.host_str().unwrap_or("").to_string())
        .unwrap_or_default();
    js::add_cookies_from_headers(&response.headers, &domain);

    // Set base URL for resolving relative script URLs
    js::set_base_url(&response.url);

    // Parse DOM
    let dom = Rc::new(Dom::parse(&response.body_text).map_err(|e| e.to_string())?);
    let title = dom.get_title();

    // Get scripts
    let scripts = dom.get_scripts();

    // Create style store
    let style_store = markdown::StyleStore::new();

    // Create JS runtime
    let mut runtime = js::JsRuntime::new_with_url_and_styles(
        Rc::clone(&dom),
        &response.url,
        Some(style_store.clone()),
    )
    .map_err(|e| e.to_string())?;

    // Fire DOMContentLoaded
    runtime.fire_dom_content_loaded();

    // Execute scripts (with limits)
    const MAX_SCRIPTS: usize = 100;
    const MAX_TOTAL_BYTES: usize = 10 * 1024 * 1024;
    let mut total_bytes = 0;

    for (i, script) in scripts.iter().enumerate() {
        if i >= MAX_SCRIPTS {
            break;
        }

        let script_src = script.src.clone();
        let script_content = if let Some(src) = &script.src {
            if src.contains("polyfill") {
                log::debug!("Skipping polyfill: {}", src);
                continue;
            }
            log::debug!("Fetching script: {}", src);
            match js::fetch_script(src) {
                Ok(content) => {
                    log::debug!("Fetched {} bytes from {}", content.len(), src);
                    content
                }
                Err(e) => {
                    log::warn!("Failed to fetch script {}: {}", src, e);
                    continue;
                }
            }
        } else {
            script.content.clone()
        };

        if script_content.is_empty() {
            continue;
        }

        total_bytes += script_content.len();
        if total_bytes > MAX_TOTAL_BYTES {
            log::warn!("Script limit reached at {} bytes", total_bytes);
            break;
        }

        if let Some(err) = runtime.execute_safe(&script_content) {
            log::warn!("Script error ({}): {}", script_src.as_deref().unwrap_or("inline"), err);
        }
        let _ = runtime.execute_pending_scripts();
    }

    // Fire load event
    runtime.fire_load();

    // Run event loop to process any scheduled tasks (timers, MessageChannel, etc.)
    // This is critical for React and other frameworks that use async scheduling
    let mut iterations = 0;
    while runtime.has_pending_tasks() && iterations < 100 {
        runtime.run_event_loop_tick();
        iterations += 1;
    }
    log::debug!("Event loop completed after {} iterations", iterations);

    let console = runtime.console_output().clone();

    Ok((dom, runtime, style_store, title, console, response.status))
}
