//! Page Visibility and Fullscreen APIs
//!
//! Implements:
//! - Page Visibility API (document.hidden, visibilityState)
//! - Fullscreen API (requestFullscreen, exitFullscreen)
//! - Screen Orientation API
//! - Screen Wake Lock API
//! - Picture-in-Picture API

use boa_engine::{
    Context, JsArgs, JsNativeError, JsResult, JsValue,
    NativeFunction, js_string, object::ObjectInitializer,
    property::Attribute,
};
use std::sync::{Arc, Mutex};

lazy_static::lazy_static! {
    /// Current visibility state
    static ref VISIBILITY_STATE: Arc<Mutex<String>> = Arc::new(Mutex::new("visible".to_string()));

    /// Fullscreen element
    static ref FULLSCREEN_ELEMENT: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));

    /// Wake lock state
    static ref WAKE_LOCK_ACTIVE: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

    /// PiP element
    static ref PIP_ELEMENT: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));
}

/// Register all visibility and fullscreen APIs
pub fn register_all_visibility_fullscreen_apis(context: &mut Context) -> JsResult<()> {
    register_page_visibility(context)?;
    register_fullscreen_api(context)?;
    register_screen_orientation(context)?;
    register_wake_lock(context)?;
    register_picture_in_picture(context)?;
    register_document_visibility_properties(context)?;
    register_visual_viewport(context)?;
    Ok(())
}

/// Register Page Visibility API
fn register_page_visibility(context: &mut Context) -> JsResult<()> {
    // Register VisibilityStateEntry for Performance Observer
    let visibility_entry = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let entry = ObjectInitializer::new(ctx)
            .property(js_string!("entryType"), JsValue::from(js_string!("visibility-state")), Attribute::all())
            .property(js_string!("name"), JsValue::from(js_string!("visible")), Attribute::all())
            .property(js_string!("startTime"), JsValue::from(0.0), Attribute::all())
            .property(js_string!("duration"), JsValue::from(0.0), Attribute::all())
            .build();
        Ok(JsValue::from(entry))
    });

    context.register_global_builtin_callable(js_string!("VisibilityStateEntry"), 0, visibility_entry)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register VisibilityStateEntry: {}", e)))?;

    Ok(())
}

/// Register document visibility properties
fn register_document_visibility_properties(context: &mut Context) -> JsResult<()> {
    // These would normally be added to document object
    // We register global getters for testing

    let get_hidden = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        let state = VISIBILITY_STATE.lock().unwrap();
        Ok(JsValue::from(*state != "visible"))
    });

    let get_visibility_state = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        let state = VISIBILITY_STATE.lock().unwrap();
        Ok(JsValue::from(js_string!(state.as_str())))
    });

    context.register_global_builtin_callable(js_string!("__getDocumentHidden"), 0, get_hidden)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register __getDocumentHidden: {}", e)))?;

    context.register_global_builtin_callable(js_string!("__getVisibilityState"), 0, get_visibility_state)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register __getVisibilityState: {}", e)))?;

    Ok(())
}

/// Register Fullscreen API
fn register_fullscreen_api(context: &mut Context) -> JsResult<()> {
    // Element.requestFullscreen()
    let request_fullscreen = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        *FULLSCREEN_ELEMENT.lock().unwrap() = Some(1); // Stub element ID

        // Return a resolved promise
        let then = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(cb) = args.get_or_undefined(0).as_callable() {
                let _ = cb.call(&JsValue::undefined(), &[], ctx);
            }
            Ok(JsValue::undefined())
        }).to_js_function(ctx.realm());

        let catch = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        }).to_js_function(ctx.realm());

        let promise = ObjectInitializer::new(ctx)
            .property(js_string!("then"), JsValue::from(then), Attribute::all())
            .property(js_string!("catch"), JsValue::from(catch), Attribute::all())
            .build();

        Ok(JsValue::from(promise))
    });

    context.register_global_builtin_callable(js_string!("requestFullscreen"), 0, request_fullscreen)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register requestFullscreen: {}", e)))?;

    // document.exitFullscreen()
    let exit_fullscreen = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        *FULLSCREEN_ELEMENT.lock().unwrap() = None;

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
    });

    context.register_global_builtin_callable(js_string!("exitFullscreen"), 0, exit_fullscreen)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register exitFullscreen: {}", e)))?;

    // document.fullscreenEnabled
    let fullscreen_enabled = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(true))
    });

    context.register_global_builtin_callable(js_string!("__fullscreenEnabled"), 0, fullscreen_enabled)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register __fullscreenEnabled: {}", e)))?;

    // document.fullscreenElement getter
    let get_fullscreen_element = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        let element = FULLSCREEN_ELEMENT.lock().unwrap();
        if element.is_some() {
            // Return a stub element
            Ok(JsValue::from(js_string!("[FullscreenElement]")))
        } else {
            Ok(JsValue::null())
        }
    });

    context.register_global_builtin_callable(js_string!("__getFullscreenElement"), 0, get_fullscreen_element)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register __getFullscreenElement: {}", e)))?;

    Ok(())
}

/// Register Screen Orientation API
fn register_screen_orientation(context: &mut Context) -> JsResult<()> {
    // screen.orientation object
    let lock = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let _orientation = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

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
    }).to_js_function(context.realm());

    let unlock = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    let remove_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    let orientation = ObjectInitializer::new(context)
        .property(js_string!("type"), JsValue::from(js_string!("landscape-primary")), Attribute::all())
        .property(js_string!("angle"), JsValue::from(0), Attribute::all())
        .property(js_string!("lock"), JsValue::from(lock), Attribute::all())
        .property(js_string!("unlock"), JsValue::from(unlock), Attribute::all())
        .property(js_string!("addEventListener"), JsValue::from(add_event_listener), Attribute::all())
        .property(js_string!("removeEventListener"), JsValue::from(remove_event_listener), Attribute::all())
        .property(js_string!("onchange"), JsValue::null(), Attribute::all())
        .build();

    // Register ScreenOrientation constructor
    let orientation_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(ctx.global_object().get(js_string!("__screenOrientation"), ctx).unwrap_or(JsValue::undefined()))
    });

    context.register_global_property(js_string!("__screenOrientation"), JsValue::from(orientation), Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register __screenOrientation: {}", e)))?;

    context.register_global_builtin_callable(js_string!("ScreenOrientation"), 0, orientation_constructor)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register ScreenOrientation: {}", e)))?;

    Ok(())
}

/// Register Wake Lock API
fn register_wake_lock(context: &mut Context) -> JsResult<()> {
    // navigator.wakeLock.request(type)
    let request = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let lock_type = args.get_or_undefined(0).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| "screen".to_string());

        *WAKE_LOCK_ACTIVE.lock().unwrap() = true;

        // Create WakeLockSentinel
        let release = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            *WAKE_LOCK_ACTIVE.lock().unwrap() = false;

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

        let sentinel = ObjectInitializer::new(ctx)
            .property(js_string!("released"), JsValue::from(false), Attribute::all())
            .property(js_string!("type"), JsValue::from(js_string!(lock_type.as_str())), Attribute::all())
            .property(js_string!("release"), JsValue::from(release), Attribute::all())
            .property(js_string!("onrelease"), JsValue::null(), Attribute::all())
            .build();

        // Return promise that resolves to sentinel
        let sentinel_value = JsValue::from(sentinel);
        let then = NativeFunction::from_copy_closure(move |_this, args, ctx| {
            if let Some(cb) = args.get_or_undefined(0).as_callable() {
                let s = ObjectInitializer::new(ctx)
                    .property(js_string!("released"), JsValue::from(false), Attribute::all())
                    .property(js_string!("type"), JsValue::from(js_string!("screen")), Attribute::all())
                    .build();
                let _ = cb.call(&JsValue::undefined(), &[JsValue::from(s)], ctx);
            }
            Ok(JsValue::undefined())
        }).to_js_function(ctx.realm());

        let promise = ObjectInitializer::new(ctx)
            .property(js_string!("then"), JsValue::from(then), Attribute::all())
            .build();

        Ok(JsValue::from(promise))
    }).to_js_function(context.realm());

    let wake_lock = ObjectInitializer::new(context)
        .property(js_string!("request"), JsValue::from(request), Attribute::all())
        .build();

    // WakeLock constructor
    let wake_lock_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(ctx.global_object().get(js_string!("__wakeLock"), ctx).unwrap_or(JsValue::undefined()))
    });

    context.register_global_property(js_string!("__wakeLock"), JsValue::from(wake_lock), Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register __wakeLock: {}", e)))?;

    context.register_global_builtin_callable(js_string!("WakeLock"), 0, wake_lock_constructor)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register WakeLock: {}", e)))?;

    // WakeLockSentinel constructor
    let sentinel_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let release = NativeFunction::from_copy_closure(|_this, _args, ctx| {
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

        let sentinel = ObjectInitializer::new(ctx)
            .property(js_string!("released"), JsValue::from(false), Attribute::all())
            .property(js_string!("type"), JsValue::from(js_string!("screen")), Attribute::all())
            .property(js_string!("release"), JsValue::from(release), Attribute::all())
            .build();

        Ok(JsValue::from(sentinel))
    });

    context.register_global_builtin_callable(js_string!("WakeLockSentinel"), 0, sentinel_constructor)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register WakeLockSentinel: {}", e)))?;

    Ok(())
}

/// Register Picture-in-Picture API
fn register_picture_in_picture(context: &mut Context) -> JsResult<()> {
    // HTMLVideoElement.requestPictureInPicture()
    let request_pip = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        *PIP_ELEMENT.lock().unwrap() = Some(1);

        // Create PictureInPictureWindow
        let pip_window = ObjectInitializer::new(ctx)
            .property(js_string!("width"), JsValue::from(320), Attribute::all())
            .property(js_string!("height"), JsValue::from(180), Attribute::all())
            .property(js_string!("onresize"), JsValue::null(), Attribute::all())
            .build();

        let pip_value = JsValue::from(pip_window);
        let then = NativeFunction::from_copy_closure(move |_this, args, ctx| {
            if let Some(cb) = args.get_or_undefined(0).as_callable() {
                let w = ObjectInitializer::new(ctx)
                    .property(js_string!("width"), JsValue::from(320), Attribute::all())
                    .property(js_string!("height"), JsValue::from(180), Attribute::all())
                    .build();
                let _ = cb.call(&JsValue::undefined(), &[JsValue::from(w)], ctx);
            }
            Ok(JsValue::undefined())
        }).to_js_function(ctx.realm());

        let promise = ObjectInitializer::new(ctx)
            .property(js_string!("then"), JsValue::from(then), Attribute::all())
            .build();

        Ok(JsValue::from(promise))
    });

    context.register_global_builtin_callable(js_string!("requestPictureInPicture"), 0, request_pip)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register requestPictureInPicture: {}", e)))?;

    // document.exitPictureInPicture()
    let exit_pip = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        *PIP_ELEMENT.lock().unwrap() = None;

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
    });

    context.register_global_builtin_callable(js_string!("exitPictureInPicture"), 0, exit_pip)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register exitPictureInPicture: {}", e)))?;

    // document.pictureInPictureEnabled
    let pip_enabled = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(true))
    });

    context.register_global_builtin_callable(js_string!("__pictureInPictureEnabled"), 0, pip_enabled)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register __pictureInPictureEnabled: {}", e)))?;

    // PictureInPictureWindow constructor
    let pip_window_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let window = ObjectInitializer::new(ctx)
            .property(js_string!("width"), JsValue::from(320), Attribute::all())
            .property(js_string!("height"), JsValue::from(180), Attribute::all())
            .property(js_string!("onresize"), JsValue::null(), Attribute::all())
            .build();
        Ok(JsValue::from(window))
    });

    context.register_global_builtin_callable(js_string!("PictureInPictureWindow"), 0, pip_window_constructor)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register PictureInPictureWindow: {}", e)))?;

    // PictureInPictureEvent
    let pip_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let type_ = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        let event = ObjectInitializer::new(ctx)
            .property(js_string!("type"), JsValue::from(js_string!(type_.as_str())), Attribute::all())
            .property(js_string!("pictureInPictureWindow"), JsValue::null(), Attribute::all())
            .property(js_string!("bubbles"), JsValue::from(false), Attribute::all())
            .property(js_string!("cancelable"), JsValue::from(false), Attribute::all())
            .build();

        Ok(JsValue::from(event))
    });

    context.register_global_builtin_callable(js_string!("PictureInPictureEvent"), 1, pip_event_constructor)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register PictureInPictureEvent: {}", e)))?;

    Ok(())
}

/// Register VisualViewport API
///
/// The VisualViewport interface represents the visual viewport for a window.
/// It provides information about the viewport's position and dimensions,
/// particularly useful for handling pinch-zoom and virtual keyboards.
fn register_visual_viewport(context: &mut Context) -> JsResult<()> {
    use boa_engine::object::FunctionObjectBuilder;
    use std::cell::RefCell;
    use std::collections::HashMap;

    // Store event listeners for VisualViewport
    thread_local! {
        static VIEWPORT_LISTENERS: RefCell<HashMap<String, Vec<boa_engine::JsObject>>> = RefCell::new(HashMap::new());
    }

    // Create addEventListener function
    let add_event_listener = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let callback = args.get_or_undefined(1);

        if let Some(cb) = callback.as_object() {
            VIEWPORT_LISTENERS.with(|listeners| {
                let mut listeners = listeners.borrow_mut();
                listeners.entry(event_type).or_insert_with(Vec::new).push(cb.clone());
            });
        }

        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    // Create removeEventListener function
    let remove_event_listener = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let _callback = args.get_or_undefined(1);

        // Simple removal - in a full implementation we'd match the callback
        VIEWPORT_LISTENERS.with(|listeners| {
            let mut listeners = listeners.borrow_mut();
            if let Some(list) = listeners.get_mut(&event_type) {
                if !list.is_empty() {
                    list.pop();
                }
            }
        });

        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    // Create dispatchEvent function
    let dispatch_event = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event = args.get_or_undefined(0);

        if let Some(event_obj) = event.as_object() {
            let event_type = event_obj.get(js_string!("type"), ctx)
                .unwrap_or(JsValue::undefined())
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();

            VIEWPORT_LISTENERS.with(|listeners| {
                let listeners = listeners.borrow();
                if let Some(list) = listeners.get(&event_type) {
                    for cb in list {
                        // Call the callback directly - it was stored as a function object
                        let _ = cb.call(&JsValue::undefined(), &[event.clone()], ctx);
                    }
                }
            });
        }

        Ok(JsValue::from(true))
    }).to_js_function(context.realm());

    // Create the VisualViewport object with all properties
    // Default values represent a typical desktop viewport
    let visual_viewport = ObjectInitializer::new(context)
        // Position of the visual viewport relative to the layout viewport
        .property(js_string!("offsetLeft"), JsValue::from(0.0), Attribute::READONLY)
        .property(js_string!("offsetTop"), JsValue::from(0.0), Attribute::READONLY)
        // Position of the visual viewport relative to the document
        .property(js_string!("pageLeft"), JsValue::from(0.0), Attribute::READONLY)
        .property(js_string!("pageTop"), JsValue::from(0.0), Attribute::READONLY)
        // Dimensions of the visual viewport
        .property(js_string!("width"), JsValue::from(1920.0), Attribute::READONLY)
        .property(js_string!("height"), JsValue::from(1080.0), Attribute::READONLY)
        // Pinch-zoom scale factor (1.0 = no zoom)
        .property(js_string!("scale"), JsValue::from(1.0), Attribute::READONLY)
        // Event handlers
        .property(js_string!("onresize"), JsValue::null(), Attribute::all())
        .property(js_string!("onscroll"), JsValue::null(), Attribute::all())
        .property(js_string!("onscrollend"), JsValue::null(), Attribute::all())
        // EventTarget methods
        .property(js_string!("addEventListener"), JsValue::from(add_event_listener), Attribute::all())
        .property(js_string!("removeEventListener"), JsValue::from(remove_event_listener), Attribute::all())
        .property(js_string!("dispatchEvent"), JsValue::from(dispatch_event), Attribute::all())
        .build();

    // Register window.visualViewport
    context.register_global_property(
        js_string!("visualViewport"),
        JsValue::from(visual_viewport),
        Attribute::READONLY
    ).map_err(|e| JsNativeError::error().with_message(format!("Failed to register visualViewport: {}", e)))?;

    // Register VisualViewport constructor (for instanceof checks)
    let viewport_constructor = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Err(JsNativeError::typ().with_message("Illegal constructor").into())
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), viewport_constructor)
        .name(js_string!("VisualViewport"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("VisualViewport"), ctor, false, context)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register VisualViewport constructor: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::Source;

    fn create_test_context() -> Context {
        let mut ctx = Context::default();
        register_all_visibility_fullscreen_apis(&mut ctx).unwrap();
        ctx
    }

    #[test]
    fn test_visibility_state() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            __getVisibilityState() === 'visible'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_fullscreen_enabled() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            __fullscreenEnabled() === true
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_request_fullscreen_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof requestFullscreen === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_screen_orientation_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof ScreenOrientation === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_wake_lock_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof WakeLock === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_picture_in_picture_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof PictureInPictureWindow === 'function' &&
            typeof requestPictureInPicture === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_visual_viewport_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            visualViewport !== null && visualViewport !== undefined
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_visual_viewport_properties() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            'offsetLeft' in visualViewport &&
            'offsetTop' in visualViewport &&
            'pageLeft' in visualViewport &&
            'pageTop' in visualViewport &&
            'width' in visualViewport &&
            'height' in visualViewport &&
            'scale' in visualViewport
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_visual_viewport_values() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            visualViewport.width === 1920 &&
            visualViewport.height === 1080 &&
            visualViewport.scale === 1 &&
            visualViewport.offsetLeft === 0 &&
            visualViewport.offsetTop === 0
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_visual_viewport_event_target() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof visualViewport.addEventListener === 'function' &&
            typeof visualViewport.removeEventListener === 'function' &&
            typeof visualViewport.dispatchEvent === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_visual_viewport_event_handlers() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            'onresize' in visualViewport &&
            'onscroll' in visualViewport &&
            'onscrollend' in visualViewport
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_visual_viewport_constructor() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof VisualViewport === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }
}
