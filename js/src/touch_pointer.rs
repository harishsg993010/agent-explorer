//! Touch and Pointer Events API
//!
//! Implements:
//! - Touch, TouchList, TouchEvent
//! - PointerEvent
//! - Pointer capture methods

use boa_engine::{
    Context, JsArgs, JsNativeError, JsObject, JsResult, JsValue,
    NativeFunction, js_string, object::ObjectInitializer, object::builtins::JsArray,
    object::FunctionObjectBuilder, property::Attribute,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

lazy_static::lazy_static! {
    /// Active pointers registry
    static ref ACTIVE_POINTERS: Arc<Mutex<HashMap<i32, PointerData>>> =
        Arc::new(Mutex::new(HashMap::new()));

    /// Touch identifier counter
    static ref TOUCH_COUNTER: Arc<Mutex<i64>> = Arc::new(Mutex::new(0));

    /// Pointer capture state
    static ref POINTER_CAPTURE: Arc<Mutex<HashMap<i32, String>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

/// Pointer data structure
#[derive(Debug, Clone)]
struct PointerData {
    pointer_id: i32,
    pointer_type: String,
    is_primary: bool,
    x: f64,
    y: f64,
    pressure: f64,
    tilt_x: i32,
    tilt_y: i32,
    width: f64,
    height: f64,
}

impl Default for PointerData {
    fn default() -> Self {
        Self {
            pointer_id: 0,
            pointer_type: "mouse".to_string(),
            is_primary: true,
            x: 0.0,
            y: 0.0,
            pressure: 0.0,
            tilt_x: 0,
            tilt_y: 0,
            width: 1.0,
            height: 1.0,
        }
    }
}

/// Register all touch and pointer APIs
pub fn register_all_touch_pointer_apis(context: &mut Context) -> JsResult<()> {
    register_touch(context)?;
    register_touch_list(context)?;
    register_touch_event(context)?;
    register_pointer_event(context)?;
    register_gesture_event(context)?;
    register_pointer_methods(context)?;
    Ok(())
}

/// Register Touch constructor
fn register_touch(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let init = args.get_or_undefined(0);

        let mut touch_id = TOUCH_COUNTER.lock().unwrap();
        *touch_id += 1;
        let id = *touch_id;
        drop(touch_id);

        let touch = create_touch_object(ctx, id, init)?;
        Ok(JsValue::from(touch))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("Touch"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("Touch"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register Touch: {}", e)))?;

    Ok(())
}

/// Create Touch object
fn create_touch_object(context: &mut Context, identifier: i64, init: &JsValue) -> JsResult<JsObject> {
    let (client_x, client_y, page_x, page_y, screen_x, screen_y, radius_x, radius_y, rotation, force) =
        if let Some(obj) = init.as_object() {
            (
                obj.get(js_string!("clientX"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(0.0),
                obj.get(js_string!("clientY"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(0.0),
                obj.get(js_string!("pageX"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(0.0),
                obj.get(js_string!("pageY"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(0.0),
                obj.get(js_string!("screenX"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(0.0),
                obj.get(js_string!("screenY"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(0.0),
                obj.get(js_string!("radiusX"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(1.0),
                obj.get(js_string!("radiusY"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(1.0),
                obj.get(js_string!("rotationAngle"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(0.0),
                obj.get(js_string!("force"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(0.0),
            )
        } else {
            (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0)
        };

    let touch = ObjectInitializer::new(context)
        .property(js_string!("identifier"), JsValue::from(identifier as i32), Attribute::all())
        .property(js_string!("target"), JsValue::null(), Attribute::all())
        .property(js_string!("clientX"), JsValue::from(client_x), Attribute::all())
        .property(js_string!("clientY"), JsValue::from(client_y), Attribute::all())
        .property(js_string!("pageX"), JsValue::from(page_x), Attribute::all())
        .property(js_string!("pageY"), JsValue::from(page_y), Attribute::all())
        .property(js_string!("screenX"), JsValue::from(screen_x), Attribute::all())
        .property(js_string!("screenY"), JsValue::from(screen_y), Attribute::all())
        .property(js_string!("radiusX"), JsValue::from(radius_x), Attribute::all())
        .property(js_string!("radiusY"), JsValue::from(radius_y), Attribute::all())
        .property(js_string!("rotationAngle"), JsValue::from(rotation), Attribute::all())
        .property(js_string!("force"), JsValue::from(force), Attribute::all())
        .build();

    Ok(touch)
}

/// Register TouchList constructor
fn register_touch_list(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let touches: Vec<JsValue> = if let Some(arr) = args.get_or_undefined(0).as_object() {
            let len = arr.get(js_string!("length"), ctx)
                .ok()
                .and_then(|v| v.to_u32(ctx).ok())
                .unwrap_or(0);

            (0..len)
                .filter_map(|i| arr.get(i, ctx).ok())
                .collect()
        } else {
            Vec::new()
        };

        let list = create_touch_list_object(ctx, touches)?;
        Ok(JsValue::from(list))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("TouchList"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("TouchList"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register TouchList: {}", e)))?;

    Ok(())
}

/// Create TouchList object
fn create_touch_list_object(context: &mut Context, touches: Vec<JsValue>) -> JsResult<JsObject> {
    let length = touches.len();

    // item(index) - always returns null since we store indexed properties directly
    let item = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        // Return null - actual values are accessed via indexed properties
        Ok(JsValue::null())
    });

    let item_fn = item.to_js_function(context.realm());

    // Build object with base properties first
    let mut list = ObjectInitializer::new(context)
        .property(js_string!("length"), JsValue::from(length as i32), Attribute::all())
        .property(js_string!("item"), JsValue::from(item_fn), Attribute::all())
        .build();

    // Add indexed properties directly to the object
    for (i, touch) in touches.iter().enumerate() {
        let _ = list.set(js_string!(i.to_string().as_str()), touch.clone(), false, context);
    }

    Ok(list)
}

/// Register TouchEvent constructor
fn register_touch_event(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let type_ = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let init = args.get_or_undefined(1);

        let event = create_touch_event_object(ctx, &type_, init)?;
        Ok(JsValue::from(event))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("TouchEvent"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("TouchEvent"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register TouchEvent: {}", e)))?;

    Ok(())
}

/// Create TouchEvent object
fn create_touch_event_object(context: &mut Context, type_: &str, _init: &JsValue) -> JsResult<JsObject> {
    // Empty touch lists
    let empty_list = create_touch_list_object(context, Vec::new())?;
    let empty_list2 = create_touch_list_object(context, Vec::new())?;
    let empty_list3 = create_touch_list_object(context, Vec::new())?;

    // Event methods
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
        .property(js_string!("touches"), JsValue::from(empty_list), Attribute::all())
        .property(js_string!("targetTouches"), JsValue::from(empty_list2), Attribute::all())
        .property(js_string!("changedTouches"), JsValue::from(empty_list3), Attribute::all())
        .property(js_string!("altKey"), JsValue::from(false), Attribute::all())
        .property(js_string!("ctrlKey"), JsValue::from(false), Attribute::all())
        .property(js_string!("metaKey"), JsValue::from(false), Attribute::all())
        .property(js_string!("shiftKey"), JsValue::from(false), Attribute::all())
        .property(js_string!("bubbles"), JsValue::from(true), Attribute::all())
        .property(js_string!("cancelable"), JsValue::from(true), Attribute::all())
        .property(js_string!("defaultPrevented"), JsValue::from(false), Attribute::all())
        .property(js_string!("isTrusted"), JsValue::from(false), Attribute::all())
        .property(js_string!("preventDefault"), JsValue::from(prevent_default), Attribute::all())
        .property(js_string!("stopPropagation"), JsValue::from(stop_propagation), Attribute::all())
        .property(js_string!("stopImmediatePropagation"), JsValue::from(stop_immediate), Attribute::all())
        .build();

    Ok(event)
}

/// Register PointerEvent constructor
fn register_pointer_event(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let type_ = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let init = args.get_or_undefined(1);

        let event = create_pointer_event_object(ctx, &type_, init)?;
        Ok(JsValue::from(event))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("PointerEvent"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("PointerEvent"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register PointerEvent: {}", e)))?;

    Ok(())
}

/// Create PointerEvent object
fn create_pointer_event_object(context: &mut Context, type_: &str, init: &JsValue) -> JsResult<JsObject> {
    let (pointer_id, width, height, pressure, tangential_pressure, tilt_x, tilt_y, twist,
         pointer_type, is_primary) = if let Some(obj) = init.as_object() {
        (
            obj.get(js_string!("pointerId"), context).ok().and_then(|v| v.to_i32(context).ok()).unwrap_or(0),
            obj.get(js_string!("width"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(1.0),
            obj.get(js_string!("height"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(1.0),
            obj.get(js_string!("pressure"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(0.0),
            obj.get(js_string!("tangentialPressure"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(0.0),
            obj.get(js_string!("tiltX"), context).ok().and_then(|v| v.to_i32(context).ok()).unwrap_or(0),
            obj.get(js_string!("tiltY"), context).ok().and_then(|v| v.to_i32(context).ok()).unwrap_or(0),
            obj.get(js_string!("twist"), context).ok().and_then(|v| v.to_i32(context).ok()).unwrap_or(0),
            obj.get(js_string!("pointerType"), context).ok()
                .and_then(|v| v.to_string(context).ok())
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_else(|| "mouse".to_string()),
            obj.get(js_string!("isPrimary"), context).ok().map(|v| v.to_boolean()).unwrap_or(false),
        )
    } else {
        (0, 1.0, 1.0, 0.0, 0.0, 0, 0, 0, "mouse".to_string(), false)
    };

    // Event methods
    let prevent_default = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    let stop_propagation = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    let stop_immediate = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    let get_coalesced = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(JsArray::new(_ctx)))
    }).to_js_function(context.realm());

    let get_predicted = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(JsArray::new(_ctx)))
    }).to_js_function(context.realm());

    let event = ObjectInitializer::new(context)
        .property(js_string!("type"), JsValue::from(js_string!(type_)), Attribute::all())
        .property(js_string!("pointerId"), JsValue::from(pointer_id), Attribute::all())
        .property(js_string!("width"), JsValue::from(width), Attribute::all())
        .property(js_string!("height"), JsValue::from(height), Attribute::all())
        .property(js_string!("pressure"), JsValue::from(pressure), Attribute::all())
        .property(js_string!("tangentialPressure"), JsValue::from(tangential_pressure), Attribute::all())
        .property(js_string!("tiltX"), JsValue::from(tilt_x), Attribute::all())
        .property(js_string!("tiltY"), JsValue::from(tilt_y), Attribute::all())
        .property(js_string!("twist"), JsValue::from(twist), Attribute::all())
        .property(js_string!("pointerType"), JsValue::from(js_string!(pointer_type.as_str())), Attribute::all())
        .property(js_string!("isPrimary"), JsValue::from(is_primary), Attribute::all())
        // Mouse event properties
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
        // Event properties
        .property(js_string!("bubbles"), JsValue::from(true), Attribute::all())
        .property(js_string!("cancelable"), JsValue::from(true), Attribute::all())
        .property(js_string!("defaultPrevented"), JsValue::from(false), Attribute::all())
        .property(js_string!("isTrusted"), JsValue::from(false), Attribute::all())
        // Methods
        .property(js_string!("preventDefault"), JsValue::from(prevent_default), Attribute::all())
        .property(js_string!("stopPropagation"), JsValue::from(stop_propagation), Attribute::all())
        .property(js_string!("stopImmediatePropagation"), JsValue::from(stop_immediate), Attribute::all())
        .property(js_string!("getCoalescedEvents"), JsValue::from(get_coalesced), Attribute::all())
        .property(js_string!("getPredictedEvents"), JsValue::from(get_predicted), Attribute::all())
        .build();

    Ok(event)
}

/// Register GestureEvent (for Safari compatibility)
fn register_gesture_event(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let type_ = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        let prevent_default = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        }).to_js_function(ctx.realm());

        let event = ObjectInitializer::new(ctx)
            .property(js_string!("type"), JsValue::from(js_string!(type_.as_str())), Attribute::all())
            .property(js_string!("scale"), JsValue::from(1.0), Attribute::all())
            .property(js_string!("rotation"), JsValue::from(0.0), Attribute::all())
            .property(js_string!("bubbles"), JsValue::from(true), Attribute::all())
            .property(js_string!("cancelable"), JsValue::from(true), Attribute::all())
            .property(js_string!("preventDefault"), JsValue::from(prevent_default), Attribute::all())
            .build();

        Ok(JsValue::from(event))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("GestureEvent"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("GestureEvent"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register GestureEvent: {}", e)))?;

    Ok(())
}

/// Register pointer capture methods on Element prototype
fn register_pointer_methods(context: &mut Context) -> JsResult<()> {
    // These would normally be added to Element.prototype
    // We'll register global helpers that can be used

    // hasPointerCapture(pointerId)
    let has_pointer_capture = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let pointer_id = args.get_or_undefined(0).to_i32(ctx).unwrap_or(0);
        let captured = POINTER_CAPTURE.lock()
            .map(|c| c.contains_key(&pointer_id))
            .unwrap_or(false);
        Ok(JsValue::from(captured))
    });

    // setPointerCapture(pointerId)
    let set_pointer_capture = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let pointer_id = args.get_or_undefined(0).to_i32(ctx).unwrap_or(0);
        if let Ok(mut capture) = POINTER_CAPTURE.lock() {
            capture.insert(pointer_id, "captured".to_string());
        }
        Ok(JsValue::undefined())
    });

    // releasePointerCapture(pointerId)
    let release_pointer_capture = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let pointer_id = args.get_or_undefined(0).to_i32(ctx).unwrap_or(0);
        if let Ok(mut capture) = POINTER_CAPTURE.lock() {
            capture.remove(&pointer_id);
        }
        Ok(JsValue::undefined())
    });

    context.register_global_builtin_callable(js_string!("hasPointerCapture"), 1, has_pointer_capture)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register hasPointerCapture: {}", e)))?;
    context.register_global_builtin_callable(js_string!("setPointerCapture"), 1, set_pointer_capture)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register setPointerCapture: {}", e)))?;
    context.register_global_builtin_callable(js_string!("releasePointerCapture"), 1, release_pointer_capture)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register releasePointerCapture: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::Source;

    fn create_test_context() -> Context {
        let mut ctx = Context::default();
        register_all_touch_pointer_apis(&mut ctx).unwrap();
        ctx
    }

    #[test]
    fn test_touch_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof Touch === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_touch_event_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof TouchEvent === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_touch_list_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof TouchList === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_pointer_event_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof PointerEvent === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_pointer_event_properties() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            var e = new PointerEvent('pointerdown', { pointerId: 1, pointerType: 'touch' });
            e.pointerId === 1 && e.pointerType === 'touch'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_gesture_event_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof GestureEvent === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }
}
