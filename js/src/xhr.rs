//! XMLHttpRequest - Full implementation with real HTTP requests
//!
//! Implements the XMLHttpRequest API with actual HTTP functionality
//! using reqwest for network operations.

use boa_engine::{
    js_string, native_function::NativeFunction, object::{ObjectInitializer, FunctionObjectBuilder},
    property::Attribute, Context, JsArgs, JsNativeError, JsObject, JsResult, JsValue,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;

/// XHR ready states
pub const UNSENT: u16 = 0;
pub const OPENED: u16 = 1;
pub const HEADERS_RECEIVED: u16 = 2;
pub const LOADING: u16 = 3;
pub const DONE: u16 = 4;

/// User agent for requests
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// XHR connection state (thread-safe, no JsObject)
struct XhrState {
    ready_state: u16,
    method: String,
    url: String,
    async_flag: bool,
    request_headers: HashMap<String, String>,
    // Response data
    status: u16,
    status_text: String,
    response_headers: HashMap<String, String>,
    response_text: String,
    response_url: String,
    response_type: String,
    timeout: u32,
    with_credentials: bool,
}

lazy_static::lazy_static! {
    static ref XHR_STATES: Mutex<HashMap<u32, XhrState>> = Mutex::new(HashMap::new());
    static ref NEXT_XHR_ID: Mutex<u32> = Mutex::new(1);
}

/// Thread-local storage for XHR JS objects (not Send/Sync)
thread_local! {
    static XHR_JS_OBJECTS: RefCell<HashMap<u32, JsObject>> = RefCell::new(HashMap::new());
}

/// Register XMLHttpRequest and XMLHttpRequestEventTarget
pub fn register_xmlhttprequest(context: &mut Context) -> JsResult<()> {
    // XMLHttpRequest constructor
    let xhr_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let xhr_id = {
            let mut id = NEXT_XHR_ID.lock().unwrap();
            let current = *id;
            *id += 1;
            current
        };

        // Initialize state
        {
            let mut states = XHR_STATES.lock().unwrap();
            states.insert(xhr_id, XhrState {
                ready_state: UNSENT,
                method: String::new(),
                url: String::new(),
                async_flag: true,
                request_headers: HashMap::new(),
                status: 0,
                status_text: String::new(),
                response_headers: HashMap::new(),
                response_text: String::new(),
                response_url: String::new(),
                response_type: String::new(),
                timeout: 0,
                with_credentials: false,
            });
        }

        let xhr = create_xhr_object(ctx, xhr_id)?;

        // Store JS object reference in thread-local storage
        XHR_JS_OBJECTS.with(|objects| {
            objects.borrow_mut().insert(xhr_id, xhr.clone());
        });

        Ok(JsValue::from(xhr))
    });

    // Build constructor
    let ctor = FunctionObjectBuilder::new(context.realm(), xhr_constructor)
        .name(js_string!("XMLHttpRequest"))
        .length(0)
        .constructor(true)
        .build();

    // Add static constants
    ctor.set(js_string!("UNSENT"), JsValue::from(UNSENT), false, context)?;
    ctor.set(js_string!("OPENED"), JsValue::from(OPENED), false, context)?;
    ctor.set(js_string!("HEADERS_RECEIVED"), JsValue::from(HEADERS_RECEIVED), false, context)?;
    ctor.set(js_string!("LOADING"), JsValue::from(LOADING), false, context)?;
    ctor.set(js_string!("DONE"), JsValue::from(DONE), false, context)?;

    context.global_object().set(js_string!("XMLHttpRequest"), ctor, false, context)?;

    // Register XMLHttpRequestEventTarget as a stub (base interface)
    register_xhr_event_target(context)?;

    Ok(())
}

/// Register XMLHttpRequestEventTarget
fn register_xhr_event_target(context: &mut Context) -> JsResult<()> {
    let xhr_event_target_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Create a basic event target object
        let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });
        let remove_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });
        let dispatch_event = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::from(true))
        });

        let obj = ObjectInitializer::new(ctx)
            .function(add_event_listener, js_string!("addEventListener"), 3)
            .function(remove_event_listener, js_string!("removeEventListener"), 3)
            .function(dispatch_event, js_string!("dispatchEvent"), 1)
            .build();

        Ok(JsValue::from(obj))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), xhr_event_target_constructor)
        .name(js_string!("XMLHttpRequestEventTarget"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("XMLHttpRequestEventTarget"), ctor, false, context)?;

    Ok(())
}

/// Create an XHR object instance
fn create_xhr_object(ctx: &mut Context, xhr_id: u32) -> JsResult<JsObject> {
    // open(method, url, async?, user?, password?)
    let open_id = xhr_id;
    let open = unsafe {
        NativeFunction::from_closure(move |this, args, ctx| {
            let method = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_uppercase();
            let url = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let async_flag = args.get(2).map(|v| v.to_boolean()).unwrap_or(true);

            let mut states = XHR_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&open_id) {
                state.method = method;
                state.url = url;
                state.async_flag = async_flag;
                state.ready_state = OPENED;
                state.request_headers.clear();
                state.status = 0;
                state.status_text = String::new();
                state.response_headers.clear();
                state.response_text = String::new();
            }
            drop(states);

            // Update readyState on the object
            if let Some(obj) = this.as_object() {
                obj.set(js_string!("readyState"), JsValue::from(OPENED), false, ctx)?;
            }

            // Fire readystatechange
            fire_readystatechange(ctx, xhr_id);

            Ok(JsValue::undefined())
        })
    };

    // setRequestHeader(name, value)
    let set_header_id = xhr_id;
    let set_request_header = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();

            let mut states = XHR_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&set_header_id) {
                if state.ready_state != OPENED {
                    return Err(JsNativeError::error()
                        .with_message("InvalidStateError: XHR not opened")
                        .into());
                }
                // Append or set header
                state.request_headers
                    .entry(name)
                    .and_modify(|v| {
                        v.push_str(", ");
                        v.push_str(&value);
                    })
                    .or_insert(value);
            }

            Ok(JsValue::undefined())
        })
    };

    // send(body?)
    let send_id = xhr_id;
    let send = unsafe {
        NativeFunction::from_closure(move |this, args, ctx| {
            let body = if args.is_empty() || args.get_or_undefined(0).is_null_or_undefined() {
                None
            } else {
                Some(args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped())
            };

            // Get state info
            let (method, url, request_headers, timeout) = {
                let states = XHR_STATES.lock().unwrap();
                if let Some(state) = states.get(&send_id) {
                    if state.ready_state != OPENED {
                        return Err(JsNativeError::error()
                            .with_message("InvalidStateError: XHR not opened")
                            .into());
                    }
                    (state.method.clone(), state.url.clone(), state.request_headers.clone(), state.timeout)
                } else {
                    return Ok(JsValue::undefined());
                }
            };

            // Build and execute request
            let client = reqwest::blocking::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(if timeout > 0 {
                    Some(std::time::Duration::from_millis(timeout as u64))
                } else {
                    Some(std::time::Duration::from_secs(30))
                })
                .redirect(reqwest::redirect::Policy::limited(10))
                .gzip(true)
                .brotli(true)
                .deflate(true)
                .build()
                .map_err(|e| JsNativeError::error().with_message(e.to_string()))?;

            let mut request = match method.as_str() {
                "GET" => client.get(&url),
                "POST" => client.post(&url),
                "PUT" => client.put(&url),
                "DELETE" => client.delete(&url),
                "PATCH" => client.patch(&url),
                "HEAD" => client.head(&url),
                "OPTIONS" => client.request(reqwest::Method::OPTIONS, &url),
                _ => client.get(&url),
            };

            // Add request headers
            for (name, value) in &request_headers {
                request = request.header(name.as_str(), value.as_str());
            }

            // Add body for non-GET requests
            if let Some(body_content) = body {
                if method != "GET" && method != "HEAD" {
                    request = request.body(body_content);
                }
            }

            // Execute request
            match request.send() {
                Ok(response) => {
                    let status = response.status().as_u16();
                    let status_text = response.status().canonical_reason().unwrap_or("").to_string();
                    let final_url = response.url().to_string();

                    // Collect response headers
                    let mut response_headers = HashMap::new();
                    for (name, value) in response.headers() {
                        if let Ok(v) = value.to_str() {
                            response_headers.insert(name.as_str().to_lowercase(), v.to_string());
                        }
                    }

                    // Update state to HEADERS_RECEIVED
                    {
                        let mut states = XHR_STATES.lock().unwrap();
                        if let Some(state) = states.get_mut(&send_id) {
                            state.status = status;
                            state.status_text = status_text.clone();
                            state.response_url = final_url;
                            state.response_headers = response_headers;
                            state.ready_state = HEADERS_RECEIVED;
                        }
                    }

                    // Update object properties
                    if let Some(obj) = this.as_object() {
                        obj.set(js_string!("readyState"), JsValue::from(HEADERS_RECEIVED), false, ctx)?;
                        obj.set(js_string!("status"), JsValue::from(status), false, ctx)?;
                        obj.set(js_string!("statusText"), JsValue::from(js_string!(status_text)), false, ctx)?;
                    }
                    fire_readystatechange(ctx, send_id);

                    // Read body
                    match response.text() {
                        Ok(text) => {
                            // Update state to DONE
                            {
                                let mut states = XHR_STATES.lock().unwrap();
                                if let Some(state) = states.get_mut(&send_id) {
                                    state.response_text = text.clone();
                                    state.ready_state = DONE;
                                }
                            }

                            // Update object properties
                            if let Some(obj) = this.as_object() {
                                obj.set(js_string!("readyState"), JsValue::from(DONE), false, ctx)?;
                                obj.set(js_string!("responseText"), JsValue::from(js_string!(text.clone())), false, ctx)?;
                                obj.set(js_string!("response"), JsValue::from(js_string!(text)), false, ctx)?;
                            }
                            fire_readystatechange(ctx, send_id);
                            fire_load(ctx, send_id);
                        }
                        Err(e) => {
                            // Error reading body
                            {
                                let mut states = XHR_STATES.lock().unwrap();
                                if let Some(state) = states.get_mut(&send_id) {
                                    state.ready_state = DONE;
                                }
                            }
                            if let Some(obj) = this.as_object() {
                                obj.set(js_string!("readyState"), JsValue::from(DONE), false, ctx)?;
                            }
                            fire_readystatechange(ctx, send_id);
                            fire_error(ctx, send_id, &e.to_string());
                        }
                    }
                }
                Err(e) => {
                    // Network error
                    {
                        let mut states = XHR_STATES.lock().unwrap();
                        if let Some(state) = states.get_mut(&send_id) {
                            state.ready_state = DONE;
                        }
                    }
                    if let Some(obj) = this.as_object() {
                        obj.set(js_string!("readyState"), JsValue::from(DONE), false, ctx)?;
                    }
                    fire_readystatechange(ctx, send_id);

                    if e.is_timeout() {
                        fire_timeout(ctx, send_id);
                    } else {
                        fire_error(ctx, send_id, &e.to_string());
                    }
                }
            }

            fire_loadend(ctx, send_id);

            Ok(JsValue::undefined())
        })
    };

    // getResponseHeader(name)
    let get_header_id = xhr_id;
    let get_response_header = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase();

            let states = XHR_STATES.lock().unwrap();
            if let Some(state) = states.get(&get_header_id) {
                if let Some(value) = state.response_headers.get(&name) {
                    return Ok(JsValue::from(js_string!(value.clone())));
                }
            }

            Ok(JsValue::null())
        })
    };

    // getAllResponseHeaders()
    let get_all_headers_id = xhr_id;
    let get_all_response_headers = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let states = XHR_STATES.lock().unwrap();
            if let Some(state) = states.get(&get_all_headers_id) {
                let headers: String = state.response_headers
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v))
                    .collect::<Vec<_>>()
                    .join("\r\n");
                return Ok(JsValue::from(js_string!(headers)));
            }
            Ok(JsValue::from(js_string!("")))
        })
    };

    // abort()
    let abort_id = xhr_id;
    let abort = unsafe {
        NativeFunction::from_closure(move |this, _args, ctx| {
            let mut states = XHR_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&abort_id) {
                if state.ready_state != UNSENT && state.ready_state != DONE {
                    state.ready_state = UNSENT;
                    state.status = 0;
                    state.status_text = String::new();
                    state.response_text = String::new();

                    if let Some(obj) = this.as_object() {
                        let _ = obj.set(js_string!("readyState"), JsValue::from(UNSENT), false, ctx);
                        let _ = obj.set(js_string!("status"), JsValue::from(0), false, ctx);
                        let _ = obj.set(js_string!("statusText"), JsValue::from(js_string!("")), false, ctx);
                        let _ = obj.set(js_string!("responseText"), JsValue::from(js_string!("")), false, ctx);
                    }
                }
            }
            drop(states);

            fire_abort(ctx, abort_id);

            Ok(JsValue::undefined())
        })
    };

    // overrideMimeType(mime)
    let override_mime_type = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        // Stub - MIME type override not fully implemented
        Ok(JsValue::undefined())
    });

    // addEventListener
    let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        // Stub for now - event listeners handled via on* properties
        Ok(JsValue::undefined())
    });

    // removeEventListener
    let remove_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // dispatchEvent
    let dispatch_event = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(true))
    });

    // Build the XHR object
    let xhr = ObjectInitializer::new(ctx)
        // Methods
        .function(open, js_string!("open"), 5)
        .function(send, js_string!("send"), 1)
        .function(set_request_header, js_string!("setRequestHeader"), 2)
        .function(get_response_header, js_string!("getResponseHeader"), 1)
        .function(get_all_response_headers, js_string!("getAllResponseHeaders"), 0)
        .function(abort, js_string!("abort"), 0)
        .function(override_mime_type, js_string!("overrideMimeType"), 1)
        .function(add_event_listener, js_string!("addEventListener"), 3)
        .function(remove_event_listener, js_string!("removeEventListener"), 3)
        .function(dispatch_event, js_string!("dispatchEvent"), 1)
        // Properties
        .property(js_string!("readyState"), UNSENT, Attribute::all())
        .property(js_string!("status"), 0, Attribute::all())
        .property(js_string!("statusText"), js_string!(""), Attribute::all())
        .property(js_string!("responseText"), js_string!(""), Attribute::all())
        .property(js_string!("responseXML"), JsValue::null(), Attribute::all())
        .property(js_string!("response"), js_string!(""), Attribute::all())
        .property(js_string!("responseType"), js_string!(""), Attribute::all())
        .property(js_string!("responseURL"), js_string!(""), Attribute::all())
        .property(js_string!("timeout"), 0, Attribute::all())
        .property(js_string!("withCredentials"), false, Attribute::all())
        .property(js_string!("upload"), JsValue::null(), Attribute::READONLY)
        // Event handlers
        .property(js_string!("onreadystatechange"), JsValue::null(), Attribute::all())
        .property(js_string!("onload"), JsValue::null(), Attribute::all())
        .property(js_string!("onerror"), JsValue::null(), Attribute::all())
        .property(js_string!("ontimeout"), JsValue::null(), Attribute::all())
        .property(js_string!("onprogress"), JsValue::null(), Attribute::all())
        .property(js_string!("onabort"), JsValue::null(), Attribute::all())
        .property(js_string!("onloadstart"), JsValue::null(), Attribute::all())
        .property(js_string!("onloadend"), JsValue::null(), Attribute::all())
        // Static constants
        .property(js_string!("UNSENT"), UNSENT, Attribute::READONLY)
        .property(js_string!("OPENED"), OPENED, Attribute::READONLY)
        .property(js_string!("HEADERS_RECEIVED"), HEADERS_RECEIVED, Attribute::READONLY)
        .property(js_string!("LOADING"), LOADING, Attribute::READONLY)
        .property(js_string!("DONE"), DONE, Attribute::READONLY)
        .build();

    // Store the xhr_id on the object for reference
    xhr.set(js_string!("_xhrId"), JsValue::from(xhr_id), false, ctx)?;

    Ok(xhr)
}

// ============================================================================
// Event firing helpers
// ============================================================================

/// Create a ProgressEvent object
fn create_progress_event(ctx: &mut Context, event_type: &str) -> JsObject {
    let prevent_default = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let stop_propagation = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    ObjectInitializer::new(ctx)
        .property(js_string!("type"), js_string!(event_type.to_string()), Attribute::READONLY)
        .property(js_string!("lengthComputable"), false, Attribute::READONLY)
        .property(js_string!("loaded"), 0, Attribute::READONLY)
        .property(js_string!("total"), 0, Attribute::READONLY)
        .property(js_string!("target"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("currentTarget"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("bubbles"), false, Attribute::READONLY)
        .property(js_string!("cancelable"), false, Attribute::READONLY)
        .function(prevent_default, js_string!("preventDefault"), 0)
        .function(stop_propagation, js_string!("stopPropagation"), 0)
        .build()
}

/// Fire onreadystatechange event
fn fire_readystatechange(ctx: &mut Context, xhr_id: u32) {
    XHR_JS_OBJECTS.with(|objects| {
        if let Some(js_obj) = objects.borrow().get(&xhr_id) {
            if let Ok(handler) = js_obj.get(js_string!("onreadystatechange"), ctx) {
                if handler.is_callable() {
                    if let Some(cb) = handler.as_callable() {
                        let event = create_progress_event(ctx, "readystatechange");
                        let _ = cb.call(&JsValue::undefined(), &[JsValue::from(event)], ctx);
                    }
                }
            }
        }
    });
}

/// Fire onload event
fn fire_load(ctx: &mut Context, xhr_id: u32) {
    XHR_JS_OBJECTS.with(|objects| {
        if let Some(js_obj) = objects.borrow().get(&xhr_id) {
            if let Ok(handler) = js_obj.get(js_string!("onload"), ctx) {
                if handler.is_callable() {
                    if let Some(cb) = handler.as_callable() {
                        let event = create_progress_event(ctx, "load");
                        let _ = cb.call(&JsValue::undefined(), &[JsValue::from(event)], ctx);
                    }
                }
            }
        }
    });
}

/// Fire onerror event
fn fire_error(ctx: &mut Context, xhr_id: u32, _message: &str) {
    XHR_JS_OBJECTS.with(|objects| {
        if let Some(js_obj) = objects.borrow().get(&xhr_id) {
            if let Ok(handler) = js_obj.get(js_string!("onerror"), ctx) {
                if handler.is_callable() {
                    if let Some(cb) = handler.as_callable() {
                        let event = create_progress_event(ctx, "error");
                        let _ = cb.call(&JsValue::undefined(), &[JsValue::from(event)], ctx);
                    }
                }
            }
        }
    });
}

/// Fire ontimeout event
fn fire_timeout(ctx: &mut Context, xhr_id: u32) {
    XHR_JS_OBJECTS.with(|objects| {
        if let Some(js_obj) = objects.borrow().get(&xhr_id) {
            if let Ok(handler) = js_obj.get(js_string!("ontimeout"), ctx) {
                if handler.is_callable() {
                    if let Some(cb) = handler.as_callable() {
                        let event = create_progress_event(ctx, "timeout");
                        let _ = cb.call(&JsValue::undefined(), &[JsValue::from(event)], ctx);
                    }
                }
            }
        }
    });
}

/// Fire onabort event
fn fire_abort(ctx: &mut Context, xhr_id: u32) {
    XHR_JS_OBJECTS.with(|objects| {
        if let Some(js_obj) = objects.borrow().get(&xhr_id) {
            if let Ok(handler) = js_obj.get(js_string!("onabort"), ctx) {
                if handler.is_callable() {
                    if let Some(cb) = handler.as_callable() {
                        let event = create_progress_event(ctx, "abort");
                        let _ = cb.call(&JsValue::undefined(), &[JsValue::from(event)], ctx);
                    }
                }
            }
        }
    });
}

/// Fire onloadend event
fn fire_loadend(ctx: &mut Context, xhr_id: u32) {
    XHR_JS_OBJECTS.with(|objects| {
        if let Some(js_obj) = objects.borrow().get(&xhr_id) {
            if let Ok(handler) = js_obj.get(js_string!("onloadend"), ctx) {
                if handler.is_callable() {
                    if let Some(cb) = handler.as_callable() {
                        let event = create_progress_event(ctx, "loadend");
                        let _ = cb.call(&JsValue::undefined(), &[JsValue::from(event)], ctx);
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xhr_constants() {
        assert_eq!(UNSENT, 0);
        assert_eq!(OPENED, 1);
        assert_eq!(HEADERS_RECEIVED, 2);
        assert_eq!(LOADING, 3);
        assert_eq!(DONE, 4);
    }
}
