//! Web Workers API - Worker, SharedWorker, and messaging
//!
//! Provides real multi-threaded worker execution using Rust threads.

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer, property::Attribute,
    Context, JsArgs, JsNativeError, JsObject, JsResult, JsValue, Source,
};
use std::collections::HashMap;
use std::sync::{mpsc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

lazy_static::lazy_static! {
    /// Storage for Worker thread handles and channels
    static ref WORKER_CHANNELS: Mutex<HashMap<u32, WorkerChannel>> = Mutex::new(HashMap::new());
    static ref NEXT_WORKER_ID: Mutex<u32> = Mutex::new(1);

    /// Messages from workers to main thread
    static ref INCOMING_MESSAGES: Mutex<Vec<WorkerIncomingMessage>> = Mutex::new(Vec::new());

    /// Storage for SharedWorker instances by name
    static ref SHARED_WORKER_STORAGE: Mutex<HashMap<String, u32>> = Mutex::new(HashMap::new());

    /// Storage for registered paint worklets (name -> serialized class info)
    static ref PAINT_WORKLET_REGISTRY: Mutex<HashMap<String, PaintWorkletEntry>> = Mutex::new(HashMap::new());

    /// Storage for registered audio processors (name -> serialized class info)
    static ref AUDIO_PROCESSOR_REGISTRY: Mutex<HashMap<String, AudioProcessorEntry>> = Mutex::new(HashMap::new());

    /// Storage for loaded worklet modules
    static ref WORKLET_MODULES: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// Audio worklet global scope state (tracks timing)
    static ref AUDIO_WORKLET_STATE: Mutex<AudioWorkletState> = Mutex::new(AudioWorkletState::new());
}

/// Audio worklet timing state
struct AudioWorkletState {
    start_time: Instant,
    sample_rate: u32,
}

impl AudioWorkletState {
    fn new() -> Self {
        Self {
            start_time: Instant::now(),
            sample_rate: 44100,
        }
    }

    fn current_time(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }

    fn current_frame(&self) -> u64 {
        let elapsed = self.current_time();
        (elapsed * self.sample_rate as f64) as u64
    }
}

/// Registered paint worklet entry
struct PaintWorkletEntry {
    name: String,
    input_properties: Vec<String>,
    input_arguments: Vec<String>,
    context_options: PaintContextOptions,
}

/// Paint rendering context options
#[derive(Clone, Default)]
struct PaintContextOptions {
    alpha: bool,
}

/// Registered audio processor entry
struct AudioProcessorEntry {
    name: String,
    parameter_descriptors: Vec<AudioParamDescriptor>,
}

/// Audio parameter descriptor
#[derive(Clone)]
struct AudioParamDescriptor {
    name: String,
    default_value: f64,
    min_value: f64,
    max_value: f64,
    automation_rate: String, // "a-rate" or "k-rate"
}

/// Thread-safe channel for communicating with workers
struct WorkerChannel {
    sender: mpsc::Sender<WorkerMessage>,
    terminated: bool,
}

/// Message from worker to main thread
#[derive(Clone)]
struct WorkerIncomingMessage {
    worker_id: u32,
    message_type: String, // "message" or "error"
    data: String,
}

#[derive(Clone, Debug)]
enum WorkerMessage {
    PostMessage(String),
    Terminate,
}

/// Register all Worker APIs
pub fn register_all_worker_apis(ctx: &mut Context) -> JsResult<()> {
    register_worker_constructor(ctx)?;
    register_shared_worker_constructor(ctx)?;
    // MessageChannel is already registered by timers.rs, skip duplicate
    // register_message_channel(ctx)?;
    register_message_port(ctx)?;
    register_worker_global_scope(ctx)?;
    register_dedicated_worker_global_scope(ctx)?;
    register_worker_location(ctx)?;
    register_worker_navigator(ctx)?;
    register_worklet(ctx)?;
    register_paint_worklet_global_scope(ctx)?;
    register_audio_worklet_global_scope(ctx)?;
    Ok(())
}

/// Register the Worker constructor
fn register_worker_constructor(ctx: &mut Context) -> JsResult<()> {
    let worker_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let script_url = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let options = args.get_or_undefined(1);

        // Parse options
        let worker_type = if let Some(opts) = options.as_object() {
            let type_val = opts.get(js_string!("type"), ctx)?;
            if !type_val.is_undefined() {
                type_val.to_string(ctx)?.to_std_string_escaped()
            } else {
                "classic".to_string()
            }
        } else {
            "classic".to_string()
        };

        let worker_name = if let Some(opts) = options.as_object() {
            let name_val = opts.get(js_string!("name"), ctx)?;
            if !name_val.is_undefined() {
                name_val.to_string(ctx)?.to_std_string_escaped()
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Create worker ID
        let worker_id = {
            let mut id = NEXT_WORKER_ID.lock().unwrap();
            let current = *id;
            *id += 1;
            current
        };

        // Create message channel
        let (main_to_worker_tx, main_to_worker_rx) = mpsc::channel::<WorkerMessage>();

        // Clone script URL for thread
        let script_url_clone = script_url.clone();
        let worker_id_clone = worker_id;

        // Spawn worker thread
        let _thread_handle = thread::spawn(move || {
            run_worker_thread(worker_id_clone, script_url_clone, main_to_worker_rx);
        });

        // Store worker channel
        {
            let mut channels = WORKER_CHANNELS.lock().unwrap();
            channels.insert(worker_id, WorkerChannel {
                sender: main_to_worker_tx,
                terminated: false,
            });
        }

        // Create Worker object
        create_worker_object(ctx, worker_id, &worker_name, &worker_type)
    });

    ctx.register_global_builtin_callable(js_string!("Worker"), 1, worker_constructor)?;
    Ok(())
}

/// Create a Worker JS object
fn create_worker_object(ctx: &mut Context, worker_id: u32, _name: &str, _worker_type: &str) -> JsResult<JsValue> {
    // postMessage(message, transfer?)
    let post_message_id = worker_id;
    let post_message = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let message = args.get_or_undefined(0);
        let message_str = serialize_message(message, ctx)?;

        let channels = WORKER_CHANNELS.lock().unwrap();
        if let Some(channel) = channels.get(&post_message_id) {
            if !channel.terminated {
                let _ = channel.sender.send(WorkerMessage::PostMessage(message_str));
            }
        }

        Ok(JsValue::undefined())
    });

    // terminate()
    let terminate_id = worker_id;
    let terminate = NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
        let mut channels = WORKER_CHANNELS.lock().unwrap();
        if let Some(channel) = channels.get_mut(&terminate_id) {
            channel.terminated = true;
            let _ = channel.sender.send(WorkerMessage::Terminate);
        }
        Ok(JsValue::undefined())
    });

    // addEventListener
    let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        // Event listeners are handled via onmessage property
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

    let worker = ObjectInitializer::new(ctx)
        .property(js_string!("_workerId"), JsValue::from(worker_id), Attribute::empty())
        .function(post_message, js_string!("postMessage"), 1)
        .function(terminate, js_string!("terminate"), 0)
        .function(add_event_listener, js_string!("addEventListener"), 2)
        .function(remove_event_listener, js_string!("removeEventListener"), 2)
        .function(dispatch_event, js_string!("dispatchEvent"), 1)
        .property(js_string!("onmessage"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
        .property(js_string!("onerror"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
        .property(js_string!("onmessageerror"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
        .build();

    Ok(worker.into())
}

/// Run a worker in its own thread with its own JS context
fn run_worker_thread(
    worker_id: u32,
    script_url: String,
    receiver: mpsc::Receiver<WorkerMessage>,
) {
    // Create a new JS context for this worker
    let mut context = Context::default();

    // Set up worker globals (self, postMessage, close, etc.)
    setup_worker_globals(&mut context, worker_id);

    // Try to load and execute the script
    let script_result = if script_url.starts_with("data:") {
        // Handle data URLs
        if let Some(code) = parse_data_url(&script_url) {
            context.eval(Source::from_bytes(&code))
        } else {
            Ok(JsValue::undefined())
        }
    } else if script_url.starts_with("blob:") {
        // Handle blob URLs - would need blob storage integration
        Ok(JsValue::undefined())
    } else {
        // For external URLs, we'd need to fetch them
        log::warn!("Worker: Cannot fetch external script: {}", script_url);
        Ok(JsValue::undefined())
    };

    if let Err(e) = script_result {
        log::error!("Worker script error: {:?}", e);
        // Queue error message
        let mut messages = INCOMING_MESSAGES.lock().unwrap();
        messages.push(WorkerIncomingMessage {
            worker_id,
            message_type: "error".to_string(),
            data: format!("{:?}", e),
        });
    }

    // Message loop
    loop {
        match receiver.recv() {
            Ok(WorkerMessage::PostMessage(msg)) => {
                // Trigger onmessage in worker
                dispatch_message_in_worker(&mut context, worker_id, &msg);
            }
            Ok(WorkerMessage::Terminate) => {
                break;
            }
            Err(_) => {
                // Channel closed
                break;
            }
        }
    }
}

/// Set up worker global scope
fn setup_worker_globals(ctx: &mut Context, worker_id: u32) {
    // self reference - points to global
    let _ = ctx.register_global_property(
        js_string!("self"),
        ctx.global_object().clone(),
        Attribute::READONLY,
    );

    // postMessage function
    let post_message = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let message = args.get_or_undefined(0);
        let msg_str = serialize_message(message, ctx)?;

        // Queue message to main thread
        let mut messages = INCOMING_MESSAGES.lock().unwrap();
        messages.push(WorkerIncomingMessage {
            worker_id,
            message_type: "message".to_string(),
            data: msg_str,
        });

        Ok(JsValue::undefined())
    });
    let _ = ctx.register_global_callable(js_string!("postMessage"), 1, post_message);

    // close function
    let close = NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
        // Mark worker as terminated
        let mut channels = WORKER_CHANNELS.lock().unwrap();
        if let Some(channel) = channels.get_mut(&worker_id) {
            channel.terminated = true;
        }
        Ok(JsValue::undefined())
    });
    let _ = ctx.register_global_callable(js_string!("close"), 0, close);

    // importScripts function
    let import_scripts = NativeFunction::from_copy_closure(|_this, args, ctx| {
        for i in 0..args.len() {
            let url = args.get_or_undefined(i).to_string(ctx)?.to_std_string_escaped();
            log::warn!("importScripts not fully implemented: {}", url);
        }
        Ok(JsValue::undefined())
    });
    let _ = ctx.register_global_callable(js_string!("importScripts"), 1, import_scripts);

    // WorkerGlobalScope properties
    let _ = ctx.register_global_property(js_string!("name"), js_string!(""), Attribute::READONLY);
    let _ = ctx.register_global_property(js_string!("onmessage"), JsValue::null(), Attribute::WRITABLE);
    let _ = ctx.register_global_property(js_string!("onerror"), JsValue::null(), Attribute::WRITABLE);
    let _ = ctx.register_global_property(js_string!("onmessageerror"), JsValue::null(), Attribute::WRITABLE);

    // navigator (minimal)
    let navigator = ObjectInitializer::new(ctx)
        .property(js_string!("hardwareConcurrency"), JsValue::from(num_cpus()), Attribute::READONLY)
        .property(js_string!("userAgent"), js_string!("SemanticBrowser/1.0"), Attribute::READONLY)
        .property(js_string!("language"), js_string!("en-US"), Attribute::READONLY)
        .property(js_string!("languages"), JsValue::undefined(), Attribute::READONLY)
        .property(js_string!("onLine"), JsValue::from(true), Attribute::READONLY)
        .build();
    let _ = ctx.register_global_property(js_string!("navigator"), navigator, Attribute::READONLY);

    // location (minimal)
    let location = ObjectInitializer::new(ctx)
        .property(js_string!("href"), js_string!("about:blank"), Attribute::READONLY)
        .property(js_string!("origin"), js_string!("null"), Attribute::READONLY)
        .property(js_string!("protocol"), js_string!("about:"), Attribute::READONLY)
        .property(js_string!("host"), js_string!(""), Attribute::READONLY)
        .property(js_string!("hostname"), js_string!(""), Attribute::READONLY)
        .property(js_string!("port"), js_string!(""), Attribute::READONLY)
        .property(js_string!("pathname"), js_string!("blank"), Attribute::READONLY)
        .property(js_string!("search"), js_string!(""), Attribute::READONLY)
        .property(js_string!("hash"), js_string!(""), Attribute::READONLY)
        .build();
    let _ = ctx.register_global_property(js_string!("location"), location, Attribute::READONLY);

    // performance.now()
    let start_time = std::time::Instant::now();
    let now = NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
        let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;
        Ok(JsValue::from(elapsed))
    });
    let performance = ObjectInitializer::new(ctx)
        .function(now, js_string!("now"), 0)
        .build();
    let _ = ctx.register_global_property(js_string!("performance"), performance, Attribute::READONLY);

    // console
    let console_log = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let mut output = String::new();
        for i in 0..args.len() {
            if i > 0 { output.push(' '); }
            output.push_str(&args.get_or_undefined(i).to_string(ctx)?.to_std_string_escaped());
        }
        log::info!("[Worker] {}", output);
        Ok(JsValue::undefined())
    });
    let console = ObjectInitializer::new(ctx)
        .function(console_log.clone(), js_string!("log"), 1)
        .function(console_log.clone(), js_string!("info"), 1)
        .function(console_log.clone(), js_string!("warn"), 1)
        .function(console_log.clone(), js_string!("error"), 1)
        .function(console_log, js_string!("debug"), 1)
        .build();
    let _ = ctx.register_global_property(js_string!("console"), console, Attribute::READONLY);

    // setTimeout/setInterval (simplified - immediate execution)
    let set_timeout = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let callback = args.get_or_undefined(0);
        if let Some(func) = callback.as_callable() {
            let _ = func.call(&JsValue::undefined(), &[], ctx);
        }
        Ok(JsValue::from(1)) // Return timer ID
    });
    let _ = ctx.register_global_callable(js_string!("setTimeout"), 2, set_timeout);

    let clear_timeout = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = ctx.register_global_callable(js_string!("clearTimeout"), 1, clear_timeout.clone());
    let _ = ctx.register_global_callable(js_string!("clearInterval"), 1, clear_timeout);

    // atob/btoa
    let btoa = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let input = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let encoded = base64_encode(input.as_bytes());
        Ok(JsValue::from(js_string!(encoded)))
    });
    let _ = ctx.register_global_callable(js_string!("btoa"), 1, btoa);

    let atob = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let input = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        match base64_decode(&input) {
            Some(decoded) => Ok(JsValue::from(js_string!(decoded))),
            None => Ok(JsValue::from(js_string!(""))),
        }
    });
    let _ = ctx.register_global_callable(js_string!("atob"), 1, atob);
}

/// Dispatch a message to the worker's onmessage handler
fn dispatch_message_in_worker(ctx: &mut Context, _worker_id: u32, message: &str) {
    // Get onmessage handler
    let global = ctx.global_object();
    if let Ok(onmessage) = global.get(js_string!("onmessage"), ctx) {
        if let Some(func) = onmessage.as_callable() {
            // Create MessageEvent
            if let Ok(event_val) = create_message_event(ctx, message) {
                let _ = func.call(&JsValue::undefined(), &[event_val], ctx);
            }
        }
    }
}

/// Create a MessageEvent object
fn create_message_event(ctx: &mut Context, data_str: &str) -> JsResult<JsValue> {
    let data = deserialize_message(data_str, ctx)?;

    let ports_array = ObjectInitializer::new(ctx)
        .property(js_string!("length"), JsValue::from(0), Attribute::READONLY)
        .build();

    let event = ObjectInitializer::new(ctx)
        .property(js_string!("type"), js_string!("message"), Attribute::READONLY)
        .property(js_string!("data"), data, Attribute::READONLY)
        .property(js_string!("origin"), js_string!(""), Attribute::READONLY)
        .property(js_string!("lastEventId"), js_string!(""), Attribute::READONLY)
        .property(js_string!("source"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("ports"), ports_array, Attribute::READONLY)
        .property(js_string!("bubbles"), JsValue::from(false), Attribute::READONLY)
        .property(js_string!("cancelable"), JsValue::from(false), Attribute::READONLY)
        .property(js_string!("defaultPrevented"), JsValue::from(false), Attribute::READONLY)
        .property(js_string!("isTrusted"), JsValue::from(true), Attribute::READONLY)
        .property(js_string!("timeStamp"), JsValue::from(0.0), Attribute::READONLY)
        .build();

    Ok(event.into())
}

/// Register SharedWorker constructor
fn register_shared_worker_constructor(ctx: &mut Context) -> JsResult<()> {
    let shared_worker_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let script_url = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let name = if args.len() > 1 && !args.get_or_undefined(1).is_undefined() {
            args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped()
        } else {
            script_url.clone()
        };

        // Check if shared worker with this name already exists or create new
        let port_id = {
            let mut storage = SHARED_WORKER_STORAGE.lock().unwrap();
            if let Some(&existing_id) = storage.get(&name) {
                existing_id
            } else {
                // Create new shared worker
                let (tx, rx) = mpsc::channel::<WorkerMessage>();

                let port_id = {
                    let mut id = NEXT_WORKER_ID.lock().unwrap();
                    let current = *id;
                    *id += 1;
                    current
                };

                // Store channel
                {
                    let mut channels = WORKER_CHANNELS.lock().unwrap();
                    channels.insert(port_id, WorkerChannel {
                        sender: tx,
                        terminated: false,
                    });
                }

                let script_url_clone = script_url.clone();
                let port_id_clone = port_id;
                let _thread_handle = thread::spawn(move || {
                    run_shared_worker_thread(port_id_clone, script_url_clone, rx);
                });

                storage.insert(name.clone(), port_id);
                port_id
            }
        };

        // Create SharedWorker object with port
        let port = create_message_port(ctx, port_id)?;

        let shared_worker = ObjectInitializer::new(ctx)
            .property(js_string!("port"), port, Attribute::READONLY)
            .property(js_string!("onerror"), JsValue::null(), Attribute::WRITABLE)
            .build();

        Ok(shared_worker.into())
    });

    ctx.register_global_builtin_callable(js_string!("SharedWorker"), 2, shared_worker_constructor)?;
    Ok(())
}

/// Run a shared worker thread
fn run_shared_worker_thread(
    worker_id: u32,
    script_url: String,
    receiver: mpsc::Receiver<WorkerMessage>,
) {
    let mut context = Context::default();

    // Set up shared worker globals
    setup_worker_globals(&mut context, worker_id);

    // Add onconnect handler support
    let _ = context.register_global_property(js_string!("onconnect"), JsValue::null(), Attribute::WRITABLE);

    // Load script
    if script_url.starts_with("data:") {
        if let Some(code) = parse_data_url(&script_url) {
            let _ = context.eval(Source::from_bytes(&code));
        }
    }

    // Message loop
    loop {
        match receiver.recv() {
            Ok(WorkerMessage::PostMessage(msg)) => {
                dispatch_message_in_worker(&mut context, worker_id, &msg);
            }
            Ok(WorkerMessage::Terminate) => break,
            Err(_) => break,
        }
    }
}

/// Register MessageChannel constructor
fn register_message_channel(ctx: &mut Context) -> JsResult<()> {
    let message_channel_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let port1_id = {
            let mut id = NEXT_WORKER_ID.lock().unwrap();
            let current = *id;
            *id += 2;
            current
        };
        let port2_id = port1_id + 1;

        let port1 = create_message_port(ctx, port1_id)?;
        let port2 = create_message_port(ctx, port2_id)?;

        let channel = ObjectInitializer::new(ctx)
            .property(js_string!("port1"), port1, Attribute::READONLY)
            .property(js_string!("port2"), port2, Attribute::READONLY)
            .build();

        Ok(channel.into())
    });

    ctx.register_global_builtin_callable(js_string!("MessageChannel"), 0, message_channel_constructor)?;
    Ok(())
}

/// Create a MessagePort object
fn create_message_port(ctx: &mut Context, port_id: u32) -> JsResult<JsValue> {
    let post_message_id = port_id;
    let post_message = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let message = args.get_or_undefined(0);
        let msg_str = serialize_message(message, ctx)?;

        // Try to send to the worker channel
        let channels = WORKER_CHANNELS.lock().unwrap();
        if let Some(channel) = channels.get(&post_message_id) {
            let _ = channel.sender.send(WorkerMessage::PostMessage(msg_str));
        }

        Ok(JsValue::undefined())
    });

    let start = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let close_id = port_id;
    let close = NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
        let mut channels = WORKER_CHANNELS.lock().unwrap();
        if let Some(channel) = channels.get_mut(&close_id) {
            channel.terminated = true;
        }
        Ok(JsValue::undefined())
    });

    let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let remove_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let port = ObjectInitializer::new(ctx)
        .property(js_string!("_portId"), JsValue::from(port_id), Attribute::empty())
        .function(post_message, js_string!("postMessage"), 1)
        .function(start, js_string!("start"), 0)
        .function(close, js_string!("close"), 0)
        .function(add_event_listener, js_string!("addEventListener"), 2)
        .function(remove_event_listener, js_string!("removeEventListener"), 2)
        .property(js_string!("onmessage"), JsValue::null(), Attribute::WRITABLE)
        .property(js_string!("onmessageerror"), JsValue::null(), Attribute::WRITABLE)
        .build();

    Ok(port.into())
}

/// Register MessagePort constructor (for standalone use)
fn register_message_port(_ctx: &mut Context) -> JsResult<()> {
    // MessagePort is already registered in network.rs
    // Skip to avoid duplicate registration
    Ok(())
}

/// Register WorkerGlobalScope constructor
/// This is the base interface for all worker global scopes
fn register_worker_global_scope(ctx: &mut Context) -> JsResult<()> {
    use boa_engine::object::FunctionObjectBuilder;

    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // WorkerGlobalScope is abstract - cannot be constructed directly
        // But we create an object with all the expected properties

        // Create importScripts function
        let import_scripts = NativeFunction::from_copy_closure(|_this, args, ctx| {
            for i in 0..args.len() {
                let _url = args.get_or_undefined(i).to_string(ctx)?.to_std_string_escaped();
                // In a real implementation, this would fetch and execute the scripts
            }
            Ok(JsValue::undefined())
        }).to_js_function(ctx.realm());

        // Create close function
        let close = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        }).to_js_function(ctx.realm());

        // Create WorkerNavigator and WorkerLocation before building the scope
        let navigator = create_worker_navigator_object_inline(ctx);
        let location = create_worker_location_object_inline(ctx, "about:blank");

        let scope = ObjectInitializer::new(ctx)
            // self reference
            .property(js_string!("self"), JsValue::undefined(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            // Location and navigator
            .property(js_string!("location"), location, Attribute::READONLY)
            .property(js_string!("navigator"), navigator, Attribute::READONLY)
            // Methods
            .property(js_string!("importScripts"), JsValue::from(import_scripts), Attribute::all())
            .property(js_string!("close"), JsValue::from(close), Attribute::all())
            // Event handlers
            .property(js_string!("onerror"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("onlanguagechange"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("onoffline"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("ononline"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("onrejectionhandled"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("onunhandledrejection"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            // isSecureContext
            .property(js_string!("isSecureContext"), JsValue::from(true), Attribute::READONLY)
            // origin
            .property(js_string!("origin"), js_string!("null"), Attribute::READONLY)
            .build();

        Ok(JsValue::from(scope))
    });

    let ctor = FunctionObjectBuilder::new(ctx.realm(), constructor)
        .name(js_string!("WorkerGlobalScope"))
        .length(0)
        .constructor(true)
        .build();

    ctx.global_object().set(js_string!("WorkerGlobalScope"), ctor, false, ctx)?;
    Ok(())
}

/// Create a simple WorkerNavigator object (inline version for constructors)
fn create_worker_navigator_object_inline(ctx: &mut Context) -> JsValue {
    let hardware_concurrency = num_cpus();

    let navigator = ObjectInitializer::new(ctx)
        .property(js_string!("appCodeName"), js_string!("Mozilla"), Attribute::READONLY)
        .property(js_string!("appName"), js_string!("Netscape"), Attribute::READONLY)
        .property(js_string!("appVersion"), js_string!("5.0 (Windows)"), Attribute::READONLY)
        .property(js_string!("platform"), js_string!("Win32"), Attribute::READONLY)
        .property(js_string!("product"), js_string!("Gecko"), Attribute::READONLY)
        .property(js_string!("userAgent"), js_string!("Mozilla/5.0 SemanticBrowser/1.0"), Attribute::READONLY)
        .property(js_string!("language"), js_string!("en-US"), Attribute::READONLY)
        .property(js_string!("onLine"), JsValue::from(true), Attribute::READONLY)
        .property(js_string!("hardwareConcurrency"), JsValue::from(hardware_concurrency), Attribute::READONLY)
        .property(js_string!("deviceMemory"), JsValue::from(8), Attribute::READONLY)
        .build();

    JsValue::from(navigator)
}

/// Create a simple WorkerLocation object (inline version for constructors)
fn create_worker_location_object_inline(ctx: &mut Context, href: &str) -> JsValue {
    let location = ObjectInitializer::new(ctx)
        .property(js_string!("href"), js_string!(href), Attribute::READONLY)
        .property(js_string!("origin"), js_string!("null"), Attribute::READONLY)
        .property(js_string!("protocol"), js_string!("about:"), Attribute::READONLY)
        .property(js_string!("host"), js_string!(""), Attribute::READONLY)
        .property(js_string!("hostname"), js_string!(""), Attribute::READONLY)
        .property(js_string!("port"), js_string!(""), Attribute::READONLY)
        .property(js_string!("pathname"), js_string!("blank"), Attribute::READONLY)
        .property(js_string!("search"), js_string!(""), Attribute::READONLY)
        .property(js_string!("hash"), js_string!(""), Attribute::READONLY)
        .build();

    JsValue::from(location)
}

/// Register DedicatedWorkerGlobalScope constructor
/// This extends WorkerGlobalScope for dedicated workers
fn register_dedicated_worker_global_scope(ctx: &mut Context) -> JsResult<()> {
    use boa_engine::object::FunctionObjectBuilder;

    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = if args.len() > 0 && !args.get_or_undefined(0).is_undefined() {
            args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped()
        } else {
            String::new()
        };

        // Create postMessage function
        let post_message = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let message = args.get_or_undefined(0);
            let _msg_str = serialize_message(message, ctx)?;
            // In worker context, this would send to parent
            Ok(JsValue::undefined())
        }).to_js_function(ctx.realm());

        // Create close function
        let close = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        }).to_js_function(ctx.realm());

        // Create importScripts function
        let import_scripts = NativeFunction::from_copy_closure(|_this, args, ctx| {
            for i in 0..args.len() {
                let _url = args.get_or_undefined(i).to_string(ctx)?.to_std_string_escaped();
            }
            Ok(JsValue::undefined())
        }).to_js_function(ctx.realm());

        // Create WorkerNavigator and WorkerLocation using inline versions
        let navigator = create_worker_navigator_object_inline(ctx);
        let location = create_worker_location_object_inline(ctx, "about:blank");

        // Need to convert name to js_string before building scope
        let name_js = js_string!(name);

        let scope = ObjectInitializer::new(ctx)
            // DedicatedWorkerGlobalScope specific
            .property(js_string!("name"), name_js, Attribute::READONLY)
            .property(js_string!("postMessage"), JsValue::from(post_message), Attribute::all())
            // Event handlers
            .property(js_string!("onmessage"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("onmessageerror"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            // Inherited from WorkerGlobalScope
            .property(js_string!("self"), JsValue::undefined(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("location"), location, Attribute::READONLY)
            .property(js_string!("navigator"), navigator, Attribute::READONLY)
            .property(js_string!("close"), JsValue::from(close), Attribute::all())
            .property(js_string!("importScripts"), JsValue::from(import_scripts), Attribute::all())
            .property(js_string!("onerror"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("onlanguagechange"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("onoffline"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("ononline"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("onrejectionhandled"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("onunhandledrejection"), JsValue::null(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("isSecureContext"), JsValue::from(true), Attribute::READONLY)
            .property(js_string!("origin"), js_string!("null"), Attribute::READONLY)
            .build();

        Ok(JsValue::from(scope))
    });

    let ctor = FunctionObjectBuilder::new(ctx.realm(), constructor)
        .name(js_string!("DedicatedWorkerGlobalScope"))
        .length(0)
        .constructor(true)
        .build();

    ctx.global_object().set(js_string!("DedicatedWorkerGlobalScope"), ctor, false, ctx)?;
    Ok(())
}

/// Register WorkerLocation constructor
fn register_worker_location(ctx: &mut Context) -> JsResult<()> {
    use boa_engine::object::FunctionObjectBuilder;

    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let href = if args.len() > 0 && !args.get_or_undefined(0).is_undefined() {
            args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped()
        } else {
            "about:blank".to_string()
        };

        Ok(JsValue::from(create_worker_location_object(ctx, &href)))
    });

    let ctor = FunctionObjectBuilder::new(ctx.realm(), constructor)
        .name(js_string!("WorkerLocation"))
        .length(0)
        .constructor(true)
        .build();

    ctx.global_object().set(js_string!("WorkerLocation"), ctor, false, ctx)?;
    Ok(())
}

/// Create a WorkerLocation object with URL parsing
fn create_worker_location_object(ctx: &mut Context, href: &str) -> JsValue {
    // Parse the URL
    let (protocol, host, hostname, port, pathname, search, hash, origin) =
        parse_url_components(href);

    // Create toString function using unsafe closure for String capture
    let href_clone = href.to_string();
    let to_string = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(href_clone.clone())))
        })
    }.to_js_function(ctx.realm());

    let location = ObjectInitializer::new(ctx)
        .property(js_string!("href"), js_string!(href), Attribute::READONLY)
        .property(js_string!("origin"), js_string!(origin), Attribute::READONLY)
        .property(js_string!("protocol"), js_string!(protocol), Attribute::READONLY)
        .property(js_string!("host"), js_string!(host), Attribute::READONLY)
        .property(js_string!("hostname"), js_string!(hostname), Attribute::READONLY)
        .property(js_string!("port"), js_string!(port), Attribute::READONLY)
        .property(js_string!("pathname"), js_string!(pathname), Attribute::READONLY)
        .property(js_string!("search"), js_string!(search), Attribute::READONLY)
        .property(js_string!("hash"), js_string!(hash), Attribute::READONLY)
        .property(js_string!("toString"), JsValue::from(to_string), Attribute::all())
        .build();

    JsValue::from(location)
}

/// Parse URL components
fn parse_url_components(href: &str) -> (String, String, String, String, String, String, String, String) {
    if let Ok(url) = url::Url::parse(href) {
        let protocol = format!("{}:", url.scheme());
        let hostname = url.host_str().unwrap_or("").to_string();
        let port = url.port().map(|p| p.to_string()).unwrap_or_default();
        let host = if port.is_empty() {
            hostname.clone()
        } else {
            format!("{}:{}", hostname, port)
        };
        let pathname = url.path().to_string();
        let search = url.query().map(|q| format!("?{}", q)).unwrap_or_default();
        let hash = url.fragment().map(|f| format!("#{}", f)).unwrap_or_default();
        let origin = url.origin().unicode_serialization();

        (protocol, host, hostname, port, pathname, search, hash, origin)
    } else {
        // Default for invalid URLs
        ("about:".to_string(), "".to_string(), "".to_string(), "".to_string(),
         "blank".to_string(), "".to_string(), "".to_string(), "null".to_string())
    }
}

/// Register WorkerNavigator constructor
fn register_worker_navigator(ctx: &mut Context) -> JsResult<()> {
    use boa_engine::object::FunctionObjectBuilder;

    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_worker_navigator_object(ctx)))
    });

    let ctor = FunctionObjectBuilder::new(ctx.realm(), constructor)
        .name(js_string!("WorkerNavigator"))
        .length(0)
        .constructor(true)
        .build();

    ctx.global_object().set(js_string!("WorkerNavigator"), ctor, false, ctx)?;
    Ok(())
}

/// Create a WorkerNavigator object with all standard properties
fn create_worker_navigator_object(ctx: &mut Context) -> JsValue {
    let hardware_concurrency = num_cpus();

    // Create languages array
    let languages = ObjectInitializer::new(ctx)
        .property(js_string!("0"), js_string!("en-US"), Attribute::all())
        .property(js_string!("1"), js_string!("en"), Attribute::all())
        .property(js_string!("length"), JsValue::from(2), Attribute::READONLY)
        .build();

    // Create connection object (NetworkInformation)
    let connection = ObjectInitializer::new(ctx)
        .property(js_string!("effectiveType"), js_string!("4g"), Attribute::READONLY)
        .property(js_string!("downlink"), JsValue::from(10.0), Attribute::READONLY)
        .property(js_string!("rtt"), JsValue::from(50), Attribute::READONLY)
        .property(js_string!("saveData"), JsValue::from(false), Attribute::READONLY)
        .property(js_string!("type"), js_string!("wifi"), Attribute::READONLY)
        .property(js_string!("onchange"), JsValue::null(), Attribute::WRITABLE)
        .build();

    // Create storage manager stub with simple estimate function
    let estimate = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Create estimate result - simple promise-like
        let result = ObjectInitializer::new(ctx)
            .property(js_string!("quota"), JsValue::from(1073741824i64), Attribute::READONLY)
            .property(js_string!("usage"), JsValue::from(0), Attribute::READONLY)
            .build();

        // Simple then method
        let then = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(cb) = args.get_or_undefined(0).as_callable() {
                let est = ObjectInitializer::new(ctx)
                    .property(js_string!("quota"), JsValue::from(1073741824i64), Attribute::READONLY)
                    .property(js_string!("usage"), JsValue::from(0), Attribute::READONLY)
                    .build();
                let _ = cb.call(&JsValue::undefined(), &[JsValue::from(est)], ctx);
            }
            Ok(JsValue::undefined())
        }).to_js_function(ctx.realm());

        let promise = ObjectInitializer::new(ctx)
            .property(js_string!("then"), JsValue::from(then), Attribute::all())
            .build();

        Ok(JsValue::from(promise))
    }).to_js_function(ctx.realm());

    let storage = ObjectInitializer::new(ctx)
        .property(js_string!("estimate"), JsValue::from(estimate), Attribute::all())
        .build();

    // Create locks manager stub with simple functions
    let request_lock = NativeFunction::from_copy_closure(|_this, args, ctx| {
        if let Some(callback) = args.get_or_undefined(1).as_callable() {
            let lock = ObjectInitializer::new(ctx)
                .property(js_string!("name"), args.get_or_undefined(0).clone(), Attribute::READONLY)
                .property(js_string!("mode"), js_string!("exclusive"), Attribute::READONLY)
                .build();
            let _ = callback.call(&JsValue::undefined(), &[JsValue::from(lock)], ctx);
        }

        let then = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(cb) = args.get_or_undefined(0).as_callable() {
                let _ = cb.call(&JsValue::undefined(), &[], ctx);
            }
            Ok(JsValue::undefined())
        }).to_js_function(ctx.realm());

        let promise = ObjectInitializer::new(ctx)
            .property(js_string!("then"), JsValue::from(then), Attribute::all())
            .build();

        Ok(JsValue::from(promise))
    }).to_js_function(ctx.realm());

    let query_locks = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let then = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(cb) = args.get_or_undefined(0).as_callable() {
                let held = ObjectInitializer::new(ctx)
                    .property(js_string!("length"), JsValue::from(0), Attribute::READONLY)
                    .build();
                let pending = ObjectInitializer::new(ctx)
                    .property(js_string!("length"), JsValue::from(0), Attribute::READONLY)
                    .build();
                let r = ObjectInitializer::new(ctx)
                    .property(js_string!("held"), JsValue::from(held), Attribute::READONLY)
                    .property(js_string!("pending"), JsValue::from(pending), Attribute::READONLY)
                    .build();
                let _ = cb.call(&JsValue::undefined(), &[JsValue::from(r)], ctx);
            }
            Ok(JsValue::undefined())
        }).to_js_function(ctx.realm());

        let promise = ObjectInitializer::new(ctx)
            .property(js_string!("then"), JsValue::from(then), Attribute::all())
            .build();

        Ok(JsValue::from(promise))
    }).to_js_function(ctx.realm());

    let locks = ObjectInitializer::new(ctx)
        .property(js_string!("request"), JsValue::from(request_lock), Attribute::all())
        .property(js_string!("query"), JsValue::from(query_locks), Attribute::all())
        .build();

    // Create permissions stub
    let query_permission = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = if let Some(obj) = args.get_or_undefined(0).as_object() {
            obj.get(js_string!("name"), ctx)
                .unwrap_or(JsValue::undefined())
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default()
        } else {
            String::new()
        };

        let state = match name.as_str() {
            "notifications" => "denied",
            "geolocation" => "denied",
            _ => "granted",
        };

        let then = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(cb) = args.get_or_undefined(0).as_callable() {
                let s = ObjectInitializer::new(ctx)
                    .property(js_string!("state"), js_string!("granted"), Attribute::READONLY)
                    .property(js_string!("onchange"), JsValue::null(), Attribute::WRITABLE)
                    .build();
                let _ = cb.call(&JsValue::undefined(), &[JsValue::from(s)], ctx);
            }
            Ok(JsValue::undefined())
        }).to_js_function(ctx.realm());

        let promise = ObjectInitializer::new(ctx)
            .property(js_string!("then"), JsValue::from(then), Attribute::all())
            .build();

        Ok(JsValue::from(promise))
    }).to_js_function(ctx.realm());

    let permissions = ObjectInitializer::new(ctx)
        .property(js_string!("query"), JsValue::from(query_permission), Attribute::all())
        .build();

    // Main navigator object
    let navigator = ObjectInitializer::new(ctx)
        // Legacy properties
        .property(js_string!("appCodeName"), js_string!("Mozilla"), Attribute::READONLY)
        .property(js_string!("appName"), js_string!("Netscape"), Attribute::READONLY)
        .property(js_string!("appVersion"), js_string!("5.0 (Windows)"), Attribute::READONLY)
        .property(js_string!("platform"), js_string!("Win32"), Attribute::READONLY)
        .property(js_string!("product"), js_string!("Gecko"), Attribute::READONLY)
        .property(js_string!("productSub"), js_string!("20030107"), Attribute::READONLY)
        .property(js_string!("vendor"), js_string!("SemanticBrowser"), Attribute::READONLY)
        .property(js_string!("vendorSub"), js_string!(""), Attribute::READONLY)
        // Standard properties
        .property(js_string!("userAgent"), js_string!("Mozilla/5.0 (Windows NT 10.0; Win64; x64) SemanticBrowser/1.0"), Attribute::READONLY)
        .property(js_string!("language"), js_string!("en-US"), Attribute::READONLY)
        .property(js_string!("languages"), JsValue::from(languages), Attribute::READONLY)
        .property(js_string!("onLine"), JsValue::from(true), Attribute::READONLY)
        .property(js_string!("hardwareConcurrency"), JsValue::from(hardware_concurrency), Attribute::READONLY)
        .property(js_string!("deviceMemory"), JsValue::from(8), Attribute::READONLY)
        .property(js_string!("maxTouchPoints"), JsValue::from(0), Attribute::READONLY)
        // Network Information API
        .property(js_string!("connection"), JsValue::from(connection), Attribute::READONLY)
        // Storage API
        .property(js_string!("storage"), JsValue::from(storage), Attribute::READONLY)
        // Web Locks API
        .property(js_string!("locks"), JsValue::from(locks), Attribute::READONLY)
        // Permissions API
        .property(js_string!("permissions"), JsValue::from(permissions), Attribute::READONLY)
        .build();

    JsValue::from(navigator)
}

/// Serialize a JS value to a string for message passing (structured clone simulation)
fn serialize_message(value: &JsValue, ctx: &mut Context) -> JsResult<String> {
    if value.is_undefined() {
        return Ok("undefined".to_string());
    }
    if value.is_null() {
        return Ok("null".to_string());
    }
    if let Some(s) = value.as_string() {
        return Ok(format!("\"{}\"", s.to_std_string_escaped().replace('\\', "\\\\").replace('"', "\\\"")));
    }
    if let Some(n) = value.as_number() {
        return Ok(n.to_string());
    }
    if let Some(b) = value.as_boolean() {
        return Ok(b.to_string());
    }

    // For objects/arrays, try JSON.stringify
    if value.is_object() {
        let json = ctx.global_object().get(js_string!("JSON"), ctx)?;
        if let Some(json_obj) = json.as_object() {
            let stringify = json_obj.get(js_string!("stringify"), ctx)?;
            if let Some(func) = stringify.as_callable() {
                if let Ok(result) = func.call(&json, &[value.clone()], ctx) {
                    if !result.is_undefined() {
                        return Ok(result.to_string(ctx)?.to_std_string_escaped());
                    }
                }
            }
        }
    }

    Ok(value.to_string(ctx)?.to_std_string_escaped())
}

/// Deserialize a string message back to a JS value
fn deserialize_message(msg: &str, ctx: &mut Context) -> JsResult<JsValue> {
    if msg == "undefined" {
        return Ok(JsValue::undefined());
    }
    if msg == "null" {
        return Ok(JsValue::null());
    }
    if msg == "true" {
        return Ok(JsValue::from(true));
    }
    if msg == "false" {
        return Ok(JsValue::from(false));
    }

    // Try to parse as number
    if let Ok(n) = msg.parse::<f64>() {
        return Ok(JsValue::from(n));
    }

    // Try JSON.parse for objects/arrays/strings
    let json = ctx.global_object().get(js_string!("JSON"), ctx)?;
    if let Some(json_obj) = json.as_object() {
        let parse = json_obj.get(js_string!("parse"), ctx)?;
        if let Some(func) = parse.as_callable() {
            if let Ok(result) = func.call(&json, &[JsValue::from(js_string!(msg.to_string()))], ctx) {
                return Ok(result);
            }
        }
    }

    // Fallback to string
    Ok(JsValue::from(js_string!(msg.to_string())))
}

/// Parse a data: URL and extract the content
fn parse_data_url(url: &str) -> Option<String> {
    if !url.starts_with("data:") {
        return None;
    }

    let content = &url[5..];

    // Handle base64 encoding
    if let Some(comma_pos) = content.find(',') {
        let metadata = &content[..comma_pos];
        let data = &content[comma_pos + 1..];

        if metadata.ends_with(";base64") {
            return base64_decode(data);
        } else {
            return Some(url_decode(data));
        }
    }

    None
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in data.chunks(3) {
        let mut buf = [0u8; 4];
        let len = chunk.len();

        buf[0] = chunk[0] >> 2;
        buf[1] = ((chunk[0] & 0x03) << 4) | if len > 1 { chunk[1] >> 4 } else { 0 };
        buf[2] = if len > 1 { ((chunk[1] & 0x0f) << 2) | if len > 2 { chunk[2] >> 6 } else { 0 } } else { 64 };
        buf[3] = if len > 2 { chunk[2] & 0x3f } else { 64 };

        result.push(ALPHABET[buf[0] as usize] as char);
        result.push(ALPHABET[buf[1] as usize] as char);
        if len > 1 {
            result.push(ALPHABET[buf[2] as usize] as char);
        } else {
            result.push('=');
        }
        if len > 2 {
            result.push(ALPHABET[buf[3] as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}

fn base64_decode(input: &str) -> Option<String> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let decode_char = |c: u8| -> Option<u8> {
        ALPHABET.iter().position(|&x| x == c).map(|p| p as u8)
    };

    let mut result = Vec::new();
    let chars: Vec<u8> = input.bytes().filter(|&c| c != b'=' && c != b'\n' && c != b'\r').collect();

    for chunk in chars.chunks(4) {
        if chunk.len() < 2 { break; }

        let a = decode_char(chunk[0]).unwrap_or(0);
        let b = decode_char(chunk[1]).unwrap_or(0);
        let c = if chunk.len() > 2 { decode_char(chunk[2]).unwrap_or(0) } else { 0 };
        let d = if chunk.len() > 3 { decode_char(chunk[3]).unwrap_or(0) } else { 0 };

        result.push((a << 2) | (b >> 4));
        if chunk.len() > 2 {
            result.push((b << 4) | (c >> 2));
        }
        if chunk.len() > 3 {
            result.push((c << 6) | d);
        }
    }

    String::from_utf8(result).ok()
}

fn url_decode(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }

    result
}

/// Get number of CPUs (simplified)
fn num_cpus() -> i32 {
    std::thread::available_parallelism()
        .map(|p| p.get() as i32)
        .unwrap_or(4)
}

/// Process any pending messages from workers (call from main thread)
/// Returns messages that need to be dispatched to worker onmessage handlers
pub fn drain_worker_messages() -> Vec<(u32, String, String)> {
    let mut messages = INCOMING_MESSAGES.lock().unwrap();
    let result: Vec<(u32, String, String)> = messages
        .iter()
        .map(|m| (m.worker_id, m.message_type.clone(), m.data.clone()))
        .collect();
    messages.clear();
    result
}

/// Register Worklet constructor
fn register_worklet(ctx: &mut Context) -> JsResult<()> {
    use boa_engine::object::FunctionObjectBuilder;

    // Worklet is abstract - cannot be constructed directly
    let constructor = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Err(JsNativeError::typ()
            .with_message("Worklet cannot be constructed directly")
            .into())
    });

    let ctor = FunctionObjectBuilder::new(ctx.realm(), constructor)
        .name(js_string!("Worklet"))
        .length(0)
        .constructor(true)
        .build();

    ctx.global_object().set(js_string!("Worklet"), ctor, false, ctx)?;

    // Note: CSS.paintWorklet and CSS.animationWorklet are registered in cssom.rs
    // since CSS object is created there

    Ok(())
}

/// Register PaintWorkletGlobalScope constructor
fn register_paint_worklet_global_scope(ctx: &mut Context) -> JsResult<()> {
    use boa_engine::object::FunctionObjectBuilder;

    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // registerPaint(name, paintCtor) - registers a paint class
        let register_paint = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let name = args.get_or_undefined(0)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();

            let paint_class = args.get_or_undefined(1);

            if name.is_empty() {
                return Err(JsNativeError::typ()
                    .with_message("registerPaint: name is required")
                    .into());
            }

            if !paint_class.is_callable() {
                return Err(JsNativeError::typ()
                    .with_message("registerPaint: class constructor is required")
                    .into());
            }

            // Extract static properties from the class
            let mut input_properties: Vec<String> = Vec::new();
            let mut input_arguments: Vec<String> = Vec::new();
            let mut context_options = PaintContextOptions::default();

            if let Some(class_obj) = paint_class.as_object() {
                // Get inputProperties static getter
                if let Ok(props) = class_obj.get(js_string!("inputProperties"), ctx) {
                    if let Some(arr) = props.as_object() {
                        let len = arr.get(js_string!("length"), ctx)
                            .ok().and_then(|v| v.to_u32(ctx).ok()).unwrap_or(0);
                        for i in 0..len {
                            if let Ok(prop) = arr.get(i, ctx) {
                                if let Ok(s) = prop.to_string(ctx) {
                                    input_properties.push(s.to_std_string_escaped());
                                }
                            }
                        }
                    }
                }

                // Get inputArguments static getter
                if let Ok(args_val) = class_obj.get(js_string!("inputArguments"), ctx) {
                    if let Some(arr) = args_val.as_object() {
                        let len = arr.get(js_string!("length"), ctx)
                            .ok().and_then(|v| v.to_u32(ctx).ok()).unwrap_or(0);
                        for i in 0..len {
                            if let Ok(arg) = arr.get(i, ctx) {
                                if let Ok(s) = arg.to_string(ctx) {
                                    input_arguments.push(s.to_std_string_escaped());
                                }
                            }
                        }
                    }
                }

                // Get contextOptions static getter
                if let Ok(opts) = class_obj.get(js_string!("contextOptions"), ctx) {
                    if let Some(opts_obj) = opts.as_object() {
                        if let Ok(alpha) = opts_obj.get(js_string!("alpha"), ctx) {
                            context_options.alpha = alpha.to_boolean();
                        }
                    }
                }
            }

            // Store in registry
            let entry = PaintWorkletEntry {
                name: name.clone(),
                input_properties,
                input_arguments,
                context_options,
            };

            PAINT_WORKLET_REGISTRY.lock().unwrap().insert(name, entry);

            Ok(JsValue::undefined())
        });

        // devicePixelRatio property (reflects display DPI)
        let device_pixel_ratio = 1.0f64;

        let scope = ObjectInitializer::new(ctx)
            .property(js_string!("devicePixelRatio"), device_pixel_ratio, Attribute::READONLY)
            .function(register_paint, js_string!("registerPaint"), 2)
            .build();

        Ok(JsValue::from(scope))
    });

    let ctor = FunctionObjectBuilder::new(ctx.realm(), constructor)
        .name(js_string!("PaintWorkletGlobalScope"))
        .length(0)
        .constructor(true)
        .build();

    ctx.global_object().set(js_string!("PaintWorkletGlobalScope"), ctor, false, ctx)?;
    Ok(())
}

/// Register AudioWorkletGlobalScope constructor
fn register_audio_worklet_global_scope(ctx: &mut Context) -> JsResult<()> {
    use boa_engine::object::FunctionObjectBuilder;

    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // registerProcessor(name, processorCtor) - registers an audio processor class
        let register_processor = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let name = args.get_or_undefined(0)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();

            let processor_class = args.get_or_undefined(1);

            if name.is_empty() {
                return Err(JsNativeError::typ()
                    .with_message("registerProcessor: name is required")
                    .into());
            }

            if !processor_class.is_callable() {
                return Err(JsNativeError::typ()
                    .with_message("registerProcessor: class constructor is required")
                    .into());
            }

            // Check if already registered
            if AUDIO_PROCESSOR_REGISTRY.lock().unwrap().contains_key(&name) {
                return Err(JsNativeError::typ()
                    .with_message(format!("registerProcessor: '{}' is already registered", name))
                    .into());
            }

            // Extract parameterDescriptors static property
            let mut parameter_descriptors: Vec<AudioParamDescriptor> = Vec::new();

            if let Some(class_obj) = processor_class.as_object() {
                if let Ok(descriptors) = class_obj.get(js_string!("parameterDescriptors"), ctx) {
                    if let Some(arr) = descriptors.as_object() {
                        let len = arr.get(js_string!("length"), ctx)
                            .ok().and_then(|v| v.to_u32(ctx).ok()).unwrap_or(0);

                        for i in 0..len {
                            if let Ok(desc) = arr.get(i, ctx) {
                                if let Some(desc_obj) = desc.as_object() {
                                    let param_name = desc_obj.get(js_string!("name"), ctx)
                                        .ok()
                                        .and_then(|v| v.to_string(ctx).ok())
                                        .map(|s| s.to_std_string_escaped())
                                        .unwrap_or_default();

                                    let default_value = desc_obj.get(js_string!("defaultValue"), ctx)
                                        .ok()
                                        .and_then(|v| v.to_number(ctx).ok())
                                        .unwrap_or(0.0);

                                    let min_value = desc_obj.get(js_string!("minValue"), ctx)
                                        .ok()
                                        .and_then(|v| v.to_number(ctx).ok())
                                        .unwrap_or(f64::NEG_INFINITY);

                                    let max_value = desc_obj.get(js_string!("maxValue"), ctx)
                                        .ok()
                                        .and_then(|v| v.to_number(ctx).ok())
                                        .unwrap_or(f64::INFINITY);

                                    let automation_rate = desc_obj.get(js_string!("automationRate"), ctx)
                                        .ok()
                                        .and_then(|v| v.to_string(ctx).ok())
                                        .map(|s| s.to_std_string_escaped())
                                        .unwrap_or_else(|| "a-rate".to_string());

                                    parameter_descriptors.push(AudioParamDescriptor {
                                        name: param_name,
                                        default_value,
                                        min_value,
                                        max_value,
                                        automation_rate,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // Store in registry
            let entry = AudioProcessorEntry {
                name: name.clone(),
                parameter_descriptors,
            };

            AUDIO_PROCESSOR_REGISTRY.lock().unwrap().insert(name, entry);

            Ok(JsValue::undefined())
        });

        // currentFrame getter - returns actual elapsed frames based on real time
        let current_frame_getter = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            let state = AUDIO_WORKLET_STATE.lock().unwrap();
            Ok(JsValue::from(state.current_frame() as f64))
        }).to_js_function(ctx.realm());

        // currentTime getter - returns actual elapsed time in seconds
        let current_time_getter = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            let state = AUDIO_WORKLET_STATE.lock().unwrap();
            Ok(JsValue::from(state.current_time()))
        }).to_js_function(ctx.realm());

        // sampleRate getter - returns configured sample rate
        let sample_rate_getter = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            let state = AUDIO_WORKLET_STATE.lock().unwrap();
            Ok(JsValue::from(state.sample_rate as f64))
        }).to_js_function(ctx.realm());

        let scope = ObjectInitializer::new(ctx)
            .accessor(
                js_string!("currentFrame"),
                Some(current_frame_getter),
                None,
                Attribute::CONFIGURABLE,
            )
            .accessor(
                js_string!("currentTime"),
                Some(current_time_getter),
                None,
                Attribute::CONFIGURABLE,
            )
            .accessor(
                js_string!("sampleRate"),
                Some(sample_rate_getter),
                None,
                Attribute::CONFIGURABLE,
            )
            .function(register_processor, js_string!("registerProcessor"), 2)
            .build();

        Ok(JsValue::from(scope))
    });

    let ctor = FunctionObjectBuilder::new(ctx.realm(), constructor)
        .name(js_string!("AudioWorkletGlobalScope"))
        .length(0)
        .constructor(true)
        .build();

    ctx.global_object().set(js_string!("AudioWorkletGlobalScope"), ctor, false, ctx)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_message_primitives() {
        let mut ctx = Context::default();

        assert_eq!(serialize_message(&JsValue::undefined(), &mut ctx).unwrap(), "undefined");
        assert_eq!(serialize_message(&JsValue::null(), &mut ctx).unwrap(), "null");
        assert_eq!(serialize_message(&JsValue::from(42), &mut ctx).unwrap(), "42");
        assert_eq!(serialize_message(&JsValue::from(true), &mut ctx).unwrap(), "true");
    }

    #[test]
    fn test_deserialize_message() {
        let mut ctx = Context::default();

        assert!(deserialize_message("undefined", &mut ctx).unwrap().is_undefined());
        assert!(deserialize_message("null", &mut ctx).unwrap().is_null());
        assert_eq!(deserialize_message("42", &mut ctx).unwrap().as_number(), Some(42.0));
        assert_eq!(deserialize_message("true", &mut ctx).unwrap().as_boolean(), Some(true));
    }

    #[test]
    fn test_parse_data_url() {
        let url = "data:text/javascript,console.log('hello')";
        assert_eq!(parse_data_url(url), Some("console.log('hello')".to_string()));

        let url_encoded = "data:text/javascript,console.log('hello%20world')";
        assert_eq!(parse_data_url(url_encoded), Some("console.log('hello world')".to_string()));
    }

    #[test]
    fn test_base64_roundtrip() {
        let original = "hello world";
        let encoded = base64_encode(original.as_bytes());
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_num_cpus() {
        let cpus = num_cpus();
        assert!(cpus >= 1);
    }
}
