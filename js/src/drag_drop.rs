//! Drag and Drop API
//!
//! Implements:
//! - DataTransfer
//! - DataTransferItem
//! - DataTransferItemList
//! - DragEvent
//! - Drag/drop element attributes

use boa_engine::{
    Context, JsArgs, JsNativeError, JsObject, JsResult, JsValue,
    NativeFunction, js_string, object::ObjectInitializer, object::builtins::JsArray,
    object::FunctionObjectBuilder, property::Attribute,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

lazy_static::lazy_static! {
    /// Storage for DataTransfer instances
    static ref DATA_TRANSFER_STORAGE: Arc<Mutex<HashMap<u32, DataTransferStore>>> =
        Arc::new(Mutex::new(HashMap::new()));

    /// Counter for DataTransfer IDs
    static ref DATA_TRANSFER_COUNTER: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
}

/// DataTransfer storage structure
#[derive(Debug, Clone)]
struct DataTransferStore {
    data: HashMap<String, String>,
    files: Vec<FileData>,
    effect_allowed: String,
    drop_effect: String,
}

/// File data for DataTransfer
#[derive(Debug, Clone)]
struct FileData {
    name: String,
    size: usize,
    mime_type: String,
    last_modified: u64,
}

impl DataTransferStore {
    fn new() -> Self {
        Self {
            data: HashMap::new(),
            files: Vec::new(),
            effect_allowed: "uninitialized".to_string(),
            drop_effect: "none".to_string(),
        }
    }
}

/// Register all drag and drop APIs
pub fn register_all_drag_drop_apis(context: &mut Context) -> JsResult<()> {
    register_data_transfer(context)?;
    register_data_transfer_item(context)?;
    register_data_transfer_item_list(context)?;
    register_drag_event(context)?;
    register_clipboard_item(context)?;
    Ok(())
}

/// Register DataTransfer constructor
fn register_data_transfer(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let mut id = DATA_TRANSFER_COUNTER.lock().unwrap();
        *id += 1;
        let dt_id = *id;
        drop(id);

        DATA_TRANSFER_STORAGE.lock().unwrap().insert(dt_id, DataTransferStore::new());

        let dt = create_data_transfer_object(ctx, dt_id)?;
        Ok(JsValue::from(dt))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("DataTransfer"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("DataTransfer"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register DataTransfer: {}", e)))?;

    Ok(())
}

/// Create DataTransfer object with all methods
fn create_data_transfer_object(context: &mut Context, id: u32) -> JsResult<JsObject> {
    // setData(format, data)
    let id_set = id;
    let set_data = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let format = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let data = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();

        if let Ok(mut storage) = DATA_TRANSFER_STORAGE.lock() {
            if let Some(store) = storage.get_mut(&id_set) {
                store.data.insert(format, data);
            }
        }
        Ok(JsValue::undefined())
    });

    // getData(format)
    let id_get = id;
    let get_data = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let format = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        if let Ok(storage) = DATA_TRANSFER_STORAGE.lock() {
            if let Some(store) = storage.get(&id_get) {
                if let Some(data) = store.data.get(&format) {
                    return Ok(JsValue::from(js_string!(data.as_str())));
                }
            }
        }
        Ok(JsValue::from(js_string!("")))
    });

    // clearData(format?)
    let id_clear = id;
    let clear_data = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let format = args.get(0).map(|v| v.to_string(ctx).ok()).flatten()
            .map(|s| s.to_std_string_escaped());

        if let Ok(mut storage) = DATA_TRANSFER_STORAGE.lock() {
            if let Some(store) = storage.get_mut(&id_clear) {
                if let Some(fmt) = format {
                    store.data.remove(&fmt);
                } else {
                    store.data.clear();
                }
            }
        }
        Ok(JsValue::undefined())
    });

    // setDragImage(element, x, y)
    let set_drag_image = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        // Stub - no visual rendering
        Ok(JsValue::undefined())
    });

    // Convert to js_function
    let set_data_fn = set_data.to_js_function(context.realm());
    let get_data_fn = get_data.to_js_function(context.realm());
    let clear_data_fn = clear_data.to_js_function(context.realm());
    let set_drag_image_fn = set_drag_image.to_js_function(context.realm());

    // Create items list
    let items = create_data_transfer_item_list(context, id)?;

    // Create files list (empty FileList)
    let files = JsArray::new(context);

    // Get effect values
    let effect_allowed = DATA_TRANSFER_STORAGE.lock()
        .map(|s| s.get(&id).map(|st| st.effect_allowed.clone()).unwrap_or_default())
        .unwrap_or_default();
    let drop_effect = DATA_TRANSFER_STORAGE.lock()
        .map(|s| s.get(&id).map(|st| st.drop_effect.clone()).unwrap_or_default())
        .unwrap_or_default();

    // Create types array
    let types = JsArray::new(context);
    if let Ok(storage) = DATA_TRANSFER_STORAGE.lock() {
        if let Some(store) = storage.get(&id) {
            for key in store.data.keys() {
                types.push(JsValue::from(js_string!(key.as_str())), context)?;
            }
        }
    }

    let dt = ObjectInitializer::new(context)
        .property(js_string!("setData"), JsValue::from(set_data_fn), Attribute::all())
        .property(js_string!("getData"), JsValue::from(get_data_fn), Attribute::all())
        .property(js_string!("clearData"), JsValue::from(clear_data_fn), Attribute::all())
        .property(js_string!("setDragImage"), JsValue::from(set_drag_image_fn), Attribute::all())
        .property(js_string!("items"), JsValue::from(items), Attribute::all())
        .property(js_string!("files"), JsValue::from(files), Attribute::all())
        .property(js_string!("types"), JsValue::from(types), Attribute::all())
        .property(js_string!("effectAllowed"), JsValue::from(js_string!(effect_allowed.as_str())), Attribute::all())
        .property(js_string!("dropEffect"), JsValue::from(js_string!(drop_effect.as_str())), Attribute::all())
        .build();

    Ok(dt)
}

/// Register DataTransferItem constructor
fn register_data_transfer_item(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let item = create_data_transfer_item_object(ctx, "string".to_string(), "text/plain".to_string())?;
        Ok(JsValue::from(item))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("DataTransferItem"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("DataTransferItem"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register DataTransferItem: {}", e)))?;

    Ok(())
}

/// Create DataTransferItem object
fn create_data_transfer_item_object(context: &mut Context, kind: String, type_: String) -> JsResult<JsObject> {
    // getAsString(callback)
    let get_as_string = NativeFunction::from_copy_closure(|_this, args, ctx| {
        if let Some(callback) = args.get_or_undefined(0).as_callable() {
            let _ = callback.call(&JsValue::undefined(), &[JsValue::from(js_string!(""))], ctx);
        }
        Ok(JsValue::undefined())
    });

    // getAsFile()
    let get_as_file = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });

    // webkitGetAsEntry() - for File System Access API compatibility
    let webkit_get_as_entry = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });

    let get_as_string_fn = get_as_string.to_js_function(context.realm());
    let get_as_file_fn = get_as_file.to_js_function(context.realm());
    let webkit_fn = webkit_get_as_entry.to_js_function(context.realm());

    let item = ObjectInitializer::new(context)
        .property(js_string!("kind"), JsValue::from(js_string!(kind.as_str())), Attribute::all())
        .property(js_string!("type"), JsValue::from(js_string!(type_.as_str())), Attribute::all())
        .property(js_string!("getAsString"), JsValue::from(get_as_string_fn), Attribute::all())
        .property(js_string!("getAsFile"), JsValue::from(get_as_file_fn), Attribute::all())
        .property(js_string!("webkitGetAsEntry"), JsValue::from(webkit_fn), Attribute::all())
        .build();

    Ok(item)
}

/// Register DataTransferItemList constructor
fn register_data_transfer_item_list(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let mut id = DATA_TRANSFER_COUNTER.lock().unwrap();
        *id += 1;
        let list_id = *id;
        drop(id);

        let list = create_data_transfer_item_list(ctx, list_id)?;
        Ok(JsValue::from(list))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("DataTransferItemList"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("DataTransferItemList"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register DataTransferItemList: {}", e)))?;

    Ok(())
}

/// Create DataTransferItemList object
fn create_data_transfer_item_list(context: &mut Context, _id: u32) -> JsResult<JsObject> {
    // add(data, type) or add(file)
    let add = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let item = create_data_transfer_item_object(ctx, "string".to_string(), "text/plain".to_string())?;
        Ok(JsValue::from(item))
    });

    // remove(index)
    let remove = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // clear()
    let clear = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let add_fn = add.to_js_function(context.realm());
    let remove_fn = remove.to_js_function(context.realm());
    let clear_fn = clear.to_js_function(context.realm());

    let list = ObjectInitializer::new(context)
        .property(js_string!("length"), JsValue::from(0), Attribute::all())
        .property(js_string!("add"), JsValue::from(add_fn), Attribute::all())
        .property(js_string!("remove"), JsValue::from(remove_fn), Attribute::all())
        .property(js_string!("clear"), JsValue::from(clear_fn), Attribute::all())
        .build();

    Ok(list)
}

/// Register DragEvent constructor
fn register_drag_event(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let type_ = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_drag_event_object(ctx, &type_)?;
        Ok(JsValue::from(event))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("DragEvent"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("DragEvent"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register DragEvent: {}", e)))?;

    Ok(())
}

/// Create DragEvent object
fn create_drag_event_object(context: &mut Context, type_: &str) -> JsResult<JsObject> {
    // Create a DataTransfer for this event
    let mut id = DATA_TRANSFER_COUNTER.lock().unwrap();
    *id += 1;
    let dt_id = *id;
    drop(id);

    DATA_TRANSFER_STORAGE.lock().unwrap().insert(dt_id, DataTransferStore::new());
    let data_transfer = create_data_transfer_object(context, dt_id)?;

    // Standard event methods
    let prevent_default = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    let stop_propagation = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    let stop_immediate = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    let event = ObjectInitializer::new(context)
        .property(js_string!("type"), JsValue::from(js_string!(type_)), Attribute::all())
        .property(js_string!("dataTransfer"), JsValue::from(data_transfer), Attribute::all())
        .property(js_string!("bubbles"), JsValue::from(true), Attribute::all())
        .property(js_string!("cancelable"), JsValue::from(true), Attribute::all())
        .property(js_string!("defaultPrevented"), JsValue::from(false), Attribute::all())
        .property(js_string!("clientX"), JsValue::from(0), Attribute::all())
        .property(js_string!("clientY"), JsValue::from(0), Attribute::all())
        .property(js_string!("screenX"), JsValue::from(0), Attribute::all())
        .property(js_string!("screenY"), JsValue::from(0), Attribute::all())
        .property(js_string!("pageX"), JsValue::from(0), Attribute::all())
        .property(js_string!("pageY"), JsValue::from(0), Attribute::all())
        .property(js_string!("button"), JsValue::from(0), Attribute::all())
        .property(js_string!("buttons"), JsValue::from(0), Attribute::all())
        .property(js_string!("altKey"), JsValue::from(false), Attribute::all())
        .property(js_string!("ctrlKey"), JsValue::from(false), Attribute::all())
        .property(js_string!("metaKey"), JsValue::from(false), Attribute::all())
        .property(js_string!("shiftKey"), JsValue::from(false), Attribute::all())
        .property(js_string!("preventDefault"), JsValue::from(prevent_default), Attribute::all())
        .property(js_string!("stopPropagation"), JsValue::from(stop_propagation), Attribute::all())
        .property(js_string!("stopImmediatePropagation"), JsValue::from(stop_immediate), Attribute::all())
        .build();

    Ok(event)
}

/// Register ClipboardItem (used in Clipboard API with drag-drop)
fn register_clipboard_item(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let item = create_clipboard_item_object(ctx)?;
        Ok(JsValue::from(item))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("ClipboardItem"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("ClipboardItem"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register ClipboardItem: {}", e)))?;

    Ok(())
}

/// Create ClipboardItem object
fn create_clipboard_item_object(context: &mut Context) -> JsResult<JsObject> {
    // getType(type) - returns a promise with a blob
    let get_type = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Create the then callback first
        let then_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(cb) = args.get_or_undefined(0).as_callable() {
                let blob = ObjectInitializer::new(ctx)
                    .property(js_string!("size"), JsValue::from(0), Attribute::all())
                    .property(js_string!("type"), JsValue::from(js_string!("text/plain")), Attribute::all())
                    .build();
                let _ = cb.call(&JsValue::undefined(), &[JsValue::from(blob)], ctx);
            }
            Ok(JsValue::undefined())
        }).to_js_function(ctx.realm());

        let promise = ObjectInitializer::new(ctx)
            .property(js_string!("then"), JsValue::from(then_fn), Attribute::all())
            .build();

        Ok(JsValue::from(promise))
    });

    let get_type_fn = get_type.to_js_function(context.realm());
    let types = JsArray::new(context);
    types.push(JsValue::from(js_string!("text/plain")), context)?;

    let item = ObjectInitializer::new(context)
        .property(js_string!("types"), JsValue::from(types), Attribute::all())
        .property(js_string!("getType"), JsValue::from(get_type_fn), Attribute::all())
        .build();

    Ok(item)
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::Source;

    fn create_test_context() -> Context {
        let mut ctx = Context::default();
        register_all_drag_drop_apis(&mut ctx).unwrap();
        ctx
    }

    #[test]
    fn test_data_transfer_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof DataTransfer === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_data_transfer_set_get() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            var dt = new DataTransfer();
            dt.setData('text/plain', 'hello');
            dt.getData('text/plain') === 'hello'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_drag_event_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof DragEvent === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_drag_event_has_data_transfer() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            var e = new DragEvent('drop');
            e.dataTransfer !== null && typeof e.dataTransfer.setData === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_clipboard_item_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof ClipboardItem === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }
}
