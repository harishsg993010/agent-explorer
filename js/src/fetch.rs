//! Fetch API - Full implementation with REAL HTTP requests
//!
//! Implements:
//! - fetch() function - REAL HTTP requests with cookie persistence
//! - Request class
//! - Response class
//! - Headers class

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer,
    object::builtins::JsArray, object::FunctionObjectBuilder, property::Attribute,
    Context, JsArgs, JsObject, JsValue, JsError as BoaJsError,
};
use boa_gc::{Finalize, Trace};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::cookies;
use crate::encoding::create_readable_stream_from_bytes;

/// User-Agent string mimicking Chrome 120
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// Response body data stored for text()/json() methods
#[derive(Clone, Trace, Finalize)]
struct ResponseBody {
    #[unsafe_ignore_trace]
    content: Rc<RefCell<String>>,
}

impl ResponseBody {
    fn new(content: String) -> Self {
        ResponseBody {
            content: Rc::new(RefCell::new(content)),
        }
    }

    fn get(&self) -> String {
        self.content.borrow().clone()
    }
}

/// Create Headers class
fn create_headers_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_headers_object(ctx)))
    })
}

/// Create a Headers object
fn create_headers_object(context: &mut Context) -> JsObject {
    // Create internal storage for headers
    let headers_data: Rc<RefCell<HashMap<String, String>>> = Rc::new(RefCell::new(HashMap::new()));

    let append = {
        let data = headers_data.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase();
                let value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
                let mut map = data.borrow_mut();
                // Append: if key exists, append with comma
                let new_value = if let Some(existing) = map.get(&name) {
                    format!("{}, {}", existing, value)
                } else {
                    value
                };
                map.insert(name, new_value);
                Ok(JsValue::undefined())
            })
        }
    };

    let delete_fn = {
        let data = headers_data.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase();
                data.borrow_mut().remove(&name);
                Ok(JsValue::undefined())
            })
        }
    };

    let get = {
        let data = headers_data.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase();
                match data.borrow().get(&name) {
                    Some(value) => Ok(JsValue::from(js_string!(value.clone()))),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    let has = {
        let data = headers_data.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase();
                Ok(JsValue::from(data.borrow().contains_key(&name)))
            })
        }
    };

    let set = {
        let data = headers_data.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase();
                let value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
                data.borrow_mut().insert(name, value);
                Ok(JsValue::undefined())
            })
        }
    };

    let entries = {
        let data = headers_data.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let map = data.borrow();
                let entries: Vec<JsValue> = map.iter()
                    .map(|(k, v)| {
                        let arr = JsArray::from_iter([
                            JsValue::from(js_string!(k.clone())),
                            JsValue::from(js_string!(v.clone())),
                        ], ctx);
                        JsValue::from(arr)
                    })
                    .collect();
                Ok(JsValue::from(JsArray::from_iter(entries, ctx)))
            })
        }
    };

    let keys = {
        let data = headers_data.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let map = data.borrow();
                let keys: Vec<JsValue> = map.keys()
                    .map(|k| JsValue::from(js_string!(k.clone())))
                    .collect();
                Ok(JsValue::from(JsArray::from_iter(keys, ctx)))
            })
        }
    };

    let values = {
        let data = headers_data.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let map = data.borrow();
                let values: Vec<JsValue> = map.values()
                    .map(|v| JsValue::from(js_string!(v.clone())))
                    .collect();
                Ok(JsValue::from(JsArray::from_iter(values, ctx)))
            })
        }
    };

    let for_each = {
        let data = headers_data.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let callback = args.get_or_undefined(0);
                if callback.is_callable() {
                    let cb = callback.as_callable().unwrap();
                    let map = data.borrow();
                    for (k, v) in map.iter() {
                        let _ = cb.call(
                            &JsValue::undefined(),
                            &[JsValue::from(js_string!(v.clone())), JsValue::from(js_string!(k.clone()))],
                            ctx,
                        );
                    }
                }
                Ok(JsValue::undefined())
            })
        }
    };

    let get_set_cookie = NativeFunction::from_copy_closure(|_this, _args, ctx| Ok(JsValue::from(JsArray::new(ctx))));

    ObjectInitializer::new(context)
        .function(append, js_string!("append"), 2)
        .function(delete_fn, js_string!("delete"), 1)
        .function(get, js_string!("get"), 1)
        .function(has, js_string!("has"), 1)
        .function(set, js_string!("set"), 2)
        .function(entries, js_string!("entries"), 0)
        .function(keys, js_string!("keys"), 0)
        .function(values, js_string!("values"), 0)
        .function(for_each, js_string!("forEach"), 1)
        .function(get_set_cookie, js_string!("getSetCookie"), 0)
        .build()
}

/// Create Request class
fn create_request_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let url = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        let mut method = "GET".to_string();
        let mut mode = "cors".to_string();
        let mut credentials = "same-origin".to_string();
        let mut cache = "default".to_string();
        let mut redirect = "follow".to_string();
        let mut referrer = "about:client".to_string();
        let mut referrer_policy = "".to_string();
        let mut integrity = "".to_string();
        let mut keepalive = false;
        let mut signal = JsValue::null();
        let mut body = JsValue::null();

        if let Some(init) = args.get(1).and_then(|v| v.as_object()) {
            if let Ok(m) = init.get(js_string!("method"), ctx) {
                if !m.is_undefined() {
                    method = m.to_string(ctx)?.to_std_string_escaped().to_uppercase();
                }
            }
            if let Ok(b) = init.get(js_string!("body"), ctx) {
                if !b.is_null() && !b.is_undefined() {
                    body = b;
                }
            }
            if let Ok(m) = init.get(js_string!("mode"), ctx) {
                if !m.is_undefined() { mode = m.to_string(ctx)?.to_std_string_escaped(); }
            }
            if let Ok(c) = init.get(js_string!("credentials"), ctx) {
                if !c.is_undefined() { credentials = c.to_string(ctx)?.to_std_string_escaped(); }
            }
            if let Ok(c) = init.get(js_string!("cache"), ctx) {
                if !c.is_undefined() { cache = c.to_string(ctx)?.to_std_string_escaped(); }
            }
            if let Ok(r) = init.get(js_string!("redirect"), ctx) {
                if !r.is_undefined() { redirect = r.to_string(ctx)?.to_std_string_escaped(); }
            }
            if let Ok(r) = init.get(js_string!("referrer"), ctx) {
                if !r.is_undefined() { referrer = r.to_string(ctx)?.to_std_string_escaped(); }
            }
            if let Ok(rp) = init.get(js_string!("referrerPolicy"), ctx) {
                if !rp.is_undefined() { referrer_policy = rp.to_string(ctx)?.to_std_string_escaped(); }
            }
            if let Ok(i) = init.get(js_string!("integrity"), ctx) {
                if !i.is_undefined() { integrity = i.to_string(ctx)?.to_std_string_escaped(); }
            }
            if let Ok(k) = init.get(js_string!("keepalive"), ctx) { keepalive = k.to_boolean(); }
            if let Ok(s) = init.get(js_string!("signal"), ctx) { signal = s; }
        }

        let headers = create_headers_object(ctx);

        // Body mixin methods
        let array_buffer = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            create_resolved_promise(ctx, JsValue::undefined())
        });
        let blob = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            create_resolved_promise(ctx, JsValue::undefined())
        });
        let form_data = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            create_resolved_promise(ctx, JsValue::undefined())
        });
        let json = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            create_resolved_promise(ctx, JsValue::null())
        });
        let text = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            create_resolved_promise(ctx, JsValue::from(js_string!("")))
        });
        let clone_fn = NativeFunction::from_copy_closure(|this, _args, _ctx| {
            Ok(this.clone())
        });

        let request = ObjectInitializer::new(ctx)
            .property(js_string!("url"), js_string!(url), Attribute::READONLY)
            .property(js_string!("method"), js_string!(method), Attribute::READONLY)
            .property(js_string!("headers"), headers, Attribute::READONLY)
            .property(js_string!("body"), body, Attribute::READONLY)
            .property(js_string!("bodyUsed"), false, Attribute::all())
            .property(js_string!("mode"), js_string!(mode), Attribute::READONLY)
            .property(js_string!("credentials"), js_string!(credentials), Attribute::READONLY)
            .property(js_string!("cache"), js_string!(cache), Attribute::READONLY)
            .property(js_string!("redirect"), js_string!(redirect), Attribute::READONLY)
            .property(js_string!("referrer"), js_string!(referrer), Attribute::READONLY)
            .property(js_string!("referrerPolicy"), js_string!(referrer_policy), Attribute::READONLY)
            .property(js_string!("integrity"), js_string!(integrity), Attribute::READONLY)
            .property(js_string!("keepalive"), keepalive, Attribute::READONLY)
            .property(js_string!("signal"), signal, Attribute::READONLY)
            .property(js_string!("destination"), js_string!(""), Attribute::READONLY)
            .function(array_buffer, js_string!("arrayBuffer"), 0)
            .function(blob, js_string!("blob"), 0)
            .function(form_data, js_string!("formData"), 0)
            .function(json, js_string!("json"), 0)
            .function(text, js_string!("text"), 0)
            .function(clone_fn, js_string!("clone"), 0)
            .build();

        Ok(JsValue::from(request))
    })
}

/// Create Response class
fn create_response_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let body = args.get(0).cloned().unwrap_or(JsValue::null());
        let body_content = if body.is_string() {
            body.to_string(ctx)?.to_std_string_escaped()
        } else {
            String::new()
        };

        let mut status = 200u16;
        let mut status_text = "OK".to_string();

        if let Some(init) = args.get(1).and_then(|v| v.as_object()) {
            if let Ok(s) = init.get(js_string!("status"), ctx) {
                status = s.to_u32(ctx).unwrap_or(200) as u16;
            }
            if let Ok(st) = init.get(js_string!("statusText"), ctx) {
                if !st.is_undefined() {
                    status_text = st.to_string(ctx)?.to_std_string_escaped();
                }
            }
        }

        let ok = status >= 200 && status < 300;
        Ok(JsValue::from(create_response_object(
            ctx, status, status_text, ok, "default".to_string(), "",
            body_content, HashMap::new()
        )))
    })
}

/// Create a Response object with real body content
fn create_response_object(
    context: &mut Context,
    status: u16,
    status_text: String,
    ok: bool,
    response_type: String,
    url: &str,
    body_content: String,
    response_headers: HashMap<String, String>,
) -> JsObject {
    let headers = create_headers_object_with_data(context, response_headers);
    let body = ResponseBody::new(body_content.clone());

    // Create a real ReadableStream for the body
    let body_stream = if body_content.is_empty() {
        JsValue::null()
    } else {
        create_readable_stream_from_bytes(context, body_content.as_bytes().to_vec())
            .unwrap_or(JsValue::null())
    };

    // Body mixin methods - text() returns actual body
    let text = {
        let body_clone = body.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let content = body_clone.get();
                create_resolved_promise(ctx, JsValue::from(js_string!(content)))
            })
        }
    };

    // json() parses and returns actual JSON
    let json = {
        let body_clone = body.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let content = body_clone.get();
                // Try to parse as JSON using Boa's JSON.parse
                match ctx.eval(boa_engine::Source::from_bytes(
                    format!("JSON.parse({})", serde_json::to_string(&content).unwrap_or_else(|_| "null".to_string())).as_bytes()
                )) {
                    Ok(parsed) => create_resolved_promise(ctx, parsed),
                    Err(_) => create_resolved_promise(ctx, JsValue::null()),
                }
            })
        }
    };

    // arrayBuffer() - create ArrayBuffer-like object
    let array_buffer = {
        let body_clone = body.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let content = body_clone.get();
                let bytes = content.as_bytes();
                let ab = ObjectInitializer::new(ctx)
                    .property(js_string!("byteLength"), bytes.len() as u32, Attribute::READONLY)
                    .build();
                create_resolved_promise(ctx, JsValue::from(ab))
            })
        }
    };

    // blob() - create Blob-like object
    let blob_fn = {
        let body_clone = body.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let content = body_clone.get();
                let blob = ObjectInitializer::new(ctx)
                    .property(js_string!("size"), content.len() as u32, Attribute::READONLY)
                    .property(js_string!("type"), js_string!("text/html"), Attribute::READONLY)
                    .build();
                create_resolved_promise(ctx, JsValue::from(blob))
            })
        }
    };

    let form_data = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let fd = ObjectInitializer::new(ctx).build();
        create_resolved_promise(ctx, JsValue::from(fd))
    });

    let clone_fn = NativeFunction::from_copy_closure(|this, _args, _ctx| {
        Ok(this.clone())
    });

    ObjectInitializer::new(context)
        .property(js_string!("status"), status as u32, Attribute::READONLY)
        .property(js_string!("statusText"), js_string!(status_text), Attribute::READONLY)
        .property(js_string!("ok"), ok, Attribute::READONLY)
        .property(js_string!("headers"), headers, Attribute::READONLY)
        .property(js_string!("redirected"), false, Attribute::READONLY)
        .property(js_string!("type"), js_string!(response_type), Attribute::READONLY)
        .property(js_string!("url"), js_string!(url), Attribute::READONLY)
        .property(js_string!("bodyUsed"), false, Attribute::all())
        .property(js_string!("body"), body_stream, Attribute::READONLY)
        .function(array_buffer, js_string!("arrayBuffer"), 0)
        .function(blob_fn, js_string!("blob"), 0)
        .function(form_data, js_string!("formData"), 0)
        .function(json, js_string!("json"), 0)
        .function(text, js_string!("text"), 0)
        .function(clone_fn, js_string!("clone"), 0)
        .build()
}

/// Create a Headers object with actual data
fn create_headers_object_with_data(context: &mut Context, headers_map: HashMap<String, String>) -> JsObject {
    let headers_data = Rc::new(RefCell::new(headers_map));

    let get = {
        let data = headers_data.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase();
                let map = data.borrow();
                match map.get(&name) {
                    Some(value) => Ok(JsValue::from(js_string!(value.clone()))),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    let has = {
        let data = headers_data.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase();
                let map = data.borrow();
                Ok(JsValue::from(map.contains_key(&name)))
            })
        }
    };

    let entries = {
        let data = headers_data.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let map = data.borrow();
                let arr = JsArray::new(ctx);
                for (i, (key, value)) in map.iter().enumerate() {
                    let entry = JsArray::new(ctx);
                    let _ = entry.set(js_string!("0"), JsValue::from(js_string!(key.clone())), false, ctx);
                    let _ = entry.set(js_string!("1"), JsValue::from(js_string!(value.clone())), false, ctx);
                    let _ = entry.set(js_string!("length"), JsValue::from(2), false, ctx);
                    let _ = arr.set(js_string!(i.to_string()), JsValue::from(entry), false, ctx);
                }
                let _ = arr.set(js_string!("length"), JsValue::from(map.len() as u32), false, ctx);
                Ok(JsValue::from(arr))
            })
        }
    };

    let keys = {
        let data = headers_data.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let map = data.borrow();
                let arr = JsArray::new(ctx);
                for (i, key) in map.keys().enumerate() {
                    let _ = arr.set(js_string!(i.to_string()), JsValue::from(js_string!(key.clone())), false, ctx);
                }
                let _ = arr.set(js_string!("length"), JsValue::from(map.len() as u32), false, ctx);
                Ok(JsValue::from(arr))
            })
        }
    };

    let values = {
        let data = headers_data.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let map = data.borrow();
                let arr = JsArray::new(ctx);
                for (i, value) in map.values().enumerate() {
                    let _ = arr.set(js_string!(i.to_string()), JsValue::from(js_string!(value.clone())), false, ctx);
                }
                let _ = arr.set(js_string!("length"), JsValue::from(map.len() as u32), false, ctx);
                Ok(JsValue::from(arr))
            })
        }
    };

    let for_each = {
        let data = headers_data.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let callback = args.get_or_undefined(0);
                if callback.is_callable() {
                    let cb = callback.as_callable().unwrap();
                    let map = data.borrow();
                    for (key, value) in map.iter() {
                        let _ = cb.call(&JsValue::undefined(), &[
                            JsValue::from(js_string!(value.clone())),
                            JsValue::from(js_string!(key.clone())),
                        ], ctx);
                    }
                }
                Ok(JsValue::undefined())
            })
        }
    };

    let append = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));
    let delete = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));
    let set = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));
    let get_set_cookie = NativeFunction::from_copy_closure(|_this, _args, ctx| Ok(JsValue::from(JsArray::new(ctx))));

    ObjectInitializer::new(context)
        .function(append, js_string!("append"), 2)
        .function(delete, js_string!("delete"), 1)
        .function(get, js_string!("get"), 1)
        .function(has, js_string!("has"), 1)
        .function(set, js_string!("set"), 2)
        .function(entries, js_string!("entries"), 0)
        .function(keys, js_string!("keys"), 0)
        .function(values, js_string!("values"), 0)
        .function(for_each, js_string!("forEach"), 1)
        .function(get_set_cookie, js_string!("getSetCookie"), 0)
        .build()
}

/// Create a resolved Promise
fn create_resolved_promise(context: &mut Context, value: JsValue) -> Result<JsValue, BoaJsError> {
    let then_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
        let callback = args.get_or_undefined(0);
        if callback.is_callable() {
            let cb = callback.as_callable().unwrap();
            // Get the resolved value from this object
            if let Some(obj) = this.as_object() {
                let val = obj.get(js_string!("__value__"), ctx).unwrap_or(JsValue::undefined());
                let result = cb.call(&JsValue::undefined(), &[val], ctx)?;
                return create_resolved_promise(ctx, result);
            }
        }
        Ok(this.clone())
    });

    let catch_fn = NativeFunction::from_copy_closure(|this, _args, _ctx| {
        Ok(this.clone())
    });

    let finally_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
        let callback = args.get_or_undefined(0);
        if callback.is_callable() {
            let cb = callback.as_callable().unwrap();
            let _ = cb.call(&JsValue::undefined(), &[], ctx);
        }
        Ok(this.clone())
    });

    let promise = ObjectInitializer::new(context)
        .property(js_string!("__value__"), value, Attribute::READONLY)
        .function(then_fn, js_string!("then"), 2)
        .function(catch_fn, js_string!("catch"), 1)
        .function(finally_fn, js_string!("finally"), 1)
        .build();

    Ok(JsValue::from(promise))
}

/// Create a rejected Promise
fn create_rejected_promise(context: &mut Context, error: &str) -> Result<JsValue, BoaJsError> {
    let err_msg = error.to_string();

    let then_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
        // Skip onFulfilled, call onRejected if provided
        if let Some(on_rejected) = args.get(1) {
            if on_rejected.is_callable() {
                let cb = on_rejected.as_callable().unwrap();
                if let Some(obj) = this.as_object() {
                    let err = obj.get(js_string!("__error__"), ctx).unwrap_or(JsValue::undefined());
                    return cb.call(&JsValue::undefined(), &[err], ctx);
                }
            }
        }
        Ok(this.clone())
    });

    let catch_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
        let callback = args.get_or_undefined(0);
        if callback.is_callable() {
            let cb = callback.as_callable().unwrap();
            if let Some(obj) = this.as_object() {
                let err = obj.get(js_string!("__error__"), ctx).unwrap_or(JsValue::undefined());
                return cb.call(&JsValue::undefined(), &[err], ctx);
            }
        }
        Ok(this.clone())
    });

    let finally_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
        let callback = args.get_or_undefined(0);
        if callback.is_callable() {
            let cb = callback.as_callable().unwrap();
            let _ = cb.call(&JsValue::undefined(), &[], ctx);
        }
        Ok(this.clone())
    });

    let promise = ObjectInitializer::new(context)
        .property(js_string!("__error__"), js_string!(err_msg), Attribute::READONLY)
        .function(then_fn, js_string!("then"), 2)
        .function(catch_fn, js_string!("catch"), 1)
        .function(finally_fn, js_string!("finally"), 1)
        .build();

    Ok(JsValue::from(promise))
}

/// Make a real HTTP request with cookie persistence and browser headers
fn do_fetch(url: &str, method: &str, body: Option<String>, req_headers: HashMap<String, String>) -> Result<(u16, String, HashMap<String, String>, String), String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(30))
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .build()
        .map_err(|e| e.to_string())?;

    let mut request = match method.to_uppercase().as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "PATCH" => client.patch(url),
        "HEAD" => client.head(url),
        _ => client.get(url),
    };

    // Add cookies from shared cookie store
    let domain = cookies::extract_domain(url);
    let path = cookies::extract_path(url);
    if let Some(cookie_header) = cookies::get_cookie_header(&domain, &path) {
        request = request.header("Cookie", cookie_header);
    }

    // Add standard browser headers
    request = request
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/json")
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Accept-Encoding", "gzip, deflate, br")
        .header("Sec-Ch-Ua", "\"Not_A Brand\";v=\"8\", \"Chromium\";v=\"120\", \"Google Chrome\";v=\"120\"")
        .header("Sec-Ch-Ua-Mobile", "?0")
        .header("Sec-Ch-Ua-Platform", "\"Windows\"")
        .header("Sec-Fetch-Dest", "empty")
        .header("Sec-Fetch-Mode", "cors")
        .header("Sec-Fetch-Site", "same-origin");

    // Add custom headers (can override defaults)
    for (key, value) in req_headers {
        request = request.header(&key, &value);
    }

    // Add body if present
    if let Some(body_content) = body {
        request = request.body(body_content);
    }

    let response = request.send().map_err(|e| e.to_string())?;

    let final_url = response.url().to_string();
    let status = response.status().as_u16();
    let _status_text = response.status().canonical_reason().unwrap_or("").to_string();

    // Extract Set-Cookie headers and add to shared cookie store
    let response_domain = cookies::extract_domain(&final_url);
    for (name, value) in response.headers().iter() {
        if name.as_str().eq_ignore_ascii_case("set-cookie") {
            if let Ok(cookie_str) = value.to_str() {
                cookies::add_cookie_from_document(cookie_str, &response_domain);
            }
        }
    }

    let headers: HashMap<String, String> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_lowercase(), v.to_str().unwrap_or("").to_string()))
        .collect();

    let body_text = response.text().map_err(|e| e.to_string())?;

    Ok((status, final_url, headers, body_text))
}

/// Create the global fetch function - MAKES REAL HTTP REQUESTS
fn create_fetch_function(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let url = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        let mut method = "GET".to_string();
        let mut body: Option<String> = None;
        let mut req_headers = HashMap::new();

        if let Some(init) = args.get(1).and_then(|v| v.as_object()) {
            if let Ok(m) = init.get(js_string!("method"), ctx) {
                if !m.is_undefined() {
                    method = m.to_string(ctx)?.to_std_string_escaped().to_uppercase();
                }
            }
            if let Ok(b) = init.get(js_string!("body"), ctx) {
                if !b.is_null() && !b.is_undefined() {
                    body = Some(b.to_string(ctx)?.to_std_string_escaped());
                }
            }
            if let Ok(h) = init.get(js_string!("headers"), ctx) {
                if let Some(headers_obj) = h.as_object() {
                    // Try to extract headers from object
                    if let Ok(ct) = headers_obj.get(js_string!("Content-Type"), ctx) {
                        if !ct.is_undefined() {
                            req_headers.insert("Content-Type".to_string(), ct.to_string(ctx)?.to_std_string_escaped());
                        }
                    }
                    if let Ok(accept) = headers_obj.get(js_string!("Accept"), ctx) {
                        if !accept.is_undefined() {
                            req_headers.insert("Accept".to_string(), accept.to_string(ctx)?.to_std_string_escaped());
                        }
                    }
                }
            }
        }

        // Make real HTTP request
        match do_fetch(&url, &method, body, req_headers) {
            Ok((status, final_url, headers, body_text)) => {
                let ok = status >= 200 && status < 300;
                let status_text = if ok { "OK" } else { "Error" };
                let response = create_response_object(
                    ctx,
                    status,
                    status_text.to_string(),
                    ok,
                    "basic".to_string(),
                    &final_url,
                    body_text,
                    headers,
                );
                create_resolved_promise(ctx, JsValue::from(response))
            }
            Err(err) => {
                // Return rejected promise on network error
                create_rejected_promise(ctx, &format!("Network error: {}", err))
            }
        }
    })
}

/// Create Response static methods
fn create_response_statics(context: &mut Context) -> JsObject {
    let error = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let response = create_response_object(
            ctx, 0, "".to_string(), false, "error".to_string(), "",
            String::new(), HashMap::new()
        );
        Ok(JsValue::from(response))
    });

    let redirect = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let url = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let status = args.get(1).and_then(|v| v.to_u32(ctx).ok()).unwrap_or(302) as u16;
        let mut headers = HashMap::new();
        headers.insert("location".to_string(), url.clone());
        let response = create_response_object(
            ctx, status, "".to_string(), false, "default".to_string(), "",
            String::new(), headers
        );
        Ok(JsValue::from(response))
    });

    let json_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let data = args.get_or_undefined(0);
        let data_str = data.to_string(ctx)?.to_std_string_escaped();
        let body_content = if let Ok(json_str) = ctx.eval(boa_engine::Source::from_bytes(
            format!("JSON.stringify({})", data_str).as_bytes()
        )) {
            json_str.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default()
        } else {
            String::new()
        };

        let status = args.get(1)
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get(js_string!("status"), ctx).ok())
            .and_then(|v| v.to_u32(ctx).ok())
            .unwrap_or(200) as u16;

        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());

        let response = create_response_object(
            ctx,
            status,
            "OK".to_string(),
            status >= 200 && status < 300,
            "default".to_string(),
            "",
            body_content,
            headers,
        );
        Ok(JsValue::from(response))
    });

    ObjectInitializer::new(context)
        .function(error, js_string!("error"), 0)
        .function(redirect, js_string!("redirect"), 2)
        .function(json_fn, js_string!("json"), 2)
        .build()
}

/// Register all fetch-related APIs
pub fn register_fetch_api(context: &mut Context) -> Result<(), BoaJsError> {
    // Create all constructors first to avoid mutable borrow conflicts
    let headers_ctor = create_headers_constructor(context);
    let request_ctor = create_request_constructor(context);
    let response_ctor = create_response_constructor(context);
    let fetch_fn = create_fetch_function(context);
    let statics = create_response_statics(context);
    let fetch_later_result_ctor = create_fetch_later_result_constructor(context);
    let fetch_later_fn = create_fetch_later_function(context);

    // Register all constructors as proper constructors with prototypes
    let headers_constructor = FunctionObjectBuilder::new(context.realm(), headers_ctor)
        .name(js_string!("Headers"))
        .length(1)
        .constructor(true)
        .build();

    // Create Headers.prototype with stub methods so polyfills can detect native support
    let headers_prototype = ObjectInitializer::new(context)
        .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())), js_string!("append"), 2)
        .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())), js_string!("delete"), 1)
        .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::null())), js_string!("get"), 1)
        .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::from(false))), js_string!("has"), 1)
        .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())), js_string!("set"), 2)
        .function(NativeFunction::from_copy_closure(|_this, _args, ctx| Ok(JsValue::from(JsArray::new(ctx)))), js_string!("keys"), 0)
        .function(NativeFunction::from_copy_closure(|_this, _args, ctx| Ok(JsValue::from(JsArray::new(ctx)))), js_string!("values"), 0)
        .function(NativeFunction::from_copy_closure(|_this, _args, ctx| Ok(JsValue::from(JsArray::new(ctx)))), js_string!("entries"), 0)
        .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())), js_string!("forEach"), 1)
        .build();
    headers_constructor.set(js_string!("prototype"), headers_prototype, false, context)?;

    context.global_object().set(js_string!("Headers"), headers_constructor, false, context)?;

    let request_constructor = FunctionObjectBuilder::new(context.realm(), request_ctor)
        .name(js_string!("Request"))
        .length(2)
        .constructor(true)
        .build();

    // Create Request.prototype with stub methods so polyfills can detect native support
    let request_prototype = ObjectInitializer::new(context)
        .function(NativeFunction::from_copy_closure(|this, _args, _ctx| Ok(this.clone())), js_string!("clone"), 0)
        .function(NativeFunction::from_copy_closure(|_this, _args, ctx| create_resolved_promise(ctx, JsValue::undefined())), js_string!("arrayBuffer"), 0)
        .function(NativeFunction::from_copy_closure(|_this, _args, ctx| create_resolved_promise(ctx, JsValue::undefined())), js_string!("blob"), 0)
        .function(NativeFunction::from_copy_closure(|_this, _args, ctx| create_resolved_promise(ctx, JsValue::undefined())), js_string!("formData"), 0)
        .function(NativeFunction::from_copy_closure(|_this, _args, ctx| create_resolved_promise(ctx, JsValue::null())), js_string!("json"), 0)
        .function(NativeFunction::from_copy_closure(|_this, _args, ctx| create_resolved_promise(ctx, JsValue::from(js_string!("")))), js_string!("text"), 0)
        .build();
    request_constructor.set(js_string!("prototype"), request_prototype, false, context)?;

    context.global_object().set(js_string!("Request"), request_constructor, false, context)?;

    let response_constructor = FunctionObjectBuilder::new(context.realm(), response_ctor)
        .name(js_string!("Response"))
        .length(2)
        .constructor(true)
        .build();

    // Create Response.prototype with stub methods so polyfills can detect native support
    let response_prototype = ObjectInitializer::new(context)
        .function(NativeFunction::from_copy_closure(|this, _args, _ctx| Ok(this.clone())), js_string!("clone"), 0)
        .function(NativeFunction::from_copy_closure(|_this, _args, ctx| create_resolved_promise(ctx, JsValue::undefined())), js_string!("arrayBuffer"), 0)
        .function(NativeFunction::from_copy_closure(|_this, _args, ctx| create_resolved_promise(ctx, JsValue::undefined())), js_string!("blob"), 0)
        .function(NativeFunction::from_copy_closure(|_this, _args, ctx| create_resolved_promise(ctx, JsValue::undefined())), js_string!("formData"), 0)
        .function(NativeFunction::from_copy_closure(|_this, _args, ctx| create_resolved_promise(ctx, JsValue::null())), js_string!("json"), 0)
        .function(NativeFunction::from_copy_closure(|_this, _args, ctx| create_resolved_promise(ctx, JsValue::from(js_string!("")))), js_string!("text"), 0)
        .build();
    response_constructor.set(js_string!("prototype"), response_prototype, false, context)?;

    context.global_object().set(js_string!("Response"), response_constructor.clone(), false, context)?;

    // Add static methods to Response
    let error_fn = statics.get(js_string!("error"), context).unwrap_or(JsValue::undefined());
    let redirect_fn = statics.get(js_string!("redirect"), context).unwrap_or(JsValue::undefined());
    let json_fn = statics.get(js_string!("json"), context).unwrap_or(JsValue::undefined());
    let _ = response_constructor.set(js_string!("error"), error_fn, false, context);
    let _ = response_constructor.set(js_string!("redirect"), redirect_fn, false, context);
    let _ = response_constructor.set(js_string!("json"), json_fn, false, context);

    // Register fetch function
    context.register_global_builtin_callable(js_string!("fetch"), 2, fetch_fn)?;

    // Register FetchLaterResult constructor
    let fetch_later_result_constructor = FunctionObjectBuilder::new(context.realm(), fetch_later_result_ctor)
        .name(js_string!("FetchLaterResult"))
        .length(1)
        .constructor(true)
        .build();
    context.global_object().set(js_string!("FetchLaterResult"), fetch_later_result_constructor, false, context)?;

    // Register fetchLater function
    context.register_global_builtin_callable(js_string!("fetchLater"), 2, fetch_later_fn)?;

    Ok(())
}

/// Create FetchLaterResult constructor
/// FetchLaterResult represents the result of a deferred fetch request
/// Properties: activated (boolean)
fn create_fetch_later_result_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let activated = args.get_or_undefined(0).to_boolean();

        // Create internal state for the result
        let result = ObjectInitializer::new(ctx)
            // activated - true if the deferred request has been sent
            .property(js_string!("activated"), activated, Attribute::READONLY)
            // Event handler for when the request activates
            .property(js_string!("onactivate"), JsValue::null(), Attribute::all())
            .build();

        // Add Symbol.toStringTag
        let tag = boa_engine::JsSymbol::to_string_tag();
        let _ = result.set(tag, js_string!("FetchLaterResult"), false, ctx);

        Ok(JsValue::from(result))
    })
}

/// Storage for deferred fetch requests
lazy_static::lazy_static! {
    static ref DEFERRED_REQUESTS: std::sync::Mutex<Vec<DeferredRequest>> = std::sync::Mutex::new(Vec::new());
    static ref NEXT_DEFERRED_ID: std::sync::Mutex<u64> = std::sync::Mutex::new(1);
}

#[derive(Clone)]
struct DeferredRequest {
    id: u64,
    url: String,
    method: String,
    body: Option<String>,
    headers: HashMap<String, String>,
    activate_after: Option<u64>,
    created_at: std::time::Instant,
    activated: bool,
}

/// Execute a deferred request (called internally)
fn execute_deferred_request(request: &DeferredRequest) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = match request.method.as_str() {
        "GET" => client.get(&request.url),
        "POST" => client.post(&request.url),
        "PUT" => client.put(&request.url),
        "DELETE" => client.delete(&request.url),
        "PATCH" => client.patch(&request.url),
        "HEAD" => client.head(&request.url),
        _ => client.post(&request.url),
    };

    // Add cookies
    let domain = crate::cookies::extract_domain(&request.url);
    let path = crate::cookies::extract_path(&request.url);
    if let Some(cookie_header) = crate::cookies::get_cookie_header(&domain, &path) {
        req = req.header("Cookie", cookie_header);
    }

    // Add custom headers
    for (key, value) in &request.headers {
        req = req.header(key.as_str(), value.as_str());
    }

    // Add body
    if let Some(ref body) = request.body {
        req = req.body(body.clone());
    }

    // Send the request (we don't care about the response for fetchLater)
    let _ = req.send();
    Ok(())
}

/// Process all pending deferred requests (should be called on page unload)
pub fn process_deferred_requests() {
    if let Ok(mut requests) = DEFERRED_REQUESTS.lock() {
        for request in requests.iter_mut() {
            if !request.activated {
                if let Err(e) = execute_deferred_request(request) {
                    eprintln!("[fetchLater] Error sending deferred request: {}", e);
                }
                request.activated = true;
            }
        }
        requests.clear();
    }
}

/// Create fetchLater function
/// fetchLater(url, options) - schedules a fetch to be made later (e.g., on page unload)
/// Returns a FetchLaterResult object
///
/// Per spec: https://wicg.github.io/pending-beacon/
/// - Request is sent on page unload or after activateAfter timeout
/// - If activateAfter is 0, request is sent immediately
/// - Returns FetchLaterResult with activated property
fn create_fetch_later_function(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let url = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        // Validate URL
        if url.is_empty() {
            return Err(boa_engine::JsNativeError::typ()
                .with_message("fetchLater: URL is required")
                .into());
        }

        // Parse options
        let mut method = "POST".to_string(); // Default to POST for beacons
        let mut body: Option<String> = None;
        let mut headers: HashMap<String, String> = HashMap::new();
        let mut activate_after: Option<u64> = None;

        if let Some(init) = args.get(1).and_then(|v| v.as_object()) {
            // Method
            if let Ok(m) = init.get(js_string!("method"), ctx) {
                if !m.is_undefined() {
                    method = m.to_string(ctx)?.to_std_string_escaped().to_uppercase();
                }
            }

            // Body
            if let Ok(b) = init.get(js_string!("body"), ctx) {
                if !b.is_null() && !b.is_undefined() {
                    body = Some(b.to_string(ctx)?.to_std_string_escaped());
                }
            }

            // Headers
            if let Ok(h) = init.get(js_string!("headers"), ctx) {
                if let Some(headers_obj) = h.as_object() {
                    // Common headers
                    for header_name in &["Content-Type", "Accept", "Authorization", "X-Requested-With"] {
                        if let Ok(val) = headers_obj.get(js_string!(*header_name), ctx) {
                            if !val.is_undefined() && !val.is_null() {
                                headers.insert(header_name.to_string(), val.to_string(ctx)?.to_std_string_escaped());
                            }
                        }
                    }
                }
            }

            // activateAfter - time in ms after which the request should be sent
            if let Ok(aa) = init.get(js_string!("activateAfter"), ctx) {
                if !aa.is_undefined() {
                    activate_after = Some(aa.to_u32(ctx).unwrap_or(0) as u64);
                }
            }
        }

        // Generate unique ID for this request
        let id = {
            let mut next_id = NEXT_DEFERRED_ID.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            id
        };

        // Create the deferred request
        let mut deferred = DeferredRequest {
            id,
            url: url.clone(),
            method,
            body,
            headers,
            activate_after,
            created_at: std::time::Instant::now(),
            activated: false,
        };

        // If activateAfter is 0, send immediately
        let activated = if activate_after == Some(0) {
            if let Err(e) = execute_deferred_request(&deferred) {
                eprintln!("[fetchLater] Error sending immediate request: {}", e);
            }
            deferred.activated = true;
            true
        } else if let Some(delay_ms) = activate_after {
            // Schedule the request to be sent after the delay
            // For simplicity, we use a separate thread
            let request_clone = deferred.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                if let Err(e) = execute_deferred_request(&request_clone) {
                    eprintln!("[fetchLater] Error sending delayed request: {}", e);
                }
                // Mark as activated in the storage
                if let Ok(mut requests) = DEFERRED_REQUESTS.lock() {
                    if let Some(req) = requests.iter_mut().find(|r| r.id == request_clone.id) {
                        req.activated = true;
                    }
                }
            });
            false
        } else {
            // No activateAfter specified - will be sent on page unload
            false
        };

        // Store the request for potential page unload activation
        if !activated {
            if let Ok(mut requests) = DEFERRED_REQUESTS.lock() {
                requests.push(deferred);
            }
        }

        // Create and return FetchLaterResult
        let result = ObjectInitializer::new(ctx)
            .property(js_string!("activated"), activated, Attribute::READONLY)
            .property(js_string!("onactivate"), JsValue::null(), Attribute::all())
            // Store request ID for status checking
            .property(js_string!("__id__"), JsValue::from(id as f64), Attribute::READONLY)
            .property(js_string!("__url__"), js_string!(url), Attribute::READONLY)
            .build();

        let tag = boa_engine::JsSymbol::to_string_tag();
        let _ = result.set(tag, js_string!("FetchLaterResult"), false, ctx);

        Ok(JsValue::from(result))
    })
}
