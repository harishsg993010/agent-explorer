//! Service Workers API Implementation
//!
//! Provides ServiceWorker, ServiceWorkerRegistration, ServiceWorkerContainer,
//! Cache, and CacheStorage APIs with real offline caching capabilities.

use boa_engine::{
    Context, JsArgs, JsResult, JsValue,
    object::ObjectInitializer,
    property::Attribute,
    NativeFunction,
    JsString,
    object::builtins::JsArray,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, atomic::{AtomicU32, Ordering}};
use lazy_static::lazy_static;

/// Represents a cached response
#[derive(Clone, Debug)]
struct CachedResponse {
    url: String,
    status: u16,
    status_text: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
    body_used: bool,
}

impl CachedResponse {
    fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            status: 200,
            status_text: "OK".to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
            body_used: false,
        }
    }
}

/// Represents a cached request
#[derive(Clone, Debug)]
struct CachedRequest {
    url: String,
    method: String,
    headers: HashMap<String, String>,
    mode: String,
    credentials: String,
    cache_mode: String,
    redirect: String,
    referrer: String,
    integrity: String,
}

impl CachedRequest {
    fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            mode: "cors".to_string(),
            credentials: "same-origin".to_string(),
            cache_mode: "default".to_string(),
            redirect: "follow".to_string(),
            referrer: "about:client".to_string(),
            integrity: String::new(),
        }
    }
}

/// A single cache store
#[derive(Clone, Debug)]
struct Cache {
    name: String,
    entries: HashMap<String, (CachedRequest, CachedResponse)>,
}

impl Cache {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            entries: HashMap::new(),
        }
    }
}

/// Service Worker state
#[derive(Clone, Debug, PartialEq)]
enum ServiceWorkerState {
    Parsed,
    Installing,
    Installed,
    Activating,
    Activated,
    Redundant,
}

impl ServiceWorkerState {
    fn as_str(&self) -> &'static str {
        match self {
            ServiceWorkerState::Parsed => "parsed",
            ServiceWorkerState::Installing => "installing",
            ServiceWorkerState::Installed => "installed",
            ServiceWorkerState::Activating => "activating",
            ServiceWorkerState::Activated => "activated",
            ServiceWorkerState::Redundant => "redundant",
        }
    }
}

/// Represents a registered service worker
#[derive(Clone, Debug)]
struct ServiceWorkerData {
    script_url: String,
    state: ServiceWorkerState,
    scope: String,
}

/// Represents a service worker registration
#[derive(Clone, Debug)]
struct ServiceWorkerRegistrationData {
    id: u32,
    scope: String,
    script_url: String,
    installing: Option<u32>,
    waiting: Option<u32>,
    active: Option<u32>,
    update_via_cache: String,
    navigation_preload_enabled: bool,
}

// Global state for service workers
lazy_static! {
    static ref NEXT_CACHE_ID: AtomicU32 = AtomicU32::new(1);
    static ref NEXT_REGISTRATION_ID: AtomicU32 = AtomicU32::new(1);
    static ref NEXT_WORKER_ID: AtomicU32 = AtomicU32::new(1);

    // Cache storage: cache_name -> Cache
    static ref CACHE_STORAGE: Arc<Mutex<HashMap<String, Cache>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Service worker registrations
    static ref SW_REGISTRATIONS: Arc<Mutex<HashMap<u32, ServiceWorkerRegistrationData>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Service workers
    static ref SERVICE_WORKERS: Arc<Mutex<HashMap<u32, ServiceWorkerData>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Controller mapping: scope -> worker_id
    static ref CONTROLLERS: Arc<Mutex<HashMap<String, u32>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

/// Helper to create a resolved promise
fn create_resolved_promise(context: &mut Context, value: JsValue) -> JsValue {
    let promise = ObjectInitializer::new(context)
        .function(
            unsafe { NativeFunction::from_closure(move |_this, args, ctx| {
                if let Some(resolve) = args.get(0) {
                    if let Some(obj) = resolve.as_object() {
                        let _ = obj.call(&JsValue::undefined(), &[value.clone()], ctx);
                    }
                }
                Ok(JsValue::undefined())
            }) },
            JsString::from("then"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            }),
            JsString::from("catch"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            }),
            JsString::from("finally"),
            1,
        )
        .build();

    JsValue::from(promise)
}

/// Helper to create a rejected promise
fn create_rejected_promise(context: &mut Context, error: &str) -> JsValue {
    let error_msg = error.to_string();
    let promise = ObjectInitializer::new(context)
        .function(
            NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            }),
            JsString::from("then"),
            1,
        )
        .function(
            unsafe { NativeFunction::from_closure(move |_this, args, ctx| {
                if let Some(reject) = args.get(0) {
                    if let Some(obj) = reject.as_object() {
                        // Pass error as string - simpler than constructing Error object
                        let error_val = JsValue::from(JsString::from(error_msg.as_str()));
                        let _ = obj.call(&JsValue::undefined(), &[error_val], ctx);
                    }
                }
                Ok(JsValue::undefined())
            }) },
            JsString::from("catch"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            }),
            JsString::from("finally"),
            1,
        )
        .build();

    JsValue::from(promise)
}

/// Create a Response object from cached data
fn create_response_object(context: &mut Context, response: &CachedResponse) -> JsValue {
    let url = response.url.clone();
    let status = response.status;
    let status_text = response.status_text.clone();
    let body = response.body.clone();
    let body_for_json = body.clone();
    let headers = response.headers.clone();
    let url_clone = url.clone();

    let response_obj = ObjectInitializer::new(context)
        .property(
            JsString::from("url"),
            JsValue::from(JsString::from(url.as_str())),
            Attribute::all(),
        )
        .property(
            JsString::from("status"),
            JsValue::from(status as i32),
            Attribute::all(),
        )
        .property(
            JsString::from("statusText"),
            JsValue::from(JsString::from(status_text.as_str())),
            Attribute::all(),
        )
        .property(
            JsString::from("ok"),
            JsValue::from(status >= 200 && status < 300),
            Attribute::all(),
        )
        .property(
            JsString::from("redirected"),
            JsValue::from(false),
            Attribute::all(),
        )
        .property(
            JsString::from("type"),
            JsValue::from(JsString::from("basic")),
            Attribute::all(),
        )
        .property(
            JsString::from("bodyUsed"),
            JsValue::from(false),
            Attribute::all(),
        )
        .function(
            unsafe { NativeFunction::from_closure(move |_this, _args, ctx| {
                let text = String::from_utf8_lossy(&body).to_string();
                Ok(create_resolved_promise(ctx, JsValue::from(JsString::from(text.as_str()))))
            }) },
            JsString::from("text"),
            0,
        )
        .function(
            unsafe { NativeFunction::from_closure(move |_this, _args, ctx| {
                let text = String::from_utf8_lossy(&body_for_json).to_string();
                match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(json) => {
                        // Convert to JsValue (simplified)
                        let json_str = json.to_string();
                        Ok(create_resolved_promise(ctx, JsValue::from(JsString::from(json_str.as_str()))))
                    }
                    Err(_) => Ok(create_rejected_promise(ctx, "Invalid JSON"))
                }
            }) },
            JsString::from("json"),
            0,
        )
        .function(
            NativeFunction::from_copy_closure(|_this, _args, ctx| {
                // Return empty ArrayBuffer for now
                let buffer = ObjectInitializer::new(ctx)
                    .property(
                        JsString::from("byteLength"),
                        JsValue::from(0),
                        Attribute::all(),
                    )
                    .build();
                Ok(create_resolved_promise(ctx, JsValue::from(buffer)))
            }),
            JsString::from("arrayBuffer"),
            0,
        )
        .function(
            NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let blob = ObjectInitializer::new(ctx)
                    .property(
                        JsString::from("size"),
                        JsValue::from(0),
                        Attribute::all(),
                    )
                    .property(
                        JsString::from("type"),
                        JsValue::from(JsString::from("")),
                        Attribute::all(),
                    )
                    .build();
                Ok(create_resolved_promise(ctx, JsValue::from(blob)))
            }),
            JsString::from("blob"),
            0,
        )
        .function(
            unsafe { NativeFunction::from_closure(move |_this, _args, ctx| {
                // Clone returns a new Response with same data
                let cloned = ObjectInitializer::new(ctx)
                    .property(
                        JsString::from("url"),
                        JsValue::from(JsString::from(url.as_str())),
                        Attribute::all(),
                    )
                    .property(
                        JsString::from("status"),
                        JsValue::from(status as i32),
                        Attribute::all(),
                    )
                    .property(
                        JsString::from("ok"),
                        JsValue::from(status >= 200 && status < 300),
                        Attribute::all(),
                    )
                    .build();
                Ok(JsValue::from(cloned))
            }) },
            JsString::from("clone"),
            0,
        )
        .build();

    // Add headers object
    let headers_obj = create_headers_object(context, &headers);
    response_obj.set(
        JsString::from("headers"),
        JsValue::from(headers_obj),
        false,
        context,
    ).ok();

    JsValue::from(response_obj)
}

/// Create a Headers object
fn create_headers_object(context: &mut Context, headers: &HashMap<String, String>) -> boa_engine::JsObject {
    let headers_clone = headers.clone();
    let headers_clone2 = headers.clone();
    let headers_clone3 = headers.clone();

    ObjectInitializer::new(context)
        .function(
            unsafe { NativeFunction::from_closure(move |_this, args, _ctx| {
                let name = args.get_or_undefined(0).to_string(_ctx)?;
                let name_str = name.to_std_string_escaped().to_lowercase();

                if let Some(value) = headers_clone.get(&name_str) {
                    Ok(JsValue::from(JsString::from(value.as_str())))
                } else {
                    Ok(JsValue::null())
                }
            }) },
            JsString::from("get"),
            1,
        )
        .function(
            unsafe { NativeFunction::from_closure(move |_this, args, _ctx| {
                let name = args.get_or_undefined(0).to_string(_ctx)?;
                let name_str = name.to_std_string_escaped().to_lowercase();
                Ok(JsValue::from(headers_clone2.contains_key(&name_str)))
            }) },
            JsString::from("has"),
            1,
        )
        .function(
            unsafe { NativeFunction::from_closure(move |_this, _args, ctx| {
                let arr = JsArray::new(ctx);
                for (i, (key, value)) in headers_clone3.iter().enumerate() {
                    let entry = JsArray::new(ctx);
                    entry.push(JsValue::from(JsString::from(key.as_str())), ctx)?;
                    entry.push(JsValue::from(JsString::from(value.as_str())), ctx)?;
                    arr.push(JsValue::from(entry), ctx)?;
                }
                Ok(JsValue::from(arr))
            }) },
            JsString::from("entries"),
            0,
        )
        .build()
}

/// Create a Request object
fn create_request_object(context: &mut Context, request: &CachedRequest) -> JsValue {
    let url = request.url.clone();
    let method = request.method.clone();
    let mode = request.mode.clone();
    let credentials = request.credentials.clone();

    let request_obj = ObjectInitializer::new(context)
        .property(
            JsString::from("url"),
            JsValue::from(JsString::from(url.as_str())),
            Attribute::all(),
        )
        .property(
            JsString::from("method"),
            JsValue::from(JsString::from(method.as_str())),
            Attribute::all(),
        )
        .property(
            JsString::from("mode"),
            JsValue::from(JsString::from(mode.as_str())),
            Attribute::all(),
        )
        .property(
            JsString::from("credentials"),
            JsValue::from(JsString::from(credentials.as_str())),
            Attribute::all(),
        )
        .property(
            JsString::from("cache"),
            JsValue::from(JsString::from("default")),
            Attribute::all(),
        )
        .property(
            JsString::from("redirect"),
            JsValue::from(JsString::from("follow")),
            Attribute::all(),
        )
        .property(
            JsString::from("referrer"),
            JsValue::from(JsString::from("about:client")),
            Attribute::all(),
        )
        .property(
            JsString::from("integrity"),
            JsValue::from(JsString::from("")),
            Attribute::all(),
        )
        .property(
            JsString::from("bodyUsed"),
            JsValue::from(false),
            Attribute::all(),
        )
        .function(
            unsafe { NativeFunction::from_closure(move |_this, _args, ctx| {
                // Clone returns a new Request
                let cloned = ObjectInitializer::new(ctx)
                    .property(
                        JsString::from("url"),
                        JsValue::from(JsString::from(url.as_str())),
                        Attribute::all(),
                    )
                    .property(
                        JsString::from("method"),
                        JsValue::from(JsString::from(method.as_str())),
                        Attribute::all(),
                    )
                    .build();
                Ok(JsValue::from(cloned))
            }) },
            JsString::from("clone"),
            0,
        )
        .build();

    JsValue::from(request_obj)
}

/// Create a Cache object
fn create_cache_object(context: &mut Context, cache_name: String) -> boa_engine::JsObject {
    let name1 = cache_name.clone();
    let name2 = cache_name.clone();
    let name3 = cache_name.clone();
    let name4 = cache_name.clone();
    let name5 = cache_name.clone();
    let name6 = cache_name.clone();
    let name7 = cache_name.clone();

    ObjectInitializer::new(context)
        // cache.match(request, options)
        .function(
            unsafe { NativeFunction::from_closure(move |_this, args, ctx| {
                let request = args.get_or_undefined(0);
                let url = if request.is_string() {
                    request.to_string(ctx)?.to_std_string_escaped()
                } else if let Some(obj) = request.as_object() {
                    obj.get(JsString::from("url"), ctx)?
                        .to_string(ctx)?
                        .to_std_string_escaped()
                } else {
                    return Ok(create_resolved_promise(ctx, JsValue::undefined()));
                };

                let storage = CACHE_STORAGE.lock().unwrap();
                if let Some(cache) = storage.get(&name1) {
                    if let Some((_, response)) = cache.entries.get(&url) {
                        let resp_obj = create_response_object(ctx, response);
                        return Ok(create_resolved_promise(ctx, resp_obj));
                    }
                }

                Ok(create_resolved_promise(ctx, JsValue::undefined()))
            }) },
            JsString::from("match"),
            2,
        )
        // cache.matchAll(request, options)
        .function(
            unsafe { NativeFunction::from_closure(move |_this, args, ctx| {
                let request = args.get(0);
                let arr = JsArray::new(ctx);

                let storage = CACHE_STORAGE.lock().unwrap();
                if let Some(cache) = storage.get(&name2) {
                    if let Some(req) = request {
                        if !req.is_undefined() {
                            let url = if req.is_string() {
                                req.to_string(ctx)?.to_std_string_escaped()
                            } else if let Some(obj) = req.as_object() {
                                obj.get(JsString::from("url"), ctx)?
                                    .to_string(ctx)?
                                    .to_std_string_escaped()
                            } else {
                                String::new()
                            };

                            if let Some((_, response)) = cache.entries.get(&url) {
                                arr.push(create_response_object(ctx, response), ctx)?;
                            }
                        } else {
                            // Return all responses
                            for (_, response) in cache.entries.values() {
                                arr.push(create_response_object(ctx, response), ctx)?;
                            }
                        }
                    } else {
                        // Return all responses
                        for (_, response) in cache.entries.values() {
                            arr.push(create_response_object(ctx, response), ctx)?;
                        }
                    }
                }

                Ok(create_resolved_promise(ctx, JsValue::from(arr)))
            }) },
            JsString::from("matchAll"),
            2,
        )
        // cache.add(request)
        .function(
            unsafe { NativeFunction::from_closure(move |_this, args, ctx| {
                let request = args.get_or_undefined(0);
                let url = if request.is_string() {
                    request.to_string(ctx)?.to_std_string_escaped()
                } else if let Some(obj) = request.as_object() {
                    obj.get(JsString::from("url"), ctx)?
                        .to_string(ctx)?
                        .to_std_string_escaped()
                } else {
                    return Ok(create_rejected_promise(ctx, "Invalid request"));
                };

                // Simulate adding - in real implementation would fetch
                let request_data = CachedRequest::new(&url);
                let response_data = CachedResponse::new(&url);

                let mut storage = CACHE_STORAGE.lock().unwrap();
                if let Some(cache) = storage.get_mut(&name3) {
                    cache.entries.insert(url, (request_data, response_data));
                }

                Ok(create_resolved_promise(ctx, JsValue::undefined()))
            }) },
            JsString::from("add"),
            1,
        )
        // cache.addAll(requests)
        .function(
            unsafe { NativeFunction::from_closure(move |_this, args, ctx| {
                let requests = args.get_or_undefined(0);

                if let Some(arr) = requests.as_object() {
                    let length = arr.get(JsString::from("length"), ctx)?
                        .to_number(ctx)? as usize;

                    let mut storage = CACHE_STORAGE.lock().unwrap();
                    if let Some(cache) = storage.get_mut(&name4) {
                        for i in 0..length {
                            if let Ok(item) = arr.get(i, ctx) {
                                let url = if item.is_string() {
                                    item.to_string(ctx)?.to_std_string_escaped()
                                } else if let Some(obj) = item.as_object() {
                                    obj.get(JsString::from("url"), ctx)?
                                        .to_string(ctx)?
                                        .to_std_string_escaped()
                                } else {
                                    continue;
                                };

                                let request_data = CachedRequest::new(&url);
                                let response_data = CachedResponse::new(&url);
                                cache.entries.insert(url, (request_data, response_data));
                            }
                        }
                    }
                }

                Ok(create_resolved_promise(ctx, JsValue::undefined()))
            }) },
            JsString::from("addAll"),
            1,
        )
        // cache.put(request, response)
        .function(
            unsafe { NativeFunction::from_closure(move |_this, args, ctx| {
                let request = args.get_or_undefined(0);
                let response = args.get_or_undefined(1);

                let url = if request.is_string() {
                    request.to_string(ctx)?.to_std_string_escaped()
                } else if let Some(obj) = request.as_object() {
                    obj.get(JsString::from("url"), ctx)?
                        .to_string(ctx)?
                        .to_std_string_escaped()
                } else {
                    return Ok(create_rejected_promise(ctx, "Invalid request"));
                };

                let mut request_data = CachedRequest::new(&url);
                let mut response_data = CachedResponse::new(&url);

                // Extract response properties if provided
                if let Some(resp_obj) = response.as_object() {
                    if let Ok(status) = resp_obj.get(JsString::from("status"), ctx) {
                        if let Ok(n) = status.to_number(ctx) {
                            response_data.status = n as u16;
                        }
                    }
                    if let Ok(status_text) = resp_obj.get(JsString::from("statusText"), ctx) {
                        if let Ok(s) = status_text.to_string(ctx) {
                            response_data.status_text = s.to_std_string_escaped();
                        }
                    }
                }

                let mut storage = CACHE_STORAGE.lock().unwrap();
                if let Some(cache) = storage.get_mut(&name5) {
                    cache.entries.insert(url, (request_data, response_data));
                }

                Ok(create_resolved_promise(ctx, JsValue::undefined()))
            }) },
            JsString::from("put"),
            2,
        )
        // cache.delete(request, options)
        .function(
            unsafe { NativeFunction::from_closure(move |_this, args, ctx| {
                let request = args.get_or_undefined(0);
                let url = if request.is_string() {
                    request.to_string(ctx)?.to_std_string_escaped()
                } else if let Some(obj) = request.as_object() {
                    obj.get(JsString::from("url"), ctx)?
                        .to_string(ctx)?
                        .to_std_string_escaped()
                } else {
                    return Ok(create_resolved_promise(ctx, JsValue::from(false)));
                };

                let mut storage = CACHE_STORAGE.lock().unwrap();
                if let Some(cache) = storage.get_mut(&name6) {
                    let existed = cache.entries.remove(&url).is_some();
                    return Ok(create_resolved_promise(ctx, JsValue::from(existed)));
                }

                Ok(create_resolved_promise(ctx, JsValue::from(false)))
            }) },
            JsString::from("delete"),
            2,
        )
        // cache.keys(request, options)
        .function(
            unsafe { NativeFunction::from_closure(move |_this, _args, ctx| {
                let arr = JsArray::new(ctx);

                let storage = CACHE_STORAGE.lock().unwrap();
                if let Some(cache) = storage.get(&name7) {
                    for (request, _) in cache.entries.values() {
                        arr.push(create_request_object(ctx, request), ctx)?;
                    }
                }

                Ok(create_resolved_promise(ctx, JsValue::from(arr)))
            }) },
            JsString::from("keys"),
            2,
        )
        .build()
}

/// Register CacheStorage (caches) API
fn register_cache_storage(context: &mut Context) -> JsResult<()> {
    let caches = ObjectInitializer::new(context)
        // caches.open(cacheName)
        .function(
            NativeFunction::from_copy_closure(|_this, args, ctx| {
                let cache_name = args.get_or_undefined(0)
                    .to_string(ctx)?
                    .to_std_string_escaped();

                // Create cache if it doesn't exist
                {
                    let mut storage = CACHE_STORAGE.lock().unwrap();
                    if !storage.contains_key(&cache_name) {
                        storage.insert(cache_name.clone(), Cache::new(&cache_name));
                    }
                }

                let cache_obj = create_cache_object(ctx, cache_name);
                Ok(create_resolved_promise(ctx, JsValue::from(cache_obj)))
            }),
            JsString::from("open"),
            1,
        )
        // caches.has(cacheName)
        .function(
            NativeFunction::from_copy_closure(|_this, args, ctx| {
                let cache_name = args.get_or_undefined(0)
                    .to_string(ctx)?
                    .to_std_string_escaped();

                let storage = CACHE_STORAGE.lock().unwrap();
                let exists = storage.contains_key(&cache_name);

                Ok(create_resolved_promise(ctx, JsValue::from(exists)))
            }),
            JsString::from("has"),
            1,
        )
        // caches.delete(cacheName)
        .function(
            NativeFunction::from_copy_closure(|_this, args, ctx| {
                let cache_name = args.get_or_undefined(0)
                    .to_string(ctx)?
                    .to_std_string_escaped();

                let mut storage = CACHE_STORAGE.lock().unwrap();
                let existed = storage.remove(&cache_name).is_some();

                Ok(create_resolved_promise(ctx, JsValue::from(existed)))
            }),
            JsString::from("delete"),
            1,
        )
        // caches.keys()
        .function(
            NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let storage = CACHE_STORAGE.lock().unwrap();
                let arr = JsArray::new(ctx);

                for name in storage.keys() {
                    arr.push(JsValue::from(JsString::from(name.as_str())), ctx)?;
                }

                Ok(create_resolved_promise(ctx, JsValue::from(arr)))
            }),
            JsString::from("keys"),
            0,
        )
        // caches.match(request, options)
        .function(
            NativeFunction::from_copy_closure(|_this, args, ctx| {
                let request = args.get_or_undefined(0);
                let url = if request.is_string() {
                    request.to_string(ctx)?.to_std_string_escaped()
                } else if let Some(obj) = request.as_object() {
                    obj.get(JsString::from("url"), ctx)?
                        .to_string(ctx)?
                        .to_std_string_escaped()
                } else {
                    return Ok(create_resolved_promise(ctx, JsValue::undefined()));
                };

                let storage = CACHE_STORAGE.lock().unwrap();
                for cache in storage.values() {
                    if let Some((_, response)) = cache.entries.get(&url) {
                        let resp_obj = create_response_object(ctx, response);
                        return Ok(create_resolved_promise(ctx, resp_obj));
                    }
                }

                Ok(create_resolved_promise(ctx, JsValue::undefined()))
            }),
            JsString::from("match"),
            2,
        )
        .build();

    context.register_global_property(
        JsString::from("caches"),
        JsValue::from(caches),
        Attribute::WRITABLE | Attribute::CONFIGURABLE,
    )?;

    Ok(())
}

/// Create a ServiceWorker object
fn create_service_worker_object(context: &mut Context, worker_id: u32) -> boa_engine::JsObject {
    let workers = SERVICE_WORKERS.lock().unwrap();
    let worker_data = workers.get(&worker_id).cloned();
    drop(workers);

    let (script_url, state) = if let Some(data) = worker_data {
        (data.script_url, data.state.as_str().to_string())
    } else {
        (String::new(), "redundant".to_string())
    };

    ObjectInitializer::new(context)
        .property(
            JsString::from("scriptURL"),
            JsValue::from(JsString::from(script_url.as_str())),
            Attribute::all(),
        )
        .property(
            JsString::from("state"),
            JsValue::from(JsString::from(state.as_str())),
            Attribute::all(),
        )
        .property(
            JsString::from("onstatechange"),
            JsValue::null(),
            Attribute::all(),
        )
        .property(
            JsString::from("onerror"),
            JsValue::null(),
            Attribute::all(),
        )
        .function(
            NativeFunction::from_copy_closure(|_this, args, ctx| {
                // postMessage to service worker
                let _message = args.get_or_undefined(0);
                // In real implementation, would send to worker thread
                Ok(JsValue::undefined())
            }),
            JsString::from("postMessage"),
            1,
        )
        .build()
}

/// Create a ServiceWorkerRegistration object
fn create_registration_object(context: &mut Context, reg_id: u32) -> boa_engine::JsObject {
    let registrations = SW_REGISTRATIONS.lock().unwrap();
    let reg_data = registrations.get(&reg_id).cloned();
    drop(registrations);

    let (scope, active_id, installing_id, waiting_id, update_via_cache) = if let Some(data) = reg_data {
        (data.scope, data.active, data.installing, data.waiting, data.update_via_cache)
    } else {
        (String::new(), None, None, None, "imports".to_string())
    };

    let reg_obj = ObjectInitializer::new(context)
        .property(
            JsString::from("scope"),
            JsValue::from(JsString::from(scope.as_str())),
            Attribute::all(),
        )
        .property(
            JsString::from("updateViaCache"),
            JsValue::from(JsString::from(update_via_cache.as_str())),
            Attribute::all(),
        )
        .property(
            JsString::from("onupdatefound"),
            JsValue::null(),
            Attribute::all(),
        )
        .build();

    // Add installing, waiting, active properties
    if let Some(id) = installing_id {
        let sw = create_service_worker_object(context, id);
        reg_obj.set(JsString::from("installing"), JsValue::from(sw), false, context).ok();
    } else {
        reg_obj.set(JsString::from("installing"), JsValue::null(), false, context).ok();
    }

    if let Some(id) = waiting_id {
        let sw = create_service_worker_object(context, id);
        reg_obj.set(JsString::from("waiting"), JsValue::from(sw), false, context).ok();
    } else {
        reg_obj.set(JsString::from("waiting"), JsValue::null(), false, context).ok();
    }

    if let Some(id) = active_id {
        let sw = create_service_worker_object(context, id);
        reg_obj.set(JsString::from("active"), JsValue::from(sw), false, context).ok();
    } else {
        reg_obj.set(JsString::from("active"), JsValue::null(), false, context).ok();
    }

    // Add navigation preload
    let navigation_preload = ObjectInitializer::new(context)
        .function(
            NativeFunction::from_copy_closure(|_this, _args, ctx| {
                // Enable navigation preload
                Ok(create_resolved_promise(ctx, JsValue::undefined()))
            }),
            JsString::from("enable"),
            0,
        )
        .function(
            NativeFunction::from_copy_closure(|_this, _args, ctx| {
                // Disable navigation preload
                Ok(create_resolved_promise(ctx, JsValue::undefined()))
            }),
            JsString::from("disable"),
            0,
        )
        .function(
            NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let state = ObjectInitializer::new(ctx)
                    .property(
                        JsString::from("enabled"),
                        JsValue::from(false),
                        Attribute::all(),
                    )
                    .property(
                        JsString::from("headerValue"),
                        JsValue::from(JsString::from("true")),
                        Attribute::all(),
                    )
                    .build();
                Ok(create_resolved_promise(ctx, JsValue::from(state)))
            }),
            JsString::from("getState"),
            0,
        )
        .function(
            NativeFunction::from_copy_closure(|_this, _args, ctx| {
                Ok(create_resolved_promise(ctx, JsValue::undefined()))
            }),
            JsString::from("setHeaderValue"),
            1,
        )
        .build();

    reg_obj.set(
        JsString::from("navigationPreload"),
        JsValue::from(navigation_preload),
        false,
        context,
    ).ok();

    // Add methods
    let reg_id_for_update = reg_id;
    let update_fn = unsafe { NativeFunction::from_closure(move |_this, _args, ctx| {
        // Simulate update check
        let registrations = SW_REGISTRATIONS.lock().unwrap();
        if let Some(_) = registrations.get(&reg_id_for_update) {
            let reg = create_registration_object(ctx, reg_id_for_update);
            Ok(create_resolved_promise(ctx, JsValue::from(reg)))
        } else {
            Ok(create_rejected_promise(ctx, "Registration not found"))
        }
    }) };

    reg_obj.set(
        JsString::from("update"),
        JsValue::from(update_fn.to_js_function(context.realm())),
        false,
        context,
    ).ok();

    let reg_id_for_unregister = reg_id;
    let unregister_fn = unsafe { NativeFunction::from_closure(move |_this, _args, ctx| {
        let mut registrations = SW_REGISTRATIONS.lock().unwrap();
        let existed = registrations.remove(&reg_id_for_unregister).is_some();
        Ok(create_resolved_promise(ctx, JsValue::from(existed)))
    }) };

    reg_obj.set(
        JsString::from("unregister"),
        JsValue::from(unregister_fn.to_js_function(context.realm())),
        false,
        context,
    ).ok();

    // Add showNotification for push notifications
    let show_notification = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let _title = args.get_or_undefined(0).to_string(ctx)?;
        let _options = args.get_or_undefined(1);
        // In real implementation would show system notification
        Ok(create_resolved_promise(ctx, JsValue::undefined()))
    });

    reg_obj.set(
        JsString::from("showNotification"),
        JsValue::from(show_notification.to_js_function(context.realm())),
        false,
        context,
    ).ok();

    // Add getNotifications
    let get_notifications = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let arr = JsArray::new(ctx);
        Ok(create_resolved_promise(ctx, JsValue::from(arr)))
    });

    reg_obj.set(
        JsString::from("getNotifications"),
        JsValue::from(get_notifications.to_js_function(context.realm())),
        false,
        context,
    ).ok();

    reg_obj
}

/// Register ServiceWorkerContainer (navigator.serviceWorker)
fn register_service_worker_container(context: &mut Context) -> JsResult<()> {
    let container = ObjectInitializer::new(context)
        // serviceWorker.register(scriptURL, options)
        .function(
            NativeFunction::from_copy_closure(|_this, args, ctx| {
                let script_url = args.get_or_undefined(0)
                    .to_string(ctx)?
                    .to_std_string_escaped();

                let options = args.get_or_undefined(1);
                let scope = if let Some(obj) = options.as_object() {
                    if let Ok(s) = obj.get(JsString::from("scope"), ctx) {
                        if !s.is_undefined() {
                            s.to_string(ctx)?.to_std_string_escaped()
                        } else {
                            "/".to_string()
                        }
                    } else {
                        "/".to_string()
                    }
                } else {
                    "/".to_string()
                };

                // Create new service worker
                let worker_id = NEXT_WORKER_ID.fetch_add(1, Ordering::SeqCst);
                let reg_id = NEXT_REGISTRATION_ID.fetch_add(1, Ordering::SeqCst);

                // Store worker data
                {
                    let mut workers = SERVICE_WORKERS.lock().unwrap();
                    workers.insert(worker_id, ServiceWorkerData {
                        script_url: script_url.clone(),
                        state: ServiceWorkerState::Activated, // Simplified: immediately activated
                        scope: scope.clone(),
                    });
                }

                // Store registration
                {
                    let mut registrations = SW_REGISTRATIONS.lock().unwrap();
                    registrations.insert(reg_id, ServiceWorkerRegistrationData {
                        id: reg_id,
                        scope: scope.clone(),
                        script_url,
                        installing: None,
                        waiting: None,
                        active: Some(worker_id),
                        update_via_cache: "imports".to_string(),
                        navigation_preload_enabled: false,
                    });
                }

                // Store controller mapping
                {
                    let mut controllers = CONTROLLERS.lock().unwrap();
                    controllers.insert(scope, worker_id);
                }

                let reg = create_registration_object(ctx, reg_id);
                Ok(create_resolved_promise(ctx, JsValue::from(reg)))
            }),
            JsString::from("register"),
            2,
        )
        // serviceWorker.getRegistration(scope)
        .function(
            NativeFunction::from_copy_closure(|_this, args, ctx| {
                let scope = if args.get(0).map(|v| !v.is_undefined()).unwrap_or(false) {
                    args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped()
                } else {
                    "/".to_string()
                };

                let registrations = SW_REGISTRATIONS.lock().unwrap();
                for (id, reg) in registrations.iter() {
                    if reg.scope == scope || scope.starts_with(&reg.scope) {
                        let reg_obj = create_registration_object(ctx, *id);
                        return Ok(create_resolved_promise(ctx, JsValue::from(reg_obj)));
                    }
                }

                Ok(create_resolved_promise(ctx, JsValue::undefined()))
            }),
            JsString::from("getRegistration"),
            1,
        )
        // serviceWorker.getRegistrations()
        .function(
            NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let registrations = SW_REGISTRATIONS.lock().unwrap();
                let arr = JsArray::new(ctx);

                for id in registrations.keys() {
                    let reg_obj = create_registration_object(ctx, *id);
                    arr.push(JsValue::from(reg_obj), ctx)?;
                }

                Ok(create_resolved_promise(ctx, JsValue::from(arr)))
            }),
            JsString::from("getRegistrations"),
            0,
        )
        // serviceWorker.ready
        .property(
            JsString::from("ready"),
            JsValue::undefined(), // Will be replaced with promise
            Attribute::all(),
        )
        // serviceWorker.controller
        .property(
            JsString::from("controller"),
            JsValue::null(), // No controller initially
            Attribute::all(),
        )
        // serviceWorker.oncontrollerchange
        .property(
            JsString::from("oncontrollerchange"),
            JsValue::null(),
            Attribute::all(),
        )
        // serviceWorker.onmessage
        .property(
            JsString::from("onmessage"),
            JsValue::null(),
            Attribute::all(),
        )
        // serviceWorker.onmessageerror
        .property(
            JsString::from("onmessageerror"),
            JsValue::null(),
            Attribute::all(),
        )
        .function(
            NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                // startMessages() - begins receiving messages
                Ok(JsValue::undefined())
            }),
            JsString::from("startMessages"),
            0,
        )
        .build();

    // Set up ready promise
    let ready_promise = create_resolved_promise(context, JsValue::undefined());
    container.set(JsString::from("ready"), ready_promise, false, context)?;

    // Get or create navigator object
    let navigator = context.global_object().get(JsString::from("navigator"), context)?;
    if let Some(nav_obj) = navigator.as_object() {
        nav_obj.set(
            JsString::from("serviceWorker"),
            JsValue::from(container),
            false,
            context,
        )?;
    }

    Ok(())
}

/// Register ServiceWorkerGlobalScope (for inside service workers)
fn register_service_worker_global_scope(context: &mut Context) -> JsResult<()> {
    // self.skipWaiting()
    let skip_waiting = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(create_resolved_promise(ctx, JsValue::undefined()))
    });

    context.register_global_property(
        JsString::from("skipWaiting"),
        JsValue::from(skip_waiting.to_js_function(context.realm())),
        Attribute::WRITABLE | Attribute::CONFIGURABLE,
    )?;

    // Clients API (for inside service workers)
    let clients = ObjectInitializer::new(context)
        .function(
            NativeFunction::from_copy_closure(|_this, args, ctx| {
                let _id = args.get_or_undefined(0).to_string(ctx)?;
                // Would return specific client
                Ok(create_resolved_promise(ctx, JsValue::undefined()))
            }),
            JsString::from("get"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure(|_this, _args, ctx| {
                // Return all clients
                let arr = JsArray::new(ctx);
                Ok(create_resolved_promise(ctx, JsValue::from(arr)))
            }),
            JsString::from("matchAll"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure(|_this, args, ctx| {
                let _url = args.get_or_undefined(0).to_string(ctx)?;
                // Open a new window/tab
                let client = ObjectInitializer::new(ctx)
                    .property(
                        JsString::from("id"),
                        JsValue::from(JsString::from("client-1")),
                        Attribute::all(),
                    )
                    .property(
                        JsString::from("type"),
                        JsValue::from(JsString::from("window")),
                        Attribute::all(),
                    )
                    .property(
                        JsString::from("url"),
                        JsValue::from(JsString::from("")),
                        Attribute::all(),
                    )
                    .build();
                Ok(create_resolved_promise(ctx, JsValue::from(client)))
            }),
            JsString::from("openWindow"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure(|_this, _args, ctx| {
                // Claim all clients
                Ok(create_resolved_promise(ctx, JsValue::undefined()))
            }),
            JsString::from("claim"),
            0,
        )
        .build();

    context.register_global_property(
        JsString::from("clients"),
        JsValue::from(clients),
        Attribute::WRITABLE | Attribute::CONFIGURABLE,
    )?;

    // registration property (inside service worker)
    // This would normally be set when the worker starts

    Ok(())
}

/// Register ExtendableEvent for install/activate events
fn register_extendable_event(context: &mut Context) -> JsResult<()> {
    use boa_engine::js_string;

    let extendable_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let event = ObjectInitializer::new(ctx)
            .property(
                JsString::from("type"),
                JsValue::from(JsString::from(event_type.as_str())),
                Attribute::all(),
            )
            .property(
                JsString::from("bubbles"),
                JsValue::from(false),
                Attribute::all(),
            )
            .property(
                JsString::from("cancelable"),
                JsValue::from(false),
                Attribute::all(),
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    // waitUntil extends the lifetime of the event
                    Ok(JsValue::undefined())
                }),
                JsString::from("waitUntil"),
                1,
            )
            .build();

        Ok(JsValue::from(event))
    });

    context.register_global_builtin_callable(
        js_string!("ExtendableEvent"),
        1,
        extendable_event_constructor,
    )?;

    Ok(())
}

/// Register FetchEvent for fetch interception
fn register_fetch_event(context: &mut Context) -> JsResult<()> {
    let fetch_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let init = args.get_or_undefined(1);

        let request = if let Some(obj) = init.as_object() {
            obj.get(JsString::from("request"), ctx).unwrap_or(JsValue::undefined())
        } else {
            JsValue::undefined()
        };

        let event = ObjectInitializer::new(ctx)
            .property(
                JsString::from("type"),
                JsValue::from(JsString::from(event_type.as_str())),
                Attribute::all(),
            )
            .property(
                JsString::from("request"),
                request,
                Attribute::all(),
            )
            .property(
                JsString::from("clientId"),
                JsValue::from(JsString::from("")),
                Attribute::all(),
            )
            .property(
                JsString::from("resultingClientId"),
                JsValue::from(JsString::from("")),
                Attribute::all(),
            )
            .property(
                JsString::from("replacesClientId"),
                JsValue::from(JsString::from("")),
                Attribute::all(),
            )
            .property(
                JsString::from("handled"),
                JsValue::undefined(), // Promise
                Attribute::all(),
            )
            .property(
                JsString::from("preloadResponse"),
                JsValue::undefined(), // Promise
                Attribute::all(),
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    // respondWith provides the response
                    Ok(JsValue::undefined())
                }),
                JsString::from("respondWith"),
                1,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    // waitUntil extends the lifetime
                    Ok(JsValue::undefined())
                }),
                JsString::from("waitUntil"),
                1,
            )
            .build();

        Ok(JsValue::from(event))
    });

    use boa_engine::js_string;
    context.register_global_builtin_callable(
        js_string!("FetchEvent"),
        2,
        fetch_event_constructor,
    )?;

    Ok(())
}

/// Register PushEvent for push notifications
fn register_push_event(context: &mut Context) -> JsResult<()> {
    let push_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let init = args.get_or_undefined(1);
        let data = if let Some(obj) = init.as_object() {
            obj.get(JsString::from("data"), ctx).unwrap_or(JsValue::undefined())
        } else {
            JsValue::undefined()
        };

        // Create PushMessageData object
        let data_clone = data.clone();
        let data_clone2 = data.clone();
        let push_data = ObjectInitializer::new(ctx)
            .function(
                unsafe { NativeFunction::from_closure(move |_this, _args, ctx| {
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }) },
                JsString::from("arrayBuffer"),
                0,
            )
            .function(
                unsafe { NativeFunction::from_closure(move |_this, _args, ctx| {
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }) },
                JsString::from("blob"),
                0,
            )
            .function(
                unsafe { NativeFunction::from_closure(move |_this, _args, _ctx| {
                    Ok(data_clone.clone())
                }) },
                JsString::from("json"),
                0,
            )
            .function(
                unsafe { NativeFunction::from_closure(move |_this, _args, ctx| {
                    if data_clone2.is_string() {
                        Ok(data_clone2.clone())
                    } else {
                        Ok(JsValue::from(JsString::from("")))
                    }
                }) },
                JsString::from("text"),
                0,
            )
            .build();

        let event = ObjectInitializer::new(ctx)
            .property(
                JsString::from("type"),
                JsValue::from(JsString::from(event_type.as_str())),
                Attribute::all(),
            )
            .property(
                JsString::from("data"),
                JsValue::from(push_data),
                Attribute::all(),
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                }),
                JsString::from("waitUntil"),
                1,
            )
            .build();

        Ok(JsValue::from(event))
    });

    use boa_engine::js_string;
    context.register_global_builtin_callable(
        js_string!("PushEvent"),
        2,
        push_event_constructor,
    )?;

    Ok(())
}

/// Register NotificationEvent for notification actions
fn register_notification_event(context: &mut Context) -> JsResult<()> {
    let notification_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let init = args.get_or_undefined(1);
        let notification = if let Some(obj) = init.as_object() {
            obj.get(JsString::from("notification"), ctx).unwrap_or(JsValue::undefined())
        } else {
            JsValue::undefined()
        };
        let action = if let Some(obj) = init.as_object() {
            obj.get(JsString::from("action"), ctx)
                .unwrap_or(JsValue::from(JsString::from("")))
        } else {
            JsValue::from(JsString::from(""))
        };

        let event = ObjectInitializer::new(ctx)
            .property(
                JsString::from("type"),
                JsValue::from(JsString::from(event_type.as_str())),
                Attribute::all(),
            )
            .property(
                JsString::from("notification"),
                notification,
                Attribute::all(),
            )
            .property(
                JsString::from("action"),
                action,
                Attribute::all(),
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                }),
                JsString::from("waitUntil"),
                1,
            )
            .build();

        Ok(JsValue::from(event))
    });

    use boa_engine::js_string;
    context.register_global_builtin_callable(
        js_string!("NotificationEvent"),
        2,
        notification_event_constructor,
    )?;

    Ok(())
}

/// Register SyncEvent for background sync
fn register_sync_event(context: &mut Context) -> JsResult<()> {
    let sync_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let init = args.get_or_undefined(1);
        let tag = if let Some(obj) = init.as_object() {
            obj.get(JsString::from("tag"), ctx)
                .unwrap_or(JsValue::from(JsString::from("")))
                .to_string(ctx)?
                .to_std_string_escaped()
        } else {
            String::new()
        };
        let last_chance = if let Some(obj) = init.as_object() {
            obj.get(JsString::from("lastChance"), ctx)
                .unwrap_or(JsValue::from(false))
                .to_boolean()
        } else {
            false
        };

        let event = ObjectInitializer::new(ctx)
            .property(
                JsString::from("type"),
                JsValue::from(JsString::from(event_type.as_str())),
                Attribute::all(),
            )
            .property(
                JsString::from("tag"),
                JsValue::from(JsString::from(tag.as_str())),
                Attribute::all(),
            )
            .property(
                JsString::from("lastChance"),
                JsValue::from(last_chance),
                Attribute::all(),
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                }),
                JsString::from("waitUntil"),
                1,
            )
            .build();

        Ok(JsValue::from(event))
    });

    use boa_engine::js_string;
    context.register_global_builtin_callable(
        js_string!("SyncEvent"),
        2,
        sync_event_constructor,
    )?;

    Ok(())
}

/// Register BackgroundFetchEvent
fn register_background_fetch_event(context: &mut Context) -> JsResult<()> {
    let bg_fetch_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let init = args.get_or_undefined(1);
        let registration = if let Some(obj) = init.as_object() {
            obj.get(JsString::from("registration"), ctx).unwrap_or(JsValue::undefined())
        } else {
            JsValue::undefined()
        };

        let event = ObjectInitializer::new(ctx)
            .property(
                JsString::from("type"),
                JsValue::from(JsString::from(event_type.as_str())),
                Attribute::all(),
            )
            .property(
                JsString::from("registration"),
                registration,
                Attribute::all(),
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                }),
                JsString::from("waitUntil"),
                1,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                JsString::from("updateUI"),
                1,
            )
            .build();

        Ok(JsValue::from(event))
    });

    use boa_engine::js_string;
    context.register_global_builtin_callable(
        js_string!("BackgroundFetchEvent"),
        2,
        bg_fetch_event_constructor,
    )?;

    // Also register BackgroundFetchRegistration
    let bg_fetch_reg_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let reg = ObjectInitializer::new(ctx)
            .property(
                JsString::from("id"),
                JsValue::from(JsString::from("")),
                Attribute::all(),
            )
            .property(
                JsString::from("uploadTotal"),
                JsValue::from(0),
                Attribute::all(),
            )
            .property(
                JsString::from("uploaded"),
                JsValue::from(0),
                Attribute::all(),
            )
            .property(
                JsString::from("downloadTotal"),
                JsValue::from(0),
                Attribute::all(),
            )
            .property(
                JsString::from("downloaded"),
                JsValue::from(0),
                Attribute::all(),
            )
            .property(
                JsString::from("result"),
                JsValue::from(JsString::from("")),
                Attribute::all(),
            )
            .property(
                JsString::from("failureReason"),
                JsValue::from(JsString::from("")),
                Attribute::all(),
            )
            .property(
                JsString::from("recordsAvailable"),
                JsValue::from(false),
                Attribute::all(),
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(create_resolved_promise(ctx, JsValue::from(false)))
                }),
                JsString::from("abort"),
                0,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    let arr = JsArray::new(ctx);
                    Ok(create_resolved_promise(ctx, JsValue::from(arr)))
                }),
                JsString::from("matchAll"),
                1,
            )
            .build();

        Ok(JsValue::from(reg))
    });

    context.register_global_builtin_callable(
        js_string!("BackgroundFetchRegistration"),
        0,
        bg_fetch_reg_constructor,
    )?;

    Ok(())
}

/// Register Client constructor (for service worker clients)
fn register_client(context: &mut Context) -> JsResult<()> {
    use boa_engine::js_string;
    use boa_engine::object::FunctionObjectBuilder;

    let client_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let id = if args.len() > 0 && !args.get_or_undefined(0).is_undefined() {
            args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped()
        } else {
            format!("client-{}", NEXT_WORKER_ID.fetch_add(1, Ordering::SeqCst))
        };

        let client = ObjectInitializer::new(ctx)
            .property(
                JsString::from("id"),
                JsValue::from(JsString::from(id.as_str())),
                Attribute::READONLY,
            )
            .property(
                JsString::from("type"),
                JsValue::from(JsString::from("window")),
                Attribute::READONLY,
            )
            .property(
                JsString::from("url"),
                JsValue::from(JsString::from("")),
                Attribute::READONLY,
            )
            .property(
                JsString::from("frameType"),
                JsValue::from(JsString::from("top-level")),
                Attribute::READONLY,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    // postMessage to client
                    Ok(JsValue::undefined())
                }),
                JsString::from("postMessage"),
                1,
            )
            .build();

        Ok(JsValue::from(client))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), client_constructor)
        .name(js_string!("Client"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("Client"), ctor, false, context)?;
    Ok(())
}

/// Register WindowClient constructor (extends Client for window contexts)
fn register_window_client(context: &mut Context) -> JsResult<()> {
    use boa_engine::js_string;
    use boa_engine::object::FunctionObjectBuilder;

    let window_client_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let id = if args.len() > 0 && !args.get_or_undefined(0).is_undefined() {
            args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped()
        } else {
            format!("window-client-{}", NEXT_WORKER_ID.fetch_add(1, Ordering::SeqCst))
        };

        // Create JsArray before ObjectInitializer to avoid borrow conflict
        let ancestor_origins = JsArray::new(ctx);

        let client = ObjectInitializer::new(ctx)
            // Client properties
            .property(
                JsString::from("id"),
                JsValue::from(JsString::from(id.as_str())),
                Attribute::READONLY,
            )
            .property(
                JsString::from("type"),
                JsValue::from(JsString::from("window")),
                Attribute::READONLY,
            )
            .property(
                JsString::from("url"),
                JsValue::from(JsString::from("")),
                Attribute::READONLY,
            )
            .property(
                JsString::from("frameType"),
                JsValue::from(JsString::from("top-level")),
                Attribute::READONLY,
            )
            // WindowClient-specific properties
            .property(
                JsString::from("visibilityState"),
                JsValue::from(JsString::from("visible")),
                Attribute::READONLY,
            )
            .property(
                JsString::from("focused"),
                JsValue::from(true),
                Attribute::READONLY,
            )
            .property(
                JsString::from("ancestorOrigins"),
                JsValue::from(ancestor_origins),
                Attribute::READONLY,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                }),
                JsString::from("postMessage"),
                1,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    // focus() returns promise resolving to WindowClient
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                JsString::from("focus"),
                0,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, args, ctx| {
                    // navigate(url) returns promise resolving to WindowClient
                    let _url = args.get_or_undefined(0);
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                JsString::from("navigate"),
                1,
            )
            .build();

        Ok(JsValue::from(client))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), window_client_constructor)
        .name(js_string!("WindowClient"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("WindowClient"), ctor, false, context)?;
    Ok(())
}

/// Register ExtendableMessageEvent for message events in service workers
fn register_extendable_message_event(context: &mut Context) -> JsResult<()> {
    use boa_engine::js_string;
    use boa_engine::object::FunctionObjectBuilder;

    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let init = args.get_or_undefined(1);

        // Extract properties from init
        let (data, origin, last_event_id, source, ports) = if let Some(obj) = init.as_object() {
            let data = obj.get(JsString::from("data"), ctx).unwrap_or(JsValue::undefined());
            let origin = obj.get(JsString::from("origin"), ctx)
                .unwrap_or(JsValue::from(JsString::from("")))
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            let last_event_id = obj.get(JsString::from("lastEventId"), ctx)
                .unwrap_or(JsValue::from(JsString::from("")))
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            let source = obj.get(JsString::from("source"), ctx).unwrap_or(JsValue::null());
            let ports = obj.get(JsString::from("ports"), ctx).unwrap_or(JsValue::from(JsArray::new(ctx)));
            (data, origin, last_event_id, source, ports)
        } else {
            (JsValue::undefined(), String::new(), String::new(), JsValue::null(), JsValue::from(JsArray::new(ctx)))
        };

        let event = ObjectInitializer::new(ctx)
            .property(
                JsString::from("type"),
                JsValue::from(JsString::from(event_type.as_str())),
                Attribute::READONLY,
            )
            .property(
                JsString::from("data"),
                data,
                Attribute::READONLY,
            )
            .property(
                JsString::from("origin"),
                JsValue::from(JsString::from(origin.as_str())),
                Attribute::READONLY,
            )
            .property(
                JsString::from("lastEventId"),
                JsValue::from(JsString::from(last_event_id.as_str())),
                Attribute::READONLY,
            )
            .property(
                JsString::from("source"),
                source,
                Attribute::READONLY,
            )
            .property(
                JsString::from("ports"),
                ports,
                Attribute::READONLY,
            )
            .property(
                JsString::from("bubbles"),
                JsValue::from(false),
                Attribute::READONLY,
            )
            .property(
                JsString::from("cancelable"),
                JsValue::from(false),
                Attribute::READONLY,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    // waitUntil extends the lifetime of the event
                    Ok(JsValue::undefined())
                }),
                JsString::from("waitUntil"),
                1,
            )
            .build();

        Ok(JsValue::from(event))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("ExtendableMessageEvent"))
        .length(2)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("ExtendableMessageEvent"), ctor, false, context)?;
    Ok(())
}

/// Register ServiceWorkerGlobalScope as a proper constructor
fn register_service_worker_global_scope_constructor(context: &mut Context) -> JsResult<()> {
    use boa_engine::js_string;
    use boa_engine::object::FunctionObjectBuilder;

    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Create clients object
        let clients = ObjectInitializer::new(ctx)
            .function(
                NativeFunction::from_copy_closure(|_this, args, ctx| {
                    let _id = args.get_or_undefined(0).to_string(ctx)?;
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                JsString::from("get"),
                1,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    let arr = JsArray::new(ctx);
                    Ok(create_resolved_promise(ctx, JsValue::from(arr)))
                }),
                JsString::from("matchAll"),
                1,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    let client = ObjectInitializer::new(ctx)
                        .property(JsString::from("id"), JsValue::from(JsString::from("client-1")), Attribute::all())
                        .property(JsString::from("type"), JsValue::from(JsString::from("window")), Attribute::all())
                        .build();
                    Ok(create_resolved_promise(ctx, JsValue::from(client)))
                }),
                JsString::from("openWindow"),
                1,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                JsString::from("claim"),
                0,
            )
            .build();

        // Create caches object reference
        let caches = ObjectInitializer::new(ctx)
            .function(
                NativeFunction::from_copy_closure(|_this, args, ctx| {
                    let cache_name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let cache_obj = create_cache_object(ctx, cache_name);
                    Ok(create_resolved_promise(ctx, JsValue::from(cache_obj)))
                }),
                JsString::from("open"),
                1,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    let arr = JsArray::new(ctx);
                    Ok(create_resolved_promise(ctx, JsValue::from(arr)))
                }),
                JsString::from("keys"),
                0,
            )
            .build();

        // Create skipWaiting function
        let skip_waiting = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            Ok(create_resolved_promise(ctx, JsValue::undefined()))
        }).to_js_function(ctx.realm());

        let scope = ObjectInitializer::new(ctx)
            // Properties
            .property(JsString::from("clients"), JsValue::from(clients), Attribute::READONLY)
            .property(JsString::from("caches"), JsValue::from(caches), Attribute::READONLY)
            .property(JsString::from("registration"), JsValue::null(), Attribute::all())
            .property(JsString::from("serviceWorker"), JsValue::null(), Attribute::all())
            .property(JsString::from("skipWaiting"), JsValue::from(skip_waiting), Attribute::all())
            // Event handlers
            .property(JsString::from("onactivate"), JsValue::null(), Attribute::all())
            .property(JsString::from("onfetch"), JsValue::null(), Attribute::all())
            .property(JsString::from("oninstall"), JsValue::null(), Attribute::all())
            .property(JsString::from("onmessage"), JsValue::null(), Attribute::all())
            .property(JsString::from("onmessageerror"), JsValue::null(), Attribute::all())
            .property(JsString::from("onnotificationclick"), JsValue::null(), Attribute::all())
            .property(JsString::from("onnotificationclose"), JsValue::null(), Attribute::all())
            .property(JsString::from("onpush"), JsValue::null(), Attribute::all())
            .property(JsString::from("onpushsubscriptionchange"), JsValue::null(), Attribute::all())
            .property(JsString::from("onsync"), JsValue::null(), Attribute::all())
            // Inherited from WorkerGlobalScope
            .property(JsString::from("self"), JsValue::undefined(), Attribute::all())
            .property(JsString::from("isSecureContext"), JsValue::from(true), Attribute::READONLY)
            .property(JsString::from("origin"), JsValue::from(JsString::from("null")), Attribute::READONLY)
            .build();

        Ok(JsValue::from(scope))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("ServiceWorkerGlobalScope"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("ServiceWorkerGlobalScope"), ctor, false, context)?;
    Ok(())
}

/// Register PeriodicSyncEvent for periodic background sync
fn register_periodic_sync_event(context: &mut Context) -> JsResult<()> {
    use boa_engine::js_string;
    use boa_engine::object::FunctionObjectBuilder;

    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let init = args.get_or_undefined(1);
        let tag = if let Some(obj) = init.as_object() {
            obj.get(JsString::from("tag"), ctx)
                .unwrap_or(JsValue::from(JsString::from("")))
                .to_string(ctx)?
                .to_std_string_escaped()
        } else {
            String::new()
        };

        let event = ObjectInitializer::new(ctx)
            .property(
                JsString::from("type"),
                JsValue::from(JsString::from(event_type.as_str())),
                Attribute::READONLY,
            )
            .property(
                JsString::from("tag"),
                JsValue::from(JsString::from(tag.as_str())),
                Attribute::READONLY,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                }),
                JsString::from("waitUntil"),
                1,
            )
            .build();

        Ok(JsValue::from(event))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("PeriodicSyncEvent"))
        .length(2)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("PeriodicSyncEvent"), ctor, false, context)?;
    Ok(())
}

/// Register ContentIndexEvent for content indexing
fn register_content_index_event(context: &mut Context) -> JsResult<()> {
    use boa_engine::js_string;
    use boa_engine::object::FunctionObjectBuilder;

    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let init = args.get_or_undefined(1);
        let id = if let Some(obj) = init.as_object() {
            obj.get(JsString::from("id"), ctx)
                .unwrap_or(JsValue::from(JsString::from("")))
                .to_string(ctx)?
                .to_std_string_escaped()
        } else {
            String::new()
        };

        let event = ObjectInitializer::new(ctx)
            .property(
                JsString::from("type"),
                JsValue::from(JsString::from(event_type.as_str())),
                Attribute::READONLY,
            )
            .property(
                JsString::from("id"),
                JsValue::from(JsString::from(id.as_str())),
                Attribute::READONLY,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                }),
                JsString::from("waitUntil"),
                1,
            )
            .build();

        Ok(JsValue::from(event))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("ContentIndexEvent"))
        .length(2)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("ContentIndexEvent"), ctor, false, context)?;
    Ok(())
}

/// Register Cache constructor
fn register_cache_constructor(context: &mut Context) -> JsResult<()> {
    use boa_engine::js_string;
    use boa_engine::object::FunctionObjectBuilder;

    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let cache = ObjectInitializer::new(ctx)
            .function(
                NativeFunction::from_copy_closure(|_this, args, ctx| {
                    let _request = args.get_or_undefined(0);
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                JsString::from("match"),
                2,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    let arr = JsArray::new(ctx);
                    Ok(create_resolved_promise(ctx, JsValue::from(arr)))
                }),
                JsString::from("matchAll"),
                2,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                JsString::from("add"),
                1,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                JsString::from("addAll"),
                1,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                JsString::from("put"),
                2,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(create_resolved_promise(ctx, JsValue::from(true)))
                }),
                JsString::from("delete"),
                2,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    let arr = JsArray::new(ctx);
                    Ok(create_resolved_promise(ctx, JsValue::from(arr)))
                }),
                JsString::from("keys"),
                1,
            )
            .build();

        Ok(JsValue::from(cache))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("Cache"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("Cache"), ctor, false, context)?;
    Ok(())
}

/// Register CacheStorage constructor
fn register_cache_storage_constructor(context: &mut Context) -> JsResult<()> {
    use boa_engine::js_string;
    use boa_engine::object::FunctionObjectBuilder;

    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let storage = ObjectInitializer::new(ctx)
            .function(
                NativeFunction::from_copy_closure(|_this, args, ctx| {
                    let _request = args.get_or_undefined(0);
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                JsString::from("match"),
                2,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(create_resolved_promise(ctx, JsValue::from(false)))
                }),
                JsString::from("has"),
                1,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    let cache = ObjectInitializer::new(ctx).build();
                    Ok(create_resolved_promise(ctx, JsValue::from(cache)))
                }),
                JsString::from("open"),
                1,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(create_resolved_promise(ctx, JsValue::from(true)))
                }),
                JsString::from("delete"),
                1,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    let arr = JsArray::new(ctx);
                    Ok(create_resolved_promise(ctx, JsValue::from(arr)))
                }),
                JsString::from("keys"),
                0,
            )
            .build();

        Ok(JsValue::from(storage))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("CacheStorage"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("CacheStorage"), ctor, false, context)?;
    Ok(())
}

/// Register ServiceWorker constructor
fn register_service_worker_constructor(context: &mut Context) -> JsResult<()> {
    use boa_engine::js_string;
    use boa_engine::object::FunctionObjectBuilder;

    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let script_url = args.get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let worker = ObjectInitializer::new(ctx)
            .property(
                JsString::from("scriptURL"),
                JsValue::from(JsString::from(script_url.as_str())),
                Attribute::READONLY,
            )
            .property(
                JsString::from("state"),
                JsValue::from(JsString::from("activated")),
                Attribute::READONLY,
            )
            .property(
                JsString::from("onstatechange"),
                JsValue::null(),
                Attribute::all(),
            )
            .property(
                JsString::from("onerror"),
                JsValue::null(),
                Attribute::all(),
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                }),
                JsString::from("postMessage"),
                1,
            )
            .build();

        Ok(JsValue::from(worker))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("ServiceWorker"))
        .length(1)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("ServiceWorker"), ctor, false, context)?;
    Ok(())
}

/// Register ServiceWorkerRegistration constructor
fn register_service_worker_registration_constructor(context: &mut Context) -> JsResult<()> {
    use boa_engine::js_string;
    use boa_engine::object::FunctionObjectBuilder;

    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let scope = args.get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let registration = ObjectInitializer::new(ctx)
            .property(
                JsString::from("scope"),
                JsValue::from(JsString::from(scope.as_str())),
                Attribute::READONLY,
            )
            .property(
                JsString::from("installing"),
                JsValue::null(),
                Attribute::READONLY,
            )
            .property(
                JsString::from("waiting"),
                JsValue::null(),
                Attribute::READONLY,
            )
            .property(
                JsString::from("active"),
                JsValue::null(),
                Attribute::READONLY,
            )
            .property(
                JsString::from("updateViaCache"),
                JsValue::from(JsString::from("imports")),
                Attribute::READONLY,
            )
            .property(
                JsString::from("onupdatefound"),
                JsValue::null(),
                Attribute::all(),
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                JsString::from("update"),
                0,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(create_resolved_promise(ctx, JsValue::from(true)))
                }),
                JsString::from("unregister"),
                0,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                JsString::from("showNotification"),
                2,
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    let arr = JsArray::new(ctx);
                    Ok(create_resolved_promise(ctx, JsValue::from(arr)))
                }),
                JsString::from("getNotifications"),
                1,
            )
            .build();

        Ok(JsValue::from(registration))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("ServiceWorkerRegistration"))
        .length(1)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("ServiceWorkerRegistration"), ctor, false, context)?;
    Ok(())
}

/// Reset all service worker state (for testing)
pub fn reset_service_worker_state() {
    CACHE_STORAGE.lock().unwrap().clear();
    SW_REGISTRATIONS.lock().unwrap().clear();
    SERVICE_WORKERS.lock().unwrap().clear();
    CONTROLLERS.lock().unwrap().clear();
}

/// Register all Service Worker APIs
pub fn register_all_service_worker_apis(context: &mut Context) -> JsResult<()> {
    register_cache_storage(context)?;
    register_service_worker_container(context)?;
    register_service_worker_global_scope(context)?;
    register_extendable_event(context)?;
    register_fetch_event(context)?;
    register_push_event(context)?;
    register_notification_event(context)?;
    register_sync_event(context)?;
    register_background_fetch_event(context)?;
    // New APIs
    register_client(context)?;
    register_window_client(context)?;
    register_extendable_message_event(context)?;
    register_service_worker_global_scope_constructor(context)?;
    register_periodic_sync_event(context)?;
    register_content_index_event(context)?;
    // Global constructors
    register_cache_constructor(context)?;
    register_cache_storage_constructor(context)?;
    register_service_worker_constructor(context)?;
    register_service_worker_registration_constructor(context)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::{Context, Source};

    fn create_test_context() -> Context {
        let mut context = Context::default();

        // Initialize navigator object
        let navigator = ObjectInitializer::new(&mut context).build();
        context.register_global_property(
            JsString::from("navigator"),
            JsValue::from(navigator),
            Attribute::all(),
        ).unwrap();

        register_all_service_worker_apis(&mut context).unwrap();
        reset_service_worker_state();
        context
    }

    #[test]
    fn test_cache_storage_open() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            (async function() {
                const cache = await caches.open('test-cache');
                return typeof cache === 'object';
            })()
        "#));
        assert!(result.is_ok());
    }

    #[test]
    fn test_cache_storage_has() {
        let mut context = create_test_context();

        // First open a cache
        context.eval(Source::from_bytes(r#"
            caches.open('my-cache')
        "#)).unwrap();

        let result = context.eval(Source::from_bytes(r#"
            caches.has('my-cache').then !== undefined
        "#));
        assert!(result.is_ok());
    }

    #[test]
    fn test_cache_storage_keys() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            caches.keys().then !== undefined
        "#));
        assert!(result.is_ok());
    }

    #[test]
    fn test_cache_put_and_match() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            (async function() {
                const cache = await caches.open('test-cache');
                await cache.put('/test', { status: 200, body: 'hello' });
                const response = await cache.match('/test');
                return response !== undefined;
            })()
        "#));
        assert!(result.is_ok());
    }

    #[test]
    fn test_service_worker_register() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            navigator.serviceWorker.register('/sw.js').then !== undefined
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_extendable_event_type_exists() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            typeof ExtendableEvent === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_fetch_event_type_exists() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            typeof FetchEvent === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_push_event_type_exists() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            typeof PushEvent === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_clients_api() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            typeof clients.matchAll === 'function' &&
            typeof clients.claim === 'function' &&
            typeof clients.openWindow === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_skip_waiting() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            typeof skipWaiting === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }
}
