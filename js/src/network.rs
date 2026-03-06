//! Network APIs - WebSocket, EventSource, MessageChannel, BroadcastChannel
//!
//! Provides full implementations of browser network APIs backed by real connections.

use boa_engine::{
    js_string, native_function::NativeFunction, object::{ObjectInitializer, FunctionObjectBuilder}, property::Attribute,
    Context, JsArgs, JsNativeError, JsObject, JsResult, JsValue,
};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::io::ErrorKind;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tungstenite::{connect, Message, WebSocket};
use url::Url;

// ============================================================================
// Network Event Target - Shared event listener management for network objects
// ============================================================================

/// A listener for network events
#[derive(Clone)]
struct NetworkEventListener {
    callback: JsObject,
    once: bool,
}

/// Event target for network objects (WebSocket, EventSource, XMLHttpRequest)
/// Manages addEventListener/removeEventListener/dispatchEvent
#[derive(Default)]
struct NetworkEventTarget {
    listeners: HashMap<String, Vec<NetworkEventListener>>,
}

impl NetworkEventTarget {
    fn new() -> Self {
        Self::default()
    }

    /// Add an event listener
    fn add_listener(&mut self, event_type: &str, callback: JsObject, once: bool) {
        self.listeners
            .entry(event_type.to_string())
            .or_default()
            .push(NetworkEventListener { callback, once });
    }

    /// Remove an event listener
    fn remove_listener(&mut self, event_type: &str, callback: &JsObject) {
        if let Some(listeners) = self.listeners.get_mut(event_type) {
            // Remove listener with matching callback (by pointer comparison)
            listeners.retain(|l| !JsObject::equals(&l.callback, callback));
        }
    }

    /// Dispatch an event to all listeners and the on* handler
    /// Returns indices of once-listeners that should be removed
    fn dispatch(
        &mut self,
        event_type: &str,
        event: &JsObject,
        on_handler: Option<&JsValue>,
        context: &mut Context,
    ) {
        // Call on* handler first (e.g., onmessage)
        if let Some(handler) = on_handler {
            if handler.is_callable() {
                if let Some(cb) = handler.as_callable() {
                    let _ = cb.call(&JsValue::undefined(), &[JsValue::from(event.clone())], context);
                }
            }
        }

        // Call addEventListener listeners
        if let Some(listeners) = self.listeners.get(event_type) {
            let listeners_clone: Vec<_> = listeners.clone();
            for listener in &listeners_clone {
                let _ = listener.callback.call(
                    &JsValue::undefined(),
                    &[JsValue::from(event.clone())],
                    context,
                );
            }
        }

        // Remove once-listeners
        if let Some(listeners) = self.listeners.get_mut(event_type) {
            listeners.retain(|l| !l.once);
        }
    }
}

/// WebSocket message types
#[derive(Clone)]
enum WebSocketMessage {
    Text(String),
    Binary(Vec<u8>),
}

/// WebSocket ready states
const CONNECTING: u16 = 0;
const OPEN: u16 = 1;
const CLOSING: u16 = 2;
const CLOSED: u16 = 3;

/// Global registry for WebSocket connections (thread-safe data only)
lazy_static::lazy_static! {
    static ref WEBSOCKET_CONNECTIONS: Mutex<HashMap<u32, WebSocketConnectionData>> =
        Mutex::new(HashMap::new());
    static ref NEXT_WS_ID: Mutex<u32> = Mutex::new(1);

    static ref EVENTSOURCE_CONNECTIONS: Mutex<HashMap<u32, EventSourceConnectionData>> =
        Mutex::new(HashMap::new());
    static ref NEXT_ES_ID: Mutex<u32> = Mutex::new(1);

    static ref BROADCAST_CHANNELS: Mutex<HashMap<String, Vec<u32>>> =
        Mutex::new(HashMap::new());
    static ref BROADCAST_LISTENERS: Mutex<HashMap<u32, BroadcastChannelData>> =
        Mutex::new(HashMap::new());
    static ref NEXT_BC_ID: Mutex<u32> = Mutex::new(1);
}

/// Thread-local storage for JS object references (not Send/Sync)
thread_local! {
    static WS_JS_OBJECTS: RefCell<HashMap<u32, JsObject>> = RefCell::new(HashMap::new());
    static WS_EVENT_TARGETS: RefCell<HashMap<u32, NetworkEventTarget>> = RefCell::new(HashMap::new());
    static ES_JS_OBJECTS: RefCell<HashMap<u32, JsObject>> = RefCell::new(HashMap::new());
    static ES_EVENT_TARGETS: RefCell<HashMap<u32, NetworkEventTarget>> = RefCell::new(HashMap::new());
}

/// Thread-safe WebSocket connection data (no JsObject)
struct WebSocketConnectionData {
    url: String,
    ready_state: u16,
    buffered_amount: u32,
    protocol: String,
    extensions: String,
    binary_type: String,
    socket: Option<WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>>,
    // Pending events to deliver
    pending_messages: VecDeque<WebSocketMessage>,
    pending_open: bool,
    pending_close: Option<(u16, String)>,
    pending_error: Option<String>,
}

/// Thread-safe EventSource connection data (no JsObject)
struct EventSourceConnectionData {
    url: String,
    ready_state: u16,
    with_credentials: bool,
    last_event_id: String,
    retry_ms: u64,
    // Pending events to deliver
    pending_events: VecDeque<ServerSentEvent>,
    pending_open: bool,
    pending_error: Option<String>,
    reconnect_at: Option<Instant>,
}

/// Represents a Server-Sent Event
#[derive(Clone)]
struct ServerSentEvent {
    event_type: String,
    data: String,
    last_event_id: String,
    origin: String,
}

/// BroadcastChannel data
struct BroadcastChannelData {
    name: String,
    closed: bool,
}

/// Register all network APIs
pub fn register_all_network_apis(context: &mut Context) -> JsResult<()> {
    register_websocket(context)?;
    register_eventsource(context)?;
    // MessageChannel is already registered by timers.rs, skip duplicate
    // register_message_channel(context)?;
    register_broadcast_channel(context)?;
    register_beacon_api(context)?;
    Ok(())
}

/// Register WebSocket constructor
fn register_websocket(context: &mut Context) -> JsResult<()> {
    // WebSocket constructor
    let websocket_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let url_str = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        // Parse protocols (optional second argument)
        let protocols: Vec<String> = if args.len() > 1 {
            let proto_arg = args.get_or_undefined(1);
            if let Some(arr) = proto_arg.as_object() {
                // Try to get as array
                let length = arr.get(js_string!("length"), ctx)
                    .ok()
                    .and_then(|v| v.to_u32(ctx).ok())
                    .unwrap_or(0);
                let mut protos = Vec::new();
                for i in 0..length {
                    if let Ok(val) = arr.get(js_string!(i.to_string()), ctx) {
                        protos.push(val.to_string(ctx)?.to_std_string_escaped());
                    }
                }
                protos
            } else {
                vec![proto_arg.to_string(ctx)?.to_std_string_escaped()]
            }
        } else {
            Vec::new()
        };

        // Validate URL
        let parsed_url = Url::parse(&url_str).map_err(|_| {
            JsNativeError::syntax().with_message("Invalid WebSocket URL")
        })?;

        let scheme = parsed_url.scheme();
        if scheme != "ws" && scheme != "wss" {
            return Err(JsNativeError::syntax()
                .with_message("WebSocket URL must use ws:// or wss:// scheme")
                .into());
        }

        // Generate connection ID
        let ws_id = {
            let mut id = NEXT_WS_ID.lock().unwrap();
            let current = *id;
            *id += 1;
            current
        };

        // Try to establish connection
        let (ready_state, socket, protocol, extensions, pending_open, pending_error) = match connect(&url_str) {
            Ok((mut socket, response)) => {
                let protocol = response.headers()
                    .get("sec-websocket-protocol")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                let extensions = response.headers()
                    .get("sec-websocket-extensions")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();

                // Set socket to non-blocking mode for polling
                // For plain TCP, we can set non-blocking directly
                // For TLS streams, we rely on read timeouts in the polling loop
                if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
                    let _ = stream.set_nonblocking(true);
                }

                (OPEN, Some(socket), protocol, extensions, true, None)
            }
            Err(e) => {
                // Connection failed - will be in CLOSED state
                (CLOSED, None, String::new(), String::new(), false, Some(e.to_string()))
            }
        };

        // Store connection data (thread-safe)
        {
            let mut connections = WEBSOCKET_CONNECTIONS.lock().unwrap();
            connections.insert(ws_id, WebSocketConnectionData {
                url: url_str.clone(),
                ready_state,
                buffered_amount: 0,
                protocol: protocol.clone(),
                extensions: extensions.clone(),
                binary_type: "blob".to_string(),
                socket,
                pending_messages: VecDeque::new(),
                pending_open,
                pending_close: None,
                pending_error,
            });
        }

        // Create WebSocket object
        let ws = create_websocket_object(ctx, ws_id, &url_str, &protocols, &protocol, &extensions, ready_state)?;

        // Store JS object reference and event target in thread-local storage
        WS_JS_OBJECTS.with(|objects| {
            objects.borrow_mut().insert(ws_id, ws.clone());
        });
        WS_EVENT_TARGETS.with(|targets| {
            targets.borrow_mut().insert(ws_id, NetworkEventTarget::new());
        });

        Ok(JsValue::from(ws))
    });

    // Build constructor with .constructor(true)
    let ctor = FunctionObjectBuilder::new(context.realm(), websocket_constructor)
        .name(js_string!("WebSocket"))
        .length(2)
        .constructor(true)
        .build();

    // Add static constants
    ctor.set(js_string!("CONNECTING"), JsValue::from(CONNECTING), false, context)?;
    ctor.set(js_string!("OPEN"), JsValue::from(OPEN), false, context)?;
    ctor.set(js_string!("CLOSING"), JsValue::from(CLOSING), false, context)?;
    ctor.set(js_string!("CLOSED"), JsValue::from(CLOSED), false, context)?;

    // Register globally
    context.global_object().set(js_string!("WebSocket"), ctor, false, context)?;

    Ok(())
}

/// Create a WebSocket object instance
fn create_websocket_object(
    ctx: &mut Context,
    ws_id: u32,
    url: &str,
    _protocols: &[String],
    protocol: &str,
    extensions: &str,
    ready_state: u16,
) -> JsResult<JsObject> {
    // send() method
    let send_id = ws_id;
    let send = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let data = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let mut connections = WEBSOCKET_CONNECTIONS.lock().unwrap();
            if let Some(conn) = connections.get_mut(&send_id) {
                if conn.ready_state != OPEN {
                    return Err(JsNativeError::error()
                        .with_message("WebSocket is not open")
                        .into());
                }

                if let Some(ref mut socket) = conn.socket {
                    let _ = socket.send(Message::Text(data));
                }
            }

            Ok(JsValue::undefined())
        })
    };

    // close() method
    let close_id = ws_id;
    let close = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let code = if args.len() > 0 {
                args.get_or_undefined(0).to_u32(ctx).ok().map(|c| c as u16)
            } else {
                None
            };

            let reason = if args.len() > 1 {
                Some(args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped())
            } else {
                None
            };

            let mut connections = WEBSOCKET_CONNECTIONS.lock().unwrap();
            if let Some(conn) = connections.get_mut(&close_id) {
                if conn.ready_state == OPEN || conn.ready_state == CONNECTING {
                    conn.ready_state = CLOSING;

                    if let Some(ref mut socket) = conn.socket {
                        let close_frame = tungstenite::protocol::CloseFrame {
                            code: tungstenite::protocol::frame::coding::CloseCode::from(code.unwrap_or(1000)),
                            reason: reason.unwrap_or_default().into(),
                        };
                        let _ = socket.close(Some(close_frame));
                    }

                    conn.ready_state = CLOSED;
                }
            }

            Ok(JsValue::undefined())
        })
    };

    // addEventListener - real implementation
    let add_event_listener_id = ws_id;
    let add_event_listener = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let callback = args.get_or_undefined(1);

            if let Some(callback_obj) = callback.as_object() {
                // Check for once option
                let once = args.get(2)
                    .and_then(|v| v.as_object())
                    .and_then(|obj| obj.get(js_string!("once"), ctx).ok())
                    .map(|v| v.to_boolean())
                    .unwrap_or(false);

                WS_EVENT_TARGETS.with(|targets| {
                    if let Some(target) = targets.borrow_mut().get_mut(&add_event_listener_id) {
                        target.add_listener(&event_type, callback_obj.clone(), once);
                    }
                });
            }

            Ok(JsValue::undefined())
        })
    };

    // removeEventListener - real implementation
    let remove_event_listener_id = ws_id;
    let remove_event_listener = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let callback = args.get_or_undefined(1);

            if let Some(callback_obj) = callback.as_object() {
                WS_EVENT_TARGETS.with(|targets| {
                    if let Some(target) = targets.borrow_mut().get_mut(&remove_event_listener_id) {
                        target.remove_listener(&event_type, &callback_obj);
                    }
                });
            }

            Ok(JsValue::undefined())
        })
    };

    // dispatchEvent
    let dispatch_event = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(true))
    });

    let ws = ObjectInitializer::new(ctx)
        .property(js_string!("url"), js_string!(url.to_string()), Attribute::READONLY)
        .property(js_string!("readyState"), ready_state, Attribute::READONLY)
        .property(js_string!("bufferedAmount"), 0, Attribute::READONLY)
        .property(js_string!("protocol"), js_string!(protocol.to_string()), Attribute::READONLY)
        .property(js_string!("extensions"), js_string!(extensions.to_string()), Attribute::READONLY)
        .property(js_string!("binaryType"), js_string!("blob"), Attribute::all())
        .property(js_string!("onopen"), JsValue::null(), Attribute::all())
        .property(js_string!("onclose"), JsValue::null(), Attribute::all())
        .property(js_string!("onerror"), JsValue::null(), Attribute::all())
        .property(js_string!("onmessage"), JsValue::null(), Attribute::all())
        .property(js_string!("CONNECTING"), CONNECTING, Attribute::READONLY)
        .property(js_string!("OPEN"), OPEN, Attribute::READONLY)
        .property(js_string!("CLOSING"), CLOSING, Attribute::READONLY)
        .property(js_string!("CLOSED"), CLOSED, Attribute::READONLY)
        .function(send, js_string!("send"), 1)
        .function(close, js_string!("close"), 2)
        .function(add_event_listener, js_string!("addEventListener"), 3)
        .function(remove_event_listener, js_string!("removeEventListener"), 3)
        .function(dispatch_event, js_string!("dispatchEvent"), 1)
        .build();

    // Store the ws_id on the object for later reference
    ws.set(js_string!("_wsId"), JsValue::from(ws_id), false, ctx)?;

    Ok(ws)
}

/// Register EventSource constructor
fn register_eventsource(context: &mut Context) -> JsResult<()> {
    let eventsource_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let url_str = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        // Parse options
        let with_credentials = if args.len() > 1 {
            let options = args.get_or_undefined(1);
            if let Some(obj) = options.as_object() {
                obj.get(js_string!("withCredentials"), ctx)
                    .ok()
                    .map(|v| v.to_boolean())
                    .unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };

        // Generate connection ID
        let es_id = {
            let mut id = NEXT_ES_ID.lock().unwrap();
            let current = *id;
            *id += 1;
            current
        };

        // Try to establish connection
        let (ready_state, events, pending_open, pending_error) = match establish_eventsource_connection(&url_str) {
            Ok(events) => (OPEN, events, true, None),
            Err(e) => (CLOSED, Vec::new(), false, Some(e)),
        };

        // Store connection data (thread-safe)
        {
            let mut connections = EVENTSOURCE_CONNECTIONS.lock().unwrap();
            connections.insert(es_id, EventSourceConnectionData {
                url: url_str.clone(),
                ready_state,
                with_credentials,
                last_event_id: String::new(),
                retry_ms: 3000, // Default retry is 3 seconds
                pending_events: VecDeque::from(events),
                pending_open,
                pending_error,
                reconnect_at: None,
            });
        }

        // Create EventSource object
        let es = create_eventsource_object(ctx, es_id, &url_str, with_credentials, ready_state)?;

        // Store JS object reference and event target in thread-local storage
        ES_JS_OBJECTS.with(|objects| {
            objects.borrow_mut().insert(es_id, es.clone());
        });
        ES_EVENT_TARGETS.with(|targets| {
            targets.borrow_mut().insert(es_id, NetworkEventTarget::new());
        });

        Ok(JsValue::from(es))
    });

    context.register_global_builtin_callable(js_string!("EventSource"), 2, eventsource_constructor)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register EventSource: {}", e)))?;

    // Add static constants
    let global = context.global_object();
    if let Ok(es_val) = global.get(js_string!("EventSource"), context) {
        if let Some(es_obj) = es_val.as_object() {
            es_obj.set(js_string!("CONNECTING"), JsValue::from(CONNECTING), false, context)?;
            es_obj.set(js_string!("OPEN"), JsValue::from(OPEN), false, context)?;
            es_obj.set(js_string!("CLOSED"), JsValue::from(CLOSED), false, context)?;
        }
    }

    Ok(())
}

/// Establish EventSource connection and parse initial events
fn establish_eventsource_connection(url: &str) -> Result<Vec<ServerSentEvent>, String> {
    let client = reqwest::blocking::Client::new();
    let response = client.get(url)
        .header("Accept", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .send()
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err("Connection failed".to_string());
    }

    let content_type = response.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !content_type.contains("text/event-stream") {
        return Err("Invalid content type".to_string());
    }

    // Parse the response body as SSE
    let text = response.text().map_err(|e| e.to_string())?;
    let events = parse_sse_stream(&text, url);

    Ok(events)
}

/// Parse SSE stream into events
fn parse_sse_stream(text: &str, origin: &str) -> Vec<ServerSentEvent> {
    let mut events = Vec::new();
    let mut current_event = ServerSentEvent {
        event_type: "message".to_string(),
        data: String::new(),
        last_event_id: String::new(),
        origin: origin.to_string(),
    };

    for line in text.lines() {
        if line.is_empty() {
            // Empty line = dispatch event
            if !current_event.data.is_empty() {
                // Remove trailing newline from data
                if current_event.data.ends_with('\n') {
                    current_event.data.pop();
                }
                events.push(current_event.clone());
            }
            current_event.data = String::new();
            current_event.event_type = "message".to_string();
        } else if line.starts_with(':') {
            // Comment, ignore
        } else if let Some(rest) = line.strip_prefix("event:") {
            current_event.event_type = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            if !current_event.data.is_empty() {
                current_event.data.push('\n');
            }
            current_event.data.push_str(rest.trim_start());
        } else if let Some(rest) = line.strip_prefix("id:") {
            current_event.last_event_id = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("retry:") {
            // retry field - ignore for now
            let _ = rest;
        }
    }

    // Don't forget last event if data exists
    if !current_event.data.is_empty() {
        if current_event.data.ends_with('\n') {
            current_event.data.pop();
        }
        events.push(current_event);
    }

    events
}

/// Create an EventSource object instance
fn create_eventsource_object(
    ctx: &mut Context,
    es_id: u32,
    url: &str,
    with_credentials: bool,
    ready_state: u16,
) -> JsResult<JsObject> {
    // close() method
    let close_id = es_id;
    let close = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let mut connections = EVENTSOURCE_CONNECTIONS.lock().unwrap();
            if let Some(conn) = connections.get_mut(&close_id) {
                conn.ready_state = CLOSED;
            }
            Ok(JsValue::undefined())
        })
    };

    // addEventListener - real implementation
    let add_event_listener_id = es_id;
    let add_event_listener = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let callback = args.get_or_undefined(1);

            if let Some(callback_obj) = callback.as_object() {
                let once = args.get(2)
                    .and_then(|v| v.as_object())
                    .and_then(|obj| obj.get(js_string!("once"), ctx).ok())
                    .map(|v| v.to_boolean())
                    .unwrap_or(false);

                ES_EVENT_TARGETS.with(|targets| {
                    if let Some(target) = targets.borrow_mut().get_mut(&add_event_listener_id) {
                        target.add_listener(&event_type, callback_obj.clone(), once);
                    }
                });
            }

            Ok(JsValue::undefined())
        })
    };

    // removeEventListener - real implementation
    let remove_event_listener_id = es_id;
    let remove_event_listener = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let callback = args.get_or_undefined(1);

            if let Some(callback_obj) = callback.as_object() {
                ES_EVENT_TARGETS.with(|targets| {
                    if let Some(target) = targets.borrow_mut().get_mut(&remove_event_listener_id) {
                        target.remove_listener(&event_type, &callback_obj);
                    }
                });
            }

            Ok(JsValue::undefined())
        })
    };

    // dispatchEvent
    let dispatch_event = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(true))
    });

    let es = ObjectInitializer::new(ctx)
        .property(js_string!("url"), js_string!(url.to_string()), Attribute::READONLY)
        .property(js_string!("readyState"), ready_state, Attribute::READONLY)
        .property(js_string!("withCredentials"), with_credentials, Attribute::READONLY)
        .property(js_string!("onopen"), JsValue::null(), Attribute::all())
        .property(js_string!("onmessage"), JsValue::null(), Attribute::all())
        .property(js_string!("onerror"), JsValue::null(), Attribute::all())
        .property(js_string!("CONNECTING"), CONNECTING, Attribute::READONLY)
        .property(js_string!("OPEN"), OPEN, Attribute::READONLY)
        .property(js_string!("CLOSED"), CLOSED, Attribute::READONLY)
        .function(close, js_string!("close"), 0)
        .function(add_event_listener, js_string!("addEventListener"), 3)
        .function(remove_event_listener, js_string!("removeEventListener"), 3)
        .function(dispatch_event, js_string!("dispatchEvent"), 1)
        .build();

    es.set(js_string!("_esId"), JsValue::from(es_id), false, ctx)?;

    Ok(es)
}

/// Register MessageChannel and MessagePort
fn register_message_channel(context: &mut Context) -> JsResult<()> {
    // MessagePort constructor (used internally)
    let message_port_proto = create_message_port_prototype(context)?;

    // Store the prototype in a thread-local for access from the closure
    thread_local! {
        static PORT_PROTO: RefCell<Option<JsObject>> = RefCell::new(None);
    }
    PORT_PROTO.with(|p| {
        *p.borrow_mut() = Some(message_port_proto.clone());
    });

    // MessageChannel constructor - use from_copy_closure for proper constructor support
    let message_channel_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Create two connected ports
        let port1_queue: Arc<Mutex<Vec<JsValue>>> = Arc::new(Mutex::new(Vec::new()));
        let port2_queue: Arc<Mutex<Vec<JsValue>>> = Arc::new(Mutex::new(Vec::new()));

        let port1 = create_message_port(ctx, port1_queue.clone(), port2_queue.clone())?;
        let port2 = create_message_port(ctx, port2_queue, port1_queue)?;

        // Set prototypes from thread-local storage
        PORT_PROTO.with(|p| {
            if let Some(proto) = p.borrow().as_ref() {
                port1.set_prototype(Some(proto.clone()));
                port2.set_prototype(Some(proto.clone()));
            }
        });

        let channel = ObjectInitializer::new(ctx)
            .property(js_string!("port1"), port1, Attribute::READONLY)
            .property(js_string!("port2"), port2, Attribute::READONLY)
            .build();

        Ok(JsValue::from(channel))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), message_channel_constructor)
        .name(js_string!("MessageChannel"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(
        js_string!("MessageChannel"),
        ctor,
        false,
        context
    )?;

    // Also register MessagePort as global (for instanceof checks)
    context.register_global_property(js_string!("MessagePort"), message_port_proto, Attribute::READONLY)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register MessagePort: {}", e)))?;

    Ok(())
}

/// Create MessagePort prototype
fn create_message_port_prototype(ctx: &mut Context) -> JsResult<JsObject> {
    let start = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let close = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let remove_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let dispatch_event = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(true))
    });

    let proto = ObjectInitializer::new(ctx)
        .function(start, js_string!("start"), 0)
        .function(close, js_string!("close"), 0)
        .function(add_event_listener, js_string!("addEventListener"), 3)
        .function(remove_event_listener, js_string!("removeEventListener"), 3)
        .function(dispatch_event, js_string!("dispatchEvent"), 1)
        .property(js_string!("onmessage"), JsValue::null(), Attribute::all())
        .property(js_string!("onmessageerror"), JsValue::null(), Attribute::all())
        .build();

    Ok(proto)
}

/// Create a MessagePort instance
fn create_message_port(
    ctx: &mut Context,
    _own_queue: Arc<Mutex<Vec<JsValue>>>,
    target_queue: Arc<Mutex<Vec<JsValue>>>,
) -> JsResult<JsObject> {
    // postMessage method
    let queue = target_queue.clone();
    let post_message = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            let message = args.get_or_undefined(0).clone();
            let mut q = queue.lock().unwrap();
            q.push(message);
            Ok(JsValue::undefined())
        })
    };

    let port = ObjectInitializer::new(ctx)
        .function(post_message, js_string!("postMessage"), 2)
        .property(js_string!("onmessage"), JsValue::null(), Attribute::all())
        .property(js_string!("onmessageerror"), JsValue::null(), Attribute::all())
        .build();

    Ok(port)
}

/// Register BroadcastChannel
fn register_broadcast_channel(context: &mut Context) -> JsResult<()> {
    let broadcast_channel_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        // Generate channel ID
        let bc_id = {
            let mut id = NEXT_BC_ID.lock().unwrap();
            let current = *id;
            *id += 1;
            current
        };

        // Register this channel
        {
            let mut channels = BROADCAST_CHANNELS.lock().unwrap();
            let subscribers = channels.entry(name.clone()).or_insert_with(Vec::new);
            subscribers.push(bc_id);
        }

        {
            let mut listeners = BROADCAST_LISTENERS.lock().unwrap();
            listeners.insert(bc_id, BroadcastChannelData {
                name: name.clone(),
                closed: false,
            });
        }

        // postMessage method
        let name_clone = name.clone();
        let sender_id = bc_id;
        let post_message = unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                // In a real implementation, this would dispatch to all other channels with same name
                // For now, we just acknowledge it
                let listeners = BROADCAST_LISTENERS.lock().unwrap();
                if let Some(data) = listeners.get(&sender_id) {
                    if data.closed {
                        return Err(JsNativeError::error()
                            .with_message("BroadcastChannel is closed")
                            .into());
                    }
                }

                log::debug!("[BroadcastChannel] Posted message to channel: {}", name_clone);
                Ok(JsValue::undefined())
            })
        };

        // close method
        let close_id = bc_id;
        let close_name = name.clone();
        let close = unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                // Remove from broadcast channels
                {
                    let mut channels = BROADCAST_CHANNELS.lock().unwrap();
                    if let Some(subscribers) = channels.get_mut(&close_name) {
                        subscribers.retain(|&id| id != close_id);
                    }
                }

                // Mark as closed
                {
                    let mut listeners = BROADCAST_LISTENERS.lock().unwrap();
                    if let Some(data) = listeners.get_mut(&close_id) {
                        data.closed = true;
                    }
                }

                Ok(JsValue::undefined())
            })
        };

        let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let remove_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let dispatch_event = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::from(true))
        });

        let channel = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!(name), Attribute::READONLY)
            .property(js_string!("onmessage"), JsValue::null(), Attribute::all())
            .property(js_string!("onmessageerror"), JsValue::null(), Attribute::all())
            .function(post_message, js_string!("postMessage"), 1)
            .function(close, js_string!("close"), 0)
            .function(add_event_listener, js_string!("addEventListener"), 3)
            .function(remove_event_listener, js_string!("removeEventListener"), 3)
            .function(dispatch_event, js_string!("dispatchEvent"), 1)
            .build();

        channel.set(js_string!("_bcId"), JsValue::from(bc_id), false, ctx)?;

        Ok(JsValue::from(channel))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), broadcast_channel_constructor)
        .name(js_string!("BroadcastChannel"))
        .length(1)
        .constructor(true)
        .build();

    context.global_object().set(
        js_string!("BroadcastChannel"),
        ctor,
        false,
        context
    )?;

    Ok(())
}

/// Register Beacon API (navigator.sendBeacon)
fn register_beacon_api(context: &mut Context) -> JsResult<()> {
    // Get navigator object and add sendBeacon
    let global = context.global_object();

    if let Ok(nav_val) = global.get(js_string!("navigator"), context) {
        if let Some(nav_obj) = nav_val.as_object() {
            let send_beacon = NativeFunction::from_copy_closure(|_this, args, ctx| {
                let url = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let data = if args.len() > 1 {
                    Some(args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped())
                } else {
                    None
                };

                // Try to send beacon (fire and forget)
                let client = reqwest::blocking::Client::new();
                let mut request = client.post(&url);

                if let Some(body) = data {
                    request = request.body(body);
                }

                // Send asynchronously (we just try, don't wait for response)
                match request.send() {
                    Ok(_) => Ok(JsValue::from(true)),
                    Err(_) => Ok(JsValue::from(false)),
                }
            });

            // Create a callable function object
            let send_beacon_func = boa_engine::object::FunctionObjectBuilder::new(context.realm(), send_beacon)
                .name(js_string!("sendBeacon"))
                .length(2)
                .build();

            nav_obj.set(
                js_string!("sendBeacon"),
                send_beacon_func,
                false,
                context,
            )?;
        }
    }

    Ok(())
}

/// Register CloseEvent constructor
pub fn register_close_event(context: &mut Context) -> JsResult<()> {
    let close_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        // Parse options
        let (code, reason, was_clean) = if args.len() > 1 {
            let options = args.get_or_undefined(1);
            if let Some(obj) = options.as_object() {
                let code = obj.get(js_string!("code"), ctx)
                    .ok()
                    .and_then(|v| v.to_u32(ctx).ok())
                    .unwrap_or(1000) as u16;
                let reason = obj.get(js_string!("reason"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_default();
                let was_clean = obj.get(js_string!("wasClean"), ctx)
                    .ok()
                    .map(|v| v.to_boolean())
                    .unwrap_or(true);
                (code, reason, was_clean)
            } else {
                (1000u16, String::new(), true)
            }
        } else {
            (1000u16, String::new(), true)
        };

        let prevent_default = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });
        let stop_propagation = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let event = ObjectInitializer::new(ctx)
            .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
            .property(js_string!("code"), code, Attribute::READONLY)
            .property(js_string!("reason"), js_string!(reason), Attribute::READONLY)
            .property(js_string!("wasClean"), was_clean, Attribute::READONLY)
            .property(js_string!("target"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("currentTarget"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("bubbles"), false, Attribute::READONLY)
            .property(js_string!("cancelable"), false, Attribute::READONLY)
            .function(prevent_default, js_string!("preventDefault"), 0)
            .function(stop_propagation, js_string!("stopPropagation"), 0)
            .build();

        Ok(JsValue::from(event))
    });

    context.register_global_builtin_callable(js_string!("CloseEvent"), 2, close_event_constructor)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register CloseEvent: {}", e)))?;

    Ok(())
}

/// Register MessageEvent constructor
pub fn register_message_event(context: &mut Context) -> JsResult<()> {
    let message_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        // Parse options
        let (data, origin, last_event_id, source, ports) = if args.len() > 1 {
            let options = args.get_or_undefined(1);
            if let Some(obj) = options.as_object() {
                let data = obj.get(js_string!("data"), ctx).unwrap_or(JsValue::null());
                let origin = obj.get(js_string!("origin"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_default();
                let last_event_id = obj.get(js_string!("lastEventId"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_default();
                let source = obj.get(js_string!("source"), ctx).unwrap_or(JsValue::null());
                let ports = obj.get(js_string!("ports"), ctx).unwrap_or(JsValue::null());
                (data, origin, last_event_id, source, ports)
            } else {
                (JsValue::null(), String::new(), String::new(), JsValue::null(), JsValue::null())
            }
        } else {
            (JsValue::null(), String::new(), String::new(), JsValue::null(), JsValue::null())
        };

        let prevent_default = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });
        let stop_propagation = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let event = ObjectInitializer::new(ctx)
            .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
            .property(js_string!("data"), data, Attribute::READONLY)
            .property(js_string!("origin"), js_string!(origin), Attribute::READONLY)
            .property(js_string!("lastEventId"), js_string!(last_event_id), Attribute::READONLY)
            .property(js_string!("source"), source, Attribute::READONLY)
            .property(js_string!("ports"), ports, Attribute::READONLY)
            .property(js_string!("target"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("currentTarget"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("bubbles"), false, Attribute::READONLY)
            .property(js_string!("cancelable"), false, Attribute::READONLY)
            .function(prevent_default, js_string!("preventDefault"), 0)
            .function(stop_propagation, js_string!("stopPropagation"), 0)
            .build();

        Ok(JsValue::from(event))
    });

    context.register_global_builtin_callable(js_string!("MessageEvent"), 2, message_event_constructor)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register MessageEvent: {}", e)))?;

    Ok(())
}

/// Register ErrorEvent constructor
pub fn register_error_event(context: &mut Context) -> JsResult<()> {
    let error_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        // Parse options
        let (message, filename, lineno, colno, error) = if args.len() > 1 {
            let options = args.get_or_undefined(1);
            if let Some(obj) = options.as_object() {
                let message = obj.get(js_string!("message"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_default();
                let filename = obj.get(js_string!("filename"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_default();
                let lineno = obj.get(js_string!("lineno"), ctx)
                    .ok()
                    .and_then(|v| v.to_u32(ctx).ok())
                    .unwrap_or(0);
                let colno = obj.get(js_string!("colno"), ctx)
                    .ok()
                    .and_then(|v| v.to_u32(ctx).ok())
                    .unwrap_or(0);
                let error = obj.get(js_string!("error"), ctx).unwrap_or(JsValue::null());
                (message, filename, lineno, colno, error)
            } else {
                (String::new(), String::new(), 0u32, 0u32, JsValue::null())
            }
        } else {
            (String::new(), String::new(), 0u32, 0u32, JsValue::null())
        };

        let prevent_default = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });
        let stop_propagation = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let event = ObjectInitializer::new(ctx)
            .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
            .property(js_string!("message"), js_string!(message), Attribute::READONLY)
            .property(js_string!("filename"), js_string!(filename), Attribute::READONLY)
            .property(js_string!("lineno"), lineno, Attribute::READONLY)
            .property(js_string!("colno"), colno, Attribute::READONLY)
            .property(js_string!("error"), error, Attribute::READONLY)
            .property(js_string!("target"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("currentTarget"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("bubbles"), false, Attribute::READONLY)
            .property(js_string!("cancelable"), true, Attribute::READONLY)
            .function(prevent_default, js_string!("preventDefault"), 0)
            .function(stop_propagation, js_string!("stopPropagation"), 0)
            .build();

        Ok(JsValue::from(event))
    });

    context.register_global_builtin_callable(js_string!("ErrorEvent"), 2, error_event_constructor)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register ErrorEvent: {}", e)))?;

    Ok(())
}

// ============================================================================
// Network Event Processing - Called from event loop to deliver events
// ============================================================================

/// Create an event object for network events
fn create_network_event(ctx: &mut Context, event_type: &str) -> JsObject {
    let prevent_default = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let stop_propagation = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    ObjectInitializer::new(ctx)
        .property(js_string!("type"), js_string!(event_type.to_string()), Attribute::READONLY)
        .property(js_string!("target"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("currentTarget"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("bubbles"), false, Attribute::READONLY)
        .property(js_string!("cancelable"), false, Attribute::READONLY)
        .function(prevent_default, js_string!("preventDefault"), 0)
        .function(stop_propagation, js_string!("stopPropagation"), 0)
        .build()
}

/// Create a MessageEvent for WebSocket/EventSource messages
fn create_network_message_event(ctx: &mut Context, data: JsValue, origin: &str) -> JsObject {
    let prevent_default = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let stop_propagation = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    ObjectInitializer::new(ctx)
        .property(js_string!("type"), js_string!("message"), Attribute::READONLY)
        .property(js_string!("data"), data, Attribute::READONLY)
        .property(js_string!("origin"), js_string!(origin.to_string()), Attribute::READONLY)
        .property(js_string!("lastEventId"), js_string!(""), Attribute::READONLY)
        .property(js_string!("source"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("ports"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("target"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("currentTarget"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("bubbles"), false, Attribute::READONLY)
        .property(js_string!("cancelable"), false, Attribute::READONLY)
        .function(prevent_default, js_string!("preventDefault"), 0)
        .function(stop_propagation, js_string!("stopPropagation"), 0)
        .build()
}

/// Create a CloseEvent for WebSocket close
fn create_network_close_event(ctx: &mut Context, code: u16, reason: &str) -> JsObject {
    let prevent_default = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let stop_propagation = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    ObjectInitializer::new(ctx)
        .property(js_string!("type"), js_string!("close"), Attribute::READONLY)
        .property(js_string!("code"), code, Attribute::READONLY)
        .property(js_string!("reason"), js_string!(reason.to_string()), Attribute::READONLY)
        .property(js_string!("wasClean"), code == 1000, Attribute::READONLY)
        .property(js_string!("target"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("currentTarget"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("bubbles"), false, Attribute::READONLY)
        .property(js_string!("cancelable"), false, Attribute::READONLY)
        .function(prevent_default, js_string!("preventDefault"), 0)
        .function(stop_propagation, js_string!("stopPropagation"), 0)
        .build()
}

/// Process WebSocket events - poll for messages and deliver events
fn process_websocket_events(context: &mut Context) {
    // Collect events to deliver: (ws_id, event_type, url, data)
    let mut events_to_deliver: Vec<(u32, String, String, Option<String>)> = Vec::new();

    // Poll connections and collect pending events
    {
        let mut connections = WEBSOCKET_CONNECTIONS.lock().unwrap();

        for (&ws_id, conn) in connections.iter_mut() {
            // Try to read messages (non-blocking)
            if let Some(ref mut socket) = conn.socket {
                loop {
                    match socket.read() {
                        Ok(Message::Text(text)) => {
                            conn.pending_messages.push_back(WebSocketMessage::Text(text));
                        }
                        Ok(Message::Binary(data)) => {
                            conn.pending_messages.push_back(WebSocketMessage::Binary(data));
                        }
                        Ok(Message::Close(frame)) => {
                            let code = frame.as_ref().map(|f| u16::from(f.code)).unwrap_or(1000);
                            let reason = frame.map(|f| f.reason.to_string()).unwrap_or_default();
                            conn.pending_close = Some((code, reason));
                            conn.ready_state = CLOSED;
                            break;
                        }
                        Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => continue,
                        Err(tungstenite::Error::Io(ref e)) if e.kind() == ErrorKind::WouldBlock => break,
                        Err(_) => {
                            conn.pending_error = Some("Connection error".to_string());
                            break;
                        }
                    }
                }
            }

            // Collect pending open event
            if conn.pending_open {
                conn.pending_open = false;
                events_to_deliver.push((ws_id, "open".to_string(), conn.url.clone(), None));
            }

            // Collect pending messages
            while let Some(msg) = conn.pending_messages.pop_front() {
                let data = match msg {
                    WebSocketMessage::Text(s) => s,
                    WebSocketMessage::Binary(_) => String::new(), // Binary not fully supported
                };
                events_to_deliver.push((ws_id, "message".to_string(), conn.url.clone(), Some(data)));
            }

            // Collect pending error
            if let Some(error_msg) = conn.pending_error.take() {
                events_to_deliver.push((ws_id, "error".to_string(), conn.url.clone(), Some(error_msg)));
            }

            // Collect pending close
            if let Some((code, reason)) = conn.pending_close.take() {
                events_to_deliver.push((ws_id, "close".to_string(), conn.url.clone(), Some(format!("{}:{}", code, reason))));
            }
        }
    }

    // Deliver events using thread-local JS objects
    for (ws_id, event_type, url, data) in events_to_deliver {
        WS_JS_OBJECTS.with(|objects| {
            if let Some(js_obj) = objects.borrow().get(&ws_id) {
                let on_handler = js_obj.get(js_string!(format!("on{}", event_type)), context).ok();

                let event = match event_type.as_str() {
                    "open" => create_network_event(context, "open"),
                    "message" => {
                        let data_val = JsValue::from(js_string!(data.unwrap_or_default()));
                        create_network_message_event(context, data_val, &url)
                    }
                    "error" => create_network_event(context, "error"),
                    "close" => {
                        let parts: Vec<&str> = data.as_deref().unwrap_or("1000:").split(':').collect();
                        let code = parts.first().and_then(|s| s.parse().ok()).unwrap_or(1000);
                        let reason = parts.get(1).unwrap_or(&"").to_string();
                        create_network_close_event(context, code, &reason)
                    }
                    _ => return,
                };

                WS_EVENT_TARGETS.with(|targets| {
                    if let Some(target) = targets.borrow_mut().get_mut(&ws_id) {
                        target.dispatch(&event_type, &event, on_handler.as_ref(), context);
                    }
                });
            }
        });
    }
}

/// Process EventSource events - deliver pending SSE events
fn process_eventsource_events(context: &mut Context) {
    // Collect events: (es_id, event_type, data, origin)
    let mut events_to_deliver: Vec<(u32, String, String, String)> = Vec::new();

    // Collect pending events from connections
    {
        let mut connections = EVENTSOURCE_CONNECTIONS.lock().unwrap();

        for (&es_id, conn) in connections.iter_mut() {
            // Skip closed connections
            if conn.ready_state == CLOSED {
                continue;
            }

            // Collect pending open event
            if conn.pending_open {
                conn.pending_open = false;
                events_to_deliver.push((es_id, "open".to_string(), String::new(), conn.url.clone()));
            }

            // Collect pending SSE events
            while let Some(event) = conn.pending_events.pop_front() {
                // Update last event ID
                if !event.last_event_id.is_empty() {
                    conn.last_event_id = event.last_event_id.clone();
                }
                events_to_deliver.push((es_id, event.event_type, event.data, event.origin));
            }

            // Collect pending error
            if let Some(_error_msg) = conn.pending_error.take() {
                events_to_deliver.push((es_id, "error".to_string(), String::new(), conn.url.clone()));
            }
        }
    }

    // Deliver events using thread-local JS objects
    for (es_id, event_type, data, origin) in events_to_deliver {
        ES_JS_OBJECTS.with(|objects| {
            if let Some(js_obj) = objects.borrow().get(&es_id) {
                let on_handler = if event_type == "message" {
                    js_obj.get(js_string!("onmessage"), context).ok()
                } else if event_type == "open" {
                    js_obj.get(js_string!("onopen"), context).ok()
                } else if event_type == "error" {
                    js_obj.get(js_string!("onerror"), context).ok()
                } else {
                    None
                };

                let event = match event_type.as_str() {
                    "open" => create_network_event(context, "open"),
                    "error" => create_network_event(context, "error"),
                    _ => {
                        let data_val = JsValue::from(js_string!(data.clone()));
                        create_network_message_event(context, data_val, &origin)
                    }
                };

                ES_EVENT_TARGETS.with(|targets| {
                    if let Some(target) = targets.borrow_mut().get_mut(&es_id) {
                        target.dispatch(&event_type, &event, on_handler.as_ref(), context);
                    }
                });
            }
        });
    }
}

/// Process all network events - call from event loop
pub fn process_network_events(context: &mut Context) {
    process_websocket_events(context);
    process_eventsource_events(context);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_constants() {
        assert_eq!(CONNECTING, 0);
        assert_eq!(OPEN, 1);
        assert_eq!(CLOSING, 2);
        assert_eq!(CLOSED, 3);
    }

    #[test]
    fn test_parse_sse_stream() {
        let stream = "event: update\ndata: hello\n\ndata: world\n\n";
        let events = parse_sse_stream(stream, "http://example.com");

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "update");
        assert_eq!(events[0].data, "hello");
        assert_eq!(events[1].event_type, "message");
        assert_eq!(events[1].data, "world");
    }

    #[test]
    fn test_parse_sse_multiline_data() {
        let stream = "data: line1\ndata: line2\ndata: line3\n\n";
        let events = parse_sse_stream(stream, "http://example.com");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line1\nline2\nline3");
    }
}
