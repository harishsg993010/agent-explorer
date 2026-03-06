//! Streams API Implementation
//!
//! Provides ReadableStream, WritableStream, TransformStream,
//! CompressionStream, DecompressionStream and associated classes.

use boa_engine::{
    Context, JsArgs, JsResult, JsValue, js_string,
    object::ObjectInitializer,
    property::Attribute,
    NativeFunction,
    JsString,
    JsNativeError,
    object::builtins::JsArray,
};
use flate2::read::{GzDecoder, DeflateDecoder, ZlibDecoder};
use flate2::write::{GzEncoder, DeflateEncoder, ZlibEncoder};
use flate2::Compression;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, atomic::{AtomicU32, Ordering}};
use lazy_static::lazy_static;

/// Stream state
#[derive(Clone, Debug, PartialEq)]
enum StreamState {
    Readable,
    Closed,
    Errored,
}

/// Represents internal stream data
#[derive(Clone, Debug)]
struct StreamData {
    state: StreamState,
    queue: VecDeque<String>, // JSON-serialized chunks
    error: Option<String>,
    high_water_mark: usize,
    strategy_size: Option<String>, // "byteLength" or "count"
}

impl StreamData {
    fn new() -> Self {
        Self {
            state: StreamState::Readable,
            queue: VecDeque::new(),
            error: None,
            high_water_mark: 1,
            strategy_size: None,
        }
    }
}

/// Writable stream state
#[derive(Clone, Debug, PartialEq)]
enum WritableState {
    Writable,
    Closed,
    Erroring,
    Errored,
}

/// Writable stream internal data
#[derive(Clone, Debug)]
struct WritableStreamData {
    state: WritableState,
    queue: VecDeque<String>,
    error: Option<String>,
    high_water_mark: usize,
    pending_writes: usize,
}

impl WritableStreamData {
    fn new() -> Self {
        Self {
            state: WritableState::Writable,
            queue: VecDeque::new(),
            error: None,
            high_water_mark: 1,
            pending_writes: 0,
        }
    }
}

// Global state for streams
lazy_static! {
    static ref NEXT_STREAM_ID: AtomicU32 = AtomicU32::new(1);
    static ref READABLE_STREAMS: Arc<Mutex<std::collections::HashMap<u32, StreamData>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));
    static ref WRITABLE_STREAMS: Arc<Mutex<std::collections::HashMap<u32, WritableStreamData>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));
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
            js_string!("then"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            }),
            js_string!("catch"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            }),
            js_string!("finally"),
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
            js_string!("then"),
            1,
        )
        .function(
            unsafe { NativeFunction::from_closure(move |_this, args, ctx| {
                if let Some(reject) = args.get(0) {
                    if let Some(obj) = reject.as_object() {
                        let error_val = JsValue::from(JsString::from(error_msg.as_str()));
                        let _ = obj.call(&JsValue::undefined(), &[error_val], ctx);
                    }
                }
                Ok(JsValue::undefined())
            }) },
            js_string!("catch"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            }),
            js_string!("finally"),
            1,
        )
        .build();

    JsValue::from(promise)
}

/// Create a ReadableStreamDefaultController object
fn create_readable_controller(context: &mut Context, stream_id: u32) -> boa_engine::JsObject {
    let id1 = stream_id;
    let id2 = stream_id;
    let id3 = stream_id;

    ObjectInitializer::new(context)
        .property(
            js_string!("desiredSize"),
            JsValue::from(1), // Simplified
            Attribute::all(),
        )
        .function(
            NativeFunction::from_copy_closure(move |_this, args, ctx| {
                let chunk = args.get_or_undefined(0);
                let chunk_str = if chunk.is_string() {
                    chunk.to_string(ctx)?.to_std_string_escaped()
                } else {
                    // Serialize to JSON
                    serde_json::to_string(&chunk.to_string(ctx)?.to_std_string_escaped())
                        .unwrap_or_default()
                };

                let mut streams = READABLE_STREAMS.lock().unwrap();
                if let Some(stream) = streams.get_mut(&id1) {
                    stream.queue.push_back(chunk_str);
                }

                Ok(JsValue::undefined())
            }),
            js_string!("enqueue"),
            1,
        )
        .function(
            NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
                let mut streams = READABLE_STREAMS.lock().unwrap();
                if let Some(stream) = streams.get_mut(&id2) {
                    stream.state = StreamState::Closed;
                }
                Ok(JsValue::undefined())
            }),
            js_string!("close"),
            0,
        )
        .function(
            NativeFunction::from_copy_closure(move |_this, args, ctx| {
                let error = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

                let mut streams = READABLE_STREAMS.lock().unwrap();
                if let Some(stream) = streams.get_mut(&id3) {
                    stream.state = StreamState::Errored;
                    stream.error = Some(error);
                }
                Ok(JsValue::undefined())
            }),
            js_string!("error"),
            1,
        )
        .build()
}

/// Create a ReadableStreamDefaultReader object
fn create_readable_reader(context: &mut Context, stream_id: u32) -> boa_engine::JsObject {
    let id1 = stream_id;
    let id2 = stream_id;
    let id3 = stream_id;

    ObjectInitializer::new(context)
        .property(
            js_string!("closed"),
            JsValue::undefined(), // Promise
            Attribute::all(),
        )
        // reader.read()
        .function(
            NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                let streams = READABLE_STREAMS.lock().unwrap();

                if let Some(stream) = streams.get(&id1) {
                    match stream.state {
                        StreamState::Closed => {
                            // Return { done: true, value: undefined }
                            let result = ObjectInitializer::new(ctx)
                                .property(js_string!("done"), JsValue::from(true), Attribute::all())
                                .property(js_string!("value"), JsValue::undefined(), Attribute::all())
                                .build();
                            return Ok(create_resolved_promise(ctx, JsValue::from(result)));
                        }
                        StreamState::Errored => {
                            let error = stream.error.clone().unwrap_or_default();
                            return Ok(create_rejected_promise(ctx, &error));
                        }
                        StreamState::Readable => {
                            // Return next chunk if available
                            drop(streams);
                            let mut streams = READABLE_STREAMS.lock().unwrap();
                            if let Some(stream) = streams.get_mut(&id1) {
                                if let Some(chunk) = stream.queue.pop_front() {
                                    let result = ObjectInitializer::new(ctx)
                                        .property(js_string!("done"), JsValue::from(false), Attribute::all())
                                        .property(js_string!("value"), JsValue::from(JsString::from(chunk.as_str())), Attribute::all())
                                        .build();
                                    return Ok(create_resolved_promise(ctx, JsValue::from(result)));
                                }
                            }
                        }
                    }
                }

                // No chunk available, return pending read
                let result = ObjectInitializer::new(ctx)
                    .property(js_string!("done"), JsValue::from(false), Attribute::all())
                    .property(js_string!("value"), JsValue::undefined(), Attribute::all())
                    .build();
                Ok(create_resolved_promise(ctx, JsValue::from(result)))
            }),
            js_string!("read"),
            0,
        )
        // reader.releaseLock()
        .function(
            NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
                // Release the lock on the stream
                Ok(JsValue::undefined())
            }),
            js_string!("releaseLock"),
            0,
        )
        // reader.cancel(reason)
        .function(
            NativeFunction::from_copy_closure(move |_this, args, ctx| {
                let _reason = args.get_or_undefined(0);

                let mut streams = READABLE_STREAMS.lock().unwrap();
                if let Some(stream) = streams.get_mut(&id2) {
                    stream.state = StreamState::Closed;
                    stream.queue.clear();
                }

                Ok(create_resolved_promise(ctx, JsValue::undefined()))
            }),
            js_string!("cancel"),
            1,
        )
        .build()
}

/// Create a WritableStreamDefaultWriter object
fn create_writable_writer(context: &mut Context, stream_id: u32) -> boa_engine::JsObject {
    let id1 = stream_id;
    let id2 = stream_id;
    let id3 = stream_id;
    let id4 = stream_id;

    ObjectInitializer::new(context)
        .property(
            js_string!("closed"),
            JsValue::undefined(), // Promise
            Attribute::all(),
        )
        .property(
            js_string!("ready"),
            JsValue::undefined(), // Promise
            Attribute::all(),
        )
        .property(
            js_string!("desiredSize"),
            JsValue::from(1),
            Attribute::all(),
        )
        // writer.write(chunk)
        .function(
            NativeFunction::from_copy_closure(move |_this, args, ctx| {
                let chunk = args.get_or_undefined(0);
                let chunk_str = chunk.to_string(ctx)?.to_std_string_escaped();

                let mut streams = WRITABLE_STREAMS.lock().unwrap();
                if let Some(stream) = streams.get_mut(&id1) {
                    match stream.state {
                        WritableState::Closed | WritableState::Errored => {
                            return Ok(create_rejected_promise(ctx, "Stream is closed"));
                        }
                        _ => {
                            stream.queue.push_back(chunk_str);
                            stream.pending_writes += 1;
                        }
                    }
                }

                Ok(create_resolved_promise(ctx, JsValue::undefined()))
            }),
            js_string!("write"),
            1,
        )
        // writer.close()
        .function(
            NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                let mut streams = WRITABLE_STREAMS.lock().unwrap();
                if let Some(stream) = streams.get_mut(&id2) {
                    stream.state = WritableState::Closed;
                }
                Ok(create_resolved_promise(ctx, JsValue::undefined()))
            }),
            js_string!("close"),
            0,
        )
        // writer.abort(reason)
        .function(
            NativeFunction::from_copy_closure(move |_this, args, ctx| {
                let reason = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

                let mut streams = WRITABLE_STREAMS.lock().unwrap();
                if let Some(stream) = streams.get_mut(&id3) {
                    stream.state = WritableState::Errored;
                    stream.error = Some(reason);
                }
                Ok(create_resolved_promise(ctx, JsValue::undefined()))
            }),
            js_string!("abort"),
            1,
        )
        // writer.releaseLock()
        .function(
            NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
                Ok(JsValue::undefined())
            }),
            js_string!("releaseLock"),
            0,
        )
        .build()
}

/// Create a WritableStreamDefaultController object
fn create_writable_controller(context: &mut Context, stream_id: u32) -> boa_engine::JsObject {
    let id1 = stream_id;

    ObjectInitializer::new(context)
        .function(
            NativeFunction::from_copy_closure(move |_this, args, ctx| {
                let error = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

                let mut streams = WRITABLE_STREAMS.lock().unwrap();
                if let Some(stream) = streams.get_mut(&id1) {
                    stream.state = WritableState::Errored;
                    stream.error = Some(error);
                }
                Ok(JsValue::undefined())
            }),
            js_string!("error"),
            1,
        )
        .build()
}

/// Register ReadableStream constructor
fn register_readable_stream(context: &mut Context) -> JsResult<()> {
    let readable_stream_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let underlying_source = args.get(0);
        let queuing_strategy = args.get(1);

        // Create stream ID
        let stream_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst);

        // Initialize stream data
        let mut stream_data = StreamData::new();

        // Parse queuing strategy
        if let Some(strategy) = queuing_strategy.and_then(|v| v.as_object()) {
            if let Ok(hwm) = strategy.get(js_string!("highWaterMark"), ctx) {
                if let Ok(n) = hwm.to_number(ctx) {
                    stream_data.high_water_mark = n as usize;
                }
            }
        }

        // Store stream
        READABLE_STREAMS.lock().unwrap().insert(stream_id, stream_data);

        // Create controller for underlying source
        let controller = create_readable_controller(ctx, stream_id);

        // Call start if provided
        if let Some(source) = underlying_source.and_then(|v| v.as_object()) {
            if let Ok(start) = source.get(js_string!("start"), ctx) {
                if let Some(start_fn) = start.as_object() {
                    let _ = start_fn.call(&JsValue::undefined(), &[JsValue::from(controller.clone())], ctx);
                }
            }
        }

        // Create ReadableStream object
        let id_for_locked = stream_id;
        let id_for_cancel = stream_id;
        let id_for_reader = stream_id;
        let id_for_tee = stream_id;
        let id_for_pipe = stream_id;
        let id_for_values = stream_id;

        let stream = ObjectInitializer::new(ctx)
            .property(
                js_string!("locked"),
                JsValue::from(false),
                Attribute::all(),
            )
            // cancel(reason)
            .function(
                NativeFunction::from_copy_closure(move |_this, args, ctx| {
                    let _reason = args.get_or_undefined(0);

                    let mut streams = READABLE_STREAMS.lock().unwrap();
                    if let Some(stream) = streams.get_mut(&id_for_cancel) {
                        stream.state = StreamState::Closed;
                        stream.queue.clear();
                    }

                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                js_string!("cancel"),
                1,
            )
            // getReader(options)
            .function(
                NativeFunction::from_copy_closure(move |_this, args, ctx| {
                    let options = args.get(0);

                    let mode = if let Some(opts) = options.and_then(|v| v.as_object()) {
                        opts.get(js_string!("mode"), ctx)
                            .ok()
                            .and_then(|v| v.to_string(ctx).ok())
                            .map(|s| s.to_std_string_escaped())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };

                    // Create appropriate reader based on mode
                    let reader = if mode == "byob" {
                        // BYOB reader (simplified - same as default for now)
                        create_readable_reader(ctx, id_for_reader)
                    } else {
                        create_readable_reader(ctx, id_for_reader)
                    };

                    Ok(JsValue::from(reader))
                }),
                js_string!("getReader"),
                1,
            )
            // pipeThrough(transform, options)
            .function(
                NativeFunction::from_copy_closure(move |_this, args, ctx| {
                    let transform = args.get_or_undefined(0);
                    // Returns the readable side of the transform
                    if let Some(obj) = transform.as_object() {
                        if let Ok(readable) = obj.get(js_string!("readable"), ctx) {
                            return Ok(readable);
                        }
                    }
                    Ok(JsValue::undefined())
                }),
                js_string!("pipeThrough"),
                2,
            )
            // pipeTo(destination, options)
            .function(
                NativeFunction::from_copy_closure(move |_this, args, ctx| {
                    let _destination = args.get_or_undefined(0);
                    let _options = args.get(1);

                    // Simplified: just return a resolved promise
                    // Real implementation would pipe data to destination
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                js_string!("pipeTo"),
                2,
            )
            // tee()
            .function(
                NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                    // Create two new streams that receive the same data
                    let stream1_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst);
                    let stream2_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst);

                    // Copy current stream data
                    let streams = READABLE_STREAMS.lock().unwrap();
                    let original_data = streams.get(&id_for_tee).cloned();
                    drop(streams);

                    if let Some(data) = original_data {
                        let mut streams = READABLE_STREAMS.lock().unwrap();
                        streams.insert(stream1_id, data.clone());
                        streams.insert(stream2_id, data);
                    }

                    // Return array of two streams
                    let arr = JsArray::new(ctx);

                    // Simplified: return two stream objects
                    let stream1 = ObjectInitializer::new(ctx)
                        .property(js_string!("locked"), JsValue::from(false), Attribute::all())
                        .build();
                    let stream2 = ObjectInitializer::new(ctx)
                        .property(js_string!("locked"), JsValue::from(false), Attribute::all())
                        .build();

                    arr.push(JsValue::from(stream1), ctx)?;
                    arr.push(JsValue::from(stream2), ctx)?;

                    Ok(JsValue::from(arr))
                }),
                js_string!("tee"),
                0,
            )
            // values(options) - async iterator
            .function(
                NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                    // Return an async iterator
                    let id = id_for_values;
                    let iterator = ObjectInitializer::new(ctx)
                        .function(
                            NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                                let streams = READABLE_STREAMS.lock().unwrap();
                                if let Some(stream) = streams.get(&id) {
                                    if stream.state == StreamState::Closed && stream.queue.is_empty() {
                                        let result = ObjectInitializer::new(ctx)
                                            .property(js_string!("done"), JsValue::from(true), Attribute::all())
                                            .property(js_string!("value"), JsValue::undefined(), Attribute::all())
                                            .build();
                                        return Ok(create_resolved_promise(ctx, JsValue::from(result)));
                                    }
                                }
                                drop(streams);

                                let mut streams = READABLE_STREAMS.lock().unwrap();
                                if let Some(stream) = streams.get_mut(&id) {
                                    if let Some(chunk) = stream.queue.pop_front() {
                                        let result = ObjectInitializer::new(ctx)
                                            .property(js_string!("done"), JsValue::from(false), Attribute::all())
                                            .property(js_string!("value"), JsValue::from(JsString::from(chunk.as_str())), Attribute::all())
                                            .build();
                                        return Ok(create_resolved_promise(ctx, JsValue::from(result)));
                                    }
                                }

                                let result = ObjectInitializer::new(ctx)
                                    .property(js_string!("done"), JsValue::from(true), Attribute::all())
                                    .property(js_string!("value"), JsValue::undefined(), Attribute::all())
                                    .build();
                                Ok(create_resolved_promise(ctx, JsValue::from(result)))
                            }),
                            js_string!("next"),
                            0,
                        )
                        .build();
                    Ok(JsValue::from(iterator))
                }),
                js_string!("values"),
                1,
            )
            .build();

        Ok(JsValue::from(stream))
    });

    context.register_global_builtin_callable(
        js_string!("ReadableStream"),
        2,
        readable_stream_constructor,
    )?;

    Ok(())
}

/// Register WritableStream constructor
fn register_writable_stream(context: &mut Context) -> JsResult<()> {
    let writable_stream_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let underlying_sink = args.get(0);
        let queuing_strategy = args.get(1);

        // Create stream ID
        let stream_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst);

        // Initialize stream data
        let mut stream_data = WritableStreamData::new();

        // Parse queuing strategy
        if let Some(strategy) = queuing_strategy.and_then(|v| v.as_object()) {
            if let Ok(hwm) = strategy.get(js_string!("highWaterMark"), ctx) {
                if let Ok(n) = hwm.to_number(ctx) {
                    stream_data.high_water_mark = n as usize;
                }
            }
        }

        // Store stream
        WRITABLE_STREAMS.lock().unwrap().insert(stream_id, stream_data);

        // Create controller for underlying sink
        let controller = create_writable_controller(ctx, stream_id);

        // Call start if provided
        if let Some(sink) = underlying_sink.and_then(|v| v.as_object()) {
            if let Ok(start) = sink.get(js_string!("start"), ctx) {
                if let Some(start_fn) = start.as_object() {
                    let _ = start_fn.call(&JsValue::undefined(), &[JsValue::from(controller)], ctx);
                }
            }
        }

        // Create WritableStream object
        let id_for_writer = stream_id;
        let id_for_abort = stream_id;
        let id_for_close = stream_id;

        let stream = ObjectInitializer::new(ctx)
            .property(
                js_string!("locked"),
                JsValue::from(false),
                Attribute::all(),
            )
            // abort(reason)
            .function(
                NativeFunction::from_copy_closure(move |_this, args, ctx| {
                    let reason = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

                    let mut streams = WRITABLE_STREAMS.lock().unwrap();
                    if let Some(stream) = streams.get_mut(&id_for_abort) {
                        stream.state = WritableState::Errored;
                        stream.error = Some(reason);
                    }

                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                js_string!("abort"),
                1,
            )
            // close()
            .function(
                NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                    let mut streams = WRITABLE_STREAMS.lock().unwrap();
                    if let Some(stream) = streams.get_mut(&id_for_close) {
                        stream.state = WritableState::Closed;
                    }
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                js_string!("close"),
                0,
            )
            // getWriter()
            .function(
                NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                    let writer = create_writable_writer(ctx, id_for_writer);
                    Ok(JsValue::from(writer))
                }),
                js_string!("getWriter"),
                0,
            )
            .build();

        Ok(JsValue::from(stream))
    });

    context.register_global_builtin_callable(
        js_string!("WritableStream"),
        2,
        writable_stream_constructor,
    )?;

    Ok(())
}

/// Register TransformStream constructor
fn register_transform_stream(context: &mut Context) -> JsResult<()> {
    let transform_stream_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let transformer = args.get(0);
        let writable_strategy = args.get(1);
        let readable_strategy = args.get(2);

        // Create stream IDs
        let readable_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst);
        let writable_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst);

        // Initialize streams
        let mut readable_data = StreamData::new();
        let mut writable_data = WritableStreamData::new();

        // Parse strategies
        if let Some(strategy) = readable_strategy.and_then(|v| v.as_object()) {
            if let Ok(hwm) = strategy.get(js_string!("highWaterMark"), ctx) {
                if let Ok(n) = hwm.to_number(ctx) {
                    readable_data.high_water_mark = n as usize;
                }
            }
        }
        if let Some(strategy) = writable_strategy.and_then(|v| v.as_object()) {
            if let Ok(hwm) = strategy.get(js_string!("highWaterMark"), ctx) {
                if let Ok(n) = hwm.to_number(ctx) {
                    writable_data.high_water_mark = n as usize;
                }
            }
        }

        // Store streams
        READABLE_STREAMS.lock().unwrap().insert(readable_id, readable_data);
        WRITABLE_STREAMS.lock().unwrap().insert(writable_id, writable_data);

        // Create TransformStreamDefaultController
        let rid = readable_id;
        let controller = ObjectInitializer::new(ctx)
            .property(
                js_string!("desiredSize"),
                JsValue::from(1),
                Attribute::all(),
            )
            .function(
                NativeFunction::from_copy_closure(move |_this, args, ctx| {
                    let chunk = args.get_or_undefined(0);
                    let chunk_str = chunk.to_string(ctx)?.to_std_string_escaped();

                    let mut streams = READABLE_STREAMS.lock().unwrap();
                    if let Some(stream) = streams.get_mut(&rid) {
                        stream.queue.push_back(chunk_str);
                    }

                    Ok(JsValue::undefined())
                }),
                js_string!("enqueue"),
                1,
            )
            .function(
                NativeFunction::from_copy_closure(move |_this, args, ctx| {
                    let _error = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    Ok(JsValue::undefined())
                }),
                js_string!("error"),
                1,
            )
            .function(
                NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                }),
                js_string!("terminate"),
                0,
            )
            .build();

        // Call start if provided
        if let Some(t) = transformer.and_then(|v| v.as_object()) {
            if let Ok(start) = t.get(js_string!("start"), ctx) {
                if let Some(start_fn) = start.as_object() {
                    let _ = start_fn.call(&JsValue::undefined(), &[JsValue::from(controller.clone())], ctx);
                }
            }
        }

        // Create readable and writable sides
        let readable = ObjectInitializer::new(ctx)
            .property(js_string!("locked"), JsValue::from(false), Attribute::all())
            .function(
                NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                    let reader = create_readable_reader(ctx, readable_id);
                    Ok(JsValue::from(reader))
                }),
                js_string!("getReader"),
                1,
            )
            .build();

        let writable = ObjectInitializer::new(ctx)
            .property(js_string!("locked"), JsValue::from(false), Attribute::all())
            .function(
                NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                    let writer = create_writable_writer(ctx, writable_id);
                    Ok(JsValue::from(writer))
                }),
                js_string!("getWriter"),
                0,
            )
            .build();

        // Create TransformStream object
        let stream = ObjectInitializer::new(ctx)
            .property(
                js_string!("readable"),
                JsValue::from(readable),
                Attribute::all(),
            )
            .property(
                js_string!("writable"),
                JsValue::from(writable),
                Attribute::all(),
            )
            .build();

        Ok(JsValue::from(stream))
    });

    context.register_global_builtin_callable(
        js_string!("TransformStream"),
        3,
        transform_stream_constructor,
    )?;

    Ok(())
}

/// Register ByteLengthQueuingStrategy constructor
fn register_byte_length_queuing_strategy(context: &mut Context) -> JsResult<()> {
    let strategy_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let init = args.get_or_undefined(0);
        let high_water_mark = if let Some(obj) = init.as_object() {
            obj.get(js_string!("highWaterMark"), ctx)
                .ok()
                .and_then(|v| v.to_number(ctx).ok())
                .unwrap_or(0.0)
        } else {
            0.0
        };

        let strategy = ObjectInitializer::new(ctx)
            .property(
                js_string!("highWaterMark"),
                JsValue::from(high_water_mark),
                Attribute::all(),
            )
            .function(
                NativeFunction::from_copy_closure(|_this, args, ctx| {
                    let chunk = args.get_or_undefined(0);
                    // Get byte length
                    if let Some(obj) = chunk.as_object() {
                        if let Ok(byte_length) = obj.get(js_string!("byteLength"), ctx) {
                            return Ok(byte_length);
                        }
                    }
                    // For strings, return length
                    if chunk.is_string() {
                        let len = chunk.to_string(ctx)?.len();
                        return Ok(JsValue::from(len as i32));
                    }
                    Ok(JsValue::from(1))
                }),
                js_string!("size"),
                1,
            )
            .build();

        Ok(JsValue::from(strategy))
    });

    context.register_global_builtin_callable(
        js_string!("ByteLengthQueuingStrategy"),
        1,
        strategy_constructor,
    )?;

    Ok(())
}

/// Register CountQueuingStrategy constructor
fn register_count_queuing_strategy(context: &mut Context) -> JsResult<()> {
    let strategy_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let init = args.get_or_undefined(0);
        let high_water_mark = if let Some(obj) = init.as_object() {
            obj.get(js_string!("highWaterMark"), ctx)
                .ok()
                .and_then(|v| v.to_number(ctx).ok())
                .unwrap_or(1.0)
        } else {
            1.0
        };

        let strategy = ObjectInitializer::new(ctx)
            .property(
                js_string!("highWaterMark"),
                JsValue::from(high_water_mark),
                Attribute::all(),
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    // Count strategy always returns 1
                    Ok(JsValue::from(1))
                }),
                js_string!("size"),
                1,
            )
            .build();

        Ok(JsValue::from(strategy))
    });

    context.register_global_builtin_callable(
        js_string!("CountQueuingStrategy"),
        1,
        strategy_constructor,
    )?;

    Ok(())
}

/// Register ReadableStreamDefaultReader constructor (standalone)
fn register_readable_stream_reader(context: &mut Context) -> JsResult<()> {
    let reader_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let _stream = args.get_or_undefined(0);
        // In real implementation, would lock the stream
        // For now, create a basic reader object
        let stream_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst);
        let reader = create_readable_reader(ctx, stream_id);
        Ok(JsValue::from(reader))
    });

    context.register_global_builtin_callable(
        js_string!("ReadableStreamDefaultReader"),
        1,
        reader_constructor,
    )?;

    Ok(())
}

/// Register WritableStreamDefaultWriter constructor (standalone)
fn register_writable_stream_writer(context: &mut Context) -> JsResult<()> {
    let writer_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let _stream = args.get_or_undefined(0);
        // In real implementation, would lock the stream
        let stream_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst);
        let writer = create_writable_writer(ctx, stream_id);
        Ok(JsValue::from(writer))
    });

    context.register_global_builtin_callable(
        js_string!("WritableStreamDefaultWriter"),
        1,
        writer_constructor,
    )?;

    Ok(())
}

// ============================================================================
// Byte Stream APIs
// ============================================================================

/// Byte stream data for BYOB readers
#[derive(Clone, Debug)]
struct ByteStreamData {
    buffer: Vec<u8>,
    state: StreamState,
    auto_allocate_chunk_size: Option<usize>,
    pending_pull_intos: VecDeque<PullIntoDescriptor>,
}

#[derive(Clone, Debug)]
struct PullIntoDescriptor {
    buffer_byte_length: usize,
    byte_offset: usize,
    byte_length: usize,
    bytes_filled: usize,
    element_size: usize,
    reader_type: String, // "default" or "byob"
}

impl ByteStreamData {
    fn new() -> Self {
        Self {
            buffer: Vec::new(),
            state: StreamState::Readable,
            auto_allocate_chunk_size: None,
            pending_pull_intos: VecDeque::new(),
        }
    }
}

lazy_static! {
    static ref BYTE_STREAMS: Arc<Mutex<std::collections::HashMap<u32, ByteStreamData>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));
}

/// Register ReadableByteStreamController constructor
fn register_readable_byte_stream_controller(context: &mut Context) -> JsResult<()> {
    let controller_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let controller_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst);

        // Initialize byte stream data
        BYTE_STREAMS.lock().unwrap().insert(controller_id, ByteStreamData::new());

        let cid = controller_id;
        let controller = ObjectInitializer::new(ctx)
            // byobRequest property
            .property(
                js_string!("byobRequest"),
                JsValue::null(),
                Attribute::all(),
            )
            // desiredSize property
            .property(
                js_string!("desiredSize"),
                JsValue::from(1),
                Attribute::all(),
            )
            // close() method
            .function(
                NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
                    let mut streams = BYTE_STREAMS.lock().unwrap();
                    if let Some(stream) = streams.get_mut(&cid) {
                        stream.state = StreamState::Closed;
                    }
                    Ok(JsValue::undefined())
                }),
                js_string!("close"),
                0,
            )
            // enqueue(chunk) method
            .function(
                NativeFunction::from_copy_closure(move |_this, args, ctx| {
                    let chunk = args.get_or_undefined(0);

                    // Get bytes from chunk (Uint8Array or ArrayBufferView)
                    let bytes: Vec<u8> = if let Some(obj) = chunk.as_object() {
                        let length = obj.get(js_string!("length"), ctx)
                            .ok()
                            .and_then(|v| v.to_u32(ctx).ok())
                            .unwrap_or(0);
                        let mut data = Vec::with_capacity(length as usize);
                        for i in 0..length {
                            if let Ok(val) = obj.get(js_string!(i.to_string()), ctx) {
                                if let Ok(byte) = val.to_u32(ctx) {
                                    data.push(byte as u8);
                                }
                            }
                        }
                        data
                    } else {
                        Vec::new()
                    };

                    let mut streams = BYTE_STREAMS.lock().unwrap();
                    if let Some(stream) = streams.get_mut(&cid) {
                        stream.buffer.extend(bytes);
                    }

                    Ok(JsValue::undefined())
                }),
                js_string!("enqueue"),
                1,
            )
            // error(e) method
            .function(
                NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
                    let mut streams = BYTE_STREAMS.lock().unwrap();
                    if let Some(stream) = streams.get_mut(&cid) {
                        stream.state = StreamState::Errored;
                    }
                    Ok(JsValue::undefined())
                }),
                js_string!("error"),
                1,
            )
            .build();

        Ok(JsValue::from(controller))
    });

    let ctor = boa_engine::object::FunctionObjectBuilder::new(context.realm(), controller_constructor)
        .name(js_string!("ReadableByteStreamController"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("ReadableByteStreamController"), ctor, false, context)?;

    Ok(())
}

/// Register ReadableStreamBYOBReader constructor
fn register_readable_stream_byob_reader(context: &mut Context) -> JsResult<()> {
    let reader_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let _stream = args.get_or_undefined(0);
        let reader_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst);

        // Initialize byte stream for this reader
        BYTE_STREAMS.lock().unwrap().insert(reader_id, ByteStreamData::new());

        let rid = reader_id;
        let closed_promise = create_resolved_promise(ctx, JsValue::undefined());

        let reader = ObjectInitializer::new(ctx)
            // closed property (promise)
            .property(
                js_string!("closed"),
                closed_promise,
                Attribute::all(),
            )
            // read(view) method - reads into provided ArrayBufferView
            .function(
                NativeFunction::from_copy_closure(move |_this, args, ctx| {
                    let view = args.get_or_undefined(0);

                    // Get the buffer length from the view
                    let buffer_length = if let Some(obj) = view.as_object() {
                        obj.get(js_string!("byteLength"), ctx)
                            .ok()
                            .and_then(|v| v.to_u32(ctx).ok())
                            .unwrap_or(0) as usize
                    } else {
                        0
                    };

                    // Read from byte stream
                    let mut streams = BYTE_STREAMS.lock().unwrap();
                    let (value, done) = if let Some(stream) = streams.get_mut(&rid) {
                        if stream.state == StreamState::Closed || stream.buffer.is_empty() {
                            (JsValue::undefined(), true)
                        } else {
                            // Read up to buffer_length bytes
                            let bytes_to_read = std::cmp::min(buffer_length, stream.buffer.len());
                            let data: Vec<u8> = stream.buffer.drain(..bytes_to_read).collect();

                            // Create Uint8Array with the data
                            let arr = ObjectInitializer::new(ctx)
                                .property(js_string!("length"), JsValue::from(data.len() as u32), Attribute::all())
                                .property(js_string!("byteLength"), JsValue::from(data.len() as u32), Attribute::all())
                                .build();

                            for (i, byte) in data.iter().enumerate() {
                                let _ = arr.set(js_string!(i.to_string()), JsValue::from(*byte as i32), false, ctx);
                            }

                            (JsValue::from(arr), false)
                        }
                    } else {
                        (JsValue::undefined(), true)
                    };
                    drop(streams);

                    // Return { value, done } wrapped in a promise
                    let result = ObjectInitializer::new(ctx)
                        .property(js_string!("value"), value, Attribute::all())
                        .property(js_string!("done"), JsValue::from(done), Attribute::all())
                        .build();

                    Ok(create_resolved_promise(ctx, JsValue::from(result)))
                }),
                js_string!("read"),
                1,
            )
            // releaseLock() method
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                }),
                js_string!("releaseLock"),
                0,
            )
            // cancel(reason) method
            .function(
                NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                    let mut streams = BYTE_STREAMS.lock().unwrap();
                    if let Some(stream) = streams.get_mut(&rid) {
                        stream.state = StreamState::Closed;
                        stream.buffer.clear();
                    }
                    Ok(create_resolved_promise(ctx, JsValue::undefined()))
                }),
                js_string!("cancel"),
                1,
            )
            .build();

        Ok(JsValue::from(reader))
    });

    let ctor = boa_engine::object::FunctionObjectBuilder::new(context.realm(), reader_constructor)
        .name(js_string!("ReadableStreamBYOBReader"))
        .length(1)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("ReadableStreamBYOBReader"), ctor, false, context)?;

    Ok(())
}

/// Register ReadableStreamBYOBRequest constructor
fn register_readable_stream_byob_request(context: &mut Context) -> JsResult<()> {
    let request_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let request = ObjectInitializer::new(ctx)
            // view property - the ArrayBufferView to write into
            .property(
                js_string!("view"),
                JsValue::null(),
                Attribute::all(),
            )
            // respond(bytesWritten) method
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    // Signal that bytesWritten bytes have been written to the view
                    Ok(JsValue::undefined())
                }),
                js_string!("respond"),
                1,
            )
            // respondWithNewView(view) method
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    // Respond with a different view
                    Ok(JsValue::undefined())
                }),
                js_string!("respondWithNewView"),
                1,
            )
            .build();

        Ok(JsValue::from(request))
    });

    let ctor = boa_engine::object::FunctionObjectBuilder::new(context.realm(), request_constructor)
        .name(js_string!("ReadableStreamBYOBRequest"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("ReadableStreamBYOBRequest"), ctor, false, context)?;

    Ok(())
}

/// Reset streams state (for testing)
pub fn reset_streams_state() {
    READABLE_STREAMS.lock().unwrap().clear();
    WRITABLE_STREAMS.lock().unwrap().clear();
    BYTE_STREAMS.lock().unwrap().clear();
}

// ============================================================================
// Compression Streams API
// ============================================================================

/// Compression format
#[derive(Clone, Copy, Debug)]
enum CompressionFormat {
    Gzip,
    Deflate,
    DeflateRaw,
}

impl CompressionFormat {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "gzip" => Some(CompressionFormat::Gzip),
            "deflate" => Some(CompressionFormat::Deflate),
            "deflate-raw" => Some(CompressionFormat::DeflateRaw),
            _ => None,
        }
    }
}

/// Compress data using the specified format
fn compress_data(data: &[u8], format: CompressionFormat) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    match format {
        CompressionFormat::Gzip => {
            let mut encoder = GzEncoder::new(&mut output, Compression::default());
            encoder.write_all(data).map_err(|e| e.to_string())?;
            encoder.finish().map_err(|e| e.to_string())?;
        }
        CompressionFormat::Deflate => {
            let mut encoder = ZlibEncoder::new(&mut output, Compression::default());
            encoder.write_all(data).map_err(|e| e.to_string())?;
            encoder.finish().map_err(|e| e.to_string())?;
        }
        CompressionFormat::DeflateRaw => {
            let mut encoder = DeflateEncoder::new(&mut output, Compression::default());
            encoder.write_all(data).map_err(|e| e.to_string())?;
            encoder.finish().map_err(|e| e.to_string())?;
        }
    }
    Ok(output)
}

/// Decompress data using the specified format
fn decompress_data(data: &[u8], format: CompressionFormat) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    match format {
        CompressionFormat::Gzip => {
            let mut decoder = GzDecoder::new(data);
            decoder.read_to_end(&mut output).map_err(|e| e.to_string())?;
        }
        CompressionFormat::Deflate => {
            let mut decoder = ZlibDecoder::new(data);
            decoder.read_to_end(&mut output).map_err(|e| e.to_string())?;
        }
        CompressionFormat::DeflateRaw => {
            let mut decoder = DeflateDecoder::new(data);
            decoder.read_to_end(&mut output).map_err(|e| e.to_string())?;
        }
    }
    Ok(output)
}

/// Register CompressionStream constructor
fn register_compression_stream(context: &mut Context) -> JsResult<()> {
    let compression_stream_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        // Get format argument
        let format_str = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let format = CompressionFormat::from_str(&format_str)
            .ok_or_else(|| JsNativeError::typ().with_message(
                format!("Unsupported compression format: {}. Supported: gzip, deflate, deflate-raw", format_str)
            ))?;

        // Create stream IDs
        let readable_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst);
        let writable_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst);

        // Initialize streams
        READABLE_STREAMS.lock().unwrap().insert(readable_id, StreamData::new());
        WRITABLE_STREAMS.lock().unwrap().insert(writable_id, WritableStreamData::new());

        // Create readable side
        let rid = readable_id;
        let readable = ObjectInitializer::new(ctx)
            .property(js_string!("locked"), JsValue::from(false), Attribute::all())
            .function(
                NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                    let reader = create_readable_reader(ctx, rid);
                    Ok(JsValue::from(reader))
                }),
                js_string!("getReader"),
                1,
            )
            .build();

        // Create writable side with compression transform
        let _wid = writable_id;
        let rid_for_write = readable_id;
        let writable = ObjectInitializer::new(ctx)
            .property(js_string!("locked"), JsValue::from(false), Attribute::all())
            .function(
                NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                    // Create promises first to avoid borrow issues
                    let closed_promise = create_resolved_promise(ctx, JsValue::undefined());
                    let ready_promise = create_resolved_promise(ctx, JsValue::undefined());

                    // Create a writer that compresses data
                    let writer = ObjectInitializer::new(ctx)
                        .property(js_string!("closed"), closed_promise, Attribute::all())
                        .property(js_string!("ready"), ready_promise, Attribute::all())
                        .property(js_string!("desiredSize"), JsValue::from(1), Attribute::all())
                        .function(
                            NativeFunction::from_copy_closure(move |_this, args, ctx| {
                                let chunk = args.get_or_undefined(0);

                                // Get bytes from chunk (could be Uint8Array or string)
                                let bytes: Vec<u8> = if let Some(obj) = chunk.as_object() {
                                    // Try to get as Uint8Array
                                    let length = obj.get(js_string!("length"), ctx)
                                        .ok()
                                        .and_then(|v| v.to_u32(ctx).ok())
                                        .unwrap_or(0);
                                    let mut data = Vec::with_capacity(length as usize);
                                    for i in 0..length {
                                        if let Ok(val) = obj.get(js_string!(i.to_string()), ctx) {
                                            if let Ok(byte) = val.to_u32(ctx) {
                                                data.push(byte as u8);
                                            }
                                        }
                                    }
                                    data
                                } else {
                                    chunk.to_string(ctx)?.to_std_string_escaped().into_bytes()
                                };

                                // Compress the data
                                match compress_data(&bytes, format) {
                                    Ok(compressed) => {
                                        // Convert to base64 string for queue storage
                                        let encoded = base64_encode(&compressed);
                                        let mut streams = READABLE_STREAMS.lock().unwrap();
                                        if let Some(stream) = streams.get_mut(&rid_for_write) {
                                            stream.queue.push_back(encoded);
                                        }
                                    }
                                    Err(e) => {
                                        return Err(JsNativeError::error().with_message(e).into());
                                    }
                                }

                                Ok(create_resolved_promise(ctx, JsValue::undefined()))
                            }),
                            js_string!("write"),
                            1,
                        )
                        .function(
                            NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                                let mut streams = READABLE_STREAMS.lock().unwrap();
                                if let Some(stream) = streams.get_mut(&rid_for_write) {
                                    stream.state = StreamState::Closed;
                                }
                                Ok(create_resolved_promise(ctx, JsValue::undefined()))
                            }),
                            js_string!("close"),
                            0,
                        )
                        .function(
                            NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                                Ok(JsValue::undefined())
                            }),
                            js_string!("abort"),
                            1,
                        )
                        .function(
                            NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                                Ok(JsValue::undefined())
                            }),
                            js_string!("releaseLock"),
                            0,
                        )
                        .build();
                    Ok(JsValue::from(writer))
                }),
                js_string!("getWriter"),
                0,
            )
            .build();

        // Create CompressionStream object
        let stream = ObjectInitializer::new(ctx)
            .property(js_string!("readable"), JsValue::from(readable), Attribute::all())
            .property(js_string!("writable"), JsValue::from(writable), Attribute::all())
            .build();

        Ok(JsValue::from(stream))
    });

    // Use FunctionObjectBuilder to make it a proper constructor
    let ctor = boa_engine::object::FunctionObjectBuilder::new(context.realm(), compression_stream_constructor)
        .name(js_string!("CompressionStream"))
        .length(1)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("CompressionStream"), ctor, false, context)?;

    Ok(())
}

/// Register DecompressionStream constructor
fn register_decompression_stream(context: &mut Context) -> JsResult<()> {
    let decompression_stream_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        // Get format argument
        let format_str = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let format = CompressionFormat::from_str(&format_str)
            .ok_or_else(|| JsNativeError::typ().with_message(
                format!("Unsupported compression format: {}. Supported: gzip, deflate, deflate-raw", format_str)
            ))?;

        // Create stream IDs
        let readable_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst);
        let writable_id = NEXT_STREAM_ID.fetch_add(1, Ordering::SeqCst);

        // Initialize streams
        READABLE_STREAMS.lock().unwrap().insert(readable_id, StreamData::new());
        WRITABLE_STREAMS.lock().unwrap().insert(writable_id, WritableStreamData::new());

        // Create readable side
        let rid = readable_id;
        let readable = ObjectInitializer::new(ctx)
            .property(js_string!("locked"), JsValue::from(false), Attribute::all())
            .function(
                NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                    let reader = create_readable_reader(ctx, rid);
                    Ok(JsValue::from(reader))
                }),
                js_string!("getReader"),
                1,
            )
            .build();

        // Create writable side with decompression transform
        let rid_for_write = readable_id;
        let writable = ObjectInitializer::new(ctx)
            .property(js_string!("locked"), JsValue::from(false), Attribute::all())
            .function(
                NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                    // Create promises first to avoid borrow issues
                    let closed_promise = create_resolved_promise(ctx, JsValue::undefined());
                    let ready_promise = create_resolved_promise(ctx, JsValue::undefined());

                    // Create a writer that decompresses data
                    let writer = ObjectInitializer::new(ctx)
                        .property(js_string!("closed"), closed_promise, Attribute::all())
                        .property(js_string!("ready"), ready_promise, Attribute::all())
                        .property(js_string!("desiredSize"), JsValue::from(1), Attribute::all())
                        .function(
                            NativeFunction::from_copy_closure(move |_this, args, ctx| {
                                let chunk = args.get_or_undefined(0);

                                // Get bytes from chunk (could be Uint8Array or string)
                                let bytes: Vec<u8> = if let Some(obj) = chunk.as_object() {
                                    // Try to get as Uint8Array
                                    let length = obj.get(js_string!("length"), ctx)
                                        .ok()
                                        .and_then(|v| v.to_u32(ctx).ok())
                                        .unwrap_or(0);
                                    let mut data = Vec::with_capacity(length as usize);
                                    for i in 0..length {
                                        if let Ok(val) = obj.get(js_string!(i.to_string()), ctx) {
                                            if let Ok(byte) = val.to_u32(ctx) {
                                                data.push(byte as u8);
                                            }
                                        }
                                    }
                                    data
                                } else {
                                    // Try base64 decode for string data
                                    let s = chunk.to_string(ctx)?.to_std_string_escaped();
                                    base64_decode(&s).unwrap_or_else(|_| s.into_bytes())
                                };

                                // Decompress the data
                                match decompress_data(&bytes, format) {
                                    Ok(decompressed) => {
                                        // Store as UTF-8 string (for text) or base64 (for binary)
                                        let output = String::from_utf8(decompressed.clone())
                                            .unwrap_or_else(|_| base64_encode(&decompressed));
                                        let mut streams = READABLE_STREAMS.lock().unwrap();
                                        if let Some(stream) = streams.get_mut(&rid_for_write) {
                                            stream.queue.push_back(output);
                                        }
                                    }
                                    Err(e) => {
                                        return Err(JsNativeError::error().with_message(e).into());
                                    }
                                }

                                Ok(create_resolved_promise(ctx, JsValue::undefined()))
                            }),
                            js_string!("write"),
                            1,
                        )
                        .function(
                            NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                                let mut streams = READABLE_STREAMS.lock().unwrap();
                                if let Some(stream) = streams.get_mut(&rid_for_write) {
                                    stream.state = StreamState::Closed;
                                }
                                Ok(create_resolved_promise(ctx, JsValue::undefined()))
                            }),
                            js_string!("close"),
                            0,
                        )
                        .function(
                            NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                                Ok(JsValue::undefined())
                            }),
                            js_string!("abort"),
                            1,
                        )
                        .function(
                            NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                                Ok(JsValue::undefined())
                            }),
                            js_string!("releaseLock"),
                            0,
                        )
                        .build();
                    Ok(JsValue::from(writer))
                }),
                js_string!("getWriter"),
                0,
            )
            .build();

        // Create DecompressionStream object
        let stream = ObjectInitializer::new(ctx)
            .property(js_string!("readable"), JsValue::from(readable), Attribute::all())
            .property(js_string!("writable"), JsValue::from(writable), Attribute::all())
            .build();

        Ok(JsValue::from(stream))
    });

    // Use FunctionObjectBuilder to make it a proper constructor
    let ctor = boa_engine::object::FunctionObjectBuilder::new(context.realm(), decompression_stream_constructor)
        .name(js_string!("DecompressionStream"))
        .length(1)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("DecompressionStream"), ctor, false, context)?;

    Ok(())
}

/// Simple base64 encoding
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3f] as char);
        } else {
            result.push('=');
        }
    }

    result
}

/// Simple base64 decoding
fn base64_decode(data: &str) -> Result<Vec<u8>, String> {
    const DECODE_TABLE: [i8; 128] = [
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,62,-1,-1,-1,63,
        52,53,54,55,56,57,58,59,60,61,-1,-1,-1,-1,-1,-1,
        -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,14,
        15,16,17,18,19,20,21,22,23,24,25,-1,-1,-1,-1,-1,
        -1,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,
        41,42,43,44,45,46,47,48,49,50,51,-1,-1,-1,-1,-1,
    ];

    let data: Vec<u8> = data.bytes().filter(|&b| b != b'=' && !b.is_ascii_whitespace()).collect();
    if data.is_empty() {
        return Ok(Vec::new());
    }

    let mut result = Vec::with_capacity(data.len() * 3 / 4);

    for chunk in data.chunks(4) {
        if chunk.len() < 2 {
            break;
        }

        let b0 = DECODE_TABLE.get(chunk[0] as usize).copied().unwrap_or(-1);
        let b1 = DECODE_TABLE.get(chunk[1] as usize).copied().unwrap_or(-1);
        let b2 = chunk.get(2).and_then(|&b| DECODE_TABLE.get(b as usize).copied()).unwrap_or(0);
        let b3 = chunk.get(3).and_then(|&b| DECODE_TABLE.get(b as usize).copied()).unwrap_or(0);

        if b0 < 0 || b1 < 0 {
            return Err("Invalid base64".to_string());
        }

        result.push(((b0 << 2) | (b1 >> 4)) as u8);
        if chunk.len() > 2 && b2 >= 0 {
            result.push((((b1 & 0x0f) << 4) | (b2 >> 2)) as u8);
        }
        if chunk.len() > 3 && b3 >= 0 {
            result.push((((b2 & 0x03) << 6) | b3) as u8);
        }
    }

    Ok(result)
}

/// Register all Streams APIs
pub fn register_all_streams_apis(context: &mut Context) -> JsResult<()> {
    register_readable_stream(context)?;
    register_writable_stream(context)?;
    register_transform_stream(context)?;
    register_byte_length_queuing_strategy(context)?;
    register_count_queuing_strategy(context)?;
    register_readable_stream_reader(context)?;
    register_writable_stream_writer(context)?;
    register_readable_byte_stream_controller(context)?;
    register_readable_stream_byob_reader(context)?;
    register_readable_stream_byob_request(context)?;
    register_compression_stream(context)?;
    register_decompression_stream(context)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::{Context, Source};

    fn create_test_context() -> Context {
        let mut context = Context::default();
        register_all_streams_apis(&mut context).unwrap();
        reset_streams_state();
        context
    }

    #[test]
    fn test_readable_stream_type_exists() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            typeof ReadableStream === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_writable_stream_type_exists() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            typeof WritableStream === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_transform_stream_type_exists() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            typeof TransformStream === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_byte_length_queuing_strategy_exists() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            typeof ByteLengthQueuingStrategy === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_count_queuing_strategy_exists() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            typeof CountQueuingStrategy === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_readable_stream_reader_exists() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            typeof ReadableStreamDefaultReader === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_writable_stream_writer_exists() {
        let mut context = create_test_context();
        let result = context.eval(Source::from_bytes(r#"
            typeof WritableStreamDefaultWriter === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }
}
