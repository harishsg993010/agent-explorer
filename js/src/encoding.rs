//! Encoding APIs - TextEncoder, TextDecoder, Blob, File, FileReader, FormData
//!
//! Provides full implementations of encoding and binary data APIs.

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer, property::Attribute,
    object::builtins::JsArray, object::FunctionObjectBuilder, Context, JsArgs, JsNativeError, JsObject, JsResult, JsValue,
};
use std::collections::HashMap;
use std::sync::Mutex;

lazy_static::lazy_static! {
    /// Storage for Blob data by ID
    static ref BLOB_STORAGE: Mutex<HashMap<u32, BlobData>> = Mutex::new(HashMap::new());
    static ref NEXT_BLOB_ID: Mutex<u32> = Mutex::new(1);

    /// Storage for FormData by ID
    static ref FORMDATA_STORAGE: Mutex<HashMap<u32, Vec<FormDataEntry>>> = Mutex::new(HashMap::new());
    static ref NEXT_FORMDATA_ID: Mutex<u32> = Mutex::new(1);

    /// FileReader state storage
    static ref FILEREADER_STORAGE: Mutex<HashMap<u32, FileReaderState>> = Mutex::new(HashMap::new());
    static ref NEXT_FILEREADER_ID: Mutex<u32> = Mutex::new(1);

    /// ReadableStream state storage
    static ref READABLE_STREAM_STORAGE: Mutex<HashMap<u32, ReadableStreamState>> = Mutex::new(HashMap::new());
    static ref NEXT_STREAM_ID: Mutex<u32> = Mutex::new(1);
}

#[derive(Clone)]
struct BlobData {
    data: Vec<u8>,
    mime_type: String,
}

#[derive(Clone)]
struct FormDataEntry {
    name: String,
    value: FormDataValue,
}

#[derive(Clone)]
enum FormDataValue {
    String(String),
    File { data: Vec<u8>, filename: String, mime_type: String },
}

#[derive(Clone)]
struct FileReaderState {
    ready_state: u16,
    result: Option<FileReaderResult>,
    error: Option<String>,
}

#[derive(Clone)]
enum FileReaderResult {
    ArrayBuffer(Vec<u8>),
    Text(String),
    DataUrl(String),
    BinaryString(String),
}

// FileReader ready states
const EMPTY: u16 = 0;
const LOADING: u16 = 1;
const DONE: u16 = 2;

/// ReadableStream state for streaming Blob data
#[derive(Clone)]
struct ReadableStreamState {
    data: Vec<u8>,
    position: usize,
    chunk_size: usize,
    locked: bool,
    closed: bool,
}

/// Register all encoding APIs
pub fn register_all_encoding_apis(context: &mut Context) -> JsResult<()> {
    register_text_encoder(context)?;
    register_text_decoder(context)?;
    register_text_encoder_stream(context)?;
    register_text_decoder_stream(context)?;
    register_blob(context)?;
    register_file(context)?;
    register_file_list(context)?;
    register_file_reader(context)?;
    // FormData is registered in forms.rs with better form extraction support
    // register_form_data(context)?;
    register_url_api(context)?;
    register_url_search_params(context)?;
    Ok(())
}

/// Register TextEncoder
fn register_text_encoder(context: &mut Context) -> JsResult<()> {
    let text_encoder_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // encode(string) -> Uint8Array
        let encode = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let input = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let bytes = input.as_bytes();

            // Create Uint8Array
            let array = create_uint8_array(ctx, bytes)?;
            Ok(JsValue::from(array))
        });

        // encodeInto(string, uint8array) -> {read, written}
        let encode_into = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let input = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let bytes = input.as_bytes();

            // Get destination array length
            let dest = args.get_or_undefined(1);
            let dest_len = if let Some(obj) = dest.as_object() {
                obj.get(js_string!("length"), ctx)
                    .ok()
                    .and_then(|v| v.to_u32(ctx).ok())
                    .unwrap_or(0) as usize
            } else {
                0
            };

            let written = bytes.len().min(dest_len);
            let read = input.chars().take(written).count();

            // Write bytes to destination
            if let Some(obj) = dest.as_object() {
                for (i, &byte) in bytes.iter().take(written).enumerate() {
                    let _ = obj.set(js_string!(i.to_string()), JsValue::from(byte as u32), false, ctx);
                }
            }

            let result = ObjectInitializer::new(ctx)
                .property(js_string!("read"), read as u32, Attribute::all())
                .property(js_string!("written"), written as u32, Attribute::all())
                .build();

            Ok(JsValue::from(result))
        });

        let encoder = ObjectInitializer::new(ctx)
            .property(js_string!("encoding"), js_string!("utf-8"), Attribute::READONLY)
            .function(encode, js_string!("encode"), 1)
            .function(encode_into, js_string!("encodeInto"), 2)
            .build();

        Ok(JsValue::from(encoder))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), text_encoder_constructor)
        .name(js_string!("TextEncoder"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("TextEncoder"), constructor, false, context)?;

    Ok(())
}

/// Register TextDecoder
fn register_text_decoder(context: &mut Context) -> JsResult<()> {
    let text_decoder_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let encoding = if args.len() > 0 && !args.get_or_undefined(0).is_undefined() {
            args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase()
        } else {
            "utf-8".to_string()
        };

        // Parse options
        let (fatal, ignore_bom) = if args.len() > 1 {
            let options = args.get_or_undefined(1);
            if let Some(obj) = options.as_object() {
                let fatal = obj.get(js_string!("fatal"), ctx)
                    .ok()
                    .map(|v| v.to_boolean())
                    .unwrap_or(false);
                let ignore_bom = obj.get(js_string!("ignoreBOM"), ctx)
                    .ok()
                    .map(|v| v.to_boolean())
                    .unwrap_or(false);
                (fatal, ignore_bom)
            } else {
                (false, false)
            }
        } else {
            (false, false)
        };

        // decode(buffer, options?) -> string
        let encoding_clone = encoding.clone();
        let fatal_clone = fatal;
        let decode = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let input = args.get_or_undefined(0);

                // Extract bytes from ArrayBuffer, Uint8Array, or similar
                let bytes = extract_bytes_from_buffer(input, ctx)?;

                // Decode based on encoding
                let result = match encoding_clone.as_str() {
                    "utf-8" | "utf8" => {
                        if fatal_clone {
                            String::from_utf8(bytes.clone())
                                .map_err(|_| JsNativeError::typ().with_message("Invalid UTF-8"))?
                        } else {
                            String::from_utf8_lossy(&bytes).to_string()
                        }
                    }
                    "utf-16le" | "utf-16" => {
                        decode_utf16le(&bytes)
                    }
                    "utf-16be" => {
                        decode_utf16be(&bytes)
                    }
                    "ascii" | "us-ascii" => {
                        bytes.iter().map(|&b| b as char).collect()
                    }
                    "iso-8859-1" | "latin1" => {
                        bytes.iter().map(|&b| b as char).collect()
                    }
                    _ => {
                        // Default to UTF-8 lossy
                        String::from_utf8_lossy(&bytes).to_string()
                    }
                };

                Ok(JsValue::from(js_string!(result)))
            })
        };

        let decoder = ObjectInitializer::new(ctx)
            .property(js_string!("encoding"), js_string!(encoding), Attribute::READONLY)
            .property(js_string!("fatal"), fatal, Attribute::READONLY)
            .property(js_string!("ignoreBOM"), ignore_bom, Attribute::READONLY)
            .function(decode, js_string!("decode"), 2)
            .build();

        Ok(JsValue::from(decoder))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), text_decoder_constructor)
        .name(js_string!("TextDecoder"))
        .length(2)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("TextDecoder"), constructor, false, context)?;

    Ok(())
}

// Stream storage for text streams
lazy_static::lazy_static! {
    static ref TEXT_STREAM_STORAGE: Mutex<HashMap<u32, TextStreamData>> = Mutex::new(HashMap::new());
    static ref NEXT_TEXT_STREAM_ID: Mutex<u32> = Mutex::new(1);
}

#[derive(Clone)]
struct TextStreamData {
    queue: Vec<Vec<u8>>,  // Output bytes (for encoder) or decoded strings storage
    closed: bool,
    encoding: String,
    fatal: bool,
    ignore_bom: bool,
    pending_bytes: Vec<u8>,  // For decoder: incomplete multi-byte sequences
}

impl TextStreamData {
    fn new_encoder() -> Self {
        Self {
            queue: Vec::new(),
            closed: false,
            encoding: "utf-8".to_string(),
            fatal: false,
            ignore_bom: false,
            pending_bytes: Vec::new(),
        }
    }

    fn new_decoder(encoding: String, fatal: bool, ignore_bom: bool) -> Self {
        Self {
            queue: Vec::new(),
            closed: false,
            encoding,
            fatal,
            ignore_bom,
            pending_bytes: Vec::new(),
        }
    }
}

/// Register TextEncoderStream
fn register_text_encoder_stream(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Create stream ID
        let stream_id = {
            let mut id = NEXT_TEXT_STREAM_ID.lock().unwrap();
            let current = *id;
            *id += 1;
            current
        };

        // Initialize stream data
        {
            let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
            storage.insert(stream_id, TextStreamData::new_encoder());
        }

        // Create readable side
        let rid = stream_id;
        let readable = create_text_encoder_readable(ctx, rid);

        // Create writable side
        let wid = stream_id;
        let writable = create_text_encoder_writable(ctx, wid);

        // Create TextEncoderStream object
        let stream = ObjectInitializer::new(ctx)
            .property(js_string!("encoding"), js_string!("utf-8"), Attribute::READONLY)
            .property(js_string!("readable"), JsValue::from(readable), Attribute::READONLY)
            .property(js_string!("writable"), JsValue::from(writable), Attribute::READONLY)
            .build();

        Ok(JsValue::from(stream))
    });

    let ctor = boa_engine::object::FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("TextEncoderStream"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("TextEncoderStream"), ctor, false, context)?;
    Ok(())
}

fn create_text_encoder_readable(ctx: &mut Context, stream_id: u32) -> JsObject {
    let sid = stream_id;
    let sid2 = stream_id;

    // getReader function
    let get_reader = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let reader = create_text_encoder_reader(ctx, sid);
        Ok(JsValue::from(reader))
    });

    // cancel function
    let cancel = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
        if let Some(data) = storage.get_mut(&sid2) {
            data.closed = true;
        }
        Ok(create_text_stream_promise(ctx, JsValue::undefined()))
    });

    ObjectInitializer::new(ctx)
        .property(js_string!("locked"), false, Attribute::all())
        .function(get_reader, js_string!("getReader"), 1)
        .function(cancel, js_string!("cancel"), 1)
        .build()
}

fn create_text_encoder_reader(ctx: &mut Context, stream_id: u32) -> JsObject {
    let sid = stream_id;
    let sid2 = stream_id;
    let sid3 = stream_id;

    // read() function - returns Promise<{done, value}>
    let read = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
        if let Some(data) = storage.get_mut(&sid) {
            if !data.queue.is_empty() {
                let bytes = data.queue.remove(0);
                drop(storage);
                let uint8array = create_uint8_array(ctx, &bytes)?;
                let result = ObjectInitializer::new(ctx)
                    .property(js_string!("done"), false, Attribute::all())
                    .property(js_string!("value"), JsValue::from(uint8array), Attribute::all())
                    .build();
                return Ok(create_text_stream_promise(ctx, JsValue::from(result)));
            } else if data.closed {
                drop(storage);
                let result = ObjectInitializer::new(ctx)
                    .property(js_string!("done"), true, Attribute::all())
                    .property(js_string!("value"), JsValue::undefined(), Attribute::all())
                    .build();
                return Ok(create_text_stream_promise(ctx, JsValue::from(result)));
            }
        }
        drop(storage);

        // No data yet - return done: false, value: undefined (simplified)
        let result = ObjectInitializer::new(ctx)
            .property(js_string!("done"), false, Attribute::all())
            .property(js_string!("value"), JsValue::undefined(), Attribute::all())
            .build();
        Ok(create_text_stream_promise(ctx, JsValue::from(result)))
    });

    // cancel() function
    let cancel = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
        if let Some(data) = storage.get_mut(&sid2) {
            data.closed = true;
        }
        Ok(create_text_stream_promise(ctx, JsValue::undefined()))
    });

    // releaseLock() function
    let release_lock = NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let closed_promise = create_text_stream_promise(ctx, JsValue::undefined());

    ObjectInitializer::new(ctx)
        .property(js_string!("closed"), closed_promise, Attribute::READONLY)
        .function(read, js_string!("read"), 0)
        .function(cancel, js_string!("cancel"), 1)
        .function(release_lock, js_string!("releaseLock"), 0)
        .build()
}

fn create_text_encoder_writable(ctx: &mut Context, stream_id: u32) -> JsObject {
    let sid = stream_id;
    let sid2 = stream_id;

    // getWriter function
    let get_writer = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let writer = create_text_encoder_writer(ctx, sid);
        Ok(JsValue::from(writer))
    });

    // abort function
    let abort = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
        if let Some(data) = storage.get_mut(&sid2) {
            data.closed = true;
        }
        Ok(create_text_stream_promise(ctx, JsValue::undefined()))
    });

    ObjectInitializer::new(ctx)
        .property(js_string!("locked"), false, Attribute::all())
        .function(get_writer, js_string!("getWriter"), 0)
        .function(abort, js_string!("abort"), 1)
        .build()
}

fn create_text_encoder_writer(ctx: &mut Context, stream_id: u32) -> JsObject {
    let sid = stream_id;
    let sid2 = stream_id;
    let sid3 = stream_id;

    // write(chunk) - encode string to Uint8Array
    let write = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let chunk = args.get_or_undefined(0);
        let input = chunk.to_string(ctx)?.to_std_string_escaped();
        let bytes = input.into_bytes();

        let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
        if let Some(data) = storage.get_mut(&sid) {
            data.queue.push(bytes);
        }
        drop(storage);

        Ok(create_text_stream_promise(ctx, JsValue::undefined()))
    });

    // close()
    let close = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
        if let Some(data) = storage.get_mut(&sid2) {
            data.closed = true;
        }
        Ok(create_text_stream_promise(ctx, JsValue::undefined()))
    });

    // abort()
    let abort = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
        if let Some(data) = storage.get_mut(&sid3) {
            data.closed = true;
        }
        Ok(create_text_stream_promise(ctx, JsValue::undefined()))
    });

    // releaseLock()
    let release_lock = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let closed_promise = create_text_stream_promise(ctx, JsValue::undefined());
    let ready_promise = create_text_stream_promise(ctx, JsValue::undefined());

    ObjectInitializer::new(ctx)
        .property(js_string!("closed"), closed_promise, Attribute::READONLY)
        .property(js_string!("ready"), ready_promise, Attribute::READONLY)
        .property(js_string!("desiredSize"), 1, Attribute::READONLY)
        .function(write, js_string!("write"), 1)
        .function(close, js_string!("close"), 0)
        .function(abort, js_string!("abort"), 1)
        .function(release_lock, js_string!("releaseLock"), 0)
        .build()
}

/// Register TextDecoderStream
fn register_text_decoder_stream(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        // Get encoding (default utf-8)
        let encoding = if args.len() > 0 && !args.get_or_undefined(0).is_undefined() {
            args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase()
        } else {
            "utf-8".to_string()
        };

        // Parse options
        let (fatal, ignore_bom) = if args.len() > 1 {
            let options = args.get_or_undefined(1);
            if let Some(obj) = options.as_object() {
                let fatal = obj.get(js_string!("fatal"), ctx)
                    .ok()
                    .map(|v| v.to_boolean())
                    .unwrap_or(false);
                let ignore_bom = obj.get(js_string!("ignoreBOM"), ctx)
                    .ok()
                    .map(|v| v.to_boolean())
                    .unwrap_or(false);
                (fatal, ignore_bom)
            } else {
                (false, false)
            }
        } else {
            (false, false)
        };

        // Create stream ID
        let stream_id = {
            let mut id = NEXT_TEXT_STREAM_ID.lock().unwrap();
            let current = *id;
            *id += 1;
            current
        };

        // Initialize stream data
        {
            let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
            storage.insert(stream_id, TextStreamData::new_decoder(encoding.clone(), fatal, ignore_bom));
        }

        // Create readable side
        let rid = stream_id;
        let readable = create_text_decoder_readable(ctx, rid);

        // Create writable side
        let wid = stream_id;
        let writable = create_text_decoder_writable(ctx, wid);

        // Create TextDecoderStream object
        let stream = ObjectInitializer::new(ctx)
            .property(js_string!("encoding"), js_string!(encoding), Attribute::READONLY)
            .property(js_string!("fatal"), fatal, Attribute::READONLY)
            .property(js_string!("ignoreBOM"), ignore_bom, Attribute::READONLY)
            .property(js_string!("readable"), JsValue::from(readable), Attribute::READONLY)
            .property(js_string!("writable"), JsValue::from(writable), Attribute::READONLY)
            .build();

        Ok(JsValue::from(stream))
    });

    let ctor = boa_engine::object::FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("TextDecoderStream"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("TextDecoderStream"), ctor, false, context)?;
    Ok(())
}

fn create_text_decoder_readable(ctx: &mut Context, stream_id: u32) -> JsObject {
    let sid = stream_id;
    let sid2 = stream_id;

    // getReader function
    let get_reader = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let reader = create_text_decoder_reader(ctx, sid);
        Ok(JsValue::from(reader))
    });

    // cancel function
    let cancel = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
        if let Some(data) = storage.get_mut(&sid2) {
            data.closed = true;
        }
        Ok(create_text_stream_promise(ctx, JsValue::undefined()))
    });

    ObjectInitializer::new(ctx)
        .property(js_string!("locked"), false, Attribute::all())
        .function(get_reader, js_string!("getReader"), 1)
        .function(cancel, js_string!("cancel"), 1)
        .build()
}

fn create_text_decoder_reader(ctx: &mut Context, stream_id: u32) -> JsObject {
    let sid = stream_id;
    let sid2 = stream_id;

    // read() function - returns Promise<{done, value}>
    let read = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
        if let Some(data) = storage.get_mut(&sid) {
            if !data.queue.is_empty() {
                let bytes = data.queue.remove(0);
                // Decode bytes to string
                let decoded = String::from_utf8_lossy(&bytes).to_string();
                drop(storage);
                let result = ObjectInitializer::new(ctx)
                    .property(js_string!("done"), false, Attribute::all())
                    .property(js_string!("value"), js_string!(decoded), Attribute::all())
                    .build();
                return Ok(create_text_stream_promise(ctx, JsValue::from(result)));
            } else if data.closed {
                drop(storage);
                let result = ObjectInitializer::new(ctx)
                    .property(js_string!("done"), true, Attribute::all())
                    .property(js_string!("value"), JsValue::undefined(), Attribute::all())
                    .build();
                return Ok(create_text_stream_promise(ctx, JsValue::from(result)));
            }
        }
        drop(storage);

        // No data yet - return done: false, value: undefined (simplified)
        let result = ObjectInitializer::new(ctx)
            .property(js_string!("done"), false, Attribute::all())
            .property(js_string!("value"), JsValue::undefined(), Attribute::all())
            .build();
        Ok(create_text_stream_promise(ctx, JsValue::from(result)))
    });

    // cancel() function
    let cancel = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
        if let Some(data) = storage.get_mut(&sid2) {
            data.closed = true;
        }
        Ok(create_text_stream_promise(ctx, JsValue::undefined()))
    });

    // releaseLock() function
    let release_lock = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let closed_promise = create_text_stream_promise(ctx, JsValue::undefined());

    ObjectInitializer::new(ctx)
        .property(js_string!("closed"), closed_promise, Attribute::READONLY)
        .function(read, js_string!("read"), 0)
        .function(cancel, js_string!("cancel"), 1)
        .function(release_lock, js_string!("releaseLock"), 0)
        .build()
}

fn create_text_decoder_writable(ctx: &mut Context, stream_id: u32) -> JsObject {
    let sid = stream_id;
    let sid2 = stream_id;

    // getWriter function
    let get_writer = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let writer = create_text_decoder_writer(ctx, sid);
        Ok(JsValue::from(writer))
    });

    // abort function
    let abort = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
        if let Some(data) = storage.get_mut(&sid2) {
            data.closed = true;
        }
        Ok(create_text_stream_promise(ctx, JsValue::undefined()))
    });

    ObjectInitializer::new(ctx)
        .property(js_string!("locked"), false, Attribute::all())
        .function(get_writer, js_string!("getWriter"), 0)
        .function(abort, js_string!("abort"), 1)
        .build()
}

fn create_text_decoder_writer(ctx: &mut Context, stream_id: u32) -> JsObject {
    let sid = stream_id;
    let sid2 = stream_id;
    let sid3 = stream_id;

    // write(chunk) - decode Uint8Array to string
    let write = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let chunk = args.get_or_undefined(0);

        // Extract bytes from chunk (Uint8Array or ArrayBuffer)
        let bytes: Vec<u8> = if let Some(obj) = chunk.as_object() {
            let length = obj.get(js_string!("length"), ctx)
                .or_else(|_| obj.get(js_string!("byteLength"), ctx))
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
            // If it's a string, convert to bytes
            chunk.to_string(ctx)?.to_std_string_escaped().into_bytes()
        };

        let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
        if let Some(data) = storage.get_mut(&sid) {
            data.queue.push(bytes);
        }
        drop(storage);

        Ok(create_text_stream_promise(ctx, JsValue::undefined()))
    });

    // close()
    let close = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
        if let Some(data) = storage.get_mut(&sid2) {
            data.closed = true;
        }
        Ok(create_text_stream_promise(ctx, JsValue::undefined()))
    });

    // abort()
    let abort = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let mut storage = TEXT_STREAM_STORAGE.lock().unwrap();
        if let Some(data) = storage.get_mut(&sid3) {
            data.closed = true;
        }
        Ok(create_text_stream_promise(ctx, JsValue::undefined()))
    });

    // releaseLock()
    let release_lock = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let closed_promise = create_text_stream_promise(ctx, JsValue::undefined());
    let ready_promise = create_text_stream_promise(ctx, JsValue::undefined());

    ObjectInitializer::new(ctx)
        .property(js_string!("closed"), closed_promise, Attribute::READONLY)
        .property(js_string!("ready"), ready_promise, Attribute::READONLY)
        .property(js_string!("desiredSize"), 1, Attribute::READONLY)
        .function(write, js_string!("write"), 1)
        .function(close, js_string!("close"), 0)
        .function(abort, js_string!("abort"), 1)
        .function(release_lock, js_string!("releaseLock"), 0)
        .build()
}

/// Helper to create a resolved promise for text streams
fn create_text_stream_promise(ctx: &mut Context, value: JsValue) -> JsValue {
    let value_clone = value.clone();
    let then = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            if let Some(callback) = args.first() {
                if callback.is_callable() {
                    let callback_obj = callback.as_callable().unwrap();
                    return callback_obj.call(&JsValue::undefined(), &[value_clone.clone()], ctx);
                }
            }
            Ok(JsValue::undefined())
        })
    };

    let catch_fn = NativeFunction::from_copy_closure(|this, _args, _ctx| {
        Ok(this.clone())
    });

    let finally_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        if let Some(callback) = args.first() {
            if callback.is_callable() {
                let callback_obj = callback.as_callable().unwrap();
                let _ = callback_obj.call(&JsValue::undefined(), &[], ctx);
            }
        }
        Ok(JsValue::undefined())
    });

    let promise = ObjectInitializer::new(ctx)
        .function(then, js_string!("then"), 2)
        .function(catch_fn, js_string!("catch"), 1)
        .function(finally_fn, js_string!("finally"), 1)
        .build();

    JsValue::from(promise)
}

/// Register Blob
fn register_blob(context: &mut Context) -> JsResult<()> {
    let blob_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let mut data = Vec::new();

        // Process blobParts array
        if args.len() > 0 {
            let parts = args.get_or_undefined(0);
            if let Some(arr) = parts.as_object() {
                let length = arr.get(js_string!("length"), ctx)
                    .ok()
                    .and_then(|v| v.to_u32(ctx).ok())
                    .unwrap_or(0);

                for i in 0..length {
                    if let Ok(part) = arr.get(js_string!(i.to_string()), ctx) {
                        if let Ok(s) = part.to_string(ctx) {
                            data.extend_from_slice(s.to_std_string_escaped().as_bytes());
                        } else if let Some(obj) = part.as_object() {
                            // Check if it's a Blob
                            if let Ok(blob_id) = obj.get(js_string!("_blobId"), ctx) {
                                if let Ok(id) = blob_id.to_u32(ctx) {
                                    let storage = BLOB_STORAGE.lock().unwrap();
                                    if let Some(blob_data) = storage.get(&id) {
                                        data.extend_from_slice(&blob_data.data);
                                    }
                                }
                            } else {
                                // Try to extract as ArrayBuffer/TypedArray
                                if let Ok(bytes) = extract_bytes_from_buffer(&part, ctx) {
                                    data.extend_from_slice(&bytes);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Parse options
        let mime_type = if args.len() > 1 {
            let options = args.get_or_undefined(1);
            if let Some(obj) = options.as_object() {
                obj.get(js_string!("type"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_default()
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Store blob data
        let blob_id = {
            let mut id = NEXT_BLOB_ID.lock().unwrap();
            let current = *id;
            *id += 1;

            let mut storage = BLOB_STORAGE.lock().unwrap();
            storage.insert(current, BlobData {
                data: data.clone(),
                mime_type: mime_type.clone(),
            });

            current
        };

        create_blob_object(ctx, blob_id, data.len(), &mime_type)
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), blob_constructor)
        .name(js_string!("Blob"))
        .length(2)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("Blob"), constructor, false, context)?;

    Ok(())
}

/// Create a Blob object
fn create_blob_object(ctx: &mut Context, blob_id: u32, size: usize, mime_type: &str) -> JsResult<JsValue> {
    let mime = mime_type.to_string();

    // slice(start?, end?, contentType?) -> Blob
    let slice_id = blob_id;
    let slice = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let storage = BLOB_STORAGE.lock().unwrap();
            let blob_data = storage.get(&slice_id).cloned();
            drop(storage);

            if let Some(data) = blob_data {
                let len = data.data.len() as i64;

                let start = args.get_or_undefined(0).to_i32(ctx).unwrap_or(0) as i64;
                let end = if args.len() > 1 && !args.get_or_undefined(1).is_undefined() {
                    args.get_or_undefined(1).to_i32(ctx).unwrap_or(len as i32) as i64
                } else {
                    len
                };

                let content_type = if args.len() > 2 {
                    args.get_or_undefined(2).to_string(ctx)?.to_std_string_escaped()
                } else {
                    data.mime_type.clone()
                };

                // Normalize indices
                let start = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
                let end = if end < 0 { (len + end).max(0) } else { end.min(len) } as usize;

                let sliced_data: Vec<u8> = if start < end {
                    data.data[start..end].to_vec()
                } else {
                    Vec::new()
                };

                // Create new blob
                let new_blob_id = {
                    let mut id = NEXT_BLOB_ID.lock().unwrap();
                    let current = *id;
                    *id += 1;

                    let mut storage = BLOB_STORAGE.lock().unwrap();
                    storage.insert(current, BlobData {
                        data: sliced_data.clone(),
                        mime_type: content_type.clone(),
                    });

                    current
                };

                create_blob_object(ctx, new_blob_id, sliced_data.len(), &content_type)
            } else {
                create_blob_object(ctx, 0, 0, "")
            }
        })
    };

    // text() -> Promise<string>
    let text_id = blob_id;
    let text = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let storage = BLOB_STORAGE.lock().unwrap();
            let text = if let Some(data) = storage.get(&text_id) {
                String::from_utf8_lossy(&data.data).to_string()
            } else {
                String::new()
            };
            drop(storage);

            create_resolved_promise(ctx, JsValue::from(js_string!(text)))
        })
    };

    // arrayBuffer() -> Promise<ArrayBuffer>
    let ab_id = blob_id;
    let array_buffer = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let storage = BLOB_STORAGE.lock().unwrap();
            let bytes = if let Some(data) = storage.get(&ab_id) {
                data.data.clone()
            } else {
                Vec::new()
            };
            drop(storage);

            let ab = create_array_buffer(ctx, &bytes)?;
            create_resolved_promise(ctx, JsValue::from(ab))
        })
    };

    // bytes() -> Promise<Uint8Array>
    let bytes_id = blob_id;
    let bytes_fn = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let storage = BLOB_STORAGE.lock().unwrap();
            let bytes = if let Some(data) = storage.get(&bytes_id) {
                data.data.clone()
            } else {
                Vec::new()
            };
            drop(storage);

            let arr = create_uint8_array(ctx, &bytes)?;
            create_resolved_promise(ctx, JsValue::from(arr))
        })
    };

    // stream() -> ReadableStream
    let stream_blob_id = blob_id;
    let stream = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            // Get blob data
            let blob_data = {
                let storage = BLOB_STORAGE.lock().unwrap();
                storage.get(&stream_blob_id).map(|b| b.data.clone()).unwrap_or_default()
            };

            // Create stream state
            let stream_id = {
                let mut id = NEXT_STREAM_ID.lock().unwrap();
                let current = *id;
                *id += 1;

                let mut storage = READABLE_STREAM_STORAGE.lock().unwrap();
                storage.insert(current, ReadableStreamState {
                    data: blob_data,
                    position: 0,
                    chunk_size: 65536, // 64KB chunks
                    locked: false,
                    closed: false,
                });
                current
            };

            create_readable_stream_object(ctx, stream_id)
        })
    };

    let blob = ObjectInitializer::new(ctx)
        .property(js_string!("size"), size as u32, Attribute::READONLY)
        .property(js_string!("type"), js_string!(mime), Attribute::READONLY)
        .property(js_string!("_blobId"), blob_id, Attribute::READONLY)
        .function(slice, js_string!("slice"), 3)
        .function(text, js_string!("text"), 0)
        .function(array_buffer, js_string!("arrayBuffer"), 0)
        .function(bytes_fn, js_string!("bytes"), 0)
        .function(stream, js_string!("stream"), 0)
        .build();

    Ok(JsValue::from(blob))
}

/// Create a ReadableStream object for streaming data
fn create_readable_stream_object(ctx: &mut Context, stream_id: u32) -> JsResult<JsValue> {
    // getReader() -> ReadableStreamDefaultReader
    let get_reader = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            // Lock the stream
            {
                let mut storage = READABLE_STREAM_STORAGE.lock().unwrap();
                if let Some(state) = storage.get_mut(&stream_id) {
                    state.locked = true;
                }
            }
            create_readable_stream_reader(ctx, stream_id)
        })
    };

    // cancel(reason?) -> Promise<undefined>
    let cancel_id = stream_id;
    let cancel = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            {
                let mut storage = READABLE_STREAM_STORAGE.lock().unwrap();
                if let Some(state) = storage.get_mut(&cancel_id) {
                    state.closed = true;
                }
            }
            create_resolved_promise(ctx, JsValue::undefined())
        })
    };

    // locked getter
    let locked_id = stream_id;
    let locked_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let storage = READABLE_STREAM_STORAGE.lock().unwrap();
            let locked = storage.get(&locked_id).map(|s| s.locked).unwrap_or(false);
            Ok(JsValue::from(locked))
        })
    };

    let stream = ObjectInitializer::new(ctx)
        .property(js_string!("_streamId"), stream_id, Attribute::READONLY)
        .function(get_reader, js_string!("getReader"), 1)
        .function(cancel, js_string!("cancel"), 1)
        .build();

    // Define locked as getter
    let _ = stream.define_property_or_throw(
        js_string!("locked"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(locked_getter.to_js_function(ctx.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        ctx,
    );

    Ok(JsValue::from(stream))
}

/// Create a ReadableStreamDefaultReader
fn create_readable_stream_reader(ctx: &mut Context, stream_id: u32) -> JsResult<JsValue> {
    // read() -> Promise<{done: boolean, value?: Uint8Array}>
    let read = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let mut storage = READABLE_STREAM_STORAGE.lock().unwrap();

            if let Some(state) = storage.get_mut(&stream_id) {
                if state.closed || state.position >= state.data.len() {
                    // Stream is done
                    drop(storage);
                    let result = ObjectInitializer::new(ctx)
                        .property(js_string!("done"), true, Attribute::all())
                        .property(js_string!("value"), JsValue::undefined(), Attribute::all())
                        .build();
                    return create_resolved_promise(ctx, JsValue::from(result));
                }

                // Read a chunk
                let end = (state.position + state.chunk_size).min(state.data.len());
                let chunk = state.data[state.position..end].to_vec();
                state.position = end;
                drop(storage);

                // Create Uint8Array with chunk data
                let arr = create_uint8_array(ctx, &chunk)?;
                let result = ObjectInitializer::new(ctx)
                    .property(js_string!("done"), false, Attribute::all())
                    .property(js_string!("value"), arr, Attribute::all())
                    .build();
                create_resolved_promise(ctx, JsValue::from(result))
            } else {
                drop(storage);
                let result = ObjectInitializer::new(ctx)
                    .property(js_string!("done"), true, Attribute::all())
                    .property(js_string!("value"), JsValue::undefined(), Attribute::all())
                    .build();
                create_resolved_promise(ctx, JsValue::from(result))
            }
        })
    };

    // releaseLock()
    let release_id = stream_id;
    let release_lock = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let mut storage = READABLE_STREAM_STORAGE.lock().unwrap();
            if let Some(state) = storage.get_mut(&release_id) {
                state.locked = false;
            }
            Ok(JsValue::undefined())
        })
    };

    // cancel(reason?) -> Promise<undefined>
    let cancel_id = stream_id;
    let cancel = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            {
                let mut storage = READABLE_STREAM_STORAGE.lock().unwrap();
                if let Some(state) = storage.get_mut(&cancel_id) {
                    state.closed = true;
                    state.locked = false;
                }
            }
            create_resolved_promise(ctx, JsValue::undefined())
        })
    };

    // closed getter - returns a Promise that resolves when stream closes
    let closed_id = stream_id;
    let closed_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let storage = READABLE_STREAM_STORAGE.lock().unwrap();
            let is_closed = storage.get(&closed_id).map(|s| s.closed).unwrap_or(true);
            drop(storage);

            if is_closed {
                create_resolved_promise(ctx, JsValue::undefined())
            } else {
                // Return a pending-like promise (resolved for simplicity)
                create_resolved_promise(ctx, JsValue::undefined())
            }
        })
    };

    let reader = ObjectInitializer::new(ctx)
        .property(js_string!("_streamId"), stream_id, Attribute::READONLY)
        .function(read, js_string!("read"), 0)
        .function(release_lock, js_string!("releaseLock"), 0)
        .function(cancel, js_string!("cancel"), 1)
        .build();

    // Define closed as getter
    let _ = reader.define_property_or_throw(
        js_string!("closed"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(closed_getter.to_js_function(ctx.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        ctx,
    );

    Ok(JsValue::from(reader))
}

/// Public function to calculate total storage usage in bytes
/// Used by navigator.storage.estimate()
pub fn get_storage_usage() -> u64 {
    let mut total: u64 = 0;

    // Sum up Blob storage
    if let Ok(storage) = BLOB_STORAGE.lock() {
        for blob in storage.values() {
            total += blob.data.len() as u64;
        }
    }

    // Sum up FormData storage
    if let Ok(storage) = FORMDATA_STORAGE.lock() {
        for entries in storage.values() {
            for entry in entries {
                total += entry.name.len() as u64;
                match &entry.value {
                    FormDataValue::String(s) => total += s.len() as u64,
                    FormDataValue::File { data, filename, mime_type } => {
                        total += data.len() as u64;
                        total += filename.len() as u64;
                        total += mime_type.len() as u64;
                    }
                }
            }
        }
    }

    // Sum up ReadableStream storage
    if let Ok(storage) = READABLE_STREAM_STORAGE.lock() {
        for stream in storage.values() {
            total += stream.data.len() as u64;
        }
    }

    // Sum up FileReader storage
    if let Ok(storage) = FILEREADER_STORAGE.lock() {
        for state in storage.values() {
            if let Some(ref result) = state.result {
                total += match result {
                    FileReaderResult::ArrayBuffer(data) => data.len() as u64,
                    FileReaderResult::Text(s) => s.len() as u64,
                    FileReaderResult::DataUrl(s) => s.len() as u64,
                    FileReaderResult::BinaryString(s) => s.len() as u64,
                };
            }
        }
    }

    total
}

/// Public function to create a ReadableStream from bytes
/// Used by fetch.rs for Response.body
pub fn create_readable_stream_from_bytes(ctx: &mut Context, data: Vec<u8>) -> JsResult<JsValue> {
    // Create stream state
    let stream_id = {
        let mut id = NEXT_STREAM_ID.lock().unwrap();
        let current = *id;
        *id += 1;

        let mut storage = READABLE_STREAM_STORAGE.lock().unwrap();
        storage.insert(current, ReadableStreamState {
            data,
            position: 0,
            chunk_size: 65536, // 64KB chunks
            locked: false,
            closed: false,
        });
        current
    };

    create_readable_stream_object(ctx, stream_id)
}

/// Register File (extends Blob)
fn register_file(context: &mut Context) -> JsResult<()> {
    let file_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let mut data = Vec::new();

        // Process fileBits array
        if args.len() > 0 {
            let parts = args.get_or_undefined(0);
            if let Some(arr) = parts.as_object() {
                let length = arr.get(js_string!("length"), ctx)
                    .ok()
                    .and_then(|v| v.to_u32(ctx).ok())
                    .unwrap_or(0);

                for i in 0..length {
                    if let Ok(part) = arr.get(js_string!(i.to_string()), ctx) {
                        if let Ok(s) = part.to_string(ctx) {
                            data.extend_from_slice(s.to_std_string_escaped().as_bytes());
                        } else if let Ok(bytes) = extract_bytes_from_buffer(&part, ctx) {
                            data.extend_from_slice(&bytes);
                        }
                    }
                }
            }
        }

        // Get filename
        let filename = if args.len() > 1 {
            args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped()
        } else {
            String::new()
        };

        // Parse options
        let (mime_type, last_modified) = if args.len() > 2 {
            let options = args.get_or_undefined(2);
            if let Some(obj) = options.as_object() {
                let mime = obj.get(js_string!("type"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_default();
                let lm = obj.get(js_string!("lastModified"), ctx)
                    .ok()
                    .and_then(|v| v.to_number(ctx).ok())
                    .unwrap_or_else(|| {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as f64)
                            .unwrap_or(0.0)
                    });
                (mime, lm)
            } else {
                (String::new(), std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as f64)
                    .unwrap_or(0.0))
            }
        } else {
            (String::new(), std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as f64)
                .unwrap_or(0.0))
        };

        // Store blob data
        let blob_id = {
            let mut id = NEXT_BLOB_ID.lock().unwrap();
            let current = *id;
            *id += 1;

            let mut storage = BLOB_STORAGE.lock().unwrap();
            storage.insert(current, BlobData {
                data: data.clone(),
                mime_type: mime_type.clone(),
            });

            current
        };

        create_file_object(ctx, blob_id, data.len(), &mime_type, &filename, last_modified)
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), file_constructor)
        .name(js_string!("File"))
        .length(3)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("File"), constructor, false, context)?;

    Ok(())
}

/// Create a File object
fn create_file_object(
    ctx: &mut Context,
    blob_id: u32,
    size: usize,
    mime_type: &str,
    filename: &str,
    last_modified: f64,
) -> JsResult<JsValue> {
    let mime = mime_type.to_string();
    let name = filename.to_string();

    // Inherit Blob methods
    let slice_id = blob_id;
    let slice = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let storage = BLOB_STORAGE.lock().unwrap();
            let blob_data = storage.get(&slice_id).cloned();
            drop(storage);

            if let Some(data) = blob_data {
                let len = data.data.len() as i64;
                let start = args.get_or_undefined(0).to_i32(ctx).unwrap_or(0) as i64;
                let end = if args.len() > 1 && !args.get_or_undefined(1).is_undefined() {
                    args.get_or_undefined(1).to_i32(ctx).unwrap_or(len as i32) as i64
                } else {
                    len
                };
                let content_type = if args.len() > 2 {
                    args.get_or_undefined(2).to_string(ctx)?.to_std_string_escaped()
                } else {
                    data.mime_type.clone()
                };

                let start = if start < 0 { (len + start).max(0) } else { start.min(len) } as usize;
                let end = if end < 0 { (len + end).max(0) } else { end.min(len) } as usize;

                let sliced_data: Vec<u8> = if start < end {
                    data.data[start..end].to_vec()
                } else {
                    Vec::new()
                };

                let new_blob_id = {
                    let mut id = NEXT_BLOB_ID.lock().unwrap();
                    let current = *id;
                    *id += 1;
                    let mut storage = BLOB_STORAGE.lock().unwrap();
                    storage.insert(current, BlobData {
                        data: sliced_data.clone(),
                        mime_type: content_type.clone(),
                    });
                    current
                };

                create_blob_object(ctx, new_blob_id, sliced_data.len(), &content_type)
            } else {
                create_blob_object(ctx, 0, 0, "")
            }
        })
    };

    let text_id = blob_id;
    let text = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let storage = BLOB_STORAGE.lock().unwrap();
            let text = if let Some(data) = storage.get(&text_id) {
                String::from_utf8_lossy(&data.data).to_string()
            } else {
                String::new()
            };
            drop(storage);
            create_resolved_promise(ctx, JsValue::from(js_string!(text)))
        })
    };

    let ab_id = blob_id;
    let array_buffer = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let storage = BLOB_STORAGE.lock().unwrap();
            let bytes = if let Some(data) = storage.get(&ab_id) {
                data.data.clone()
            } else {
                Vec::new()
            };
            drop(storage);
            let ab = create_array_buffer(ctx, &bytes)?;
            create_resolved_promise(ctx, JsValue::from(ab))
        })
    };

    let file = ObjectInitializer::new(ctx)
        .property(js_string!("size"), size as u32, Attribute::READONLY)
        .property(js_string!("type"), js_string!(mime), Attribute::READONLY)
        .property(js_string!("name"), js_string!(name), Attribute::READONLY)
        .property(js_string!("lastModified"), last_modified, Attribute::READONLY)
        .property(js_string!("_blobId"), blob_id, Attribute::READONLY)
        .property(js_string!("webkitRelativePath"), js_string!(""), Attribute::READONLY)
        .function(slice, js_string!("slice"), 3)
        .function(text, js_string!("text"), 0)
        .function(array_buffer, js_string!("arrayBuffer"), 0)
        .build();

    Ok(JsValue::from(file))
}

/// Register FileList
fn register_file_list(context: &mut Context) -> JsResult<()> {
    // FileList constructor (typically not called directly, created by input elements)
    let file_list_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        // Accept an optional array of File objects
        let mut files: Vec<JsValue> = Vec::new();

        if args.len() > 0 {
            let input = args.get_or_undefined(0);
            if let Some(arr) = input.as_object() {
                let length = arr.get(js_string!("length"), ctx)
                    .ok()
                    .and_then(|v| v.to_u32(ctx).ok())
                    .unwrap_or(0);

                for i in 0..length {
                    if let Ok(file) = arr.get(js_string!(i.to_string()), ctx) {
                        files.push(file);
                    }
                }
            }
        }

        create_file_list_object(ctx, files)
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), file_list_constructor)
        .name(js_string!("FileList"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("FileList"), constructor, false, context)?;

    Ok(())
}

/// Create a FileList object
pub fn create_file_list_object(ctx: &mut Context, files: Vec<JsValue>) -> JsResult<JsValue> {
    let len = files.len();

    // item(index) method
    let files_for_item = files.clone();
    let item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0) as usize;
            if index < files_for_item.len() {
                Ok(files_for_item[index].clone())
            } else {
                Ok(JsValue::null())
            }
        })
    };

    // Symbol.iterator for for...of support
    let files_for_iter = files.clone();
    let iterator = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let values = files_for_iter.clone();
            let index = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let len = values.len();

            let next = {
                let values = values.clone();
                let index = index.clone();
                unsafe {
                    NativeFunction::from_closure(move |_this, _args, ctx| {
                        let current = index.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        let (done, value) = if current < len {
                            (false, values.get(current).cloned().unwrap_or(JsValue::undefined()))
                        } else {
                            (true, JsValue::undefined())
                        };

                        let result = ObjectInitializer::new(ctx)
                            .property(js_string!("done"), done, Attribute::all())
                            .property(js_string!("value"), value, Attribute::all())
                            .build();
                        Ok(JsValue::from(result))
                    })
                }
            };

            let iter_obj = ObjectInitializer::new(ctx)
                .function(next, js_string!("next"), 0)
                .build();
            Ok(JsValue::from(iter_obj))
        })
    };

    let mut file_list = ObjectInitializer::new(ctx)
        .property(js_string!("length"), len as u32, Attribute::READONLY)
        .function(item, js_string!("item"), 1)
        .build();

    // Add indexed access (0, 1, 2, ...)
    for (i, file) in files.iter().enumerate() {
        file_list.set(js_string!(i.to_string()), file.clone(), false, ctx)?;
    }

    // Add Symbol.iterator
    let symbol_iterator = boa_engine::JsSymbol::iterator();
    let iterator_fn = iterator.to_js_function(ctx.realm());
    let prop_key: boa_engine::property::PropertyKey = symbol_iterator.into();
    file_list.set(prop_key, JsValue::from(iterator_fn), false, ctx)?;

    Ok(JsValue::from(file_list))
}

/// Register FileReader
fn register_file_reader(context: &mut Context) -> JsResult<()> {
    let file_reader_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let reader_id = {
            let mut id = NEXT_FILEREADER_ID.lock().unwrap();
            let current = *id;
            *id += 1;

            let mut storage = FILEREADER_STORAGE.lock().unwrap();
            storage.insert(current, FileReaderState {
                ready_state: EMPTY,
                result: None,
                error: None,
            });

            current
        };

        create_file_reader_object(ctx, reader_id)
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), file_reader_constructor)
        .name(js_string!("FileReader"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("FileReader"), constructor, false, context)?;

    Ok(())
}

/// Helper to trigger FileReader callbacks
fn trigger_file_reader_callbacks(reader: &JsObject, ctx: &mut Context) {
    // Trigger onload if set
    if let Ok(onload) = reader.get(js_string!("onload"), ctx) {
        if let Some(onload_fn) = onload.as_callable() {
            let event = ObjectInitializer::new(ctx)
                .property(js_string!("type"), js_string!("load"), Attribute::all())
                .property(js_string!("target"), reader.clone(), Attribute::all())
                .build();
            let _ = onload_fn.call(&JsValue::from(reader.clone()), &[JsValue::from(event)], ctx);
        }
    }

    // Trigger onloadend if set
    if let Ok(onloadend) = reader.get(js_string!("onloadend"), ctx) {
        if let Some(onloadend_fn) = onloadend.as_callable() {
            let event = ObjectInitializer::new(ctx)
                .property(js_string!("type"), js_string!("loadend"), Attribute::all())
                .property(js_string!("target"), reader.clone(), Attribute::all())
                .build();
            let _ = onloadend_fn.call(&JsValue::from(reader.clone()), &[JsValue::from(event)], ctx);
        }
    }
}

/// Create a FileReader object with dynamic getters and callback support
fn create_file_reader_object(ctx: &mut Context, reader_id: u32) -> JsResult<JsValue> {
    // Create reader object first so we can pass it to closures
    let reader = ObjectInitializer::new(ctx)
        .property(js_string!("_readerId"), reader_id, Attribute::READONLY)
        .property(js_string!("EMPTY"), EMPTY, Attribute::READONLY)
        .property(js_string!("LOADING"), LOADING, Attribute::READONLY)
        .property(js_string!("DONE"), DONE, Attribute::READONLY)
        .property(js_string!("onloadstart"), JsValue::null(), Attribute::all())
        .property(js_string!("onprogress"), JsValue::null(), Attribute::all())
        .property(js_string!("onload"), JsValue::null(), Attribute::all())
        .property(js_string!("onabort"), JsValue::null(), Attribute::all())
        .property(js_string!("onerror"), JsValue::null(), Attribute::all())
        .property(js_string!("onloadend"), JsValue::null(), Attribute::all())
        .build();

    // readyState getter
    let rs_id = reader_id;
    let ready_state_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let storage = FILEREADER_STORAGE.lock().unwrap();
            let state = storage.get(&rs_id).map(|s| s.ready_state).unwrap_or(EMPTY);
            Ok(JsValue::from(state))
        })
    };

    // result getter
    let result_id = reader_id;
    let result_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let storage = FILEREADER_STORAGE.lock().unwrap();
            if let Some(state) = storage.get(&result_id) {
                match &state.result {
                    Some(FileReaderResult::ArrayBuffer(data)) => {
                        let data_clone = data.clone();
                        drop(storage);
                        let ab = create_array_buffer(ctx, &data_clone)?;
                        Ok(JsValue::from(ab))
                    }
                    Some(FileReaderResult::Text(text)) => {
                        let text_clone = text.clone();
                        drop(storage);
                        Ok(JsValue::from(js_string!(text_clone)))
                    }
                    Some(FileReaderResult::DataUrl(url)) => {
                        let url_clone = url.clone();
                        drop(storage);
                        Ok(JsValue::from(js_string!(url_clone)))
                    }
                    Some(FileReaderResult::BinaryString(bin)) => {
                        let bin_clone = bin.clone();
                        drop(storage);
                        Ok(JsValue::from(js_string!(bin_clone)))
                    }
                    None => Ok(JsValue::null()),
                }
            } else {
                Ok(JsValue::null())
            }
        })
    };

    // error getter
    let error_id = reader_id;
    let error_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let storage = FILEREADER_STORAGE.lock().unwrap();
            if let Some(state) = storage.get(&error_id) {
                if let Some(err) = &state.error {
                    let err_clone = err.clone();
                    drop(storage);
                    let error_obj = ObjectInitializer::new(ctx)
                        .property(js_string!("name"), js_string!("NotReadableError"), Attribute::all())
                        .property(js_string!("message"), js_string!(err_clone), Attribute::all())
                        .build();
                    Ok(JsValue::from(error_obj))
                } else {
                    Ok(JsValue::null())
                }
            } else {
                Ok(JsValue::null())
            }
        })
    };

    // Define dynamic property getters
    let _ = reader.define_property_or_throw(
        js_string!("readyState"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(ready_state_getter.to_js_function(ctx.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        ctx,
    );

    let _ = reader.define_property_or_throw(
        js_string!("result"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(result_getter.to_js_function(ctx.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        ctx,
    );

    let _ = reader.define_property_or_throw(
        js_string!("error"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(error_getter.to_js_function(ctx.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        ctx,
    );

    // readAsArrayBuffer(blob)
    let rab_id = reader_id;
    let reader_clone = reader.clone();
    let read_as_array_buffer = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let blob = args.get_or_undefined(0);
            if let Some(obj) = blob.as_object() {
                if let Ok(blob_id_val) = obj.get(js_string!("_blobId"), ctx) {
                    if let Ok(blob_id) = blob_id_val.to_u32(ctx) {
                        let blob_storage = BLOB_STORAGE.lock().unwrap();
                        let data = blob_storage.get(&blob_id).map(|b| b.data.clone()).unwrap_or_default();
                        drop(blob_storage);

                        let mut storage = FILEREADER_STORAGE.lock().unwrap();
                        if let Some(state) = storage.get_mut(&rab_id) {
                            state.ready_state = DONE;
                            state.result = Some(FileReaderResult::ArrayBuffer(data));
                        }
                        drop(storage);

                        // Trigger callbacks
                        trigger_file_reader_callbacks(&reader_clone, ctx);
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // readAsText(blob, encoding?)
    let rat_id = reader_id;
    let reader_clone = reader.clone();
    let read_as_text = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let blob = args.get_or_undefined(0);
            let _encoding = if args.len() > 1 {
                args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped()
            } else {
                "utf-8".to_string()
            };

            if let Some(obj) = blob.as_object() {
                if let Ok(blob_id_val) = obj.get(js_string!("_blobId"), ctx) {
                    if let Ok(blob_id) = blob_id_val.to_u32(ctx) {
                        let blob_storage = BLOB_STORAGE.lock().unwrap();
                        let text = blob_storage.get(&blob_id)
                            .map(|b| String::from_utf8_lossy(&b.data).to_string())
                            .unwrap_or_default();
                        drop(blob_storage);

                        let mut storage = FILEREADER_STORAGE.lock().unwrap();
                        if let Some(state) = storage.get_mut(&rat_id) {
                            state.ready_state = DONE;
                            state.result = Some(FileReaderResult::Text(text));
                        }
                        drop(storage);

                        // Trigger callbacks
                        trigger_file_reader_callbacks(&reader_clone, ctx);
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // readAsDataURL(blob)
    let radu_id = reader_id;
    let reader_clone = reader.clone();
    let read_as_data_url = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let blob = args.get_or_undefined(0);
            if let Some(obj) = blob.as_object() {
                if let Ok(blob_id_val) = obj.get(js_string!("_blobId"), ctx) {
                    if let Ok(blob_id) = blob_id_val.to_u32(ctx) {
                        let blob_storage = BLOB_STORAGE.lock().unwrap();
                        let (data, mime) = blob_storage.get(&blob_id)
                            .map(|b| (b.data.clone(), b.mime_type.clone()))
                            .unwrap_or_default();
                        drop(blob_storage);

                        // Base64 encode
                        let base64 = base64_encode(&data);
                        let mime_type = if mime.is_empty() { "application/octet-stream" } else { &mime };
                        let data_url = format!("data:{};base64,{}", mime_type, base64);

                        let mut storage = FILEREADER_STORAGE.lock().unwrap();
                        if let Some(state) = storage.get_mut(&radu_id) {
                            state.ready_state = DONE;
                            state.result = Some(FileReaderResult::DataUrl(data_url));
                        }
                        drop(storage);

                        // Trigger callbacks
                        trigger_file_reader_callbacks(&reader_clone, ctx);
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // readAsBinaryString(blob) - deprecated but still used
    let rabs_id = reader_id;
    let reader_clone = reader.clone();
    let read_as_binary_string = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let blob = args.get_or_undefined(0);
            if let Some(obj) = blob.as_object() {
                if let Ok(blob_id_val) = obj.get(js_string!("_blobId"), ctx) {
                    if let Ok(blob_id) = blob_id_val.to_u32(ctx) {
                        let blob_storage = BLOB_STORAGE.lock().unwrap();
                        let binary = blob_storage.get(&blob_id)
                            .map(|b| b.data.iter().map(|&byte| byte as char).collect::<String>())
                            .unwrap_or_default();
                        drop(blob_storage);

                        let mut storage = FILEREADER_STORAGE.lock().unwrap();
                        if let Some(state) = storage.get_mut(&rabs_id) {
                            state.ready_state = DONE;
                            state.result = Some(FileReaderResult::BinaryString(binary));
                        }
                        drop(storage);

                        // Trigger callbacks
                        trigger_file_reader_callbacks(&reader_clone, ctx);
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // abort()
    let abort_id = reader_id;
    let reader_clone = reader.clone();
    let abort = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let mut storage = FILEREADER_STORAGE.lock().unwrap();
            if let Some(state) = storage.get_mut(&abort_id) {
                state.ready_state = DONE;
                state.result = None;
            }
            drop(storage);

            // Trigger onabort if set
            if let Ok(onabort) = reader_clone.get(js_string!("onabort"), ctx) {
                if let Some(onabort_fn) = onabort.as_callable() {
                    let event = ObjectInitializer::new(ctx)
                        .property(js_string!("type"), js_string!("abort"), Attribute::all())
                        .build();
                    let _ = onabort_fn.call(&JsValue::from(reader_clone.clone()), &[JsValue::from(event)], ctx);
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

    // Add methods to reader
    reader.set(js_string!("readAsArrayBuffer"), read_as_array_buffer.to_js_function(ctx.realm()), false, ctx)?;
    reader.set(js_string!("readAsText"), read_as_text.to_js_function(ctx.realm()), false, ctx)?;
    reader.set(js_string!("readAsDataURL"), read_as_data_url.to_js_function(ctx.realm()), false, ctx)?;
    reader.set(js_string!("readAsBinaryString"), read_as_binary_string.to_js_function(ctx.realm()), false, ctx)?;
    reader.set(js_string!("abort"), abort.to_js_function(ctx.realm()), false, ctx)?;
    reader.set(js_string!("addEventListener"), add_event_listener.to_js_function(ctx.realm()), false, ctx)?;
    reader.set(js_string!("removeEventListener"), remove_event_listener.to_js_function(ctx.realm()), false, ctx)?;

    Ok(JsValue::from(reader))
}

/// Register FormData
fn register_form_data(context: &mut Context) -> JsResult<()> {
    let form_data_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let fd_id = {
            let mut id = NEXT_FORMDATA_ID.lock().unwrap();
            let current = *id;
            *id += 1;

            let mut storage = FORMDATA_STORAGE.lock().unwrap();
            storage.insert(current, Vec::new());

            current
        };

        create_form_data_object(ctx, fd_id)
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), form_data_constructor)
        .name(js_string!("FormData"))
        .length(1)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("FormData"), constructor, false, context)?;

    Ok(())
}

/// Create a FormData object
fn create_form_data_object(ctx: &mut Context, fd_id: u32) -> JsResult<JsValue> {
    // append(name, value, filename?)
    let append_id = fd_id;
    let append = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let value = args.get_or_undefined(1);

            let entry = if let Some(obj) = value.as_object() {
                // Check if it's a Blob/File
                if let Ok(blob_id_val) = obj.get(js_string!("_blobId"), ctx) {
                    if let Ok(blob_id) = blob_id_val.to_u32(ctx) {
                        let filename = if args.len() > 2 {
                            args.get_or_undefined(2).to_string(ctx)?.to_std_string_escaped()
                        } else {
                            obj.get(js_string!("name"), ctx)
                                .ok()
                                .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                                .unwrap_or_else(|| "blob".to_string())
                        };

                        let blob_storage = BLOB_STORAGE.lock().unwrap();
                        let (data, mime) = blob_storage.get(&blob_id)
                            .map(|b| (b.data.clone(), b.mime_type.clone()))
                            .unwrap_or_default();
                        drop(blob_storage);

                        FormDataEntry {
                            name,
                            value: FormDataValue::File { data, filename, mime_type: mime },
                        }
                    } else {
                        FormDataEntry {
                            name,
                            value: FormDataValue::String(value.to_string(ctx)?.to_std_string_escaped()),
                        }
                    }
                } else {
                    FormDataEntry {
                        name,
                        value: FormDataValue::String(value.to_string(ctx)?.to_std_string_escaped()),
                    }
                }
            } else {
                FormDataEntry {
                    name,
                    value: FormDataValue::String(value.to_string(ctx)?.to_std_string_escaped()),
                }
            };

            let mut storage = FORMDATA_STORAGE.lock().unwrap();
            if let Some(entries) = storage.get_mut(&append_id) {
                entries.push(entry);
            }

            Ok(JsValue::undefined())
        })
    };

    // delete(name)
    let delete_id = fd_id;
    let delete = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let mut storage = FORMDATA_STORAGE.lock().unwrap();
            if let Some(entries) = storage.get_mut(&delete_id) {
                entries.retain(|e| e.name != name);
            }

            Ok(JsValue::undefined())
        })
    };

    // get(name) -> value or null
    let get_id = fd_id;
    let get = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let found_value = {
                let storage = FORMDATA_STORAGE.lock().unwrap();
                if let Some(entries) = storage.get(&get_id) {
                    entries.iter().find(|e| e.name == name).map(|e| e.value.clone())
                } else {
                    None
                }
            };

            match found_value {
                Some(FormDataValue::String(s)) => Ok(JsValue::from(js_string!(s))),
                Some(FormDataValue::File { data, filename, mime_type }) => {
                    let data_len = data.len();
                    let blob_id = {
                        let mut id = NEXT_BLOB_ID.lock().unwrap();
                        let current = *id;
                        *id += 1;
                        let mut blob_storage = BLOB_STORAGE.lock().unwrap();
                        blob_storage.insert(current, BlobData {
                            data,
                            mime_type: mime_type.clone(),
                        });
                        current
                    };
                    create_file_object(ctx, blob_id, data_len, &mime_type, &filename, 0.0)
                }
                None => Ok(JsValue::null()),
            }
        })
    };

    // getAll(name) -> array
    let get_all_id = fd_id;
    let get_all = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let storage = FORMDATA_STORAGE.lock().unwrap();
            let mut values: Vec<JsValue> = Vec::new();

            if let Some(entries) = storage.get(&get_all_id) {
                for entry in entries {
                    if entry.name == name {
                        match &entry.value {
                            FormDataValue::String(s) => {
                                values.push(JsValue::from(js_string!(s.clone())));
                            }
                            FormDataValue::File { .. } => {
                                // For files, push a placeholder
                                values.push(JsValue::from(js_string!("[File]")));
                            }
                        }
                    }
                }
            }
            drop(storage);

            let arr = JsArray::from_iter(values, ctx);
            Ok(JsValue::from(arr))
        })
    };

    // has(name) -> boolean
    let has_id = fd_id;
    let has = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let storage = FORMDATA_STORAGE.lock().unwrap();
            let result = if let Some(entries) = storage.get(&has_id) {
                entries.iter().any(|e| e.name == name)
            } else {
                false
            };

            Ok(JsValue::from(result))
        })
    };

    // set(name, value, filename?)
    let set_id = fd_id;
    let set = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let value = args.get_or_undefined(1);

            let entry = if let Some(obj) = value.as_object() {
                if let Ok(blob_id_val) = obj.get(js_string!("_blobId"), ctx) {
                    if let Ok(blob_id) = blob_id_val.to_u32(ctx) {
                        let filename = if args.len() > 2 {
                            args.get_or_undefined(2).to_string(ctx)?.to_std_string_escaped()
                        } else {
                            obj.get(js_string!("name"), ctx)
                                .ok()
                                .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                                .unwrap_or_else(|| "blob".to_string())
                        };

                        let blob_storage = BLOB_STORAGE.lock().unwrap();
                        let (data, mime) = blob_storage.get(&blob_id)
                            .map(|b| (b.data.clone(), b.mime_type.clone()))
                            .unwrap_or_default();
                        drop(blob_storage);

                        FormDataEntry {
                            name: name.clone(),
                            value: FormDataValue::File { data, filename, mime_type: mime },
                        }
                    } else {
                        FormDataEntry {
                            name: name.clone(),
                            value: FormDataValue::String(value.to_string(ctx)?.to_std_string_escaped()),
                        }
                    }
                } else {
                    FormDataEntry {
                        name: name.clone(),
                        value: FormDataValue::String(value.to_string(ctx)?.to_std_string_escaped()),
                    }
                }
            } else {
                FormDataEntry {
                    name: name.clone(),
                    value: FormDataValue::String(value.to_string(ctx)?.to_std_string_escaped()),
                }
            };

            let mut storage = FORMDATA_STORAGE.lock().unwrap();
            if let Some(entries) = storage.get_mut(&set_id) {
                // Remove existing entries with this name
                entries.retain(|e| e.name != name);
                entries.push(entry);
            }

            Ok(JsValue::undefined())
        })
    };

    // keys() -> iterator
    let keys_id = fd_id;
    let keys = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let storage = FORMDATA_STORAGE.lock().unwrap();
            let names: Vec<String> = storage.get(&keys_id)
                .map(|entries| entries.iter().map(|e| e.name.clone()).collect())
                .unwrap_or_default();
            drop(storage);

            create_iterator(ctx, names.into_iter().map(|n| JsValue::from(js_string!(n))).collect())
        })
    };

    // values() -> iterator
    let values_id = fd_id;
    let values = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let storage = FORMDATA_STORAGE.lock().unwrap();
            let vals: Vec<JsValue> = storage.get(&values_id)
                .map(|entries| entries.iter().map(|e| match &e.value {
                    FormDataValue::String(s) => JsValue::from(js_string!(s.clone())),
                    FormDataValue::File { .. } => JsValue::from(js_string!("[File]")),
                }).collect())
                .unwrap_or_default();
            drop(storage);

            create_iterator(ctx, vals)
        })
    };

    // entries() -> iterator
    let entries_id = fd_id;
    let entries = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let storage = FORMDATA_STORAGE.lock().unwrap();
            let pairs: Vec<JsValue> = storage.get(&entries_id)
                .map(|entries| entries.iter().map(|e| {
                    let val = match &e.value {
                        FormDataValue::String(s) => JsValue::from(js_string!(s.clone())),
                        FormDataValue::File { .. } => JsValue::from(js_string!("[File]")),
                    };
                    let arr = JsArray::from_iter([
                        JsValue::from(js_string!(e.name.clone())),
                        val,
                    ], ctx);
                    JsValue::from(arr)
                }).collect())
                .unwrap_or_default();
            drop(storage);

            create_iterator(ctx, pairs)
        })
    };

    // forEach(callback)
    let foreach_id = fd_id;
    let for_each = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let callback = args.get_or_undefined(0);

            if callback.is_callable() {
                let storage = FORMDATA_STORAGE.lock().unwrap();
                let entries_clone: Vec<FormDataEntry> = storage.get(&foreach_id)
                    .cloned()
                    .unwrap_or_default();
                drop(storage);

                let callback_obj = callback.as_callable().unwrap();
                for entry in entries_clone {
                    let val = match &entry.value {
                        FormDataValue::String(s) => JsValue::from(js_string!(s.clone())),
                        FormDataValue::File { .. } => JsValue::from(js_string!("[File]")),
                    };
                    let _ = callback_obj.call(
                        &JsValue::undefined(),
                        &[val, JsValue::from(js_string!(entry.name))],
                        ctx,
                    );
                }
            }

            Ok(JsValue::undefined())
        })
    };

    let fd = ObjectInitializer::new(ctx)
        .property(js_string!("_fdId"), fd_id, Attribute::READONLY)
        .function(append, js_string!("append"), 3)
        .function(delete, js_string!("delete"), 1)
        .function(get, js_string!("get"), 1)
        .function(get_all, js_string!("getAll"), 1)
        .function(has, js_string!("has"), 1)
        .function(set, js_string!("set"), 3)
        .function(keys, js_string!("keys"), 0)
        .function(values, js_string!("values"), 0)
        .function(entries, js_string!("entries"), 0)
        .function(for_each, js_string!("forEach"), 1)
        .build();

    Ok(JsValue::from(fd))
}

/// Register URL API
fn register_url_api(context: &mut Context) -> JsResult<()> {
    let url_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let url_str = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let base = if args.len() > 1 && !args.get_or_undefined(1).is_undefined() {
            Some(args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped())
        } else {
            None
        };

        // Parse URL
        let parsed = if let Some(base_url) = base {
            url::Url::parse(&base_url)
                .and_then(|b| b.join(&url_str))
        } else {
            url::Url::parse(&url_str)
        };

        match parsed {
            Ok(url) => create_url_object(ctx, url),
            Err(_) => Err(JsNativeError::typ().with_message("Invalid URL").into()),
        }
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), url_constructor)
        .name(js_string!("URL"))
        .length(2)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("URL"), constructor.clone(), false, context)?;

    // Add URL.createObjectURL and URL.revokeObjectURL as static methods
    let create_object_url = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let blob = args.get_or_undefined(0);
        if let Some(obj) = blob.as_object() {
            if let Ok(blob_id) = obj.get(js_string!("_blobId"), ctx) {
                if let Ok(id) = blob_id.to_u32(ctx) {
                    return Ok(JsValue::from(js_string!(format!("blob:null/{}", id))));
                }
            }
        }
        Ok(JsValue::from(js_string!("blob:null/0")))
    });

    let revoke_object_url = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let create_func = FunctionObjectBuilder::new(context.realm(), create_object_url)
        .name(js_string!("createObjectURL"))
        .length(1)
        .build();

    let revoke_func = FunctionObjectBuilder::new(context.realm(), revoke_object_url)
        .name(js_string!("revokeObjectURL"))
        .length(1)
        .build();

    constructor.set(js_string!("createObjectURL"), create_func, false, context)?;
    constructor.set(js_string!("revokeObjectURL"), revoke_func, false, context)?;

    Ok(())
}

/// Create a URL object
fn create_url_object(ctx: &mut Context, url: url::Url) -> JsResult<JsValue> {
    let href = url.as_str().to_string();
    let protocol = url.scheme().to_string() + ":";
    let host = url.host_str().unwrap_or("").to_string();
    let hostname = host.clone();
    let port = url.port().map(|p| p.to_string()).unwrap_or_default();
    let pathname = url.path().to_string();
    let search = url.query().map(|q| format!("?{}", q)).unwrap_or_default();
    let hash = url.fragment().map(|f| format!("#{}", f)).unwrap_or_default();
    let origin = url.origin().unicode_serialization();
    let username = url.username().to_string();
    let password = url.password().unwrap_or("").to_string();

    let href_for_tostring = href.clone();
    let to_string = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(href_for_tostring.clone())))
        })
    };

    let href_for_tojson = href.clone();
    let to_json = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(href_for_tojson.clone())))
        })
    };

    // Create URLSearchParams for searchParams property
    let search_params = create_url_search_params_from_string(ctx, url.query().unwrap_or(""))?;

    let url_obj = ObjectInitializer::new(ctx)
        .property(js_string!("href"), js_string!(href.clone()), Attribute::all())
        .property(js_string!("protocol"), js_string!(protocol), Attribute::all())
        .property(js_string!("host"), js_string!(if port.is_empty() { host.clone() } else { format!("{}:{}", host, port) }), Attribute::all())
        .property(js_string!("hostname"), js_string!(hostname), Attribute::all())
        .property(js_string!("port"), js_string!(port), Attribute::all())
        .property(js_string!("pathname"), js_string!(pathname), Attribute::all())
        .property(js_string!("search"), js_string!(search), Attribute::all())
        .property(js_string!("hash"), js_string!(hash), Attribute::all())
        .property(js_string!("origin"), js_string!(origin), Attribute::READONLY)
        .property(js_string!("username"), js_string!(username), Attribute::all())
        .property(js_string!("password"), js_string!(password), Attribute::all())
        .property(js_string!("searchParams"), search_params, Attribute::READONLY)
        .function(to_string, js_string!("toString"), 0)
        .function(to_json, js_string!("toJSON"), 0)
        .build();

    Ok(JsValue::from(url_obj))
}

/// Register URLSearchParams
fn register_url_search_params(context: &mut Context) -> JsResult<()> {
    let url_search_params_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let init = args.get_or_undefined(0);

        if init.is_undefined() {
            return create_url_search_params_from_string(ctx, "");
        }

        if let Ok(s) = init.to_string(ctx) {
            let str_val = s.to_std_string_escaped();
            // Remove leading ? if present
            let query = str_val.strip_prefix('?').unwrap_or(&str_val);
            return create_url_search_params_from_string(ctx, query);
        }

        create_url_search_params_from_string(ctx, "")
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), url_search_params_constructor)
        .name(js_string!("URLSearchParams"))
        .length(1)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("URLSearchParams"), constructor, false, context)?;

    Ok(())
}

/// Create URLSearchParams from query string
fn create_url_search_params_from_string(ctx: &mut Context, query: &str) -> JsResult<JsValue> {
    // Parse query string into params
    let params: Vec<(String, String)> = query.split('&')
        .filter(|s| !s.is_empty())
        .map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = url_decode(parts.next().unwrap_or(""));
            let value = url_decode(parts.next().unwrap_or(""));
            (key, value)
        })
        .collect();

    create_url_search_params_object(ctx, params)
}

/// Create URLSearchParams object
fn create_url_search_params_object(ctx: &mut Context, initial_params: Vec<(String, String)>) -> JsResult<JsValue> {
    // Store params in thread-local or use a unique ID
    let params_id = {
        let mut id = NEXT_FORMDATA_ID.lock().unwrap();
        let current = *id;
        *id += 1;
        current
    };

    // Store as FormData entries (reusing storage)
    {
        let mut storage = FORMDATA_STORAGE.lock().unwrap();
        let entries: Vec<FormDataEntry> = initial_params.into_iter()
            .map(|(name, value)| FormDataEntry {
                name,
                value: FormDataValue::String(value),
            })
            .collect();
        storage.insert(params_id, entries);
    }

    // append(name, value)
    let append_id = params_id;
    let append = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();

            let mut storage = FORMDATA_STORAGE.lock().unwrap();
            if let Some(entries) = storage.get_mut(&append_id) {
                entries.push(FormDataEntry {
                    name,
                    value: FormDataValue::String(value),
                });
            }

            Ok(JsValue::undefined())
        })
    };

    // delete(name)
    let delete_id = params_id;
    let delete = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let mut storage = FORMDATA_STORAGE.lock().unwrap();
            if let Some(entries) = storage.get_mut(&delete_id) {
                entries.retain(|e| e.name != name);
            }

            Ok(JsValue::undefined())
        })
    };

    // get(name)
    let get_id = params_id;
    let get = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let storage = FORMDATA_STORAGE.lock().unwrap();
            if let Some(entries) = storage.get(&get_id) {
                for entry in entries {
                    if entry.name == name {
                        if let FormDataValue::String(s) = &entry.value {
                            return Ok(JsValue::from(js_string!(s.clone())));
                        }
                    }
                }
            }

            Ok(JsValue::null())
        })
    };

    // getAll(name)
    let get_all_id = params_id;
    let get_all = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let storage = FORMDATA_STORAGE.lock().unwrap();
            let values: Vec<JsValue> = storage.get(&get_all_id)
                .map(|entries| entries.iter()
                    .filter(|e| e.name == name)
                    .filter_map(|e| match &e.value {
                        FormDataValue::String(s) => Some(JsValue::from(js_string!(s.clone()))),
                        _ => None,
                    })
                    .collect())
                .unwrap_or_default();
            drop(storage);

            let arr = JsArray::from_iter(values, ctx);
            Ok(JsValue::from(arr))
        })
    };

    // has(name)
    let has_id = params_id;
    let has = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let storage = FORMDATA_STORAGE.lock().unwrap();
            let result = storage.get(&has_id)
                .map(|entries| entries.iter().any(|e| e.name == name))
                .unwrap_or(false);

            Ok(JsValue::from(result))
        })
    };

    // set(name, value)
    let set_id = params_id;
    let set = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();

            let mut storage = FORMDATA_STORAGE.lock().unwrap();
            if let Some(entries) = storage.get_mut(&set_id) {
                entries.retain(|e| e.name != name);
                entries.push(FormDataEntry {
                    name,
                    value: FormDataValue::String(value),
                });
            }

            Ok(JsValue::undefined())
        })
    };

    // sort()
    let sort_id = params_id;
    let sort = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let mut storage = FORMDATA_STORAGE.lock().unwrap();
            if let Some(entries) = storage.get_mut(&sort_id) {
                entries.sort_by(|a, b| a.name.cmp(&b.name));
            }

            Ok(JsValue::undefined())
        })
    };

    // toString()
    let to_string_id = params_id;
    let to_string = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let storage = FORMDATA_STORAGE.lock().unwrap();
            let result = storage.get(&to_string_id)
                .map(|entries| entries.iter()
                    .map(|e| {
                        let value = match &e.value {
                            FormDataValue::String(s) => s.clone(),
                            _ => String::new(),
                        };
                        format!("{}={}", url_encode(&e.name), url_encode(&value))
                    })
                    .collect::<Vec<_>>()
                    .join("&"))
                .unwrap_or_default();

            Ok(JsValue::from(js_string!(result)))
        })
    };

    // keys()
    let keys_id = params_id;
    let keys = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let storage = FORMDATA_STORAGE.lock().unwrap();
            let names: Vec<JsValue> = storage.get(&keys_id)
                .map(|entries| entries.iter().map(|e| JsValue::from(js_string!(e.name.clone()))).collect())
                .unwrap_or_default();
            drop(storage);

            create_iterator(ctx, names)
        })
    };

    // values()
    let values_id = params_id;
    let values = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let storage = FORMDATA_STORAGE.lock().unwrap();
            let vals: Vec<JsValue> = storage.get(&values_id)
                .map(|entries| entries.iter()
                    .filter_map(|e| match &e.value {
                        FormDataValue::String(s) => Some(JsValue::from(js_string!(s.clone()))),
                        _ => None,
                    })
                    .collect())
                .unwrap_or_default();
            drop(storage);

            create_iterator(ctx, vals)
        })
    };

    // entries()
    let entries_id = params_id;
    let entries_fn = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let storage = FORMDATA_STORAGE.lock().unwrap();
            let pairs: Vec<JsValue> = storage.get(&entries_id)
                .map(|entries| entries.iter()
                    .filter_map(|e| match &e.value {
                        FormDataValue::String(s) => {
                            let arr = JsArray::from_iter([
                                JsValue::from(js_string!(e.name.clone())),
                                JsValue::from(js_string!(s.clone())),
                            ], ctx);
                            Some(JsValue::from(arr))
                        }
                        _ => None,
                    })
                    .collect())
                .unwrap_or_default();
            drop(storage);

            create_iterator(ctx, pairs)
        })
    };

    // forEach(callback)
    let foreach_id = params_id;
    let for_each = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let callback = args.get_or_undefined(0);

            if callback.is_callable() {
                let storage = FORMDATA_STORAGE.lock().unwrap();
                let entries_clone: Vec<FormDataEntry> = storage.get(&foreach_id)
                    .cloned()
                    .unwrap_or_default();
                drop(storage);

                let callback_obj = callback.as_callable().unwrap();
                for entry in entries_clone {
                    if let FormDataValue::String(value) = &entry.value {
                        let _ = callback_obj.call(
                            &JsValue::undefined(),
                            &[JsValue::from(js_string!(value.clone())), JsValue::from(js_string!(entry.name))],
                            ctx,
                        );
                    }
                }
            }

            Ok(JsValue::undefined())
        })
    };

    // size getter
    let size_id = params_id;
    let size = {
        let storage = FORMDATA_STORAGE.lock().unwrap();
        storage.get(&size_id).map(|e| e.len()).unwrap_or(0)
    };

    let params = ObjectInitializer::new(ctx)
        .property(js_string!("size"), size as u32, Attribute::READONLY)
        .function(append, js_string!("append"), 2)
        .function(delete, js_string!("delete"), 1)
        .function(get, js_string!("get"), 1)
        .function(get_all, js_string!("getAll"), 1)
        .function(has, js_string!("has"), 1)
        .function(set, js_string!("set"), 2)
        .function(sort, js_string!("sort"), 0)
        .function(to_string, js_string!("toString"), 0)
        .function(keys, js_string!("keys"), 0)
        .function(values, js_string!("values"), 0)
        .function(entries_fn, js_string!("entries"), 0)
        .function(for_each, js_string!("forEach"), 1)
        .build();

    Ok(JsValue::from(params))
}

// Helper functions

fn create_uint8_array(ctx: &mut Context, bytes: &[u8]) -> JsResult<JsObject> {
    let arr = ObjectInitializer::new(ctx)
        .property(js_string!("length"), bytes.len() as u32, Attribute::READONLY)
        .property(js_string!("byteLength"), bytes.len() as u32, Attribute::READONLY)
        .property(js_string!("byteOffset"), 0, Attribute::READONLY)
        .property(js_string!("BYTES_PER_ELEMENT"), 1, Attribute::READONLY)
        .build();

    for (i, &byte) in bytes.iter().enumerate() {
        arr.set(js_string!(i.to_string()), JsValue::from(byte as u32), false, ctx)?;
    }

    Ok(arr)
}

fn create_array_buffer(ctx: &mut Context, bytes: &[u8]) -> JsResult<JsObject> {
    use boa_engine::object::builtins::JsArrayBuffer;

    // Create a real ArrayBuffer of the right size
    let array_buffer = JsArrayBuffer::new(bytes.len(), ctx)?;

    // Copy data into the ArrayBuffer
    if let Some(mut data) = array_buffer.data_mut() {
        data.copy_from_slice(bytes);
    }

    Ok(array_buffer.into())
}

fn extract_bytes_from_buffer(value: &JsValue, ctx: &mut Context) -> JsResult<Vec<u8>> {
    if let Some(obj) = value.as_object() {
        // Try to get length
        let length = obj.get(js_string!("length"), ctx)
            .or_else(|_| obj.get(js_string!("byteLength"), ctx))
            .ok()
            .and_then(|v| v.to_u32(ctx).ok())
            .unwrap_or(0);

        let mut bytes = Vec::with_capacity(length as usize);
        for i in 0..length {
            if let Ok(byte_val) = obj.get(js_string!(i.to_string()), ctx) {
                let byte = byte_val.to_u32(ctx).unwrap_or(0) as u8;
                bytes.push(byte);
            } else if let Ok(byte_val) = obj.get(js_string!(format!("_byte{}", i)), ctx) {
                let byte = byte_val.to_u32(ctx).unwrap_or(0) as u8;
                bytes.push(byte);
            }
        }

        Ok(bytes)
    } else {
        Ok(Vec::new())
    }
}

fn decode_utf16le(bytes: &[u8]) -> String {
    let u16s: Vec<u16> = bytes.chunks(2)
        .map(|chunk| {
            let low = chunk.get(0).copied().unwrap_or(0) as u16;
            let high = chunk.get(1).copied().unwrap_or(0) as u16;
            low | (high << 8)
        })
        .collect();
    String::from_utf16_lossy(&u16s)
}

fn decode_utf16be(bytes: &[u8]) -> String {
    let u16s: Vec<u16> = bytes.chunks(2)
        .map(|chunk| {
            let high = chunk.get(0).copied().unwrap_or(0) as u16;
            let low = chunk.get(1).copied().unwrap_or(0) as u16;
            (high << 8) | low
        })
        .collect();
    String::from_utf16_lossy(&u16s)
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk.get(0).copied().unwrap_or(0) as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0F) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3F] as char);
        } else {
            result.push('=');
        }
    }

    result
}

fn url_encode(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                result.push(c);
            }
            ' ' => result.push('+'),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

fn url_decode(s: &str) -> String {
    let mut result = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '%' => {
                let hex: String = chars.by_ref().take(2).collect();
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte);
                }
            }
            '+' => result.push(b' '),
            _ => result.extend_from_slice(c.to_string().as_bytes()),
        }
    }

    String::from_utf8_lossy(&result).to_string()
}

fn create_resolved_promise(ctx: &mut Context, value: JsValue) -> JsResult<JsValue> {
    let value_clone = value.clone();
    let then = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            if let Some(callback) = args.first() {
                if callback.is_callable() {
                    let callback_obj = callback.as_callable().unwrap();
                    return callback_obj.call(&JsValue::undefined(), &[value_clone.clone()], ctx);
                }
            }
            Ok(JsValue::undefined())
        })
    };

    let catch_fn = NativeFunction::from_copy_closure(|this, _args, _ctx| {
        Ok(this.clone())
    });

    let finally_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        if let Some(callback) = args.first() {
            if callback.is_callable() {
                let callback_obj = callback.as_callable().unwrap();
                let _ = callback_obj.call(&JsValue::undefined(), &[], ctx);
            }
        }
        Ok(JsValue::undefined())
    });

    let promise = ObjectInitializer::new(ctx)
        .function(then, js_string!("then"), 2)
        .function(catch_fn, js_string!("catch"), 1)
        .function(finally_fn, js_string!("finally"), 1)
        .build();

    Ok(JsValue::from(promise))
}

fn create_iterator(ctx: &mut Context, values: Vec<JsValue>) -> JsResult<JsValue> {
    let values_clone = values.clone();
    let index = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let index_clone = index.clone();
    let len = values.len();

    let next = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let current = index_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            let (done, value) = if current < len {
                (false, values_clone.get(current).cloned().unwrap_or(JsValue::undefined()))
            } else {
                (true, JsValue::undefined())
            };

            let result = ObjectInitializer::new(ctx)
                .property(js_string!("done"), done, Attribute::all())
                .property(js_string!("value"), value, Attribute::all())
                .build();

            Ok(JsValue::from(result))
        })
    };

    let iterator = ObjectInitializer::new(ctx)
        .function(next, js_string!("next"), 0)
        .build();

    Ok(JsValue::from(iterator))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b"Hello"), "SGVsbG8=");
        assert_eq!(base64_encode(b"Hello World"), "SGVsbG8gV29ybGQ=");
        assert_eq!(base64_encode(b""), "");
    }

    #[test]
    fn test_url_encode_decode() {
        assert_eq!(url_encode("hello world"), "hello+world");
        assert_eq!(url_encode("foo=bar"), "foo%3Dbar");
        assert_eq!(url_decode("hello+world"), "hello world");
        assert_eq!(url_decode("foo%3Dbar"), "foo=bar");
    }

    #[test]
    fn test_utf16_decode() {
        // "Hi" in UTF-16LE
        let utf16le = vec![0x48, 0x00, 0x69, 0x00];
        assert_eq!(decode_utf16le(&utf16le), "Hi");

        // "Hi" in UTF-16BE
        let utf16be = vec![0x00, 0x48, 0x00, 0x69];
        assert_eq!(decode_utf16be(&utf16be), "Hi");
    }
}
