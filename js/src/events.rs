//! Event System - Full DOM Events implementation
//!
//! Implements the DOM Events specification including:
//! - Event interface with all properties and methods
//! - Event constructors (Event, CustomEvent, MouseEvent, KeyboardEvent, etc.)

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer,
    object::builtins::JsArray, object::builtins::JsPromise, object::FunctionObjectBuilder,
    property::Attribute, Context, JsArgs, JsObject, JsValue, JsError as BoaJsError,
};

/// Event phases
pub const NONE: u16 = 0;
pub const CAPTURING_PHASE: u16 = 1;
pub const AT_TARGET: u16 = 2;
pub const BUBBLING_PHASE: u16 = 3;

/// Create an Event object with all standard properties
fn create_event_object(
    context: &mut Context,
    event_type: &str,
    bubbles: bool,
    cancelable: bool,
    composed: bool,
) -> JsObject {
    let prevent_default = NativeFunction::from_copy_closure(|this, _args, ctx| {
        if let Some(obj) = this.as_object() {
            let _ = obj.set(js_string!("defaultPrevented"), true, false, ctx);
        }
        Ok(JsValue::undefined())
    });

    let stop_propagation = NativeFunction::from_copy_closure(|this, _args, ctx| {
        if let Some(obj) = this.as_object() {
            let _ = obj.set(js_string!("cancelBubble"), true, false, ctx);
        }
        Ok(JsValue::undefined())
    });

    let stop_immediate_propagation = NativeFunction::from_copy_closure(|this, _args, ctx| {
        if let Some(obj) = this.as_object() {
            let _ = obj.set(js_string!("cancelBubble"), true, false, ctx);
            let _ = obj.set(js_string!("immediatePropagationStopped"), true, false, ctx);
        }
        Ok(JsValue::undefined())
    });

    let composed_path = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let arr = JsArray::new(ctx);
        Ok(JsValue::from(arr))
    });

    let init_event = NativeFunction::from_copy_closure(|this, args, ctx| {
        if let Some(obj) = this.as_object() {
            if let Some(type_arg) = args.get(0) {
                let type_str = type_arg.to_string(ctx)?;
                let _ = obj.set(js_string!("type"), JsValue::from(type_str), false, ctx);
            }
            if let Some(bubbles) = args.get(1) {
                let _ = obj.set(js_string!("bubbles"), bubbles.clone(), false, ctx);
            }
            if let Some(cancelable) = args.get(2) {
                let _ = obj.set(js_string!("cancelable"), cancelable.clone(), false, ctx);
            }
        }
        Ok(JsValue::undefined())
    });

    ObjectInitializer::new(context)
        // Standard Event properties
        .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
        .property(js_string!("target"), JsValue::null(), Attribute::all())
        .property(js_string!("currentTarget"), JsValue::null(), Attribute::all())
        .property(js_string!("srcElement"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("eventPhase"), NONE as i32, Attribute::all())
        .property(js_string!("bubbles"), bubbles, Attribute::READONLY)
        .property(js_string!("cancelable"), cancelable, Attribute::READONLY)
        .property(js_string!("composed"), composed, Attribute::READONLY)
        .property(js_string!("defaultPrevented"), false, Attribute::all())
        .property(js_string!("timeStamp"), 0.0, Attribute::READONLY)
        .property(js_string!("isTrusted"), false, Attribute::READONLY)
        .property(js_string!("returnValue"), true, Attribute::all())
        .property(js_string!("cancelBubble"), false, Attribute::all())
        // Methods
        .function(prevent_default, js_string!("preventDefault"), 0)
        .function(stop_propagation, js_string!("stopPropagation"), 0)
        .function(stop_immediate_propagation, js_string!("stopImmediatePropagation"), 0)
        .function(composed_path, js_string!("composedPath"), 0)
        .function(init_event, js_string!("initEvent"), 3)
        // Constants
        .property(js_string!("NONE"), NONE as i32, Attribute::READONLY)
        .property(js_string!("CAPTURING_PHASE"), CAPTURING_PHASE as i32, Attribute::READONLY)
        .property(js_string!("AT_TARGET"), AT_TARGET as i32, Attribute::READONLY)
        .property(js_string!("BUBBLING_PHASE"), BUBBLING_PHASE as i32, Attribute::READONLY)
        .build()
}

/// Create the Event constructor
fn create_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        // Parse options if provided
        let (bubbles, cancelable, composed) = if let Some(options) = args.get(1) {
            if let Some(obj) = options.as_object() {
                let bubbles = obj
                    .get(js_string!("bubbles"), ctx)
                    .map(|v| v.to_boolean())
                    .unwrap_or(false);
                let cancelable = obj
                    .get(js_string!("cancelable"), ctx)
                    .map(|v| v.to_boolean())
                    .unwrap_or(false);
                let composed = obj
                    .get(js_string!("composed"), ctx)
                    .map(|v| v.to_boolean())
                    .unwrap_or(false);
                (bubbles, cancelable, composed)
            } else {
                (false, false, false)
            }
        } else {
            (false, false, false)
        };

        Ok(JsValue::from(create_event_object(ctx, &event_type, bubbles, cancelable, composed)))
    })
}

/// Create CustomEvent constructor
fn create_custom_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let (bubbles, cancelable, detail) = if let Some(options) = args.get(1) {
            if let Some(obj) = options.as_object() {
                let bubbles = obj.get(js_string!("bubbles"), ctx).map(|v| v.to_boolean()).unwrap_or(false);
                let cancelable = obj.get(js_string!("cancelable"), ctx).map(|v| v.to_boolean()).unwrap_or(false);
                let detail = obj.get(js_string!("detail"), ctx).unwrap_or(JsValue::null());
                (bubbles, cancelable, detail)
            } else {
                (false, false, JsValue::null())
            }
        } else {
            (false, false, JsValue::null())
        };

        let event = create_event_object(ctx, &event_type, bubbles, cancelable, false);
        let _ = event.set(js_string!("detail"), detail, false, ctx);

        let init_custom_event = NativeFunction::from_copy_closure(|this, args, ctx| {
            if let Some(obj) = this.as_object() {
                if args.len() > 0 {
                    if let Ok(s) = args[0].to_string(ctx) {
                        let _ = obj.set(js_string!("type"), JsValue::from(s), false, ctx);
                    }
                }
                if args.len() > 1 { let _ = obj.set(js_string!("bubbles"), args[1].clone(), false, ctx); }
                if args.len() > 2 { let _ = obj.set(js_string!("cancelable"), args[2].clone(), false, ctx); }
                if args.len() > 3 { let _ = obj.set(js_string!("detail"), args[3].clone(), false, ctx); }
            }
            Ok(JsValue::undefined())
        });
        let _ = event.set(js_string!("initCustomEvent"), init_custom_event.to_js_function(ctx.realm()), false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create MouseEvent constructor
fn create_mouse_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, true, true, false);

        // Parse MouseEventInit
        let (screen_x, screen_y, client_x, client_y, ctrl_key, shift_key, alt_key, meta_key, button, buttons, related_target) =
            if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
                (
                    options.get(js_string!("screenX"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0),
                    options.get(js_string!("screenY"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0),
                    options.get(js_string!("clientX"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0),
                    options.get(js_string!("clientY"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0),
                    options.get(js_string!("ctrlKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    options.get(js_string!("shiftKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    options.get(js_string!("altKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    options.get(js_string!("metaKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    options.get(js_string!("button"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0) as i16,
                    options.get(js_string!("buttons"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0) as u16,
                    options.get(js_string!("relatedTarget"), ctx).unwrap_or(JsValue::null()),
                )
            } else {
                (0, 0, 0, 0, false, false, false, false, 0, 0, JsValue::null())
            };

        let _ = event.set(js_string!("screenX"), screen_x, false, ctx);
        let _ = event.set(js_string!("screenY"), screen_y, false, ctx);
        let _ = event.set(js_string!("clientX"), client_x, false, ctx);
        let _ = event.set(js_string!("clientY"), client_y, false, ctx);
        let _ = event.set(js_string!("pageX"), client_x, false, ctx);
        let _ = event.set(js_string!("pageY"), client_y, false, ctx);
        let _ = event.set(js_string!("offsetX"), client_x, false, ctx);
        let _ = event.set(js_string!("offsetY"), client_y, false, ctx);
        let _ = event.set(js_string!("movementX"), 0, false, ctx);
        let _ = event.set(js_string!("movementY"), 0, false, ctx);
        let _ = event.set(js_string!("ctrlKey"), ctrl_key, false, ctx);
        let _ = event.set(js_string!("shiftKey"), shift_key, false, ctx);
        let _ = event.set(js_string!("altKey"), alt_key, false, ctx);
        let _ = event.set(js_string!("metaKey"), meta_key, false, ctx);
        let _ = event.set(js_string!("button"), button as i32, false, ctx);
        let _ = event.set(js_string!("buttons"), buttons as i32, false, ctx);
        let _ = event.set(js_string!("relatedTarget"), related_target, false, ctx);
        let _ = event.set(js_string!("x"), client_x, false, ctx);
        let _ = event.set(js_string!("y"), client_y, false, ctx);

        let get_modifier_state = NativeFunction::from_copy_closure(|this, args, ctx| {
            let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            if let Some(obj) = this.as_object() {
                let result = match key.as_str() {
                    "Control" => obj.get(js_string!("ctrlKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    "Shift" => obj.get(js_string!("shiftKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    "Alt" => obj.get(js_string!("altKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    "Meta" => obj.get(js_string!("metaKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    _ => false,
                };
                return Ok(JsValue::from(result));
            }
            Ok(JsValue::from(false))
        });
        let _ = event.set(js_string!("getModifierState"), get_modifier_state.to_js_function(ctx.realm()), false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create KeyboardEvent constructor
fn create_keyboard_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, true, true, false);

        let (key, code, location, ctrl_key, shift_key, alt_key, meta_key, repeat, is_composing) =
            if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
                (
                    options.get(js_string!("key"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                    options.get(js_string!("code"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                    options.get(js_string!("location"), ctx).ok().and_then(|v| v.to_u32(ctx).ok()).unwrap_or(0),
                    options.get(js_string!("ctrlKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    options.get(js_string!("shiftKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    options.get(js_string!("altKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    options.get(js_string!("metaKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    options.get(js_string!("repeat"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    options.get(js_string!("isComposing"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                )
            } else {
                (String::new(), String::new(), 0, false, false, false, false, false, false)
            };

        let key_code = match key.as_str() {
            "Enter" => 13, "Escape" => 27, "Space" | " " => 32,
            "ArrowLeft" => 37, "ArrowUp" => 38, "ArrowRight" => 39, "ArrowDown" => 40,
            "Backspace" => 8, "Tab" => 9, "Delete" => 46,
            _ if key.len() == 1 => key.chars().next().map(|c| c as u32).unwrap_or(0),
            _ => 0,
        };

        let _ = event.set(js_string!("key"), js_string!(key), false, ctx);
        let _ = event.set(js_string!("code"), js_string!(code), false, ctx);
        let _ = event.set(js_string!("location"), location, false, ctx);
        let _ = event.set(js_string!("ctrlKey"), ctrl_key, false, ctx);
        let _ = event.set(js_string!("shiftKey"), shift_key, false, ctx);
        let _ = event.set(js_string!("altKey"), alt_key, false, ctx);
        let _ = event.set(js_string!("metaKey"), meta_key, false, ctx);
        let _ = event.set(js_string!("repeat"), repeat, false, ctx);
        let _ = event.set(js_string!("isComposing"), is_composing, false, ctx);
        let _ = event.set(js_string!("keyCode"), key_code, false, ctx);
        let _ = event.set(js_string!("charCode"), key_code, false, ctx);
        let _ = event.set(js_string!("which"), key_code, false, ctx);
        let _ = event.set(js_string!("DOM_KEY_LOCATION_STANDARD"), 0, false, ctx);
        let _ = event.set(js_string!("DOM_KEY_LOCATION_LEFT"), 1, false, ctx);
        let _ = event.set(js_string!("DOM_KEY_LOCATION_RIGHT"), 2, false, ctx);
        let _ = event.set(js_string!("DOM_KEY_LOCATION_NUMPAD"), 3, false, ctx);

        let get_modifier_state = NativeFunction::from_copy_closure(|this, args, ctx| {
            let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            if let Some(obj) = this.as_object() {
                let result = match key.as_str() {
                    "Control" => obj.get(js_string!("ctrlKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    "Shift" => obj.get(js_string!("shiftKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    "Alt" => obj.get(js_string!("altKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    "Meta" => obj.get(js_string!("metaKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    _ => false,
                };
                return Ok(JsValue::from(result));
            }
            Ok(JsValue::from(false))
        });
        let _ = event.set(js_string!("getModifierState"), get_modifier_state.to_js_function(ctx.realm()), false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create FocusEvent constructor
fn create_focus_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, false, false, true);

        let related_target = args.get(1).and_then(|v| v.as_object())
            .and_then(|obj| obj.get(js_string!("relatedTarget"), ctx).ok())
            .unwrap_or(JsValue::null());
        let _ = event.set(js_string!("relatedTarget"), related_target, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create InputEvent constructor
fn create_input_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, true, false, false);

        let (data, input_type, is_composing) = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            (
                options.get(js_string!("data"), ctx).unwrap_or(JsValue::null()),
                options.get(js_string!("inputType"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                options.get(js_string!("isComposing"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
            )
        } else {
            (JsValue::null(), String::new(), false)
        };

        let _ = event.set(js_string!("data"), data, false, ctx);
        let _ = event.set(js_string!("inputType"), js_string!(input_type), false, ctx);
        let _ = event.set(js_string!("isComposing"), is_composing, false, ctx);
        let _ = event.set(js_string!("dataTransfer"), JsValue::null(), false, ctx);

        let get_target_ranges = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            Ok(JsValue::from(JsArray::new(ctx)))
        });
        let _ = event.set(js_string!("getTargetRanges"), get_target_ranges.to_js_function(ctx.realm()), false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create WheelEvent constructor
/// WheelEvent inherits from MouseEvent and adds deltaX, deltaY, deltaZ, deltaMode
fn create_wheel_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, true, true, false);

        // Parse WheelEventInit (extends MouseEventInit)
        let (screen_x, screen_y, client_x, client_y, ctrl_key, shift_key, alt_key, meta_key,
             button, buttons, related_target, delta_x, delta_y, delta_z, delta_mode) =
            if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
                (
                    // MouseEvent properties
                    options.get(js_string!("screenX"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0),
                    options.get(js_string!("screenY"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0),
                    options.get(js_string!("clientX"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0),
                    options.get(js_string!("clientY"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0),
                    options.get(js_string!("ctrlKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    options.get(js_string!("shiftKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    options.get(js_string!("altKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    options.get(js_string!("metaKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    options.get(js_string!("button"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0) as i16,
                    options.get(js_string!("buttons"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0) as u16,
                    options.get(js_string!("relatedTarget"), ctx).unwrap_or(JsValue::null()),
                    // WheelEvent-specific properties
                    options.get(js_string!("deltaX"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0),
                    options.get(js_string!("deltaY"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0),
                    options.get(js_string!("deltaZ"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0),
                    options.get(js_string!("deltaMode"), ctx).ok().and_then(|v| v.to_u32(ctx).ok()).unwrap_or(0),
                )
            } else {
                (0, 0, 0, 0, false, false, false, false, 0, 0, JsValue::null(), 0.0, 0.0, 0.0, 0)
            };

        // Set MouseEvent properties (WheelEvent inherits from MouseEvent)
        let _ = event.set(js_string!("screenX"), screen_x, false, ctx);
        let _ = event.set(js_string!("screenY"), screen_y, false, ctx);
        let _ = event.set(js_string!("clientX"), client_x, false, ctx);
        let _ = event.set(js_string!("clientY"), client_y, false, ctx);
        let _ = event.set(js_string!("pageX"), client_x, false, ctx);
        let _ = event.set(js_string!("pageY"), client_y, false, ctx);
        let _ = event.set(js_string!("offsetX"), client_x, false, ctx);
        let _ = event.set(js_string!("offsetY"), client_y, false, ctx);
        let _ = event.set(js_string!("movementX"), 0, false, ctx);
        let _ = event.set(js_string!("movementY"), 0, false, ctx);
        let _ = event.set(js_string!("ctrlKey"), ctrl_key, false, ctx);
        let _ = event.set(js_string!("shiftKey"), shift_key, false, ctx);
        let _ = event.set(js_string!("altKey"), alt_key, false, ctx);
        let _ = event.set(js_string!("metaKey"), meta_key, false, ctx);
        let _ = event.set(js_string!("button"), button as i32, false, ctx);
        let _ = event.set(js_string!("buttons"), buttons as i32, false, ctx);
        let _ = event.set(js_string!("relatedTarget"), related_target, false, ctx);
        let _ = event.set(js_string!("x"), client_x, false, ctx);
        let _ = event.set(js_string!("y"), client_y, false, ctx);

        // WheelEvent-specific properties
        let _ = event.set(js_string!("deltaX"), delta_x, false, ctx);
        let _ = event.set(js_string!("deltaY"), delta_y, false, ctx);
        let _ = event.set(js_string!("deltaZ"), delta_z, false, ctx);
        let _ = event.set(js_string!("deltaMode"), delta_mode, false, ctx);

        // WheelEvent DOM_DELTA constants
        let _ = event.set(js_string!("DOM_DELTA_PIXEL"), 0, false, ctx);
        let _ = event.set(js_string!("DOM_DELTA_LINE"), 1, false, ctx);
        let _ = event.set(js_string!("DOM_DELTA_PAGE"), 2, false, ctx);

        // getModifierState method (inherited from MouseEvent/UIEvent)
        let get_modifier_state = NativeFunction::from_copy_closure(|this, args, ctx| {
            let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            if let Some(obj) = this.as_object() {
                let result = match key.as_str() {
                    "Control" => obj.get(js_string!("ctrlKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    "Shift" => obj.get(js_string!("shiftKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    "Alt" => obj.get(js_string!("altKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    "Meta" => obj.get(js_string!("metaKey"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                    "CapsLock" | "NumLock" | "ScrollLock" => false,
                    _ => false,
                };
                return Ok(JsValue::from(result));
            }
            Ok(JsValue::from(false))
        });
        let _ = event.set(js_string!("getModifierState"), get_modifier_state.to_js_function(ctx.realm()), false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create PointerEvent constructor
fn create_pointer_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, true, true, false);

        let (pointer_id, width, height, pressure, tangential_pressure, tilt_x, tilt_y, twist, pointer_type, is_primary) =
            if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
                (
                    options.get(js_string!("pointerId"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0),
                    options.get(js_string!("width"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0),
                    options.get(js_string!("height"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0),
                    options.get(js_string!("pressure"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0),
                    options.get(js_string!("tangentialPressure"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0),
                    options.get(js_string!("tiltX"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0),
                    options.get(js_string!("tiltY"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0),
                    options.get(js_string!("twist"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0),
                    options.get(js_string!("pointerType"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_else(|| "mouse".to_string()),
                    options.get(js_string!("isPrimary"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                )
            } else {
                (0, 1.0, 1.0, 0.0, 0.0, 0, 0, 0, "mouse".to_string(), false)
            };

        let _ = event.set(js_string!("pointerId"), pointer_id, false, ctx);
        let _ = event.set(js_string!("width"), width, false, ctx);
        let _ = event.set(js_string!("height"), height, false, ctx);
        let _ = event.set(js_string!("pressure"), pressure, false, ctx);
        let _ = event.set(js_string!("tangentialPressure"), tangential_pressure, false, ctx);
        let _ = event.set(js_string!("tiltX"), tilt_x, false, ctx);
        let _ = event.set(js_string!("tiltY"), tilt_y, false, ctx);
        let _ = event.set(js_string!("twist"), twist, false, ctx);
        let _ = event.set(js_string!("pointerType"), js_string!(pointer_type), false, ctx);
        let _ = event.set(js_string!("isPrimary"), is_primary, false, ctx);

        let get_coalesced = NativeFunction::from_copy_closure(|_this, _args, ctx| Ok(JsValue::from(JsArray::new(ctx))));
        let get_predicted = NativeFunction::from_copy_closure(|_this, _args, ctx| Ok(JsValue::from(JsArray::new(ctx))));
        let _ = event.set(js_string!("getCoalescedEvents"), get_coalesced.to_js_function(ctx.realm()), false, ctx);
        let _ = event.set(js_string!("getPredictedEvents"), get_predicted.to_js_function(ctx.realm()), false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create TouchEvent constructor
fn create_touch_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, true, true, false);

        let create_touch_list = |ctx: &mut Context| -> JsObject {
            let item = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::null()));
            ObjectInitializer::new(ctx)
                .property(js_string!("length"), 0, Attribute::READONLY)
                .function(item, js_string!("item"), 1)
                .build()
        };

        let _ = event.set(js_string!("touches"), create_touch_list(ctx), false, ctx);
        let _ = event.set(js_string!("targetTouches"), create_touch_list(ctx), false, ctx);
        let _ = event.set(js_string!("changedTouches"), create_touch_list(ctx), false, ctx);
        let _ = event.set(js_string!("ctrlKey"), false, false, ctx);
        let _ = event.set(js_string!("shiftKey"), false, false, ctx);
        let _ = event.set(js_string!("altKey"), false, false, ctx);
        let _ = event.set(js_string!("metaKey"), false, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create DragEvent constructor
fn create_drag_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, true, true, false);

        let data_transfer = create_data_transfer(ctx);
        let _ = event.set(js_string!("dataTransfer"), data_transfer, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create DataTransfer object for drag events and clipboard
fn create_data_transfer(context: &mut Context) -> JsObject {
    let set_data = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));
    let get_data = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::from(js_string!(""))));
    let clear_data = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));
    let set_drag_image = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));

    let files_list = ObjectInitializer::new(context)
        .property(js_string!("length"), 0, Attribute::READONLY)
        .build();

    let items = ObjectInitializer::new(context)
        .property(js_string!("length"), 0, Attribute::READONLY)
        .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())), js_string!("add"), 2)
        .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())), js_string!("remove"), 1)
        .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())), js_string!("clear"), 0)
        .build();

    let types_arr = JsArray::new(context);

    ObjectInitializer::new(context)
        .property(js_string!("dropEffect"), js_string!("none"), Attribute::all())
        .property(js_string!("effectAllowed"), js_string!("uninitialized"), Attribute::all())
        .property(js_string!("files"), files_list, Attribute::READONLY)
        .property(js_string!("items"), items, Attribute::READONLY)
        .property(js_string!("types"), types_arr, Attribute::READONLY)
        .function(set_data, js_string!("setData"), 2)
        .function(get_data, js_string!("getData"), 1)
        .function(clear_data, js_string!("clearData"), 1)
        .function(set_drag_image, js_string!("setDragImage"), 3)
        .build()
}

/// Create ClipboardEvent constructor
fn create_clipboard_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        // Parse ClipboardEventInit options
        let (bubbles, cancelable, clipboard_data) = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            let bubbles = options.get(js_string!("bubbles"), ctx).map(|v| v.to_boolean()).unwrap_or(false);
            let cancelable = options.get(js_string!("cancelable"), ctx).map(|v| v.to_boolean()).unwrap_or(false);
            let clipboard_data = options.get(js_string!("clipboardData"), ctx).ok();
            (bubbles, cancelable, clipboard_data)
        } else {
            (false, false, None)
        };

        let event = create_event_object(ctx, &event_type, bubbles, cancelable, false);

        // Use provided clipboardData or create a new DataTransfer
        if let Some(data) = clipboard_data {
            let _ = event.set(js_string!("clipboardData"), data, false, ctx);
        } else {
            let _ = event.set(js_string!("clipboardData"), create_data_transfer(ctx), false, ctx);
        }

        Ok(JsValue::from(event))
    })
}

/// Create AnimationEvent constructor
fn create_animation_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, true, false, false);

        let (animation_name, elapsed_time, pseudo_element) = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            (
                options.get(js_string!("animationName"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                options.get(js_string!("elapsedTime"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0),
                options.get(js_string!("pseudoElement"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
            )
        } else {
            (String::new(), 0.0, String::new())
        };

        let _ = event.set(js_string!("animationName"), js_string!(animation_name), false, ctx);
        let _ = event.set(js_string!("elapsedTime"), elapsed_time, false, ctx);
        let _ = event.set(js_string!("pseudoElement"), js_string!(pseudo_element), false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create TransitionEvent constructor
fn create_transition_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, true, false, false);

        let (property_name, elapsed_time, pseudo_element) = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            (
                options.get(js_string!("propertyName"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                options.get(js_string!("elapsedTime"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0),
                options.get(js_string!("pseudoElement"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
            )
        } else {
            (String::new(), 0.0, String::new())
        };

        let _ = event.set(js_string!("propertyName"), js_string!(property_name), false, ctx);
        let _ = event.set(js_string!("elapsedTime"), elapsed_time, false, ctx);
        let _ = event.set(js_string!("pseudoElement"), js_string!(pseudo_element), false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create ErrorEvent constructor
fn create_error_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, false, true, false);

        let (message, filename, lineno, colno, error) = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            (
                options.get(js_string!("message"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                options.get(js_string!("filename"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                options.get(js_string!("lineno"), ctx).ok().and_then(|v| v.to_u32(ctx).ok()).unwrap_or(0),
                options.get(js_string!("colno"), ctx).ok().and_then(|v| v.to_u32(ctx).ok()).unwrap_or(0),
                options.get(js_string!("error"), ctx).unwrap_or(JsValue::undefined()),
            )
        } else {
            (String::new(), String::new(), 0, 0, JsValue::undefined())
        };

        let _ = event.set(js_string!("message"), js_string!(message), false, ctx);
        let _ = event.set(js_string!("filename"), js_string!(filename), false, ctx);
        let _ = event.set(js_string!("lineno"), lineno, false, ctx);
        let _ = event.set(js_string!("colno"), colno, false, ctx);
        let _ = event.set(js_string!("error"), error, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create ProgressEvent constructor
fn create_progress_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, false, false, false);

        let (length_computable, loaded, total) = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            (
                options.get(js_string!("lengthComputable"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
                options.get(js_string!("loaded"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0) as u64,
                options.get(js_string!("total"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0) as u64,
            )
        } else {
            (false, 0, 0)
        };

        let _ = event.set(js_string!("lengthComputable"), length_computable, false, ctx);
        let _ = event.set(js_string!("loaded"), loaded as f64, false, ctx);
        let _ = event.set(js_string!("total"), total as f64, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create MessageEvent constructor
fn create_message_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, false, false, false);

        let (data, origin, last_event_id, source, ports) = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            (
                options.get(js_string!("data"), ctx).unwrap_or(JsValue::null()),
                options.get(js_string!("origin"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                options.get(js_string!("lastEventId"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                options.get(js_string!("source"), ctx).unwrap_or(JsValue::null()),
                options.get(js_string!("ports"), ctx).unwrap_or_else(|_| JsValue::from(JsArray::new(ctx))),
            )
        } else {
            (JsValue::null(), String::new(), String::new(), JsValue::null(), JsValue::from(JsArray::new(ctx)))
        };

        let _ = event.set(js_string!("data"), data, false, ctx);
        let _ = event.set(js_string!("origin"), js_string!(origin), false, ctx);
        let _ = event.set(js_string!("lastEventId"), js_string!(last_event_id), false, ctx);
        let _ = event.set(js_string!("source"), source, false, ctx);
        let _ = event.set(js_string!("ports"), ports, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create HashChangeEvent constructor
fn create_hash_change_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, true, false, false);

        let (old_url, new_url) = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            (
                options.get(js_string!("oldURL"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                options.get(js_string!("newURL"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
            )
        } else {
            (String::new(), String::new())
        };

        let _ = event.set(js_string!("oldURL"), js_string!(old_url), false, ctx);
        let _ = event.set(js_string!("newURL"), js_string!(new_url), false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create PopStateEvent constructor
fn create_pop_state_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, true, false, false);

        let state = args.get(1).and_then(|v| v.as_object())
            .and_then(|obj| obj.get(js_string!("state"), ctx).ok())
            .unwrap_or(JsValue::null());
        let _ = event.set(js_string!("state"), state, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create StorageEvent constructor
fn create_storage_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, false, false, false);

        let (key, old_value, new_value, url, storage_area) = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            (
                options.get(js_string!("key"), ctx).unwrap_or(JsValue::null()),
                options.get(js_string!("oldValue"), ctx).unwrap_or(JsValue::null()),
                options.get(js_string!("newValue"), ctx).unwrap_or(JsValue::null()),
                options.get(js_string!("url"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                options.get(js_string!("storageArea"), ctx).unwrap_or(JsValue::null()),
            )
        } else {
            (JsValue::null(), JsValue::null(), JsValue::null(), String::new(), JsValue::null())
        };

        let _ = event.set(js_string!("key"), key, false, ctx);
        let _ = event.set(js_string!("oldValue"), old_value, false, ctx);
        let _ = event.set(js_string!("newValue"), new_value, false, ctx);
        let _ = event.set(js_string!("url"), js_string!(url), false, ctx);
        let _ = event.set(js_string!("storageArea"), storage_area, false, ctx);

        let init_storage_event = NativeFunction::from_copy_closure(|this, args, ctx| {
            if let Some(obj) = this.as_object() {
                if args.len() > 0 {
                    if let Ok(s) = args[0].to_string(ctx) {
                        let _ = obj.set(js_string!("type"), JsValue::from(s), false, ctx);
                    }
                }
                if args.len() > 1 { let _ = obj.set(js_string!("bubbles"), args[1].clone(), false, ctx); }
                if args.len() > 2 { let _ = obj.set(js_string!("cancelable"), args[2].clone(), false, ctx); }
                if args.len() > 3 { let _ = obj.set(js_string!("key"), args[3].clone(), false, ctx); }
                if args.len() > 4 { let _ = obj.set(js_string!("oldValue"), args[4].clone(), false, ctx); }
                if args.len() > 5 { let _ = obj.set(js_string!("newValue"), args[5].clone(), false, ctx); }
                if args.len() > 6 { let _ = obj.set(js_string!("url"), args[6].clone(), false, ctx); }
                if args.len() > 7 { let _ = obj.set(js_string!("storageArea"), args[7].clone(), false, ctx); }
            }
            Ok(JsValue::undefined())
        });
        let _ = event.set(js_string!("initStorageEvent"), init_storage_event.to_js_function(ctx.realm()), false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create BeforeUnloadEvent constructor
fn create_before_unload_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // BeforeUnloadEvent: bubbles=false, cancelable=true
        let event = create_event_object(ctx, &event_type, false, true, false);

        let return_value = args.get(1).and_then(|v| v.as_object())
            .and_then(|obj| obj.get(js_string!("returnValue"), ctx).ok())
            .and_then(|v| v.to_string(ctx).ok())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        let _ = event.set(js_string!("returnValue"), js_string!(return_value), false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create CompositionEvent constructor
fn create_composition_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // CompositionEvent: bubbles=true, cancelable=true
        let event = create_event_object(ctx, &event_type, true, true, false);

        let (data, locale) = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            (
                options.get(js_string!("data"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                options.get(js_string!("locale"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
            )
        } else {
            (String::new(), String::new())
        };

        let _ = event.set(js_string!("data"), js_string!(data), false, ctx);
        let _ = event.set(js_string!("locale"), js_string!(locale), false, ctx);

        // initCompositionEvent method (deprecated but still used)
        let init_composition_event = NativeFunction::from_copy_closure(|this, args, ctx| {
            if let Some(obj) = this.as_object() {
                if args.len() > 0 {
                    if let Ok(s) = args[0].to_string(ctx) {
                        let _ = obj.set(js_string!("type"), JsValue::from(s), false, ctx);
                    }
                }
                if args.len() > 1 { let _ = obj.set(js_string!("bubbles"), args[1].clone(), false, ctx); }
                if args.len() > 2 { let _ = obj.set(js_string!("cancelable"), args[2].clone(), false, ctx); }
                if args.len() > 3 { let _ = obj.set(js_string!("view"), args[3].clone(), false, ctx); }
                if args.len() > 4 {
                    if let Ok(s) = args[4].to_string(ctx) {
                        let _ = obj.set(js_string!("data"), JsValue::from(s), false, ctx);
                    }
                }
            }
            Ok(JsValue::undefined())
        });
        let _ = event.set(js_string!("initCompositionEvent"), init_composition_event.to_js_function(ctx.realm()), false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create FormDataEvent constructor
fn create_form_data_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // FormDataEvent: bubbles=true, cancelable=false
        let event = create_event_object(ctx, &event_type, true, false, false);

        let form_data = args.get(1).and_then(|v| v.as_object())
            .and_then(|obj| obj.get(js_string!("formData"), ctx).ok())
            .unwrap_or(JsValue::null());

        let _ = event.set(js_string!("formData"), form_data, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create MediaQueryListEvent constructor
fn create_media_query_list_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // MediaQueryListEvent: bubbles=false, cancelable=false
        let event = create_event_object(ctx, &event_type, false, false, false);

        let (media, matches) = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            (
                options.get(js_string!("media"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                options.get(js_string!("matches"), ctx).map(|v| v.to_boolean()).unwrap_or(false),
            )
        } else {
            (String::new(), false)
        };

        let _ = event.set(js_string!("media"), js_string!(media), false, ctx);
        let _ = event.set(js_string!("matches"), matches, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create PageTransitionEvent constructor
fn create_page_transition_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // PageTransitionEvent: bubbles=false, cancelable=false
        let event = create_event_object(ctx, &event_type, false, false, false);

        let persisted = args.get(1).and_then(|v| v.as_object())
            .and_then(|obj| obj.get(js_string!("persisted"), ctx).ok())
            .map(|v| v.to_boolean())
            .unwrap_or(false);

        let _ = event.set(js_string!("persisted"), persisted, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create PromiseRejectionEvent constructor
fn create_promise_rejection_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // PromiseRejectionEvent: bubbles=false, cancelable=true
        let event = create_event_object(ctx, &event_type, false, true, false);

        let (promise, reason) = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            (
                options.get(js_string!("promise"), ctx).unwrap_or(JsValue::undefined()),
                options.get(js_string!("reason"), ctx).unwrap_or(JsValue::undefined()),
            )
        } else {
            (JsValue::undefined(), JsValue::undefined())
        };

        let _ = event.set(js_string!("promise"), promise, false, ctx);
        let _ = event.set(js_string!("reason"), reason, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create SecurityPolicyViolationEvent constructor
fn create_security_policy_violation_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // SecurityPolicyViolationEvent: bubbles=true, cancelable=false, composed=true
        let event = create_event_object(ctx, &event_type, true, false, true);

        let (document_uri, referrer, blocked_uri, violated_directive, effective_directive,
             original_policy, disposition, source_file, status_code, line_number, column_number, sample) =
            if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
                (
                    options.get(js_string!("documentURI"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                    options.get(js_string!("referrer"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                    options.get(js_string!("blockedURI"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                    options.get(js_string!("violatedDirective"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                    options.get(js_string!("effectiveDirective"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                    options.get(js_string!("originalPolicy"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                    options.get(js_string!("disposition"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_else(|| "enforce".to_string()),
                    options.get(js_string!("sourceFile"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                    options.get(js_string!("statusCode"), ctx).ok().and_then(|v| v.to_u32(ctx).ok()).unwrap_or(0) as u16,
                    options.get(js_string!("lineNumber"), ctx).ok().and_then(|v| v.to_u32(ctx).ok()).unwrap_or(0),
                    options.get(js_string!("columnNumber"), ctx).ok().and_then(|v| v.to_u32(ctx).ok()).unwrap_or(0),
                    options.get(js_string!("sample"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                )
            } else {
                (String::new(), String::new(), String::new(), String::new(), String::new(),
                 String::new(), "enforce".to_string(), String::new(), 0, 0, 0, String::new())
            };

        let _ = event.set(js_string!("documentURI"), js_string!(document_uri), false, ctx);
        let _ = event.set(js_string!("referrer"), js_string!(referrer), false, ctx);
        let _ = event.set(js_string!("blockedURI"), js_string!(blocked_uri), false, ctx);
        let _ = event.set(js_string!("violatedDirective"), js_string!(violated_directive), false, ctx);
        let _ = event.set(js_string!("effectiveDirective"), js_string!(effective_directive), false, ctx);
        let _ = event.set(js_string!("originalPolicy"), js_string!(original_policy), false, ctx);
        let _ = event.set(js_string!("disposition"), js_string!(disposition), false, ctx);
        let _ = event.set(js_string!("sourceFile"), js_string!(source_file), false, ctx);
        let _ = event.set(js_string!("statusCode"), status_code as i32, false, ctx);
        let _ = event.set(js_string!("lineNumber"), line_number, false, ctx);
        let _ = event.set(js_string!("columnNumber"), column_number, false, ctx);
        let _ = event.set(js_string!("sample"), js_string!(sample), false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create SubmitEvent constructor
fn create_submit_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // SubmitEvent: bubbles=true, cancelable=true
        let event = create_event_object(ctx, &event_type, true, true, false);

        let submitter = args.get(1).and_then(|v| v.as_object())
            .and_then(|obj| obj.get(js_string!("submitter"), ctx).ok())
            .unwrap_or(JsValue::null());

        let _ = event.set(js_string!("submitter"), submitter, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create ToggleEvent constructor
fn create_toggle_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // ToggleEvent: bubbles=false, cancelable=false
        let event = create_event_object(ctx, &event_type, false, false, false);

        let (old_state, new_state) = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            (
                options.get(js_string!("oldState"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                options.get(js_string!("newState"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
            )
        } else {
            (String::new(), String::new())
        };

        let _ = event.set(js_string!("oldState"), js_string!(old_state), false, ctx);
        let _ = event.set(js_string!("newState"), js_string!(new_state), false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create UIEvent constructor (base for keyboard/mouse events)
fn create_ui_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = create_event_object(ctx, &event_type, true, true, false);

        let (view, detail) = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            (
                options.get(js_string!("view"), ctx).unwrap_or(JsValue::null()),
                options.get(js_string!("detail"), ctx).ok().and_then(|v| v.to_i32(ctx).ok()).unwrap_or(0),
            )
        } else {
            (JsValue::null(), 0)
        };

        let _ = event.set(js_string!("view"), view, false, ctx);
        let _ = event.set(js_string!("detail"), detail, false, ctx);
        let _ = event.set(js_string!("which"), 0, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create CommandEvent constructor (experimental - for invoking commands)
/// Used with the Invoker Commands API (commandfor/command attributes)
fn create_command_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // CommandEvent: bubbles=true, cancelable=true, composed=true
        let event = create_event_object(ctx, &event_type, true, true, true);

        let (command, source) = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            (
                options.get(js_string!("command"), ctx).ok().and_then(|v| v.to_string(ctx).ok()).map(|s| s.to_std_string_escaped()).unwrap_or_default(),
                options.get(js_string!("source"), ctx).unwrap_or(JsValue::null()),
            )
        } else {
            (String::new(), JsValue::null())
        };

        let _ = event.set(js_string!("command"), js_string!(command), false, ctx);
        let _ = event.set(js_string!("source"), source, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create TrackEvent constructor (for media track events)
/// Fired when a track is added/removed from HTMLMediaElement
fn create_track_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // TrackEvent: bubbles=false, cancelable=false
        let event = create_event_object(ctx, &event_type, false, false, false);

        let track = args.get(1).and_then(|v| v.as_object())
            .and_then(|obj| obj.get(js_string!("track"), ctx).ok())
            .unwrap_or(JsValue::null());

        let _ = event.set(js_string!("track"), track, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create GamepadEvent constructor
/// Fired when a gamepad is connected or disconnected
fn create_gamepad_event_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // GamepadEvent: bubbles=false, cancelable=false
        let event = create_event_object(ctx, &event_type, false, false, false);

        // Extract gamepad from options or create a default one
        let gamepad = if let Some(options) = args.get(1).and_then(|v| v.as_object()) {
            options.get(js_string!("gamepad"), ctx).unwrap_or(JsValue::null())
        } else {
            // Create a default Gamepad object
            let axes = JsArray::new(ctx);
            let buttons = JsArray::new(ctx);
            let default_gamepad = ObjectInitializer::new(ctx)
                .property(js_string!("id"), js_string!(""), Attribute::READONLY)
                .property(js_string!("index"), 0, Attribute::READONLY)
                .property(js_string!("connected"), false, Attribute::READONLY)
                .property(js_string!("timestamp"), 0.0, Attribute::READONLY)
                .property(js_string!("mapping"), js_string!(""), Attribute::READONLY)
                .property(js_string!("axes"), JsValue::from(axes), Attribute::READONLY)
                .property(js_string!("buttons"), JsValue::from(buttons), Attribute::READONLY)
                .property(js_string!("vibrationActuator"), JsValue::null(), Attribute::READONLY)
                .build();
            JsValue::from(default_gamepad)
        };

        let _ = event.set(js_string!("gamepad"), gamepad, false, ctx);

        Ok(JsValue::from(event))
    })
}

/// Create GamepadButton constructor
/// Represents the state of a button on a gamepad
fn create_gamepad_button_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let pressed = args.get(0)
            .map(|v| v.to_boolean())
            .unwrap_or(false);
        let touched = args.get(1)
            .map(|v| v.to_boolean())
            .unwrap_or(false);
        let value = if args.len() > 2 && !args[2].is_undefined() && !args[2].is_null() {
            args[2].to_number(ctx).unwrap_or(0.0)
        } else {
            0.0
        };

        let button = ObjectInitializer::new(ctx)
            .property(js_string!("pressed"), pressed, Attribute::READONLY)
            .property(js_string!("touched"), touched, Attribute::READONLY)
            .property(js_string!("value"), value, Attribute::READONLY)
            .build();

        Ok(JsValue::from(button))
    })
}

/// Create GamepadHapticActuator constructor
/// Represents hardware for haptic feedback
fn create_gamepad_haptic_actuator_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let actuator_type = args.get(0)
            .and_then(|v| v.to_string(ctx).ok())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|| "vibration".to_string());

        // Create pulse method - returns a Promise
        let pulse_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            // Stub: returns a resolved promise (no actual haptic hardware)
            let promise = JsPromise::resolve(JsValue::from(true), ctx);
            Ok(JsValue::from(promise))
        });

        // Create playEffect method - returns a Promise
        let play_effect_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let promise = JsPromise::resolve(js_string!("complete"), ctx);
            Ok(JsValue::from(promise))
        });

        // Create reset method - returns a Promise
        let reset_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let promise = JsPromise::resolve(js_string!("complete"), ctx);
            Ok(JsValue::from(promise))
        });

        let actuator = ObjectInitializer::new(ctx)
            .property(js_string!("type"), js_string!(actuator_type.as_str()), Attribute::READONLY)
            .function(pulse_fn, js_string!("pulse"), 2)
            .function(play_effect_fn, js_string!("playEffect"), 2)
            .function(reset_fn, js_string!("reset"), 0)
            .build();

        Ok(JsValue::from(actuator))
    })
}

/// Create GamepadPose constructor
/// Represents the pose (position/orientation) of a gamepad
fn create_gamepad_pose_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        let has_orientation = args.get(0)
            .map(|v| v.to_boolean())
            .unwrap_or(false);
        let has_position = args.get(1)
            .map(|v| v.to_boolean())
            .unwrap_or(false);

        // Position and orientation are Float32Array or null
        // For now, we return null since we don't have real VR hardware
        let pose = ObjectInitializer::new(ctx)
            .property(js_string!("hasOrientation"), has_orientation, Attribute::READONLY)
            .property(js_string!("hasPosition"), has_position, Attribute::READONLY)
            .property(js_string!("position"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("linearVelocity"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("linearAcceleration"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("orientation"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("angularVelocity"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("angularAcceleration"), JsValue::null(), Attribute::READONLY)
            .build();

        Ok(JsValue::from(pose))
    })
}

/// Create Gamepad constructor
/// Represents a gamepad/game controller
fn create_gamepad_constructor(_context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        // Extract options from first argument if provided
        let (id, index, connected, timestamp, mapping) = if let Some(options) = args.get(0).and_then(|v| v.as_object()) {
            let id = options.get(js_string!("id"), ctx)
                .ok()
                .and_then(|v| if v.is_undefined() { None } else { v.to_string(ctx).ok() })
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            let index = options.get(js_string!("index"), ctx)
                .ok()
                .and_then(|v| v.to_i32(ctx).ok())
                .unwrap_or(0);
            let connected = options.get(js_string!("connected"), ctx)
                .ok()
                .map(|v| v.to_boolean())
                .unwrap_or(false);
            let timestamp = options.get(js_string!("timestamp"), ctx)
                .ok()
                .and_then(|v| v.to_number(ctx).ok())
                .unwrap_or(0.0);
            let mapping = options.get(js_string!("mapping"), ctx)
                .ok()
                .and_then(|v| if v.is_undefined() { None } else { v.to_string(ctx).ok() })
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            (id, index, connected, timestamp, mapping)
        } else {
            (String::new(), 0, false, 0.0, String::new())
        };

        // Create axes array (default 4 axes for standard mapping)
        let axes = JsArray::new(ctx);
        if let Some(options) = args.get(0).and_then(|v| v.as_object()) {
            if let Ok(axes_val) = options.get(js_string!("axes"), ctx) {
                if let Some(axes_arr) = axes_val.as_object().and_then(|o| JsArray::from_object(o.clone()).ok()) {
                    let len = axes_arr.length(ctx).unwrap_or(0);
                    for i in 0..len {
                        if let Ok(val) = axes_arr.get(i, ctx) {
                            let _ = axes.push(val, ctx);
                        }
                    }
                }
            }
        }
        // If no axes provided, create default 4 axes with value 0
        if axes.length(ctx).unwrap_or(0) == 0 {
            for _ in 0..4 {
                let _ = axes.push(0.0, ctx);
            }
        }

        // Create buttons array (default 17 buttons for standard mapping)
        let buttons = JsArray::new(ctx);
        if let Some(options) = args.get(0).and_then(|v| v.as_object()) {
            if let Ok(buttons_val) = options.get(js_string!("buttons"), ctx) {
                if let Some(buttons_arr) = buttons_val.as_object().and_then(|o| JsArray::from_object(o.clone()).ok()) {
                    let len = buttons_arr.length(ctx).unwrap_or(0);
                    for i in 0..len {
                        if let Ok(val) = buttons_arr.get(i, ctx) {
                            let _ = buttons.push(val, ctx);
                        }
                    }
                }
            }
        }
        // If no buttons provided, create default 17 buttons
        if buttons.length(ctx).unwrap_or(0) == 0 {
            for _ in 0..17 {
                let button = ObjectInitializer::new(ctx)
                    .property(js_string!("pressed"), false, Attribute::READONLY)
                    .property(js_string!("touched"), false, Attribute::READONLY)
                    .property(js_string!("value"), 0.0, Attribute::READONLY)
                    .build();
                let _ = buttons.push(JsValue::from(button), ctx);
            }
        }

        // Create vibrationActuator (null by default, or from options)
        let vibration_actuator = if let Some(options) = args.get(0).and_then(|v| v.as_object()) {
            options.get(js_string!("vibrationActuator"), ctx).unwrap_or(JsValue::null())
        } else {
            JsValue::null()
        };

        let gamepad = ObjectInitializer::new(ctx)
            .property(js_string!("id"), js_string!(id.as_str()), Attribute::READONLY)
            .property(js_string!("index"), index, Attribute::READONLY)
            .property(js_string!("connected"), connected, Attribute::READONLY)
            .property(js_string!("timestamp"), timestamp, Attribute::READONLY)
            .property(js_string!("mapping"), js_string!(mapping.as_str()), Attribute::READONLY)
            .property(js_string!("axes"), JsValue::from(axes), Attribute::READONLY)
            .property(js_string!("buttons"), JsValue::from(buttons), Attribute::READONLY)
            .property(js_string!("vibrationActuator"), vibration_actuator, Attribute::READONLY)
            .build();

        Ok(JsValue::from(gamepad))
    })
}

/// Create an event object for document.createEvent()
pub fn create_event_object_for_type(context: &mut Context, event_type: &str) -> JsObject {
    let normalized = event_type.to_lowercase();
    let (bubbles, cancelable) = match normalized.as_str() {
        "mouseevent" | "mouseevents" => (true, true),
        "wheelevent" | "wheelevents" => (true, true),
        "keyboardevent" | "keyevents" => (true, true),
        "uievent" | "uievents" => (true, true),
        "focusevent" => (false, false),
        "customevent" => (false, false),
        _ => (false, false),
    };

    // For WheelEvent, add the delta properties and constants
    let event = create_event_object(context, "", bubbles, cancelable, false);

    if normalized == "wheelevent" || normalized == "wheelevents" {
        // Add WheelEvent-specific properties with default values
        let _ = event.set(js_string!("deltaX"), 0.0, false, context);
        let _ = event.set(js_string!("deltaY"), 0.0, false, context);
        let _ = event.set(js_string!("deltaZ"), 0.0, false, context);
        let _ = event.set(js_string!("deltaMode"), 0, false, context);
        let _ = event.set(js_string!("DOM_DELTA_PIXEL"), 0, false, context);
        let _ = event.set(js_string!("DOM_DELTA_LINE"), 1, false, context);
        let _ = event.set(js_string!("DOM_DELTA_PAGE"), 2, false, context);
        // MouseEvent properties
        let _ = event.set(js_string!("screenX"), 0, false, context);
        let _ = event.set(js_string!("screenY"), 0, false, context);
        let _ = event.set(js_string!("clientX"), 0, false, context);
        let _ = event.set(js_string!("clientY"), 0, false, context);
        let _ = event.set(js_string!("ctrlKey"), false, false, context);
        let _ = event.set(js_string!("shiftKey"), false, false, context);
        let _ = event.set(js_string!("altKey"), false, false, context);
        let _ = event.set(js_string!("metaKey"), false, false, context);
        let _ = event.set(js_string!("button"), 0, false, context);
        let _ = event.set(js_string!("buttons"), 0, false, context);
    }

    event
}

/// Helper to register a constructor using FunctionObjectBuilder
fn register_constructor(
    context: &mut Context,
    name: &str,
    native_fn: NativeFunction,
) -> Result<(), BoaJsError> {
    let constructor = FunctionObjectBuilder::new(context.realm(), native_fn)
        .name(js_string!(name))
        .length(1)
        .constructor(true)
        .build();

    // Create a prototype object for instanceof checks
    let prototype = ObjectInitializer::new(context)
        .property(js_string!("constructor"), constructor.clone(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
        .build();

    // Set Constructor.prototype = prototype
    constructor.set(js_string!("prototype"), prototype, false, context)?;

    context.register_global_property(js_string!(name), constructor, Attribute::all())?;
    Ok(())
}

/// Register all event constructors
pub fn register_event_constructors(context: &mut Context) -> Result<(), BoaJsError> {
    // Create all constructors first to avoid mutable borrow conflicts
    let event_ctor = create_event_constructor(context);
    let custom_event_ctor = create_custom_event_constructor(context);
    let mouse_event_ctor = create_mouse_event_constructor(context);
    let keyboard_event_ctor = create_keyboard_event_constructor(context);
    let focus_event_ctor = create_focus_event_constructor(context);
    let input_event_ctor = create_input_event_constructor(context);
    let wheel_event_ctor = create_wheel_event_constructor(context);
    let pointer_event_ctor = create_pointer_event_constructor(context);
    let touch_event_ctor = create_touch_event_constructor(context);
    let drag_event_ctor = create_drag_event_constructor(context);
    let clipboard_event_ctor = create_clipboard_event_constructor(context);
    let animation_event_ctor = create_animation_event_constructor(context);
    let transition_event_ctor = create_transition_event_constructor(context);
    let error_event_ctor = create_error_event_constructor(context);
    let progress_event_ctor = create_progress_event_constructor(context);
    let message_event_ctor = create_message_event_constructor(context);
    let hash_change_event_ctor = create_hash_change_event_constructor(context);
    let pop_state_event_ctor = create_pop_state_event_constructor(context);
    let storage_event_ctor = create_storage_event_constructor(context);
    // New event constructors
    let before_unload_event_ctor = create_before_unload_event_constructor(context);
    let composition_event_ctor = create_composition_event_constructor(context);
    let form_data_event_ctor = create_form_data_event_constructor(context);
    let media_query_list_event_ctor = create_media_query_list_event_constructor(context);
    let page_transition_event_ctor = create_page_transition_event_constructor(context);
    let promise_rejection_event_ctor = create_promise_rejection_event_constructor(context);
    let security_policy_violation_event_ctor = create_security_policy_violation_event_constructor(context);
    let submit_event_ctor = create_submit_event_constructor(context);
    let toggle_event_ctor = create_toggle_event_constructor(context);
    let ui_event_ctor = create_ui_event_constructor(context);
    let command_event_ctor = create_command_event_constructor(context);
    let track_event_ctor = create_track_event_constructor(context);
    let gamepad_event_ctor = create_gamepad_event_constructor(context);
    let gamepad_ctor = create_gamepad_constructor(context);
    let gamepad_button_ctor = create_gamepad_button_constructor(context);
    let gamepad_haptic_actuator_ctor = create_gamepad_haptic_actuator_constructor(context);
    let gamepad_pose_ctor = create_gamepad_pose_constructor(context);

    // Register all event constructors using FunctionObjectBuilder with constructor(true)
    register_constructor(context, "Event", event_ctor)?;
    register_constructor(context, "CustomEvent", custom_event_ctor)?;
    register_constructor(context, "MouseEvent", mouse_event_ctor)?;
    register_constructor(context, "KeyboardEvent", keyboard_event_ctor)?;
    register_constructor(context, "FocusEvent", focus_event_ctor)?;
    register_constructor(context, "InputEvent", input_event_ctor)?;
    register_constructor(context, "WheelEvent", wheel_event_ctor)?;
    register_constructor(context, "PointerEvent", pointer_event_ctor)?;
    register_constructor(context, "TouchEvent", touch_event_ctor)?;
    register_constructor(context, "DragEvent", drag_event_ctor)?;
    register_constructor(context, "ClipboardEvent", clipboard_event_ctor)?;
    register_constructor(context, "AnimationEvent", animation_event_ctor)?;
    register_constructor(context, "TransitionEvent", transition_event_ctor)?;
    register_constructor(context, "ErrorEvent", error_event_ctor)?;
    register_constructor(context, "ProgressEvent", progress_event_ctor)?;
    register_constructor(context, "MessageEvent", message_event_ctor)?;
    register_constructor(context, "HashChangeEvent", hash_change_event_ctor)?;
    register_constructor(context, "PopStateEvent", pop_state_event_ctor)?;
    register_constructor(context, "StorageEvent", storage_event_ctor)?;
    // Register new event constructors
    register_constructor(context, "BeforeUnloadEvent", before_unload_event_ctor)?;
    register_constructor(context, "CompositionEvent", composition_event_ctor)?;
    register_constructor(context, "FormDataEvent", form_data_event_ctor)?;
    register_constructor(context, "MediaQueryListEvent", media_query_list_event_ctor)?;
    register_constructor(context, "PageTransitionEvent", page_transition_event_ctor)?;
    register_constructor(context, "PromiseRejectionEvent", promise_rejection_event_ctor)?;
    register_constructor(context, "SecurityPolicyViolationEvent", security_policy_violation_event_ctor)?;
    register_constructor(context, "SubmitEvent", submit_event_ctor)?;
    register_constructor(context, "ToggleEvent", toggle_event_ctor)?;
    register_constructor(context, "UIEvent", ui_event_ctor)?;
    register_constructor(context, "CommandEvent", command_event_ctor)?;
    register_constructor(context, "TrackEvent", track_event_ctor)?;
    register_constructor(context, "GamepadEvent", gamepad_event_ctor)?;
    register_constructor(context, "Gamepad", gamepad_ctor)?;
    register_constructor(context, "GamepadButton", gamepad_button_ctor)?;
    register_constructor(context, "GamepadHapticActuator", gamepad_haptic_actuator_ctor)?;
    register_constructor(context, "GamepadPose", gamepad_pose_ctor)?;

    Ok(())
}
