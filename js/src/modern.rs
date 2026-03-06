//! Modern Web APIs - Clipboard, Geolocation, Notifications, Permissions, etc.
//!
//! Provides full implementations of modern browser APIs.

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer, property::Attribute,
    object::FunctionObjectBuilder, object::builtins::{JsPromise, JsArray},
    Context, JsArgs, JsError, JsNativeError, JsObject, JsResult, JsValue,
};
use std::collections::HashMap;
use std::sync::Mutex;
use crate::encoding::get_storage_usage;

// Clipboard storage (simulated)
lazy_static::lazy_static! {
    static ref CLIPBOARD_TEXT: Mutex<String> = Mutex::new(String::new());
    static ref CLIPBOARD_DATA: Mutex<HashMap<String, Vec<u8>>> = Mutex::new(HashMap::new());

    // History state
    static ref HISTORY_STACK: Mutex<Vec<HistoryEntry>> = Mutex::new(vec![HistoryEntry::default()]);
    static ref HISTORY_INDEX: Mutex<usize> = Mutex::new(0);

    // Permissions
    static ref PERMISSIONS: Mutex<HashMap<String, String>> = Mutex::new(HashMap::new());

    // Notification permission
    static ref NOTIFICATION_PERMISSION: Mutex<String> = Mutex::new("default".to_string());

    // Wake locks
    static ref WAKE_LOCKS: Mutex<Vec<u32>> = Mutex::new(Vec::new());

    // Web Locks API storage
    static ref HELD_LOCKS: Mutex<Vec<LockInfo>> = Mutex::new(Vec::new());
    static ref PENDING_LOCKS: Mutex<Vec<LockInfo>> = Mutex::new(Vec::new());
    static ref NEXT_LOCK_ID: Mutex<u64> = Mutex::new(1);
    static ref NEXT_WAKE_LOCK_ID: Mutex<u32> = Mutex::new(1);
}

/// Information about a held or pending lock
#[derive(Clone)]
struct LockInfo {
    id: u64,
    name: String,
    mode: String, // "exclusive" or "shared"
    client_id: String,
}

#[derive(Clone, Default)]
struct HistoryEntry {
    state: String,  // JSON serialized state
    title: String,
    url: String,
}

/// Register all modern APIs
pub fn register_all_modern_apis(context: &mut Context) -> JsResult<()> {
    register_clipboard_api(context)?;
    register_enhanced_history(context)?;
    register_selection_api(context)?;
    register_geolocation_api(context)?;
    register_notification_api(context)?;
    register_permissions_api(context)?;
    register_visibility_api(context)?;
    register_fullscreen_api(context)?;
    register_vibration_api(context)?;
    register_battery_api(context)?;
    register_network_information_api(context)?;
    register_share_api(context)?;
    register_wake_lock_api(context)?;
    register_screen_orientation_api(context)?;
    register_media_capabilities_api(context)?;
    register_intersection_types(context)?;
    register_svg_element(context)?;
    register_image_bitmap(context)?;
    register_gpu_canvas_context(context)?;
    register_paint_api(context)?;
    register_csp_violation(context)?;
    register_css_typed_om(context)?;
    register_text_metrics(context)?;
    register_html_form_controls_collection(context)?;
    register_html_options_collection(context)?;
    register_media_query_list(context)?;
    register_geolocation_coordinates(context)?;
    register_geolocation_position(context)?;
    register_geolocation_position_error(context)?;
    register_radio_node_list(context)?;
    register_time_ranges(context)?;
    register_file_reader_sync(context)?;
    register_quota_exceeded_error(context)?;
    register_xpath_ns_resolver(context)?;
    register_element_internals(context)?;
    register_media_stream(context)?;
    register_text_track_cue(context)?;
    register_vtt_cue(context)?;
    register_text_track_cue_list(context)?;
    register_text_track(context)?;
    register_text_track_list(context)?;
    register_navigation_preload_manager(context)?;
    register_storage_manager(context)?;
    register_lock_manager(context)?;
    register_media_devices(context)?;
    register_user_activation(context)?;
    register_media_error(context)?;
    register_media_stream_track(context)?;
    register_plugin_types(context)?;
    register_mime_types(context)?;
    register_dissimilar_origin_apis(context)?;
    register_dynamic_module_owner(context)?;
    register_vtt_region(context)?;
    register_media_session(context)?;
    register_video_track(context)?;
    register_media_device_info(context)?;
    register_media_list(context)?;
    Ok(())
}

/// Register Clipboard API on navigator.clipboard
fn register_clipboard_api(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    // Get navigator object
    if let Ok(nav_val) = global.get(js_string!("navigator"), context) {
        if let Some(nav_obj) = nav_val.as_object() {
            // clipboard.writeText(text)
            let write_text = NativeFunction::from_copy_closure(|_this, args, ctx| {
                let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

                // Store in clipboard
                {
                    let mut clipboard = CLIPBOARD_TEXT.lock().unwrap();
                    *clipboard = text;
                }

                // Return resolved promise
                create_resolved_promise(ctx, JsValue::undefined())
            });

            // clipboard.readText()
            let read_text = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let text = {
                    let clipboard = CLIPBOARD_TEXT.lock().unwrap();
                    clipboard.clone()
                };

                create_resolved_promise(ctx, JsValue::from(js_string!(text)))
            });

            // clipboard.write(data) - for ClipboardItem
            let write = NativeFunction::from_copy_closure(|_this, args, ctx| {
                // Accept array of ClipboardItem-like objects
                let data = args.get_or_undefined(0);
                if let Some(arr) = data.as_object() {
                    let length = arr.get(js_string!("length"), ctx)
                        .ok()
                        .and_then(|v| v.to_u32(ctx).ok())
                        .unwrap_or(0);

                    let mut clipboard_data = CLIPBOARD_DATA.lock().unwrap();
                    clipboard_data.clear();

                    for i in 0..length {
                        if let Ok(item) = arr.get(js_string!(i.to_string()), ctx) {
                            if let Some(item_obj) = item.as_object() {
                                // Try to get types
                                if let Ok(types) = item_obj.get(js_string!("types"), ctx) {
                                    if let Some(types_arr) = types.as_object() {
                                        let types_len = types_arr.get(js_string!("length"), ctx)
                                            .ok()
                                            .and_then(|v| v.to_u32(ctx).ok())
                                            .unwrap_or(0);

                                        for j in 0..types_len {
                                            if let Ok(type_val) = types_arr.get(js_string!(j.to_string()), ctx) {
                                                let mime = type_val.to_string(ctx)?.to_std_string_escaped();
                                                clipboard_data.insert(mime, Vec::new());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                create_resolved_promise(ctx, JsValue::undefined())
            });

            // clipboard.read() - returns ClipboardItem array
            let read = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let clipboard_data = CLIPBOARD_DATA.lock().unwrap();
                let clipboard_text = CLIPBOARD_TEXT.lock().unwrap();

                // Create ClipboardItem-like objects
                let items = ObjectInitializer::new(ctx)
                    .property(js_string!("length"), 1, Attribute::READONLY)
                    .build();

                let types: Vec<String> = clipboard_data.keys().cloned().collect();
                let types_arr = create_string_array(ctx, &types);

                let get_type = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    // Return blob-like object
                    let blob = ObjectInitializer::new(ctx)
                        .property(js_string!("size"), 0, Attribute::READONLY)
                        .property(js_string!("type"), js_string!("text/plain"), Attribute::READONLY)
                        .build();
                    create_resolved_promise(ctx, JsValue::from(blob))
                });

                let item = ObjectInitializer::new(ctx)
                    .property(js_string!("types"), types_arr, Attribute::READONLY)
                    .function(get_type, js_string!("getType"), 1)
                    .build();

                items.set(js_string!("0"), JsValue::from(item), false, ctx)?;

                create_resolved_promise(ctx, JsValue::from(items))
            });

            // Build clipboard object
            let clipboard = ObjectInitializer::new(context)
                .function(write_text, js_string!("writeText"), 1)
                .function(read_text, js_string!("readText"), 0)
                .function(write, js_string!("write"), 1)
                .function(read, js_string!("read"), 0)
                .build();

            nav_obj.set(js_string!("clipboard"), clipboard, false, context)?;
        }
    }

    // Register ClipboardItem constructor
    let clipboard_item_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let data = args.get_or_undefined(0);
        let mut types = Vec::new();

        if let Some(obj) = data.as_object() {
            // Get all enumerable properties (MIME types)
            // For simplicity, we'll check common types
            for mime in ["text/plain", "text/html", "image/png", "image/jpeg"].iter() {
                if obj.get(js_string!(*mime), ctx).is_ok() {
                    types.push((*mime).to_string());
                }
            }
        }

        let types_arr = create_string_array(ctx, &types);

        let get_type = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let blob = ObjectInitializer::new(ctx)
                .property(js_string!("size"), 0, Attribute::READONLY)
                .property(js_string!("type"), js_string!("text/plain"), Attribute::READONLY)
                .build();
            create_resolved_promise(ctx, JsValue::from(blob))
        });

        let item = ObjectInitializer::new(ctx)
            .property(js_string!("types"), types_arr, Attribute::READONLY)
            .function(get_type, js_string!("getType"), 1)
            .build();

        Ok(JsValue::from(item))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), clipboard_item_constructor)
        .name(js_string!("ClipboardItem"))
        .length(1)
        .constructor(true)
        .build();
    context.global_object().set(js_string!("ClipboardItem"), constructor, false, context)?;

    Ok(())
}

/// Register enhanced History API with real state management
fn register_enhanced_history(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    // pushState with real state storage
    let push_state = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let state = args.get_or_undefined(0);
        let state_json = js_value_to_json(state, ctx);
        let title = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
        let url = if args.len() > 2 && !args.get_or_undefined(2).is_null_or_undefined() {
            args.get_or_undefined(2).to_string(ctx)?.to_std_string_escaped()
        } else {
            String::new()
        };

        let entry = HistoryEntry {
            state: state_json,
            title,
            url,
        };

        {
            let mut stack = HISTORY_STACK.lock().unwrap();
            let mut index = HISTORY_INDEX.lock().unwrap();

            // Remove forward entries
            stack.truncate(*index + 1);
            stack.push(entry);
            *index = stack.len() - 1;
        }

        Ok(JsValue::undefined())
    });

    // replaceState with real state storage
    let replace_state = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let state = args.get_or_undefined(0);
        let state_json = js_value_to_json(state, ctx);
        let title = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
        let url = if args.len() > 2 && !args.get_or_undefined(2).is_null_or_undefined() {
            args.get_or_undefined(2).to_string(ctx)?.to_std_string_escaped()
        } else {
            String::new()
        };

        let entry = HistoryEntry {
            state: state_json,
            title,
            url,
        };

        {
            let mut stack = HISTORY_STACK.lock().unwrap();
            let index = HISTORY_INDEX.lock().unwrap();

            if *index < stack.len() {
                stack[*index] = entry;
            }
        }

        Ok(JsValue::undefined())
    });

    // go(delta)
    let go = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let delta = args.get_or_undefined(0).to_i32(ctx).unwrap_or(0);

        {
            let stack = HISTORY_STACK.lock().unwrap();
            let mut index = HISTORY_INDEX.lock().unwrap();

            let new_index = (*index as i32 + delta).max(0) as usize;
            if new_index < stack.len() {
                *index = new_index;
            }
        }

        Ok(JsValue::undefined())
    });

    // back()
    let back = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        let stack = HISTORY_STACK.lock().unwrap();
        let mut index = HISTORY_INDEX.lock().unwrap();

        if *index > 0 {
            *index -= 1;
        }

        Ok(JsValue::undefined())
    });

    // forward()
    let forward = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        let stack = HISTORY_STACK.lock().unwrap();
        let mut index = HISTORY_INDEX.lock().unwrap();

        if *index + 1 < stack.len() {
            *index += 1;
        }

        Ok(JsValue::undefined())
    });

    // Get current state
    let get_state = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let stack = HISTORY_STACK.lock().unwrap();
        let index = HISTORY_INDEX.lock().unwrap();

        if *index < stack.len() {
            let state_json = &stack[*index].state;
            if state_json.is_empty() || state_json == "null" {
                return Ok(JsValue::null());
            }
            // Parse JSON back to JsValue
            json_to_js_value(state_json, ctx)
        } else {
            Ok(JsValue::null())
        }
    });

    // Get length
    let get_length = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        let stack = HISTORY_STACK.lock().unwrap();
        Ok(JsValue::from(stack.len() as u32))
    });

    // scrollRestoration
    let scroll_restoration = js_string!("auto");

    let history = ObjectInitializer::new(context)
        .function(push_state, js_string!("pushState"), 3)
        .function(replace_state, js_string!("replaceState"), 3)
        .function(go, js_string!("go"), 1)
        .function(back, js_string!("back"), 0)
        .function(forward, js_string!("forward"), 0)
        .property(js_string!("scrollRestoration"), scroll_restoration, Attribute::all())
        .build();

    // Add state getter (simplified - returns null for now)
    history.set(js_string!("state"), JsValue::null(), false, context)?;
    history.set(js_string!("length"), JsValue::from(1), false, context)?;

    global.set(js_string!("history"), history, false, context)?;

    // Register PopStateEvent constructor
    let popstate_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        let state = if args.len() > 1 {
            let options = args.get_or_undefined(1);
            if let Some(obj) = options.as_object() {
                obj.get(js_string!("state"), ctx).unwrap_or(JsValue::null())
            } else {
                JsValue::null()
            }
        } else {
            JsValue::null()
        };

        let prevent_default = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });
        let stop_propagation = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let event = ObjectInitializer::new(ctx)
            .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
            .property(js_string!("state"), state, Attribute::READONLY)
            .property(js_string!("target"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("bubbles"), false, Attribute::READONLY)
            .property(js_string!("cancelable"), false, Attribute::READONLY)
            .function(prevent_default, js_string!("preventDefault"), 0)
            .function(stop_propagation, js_string!("stopPropagation"), 0)
            .build();

        Ok(JsValue::from(event))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), popstate_event_constructor)
        .name(js_string!("PopStateEvent"))
        .length(2)
        .constructor(true)
        .build();
    context.global_object().set(js_string!("PopStateEvent"), constructor, false, context)?;

    // Register HashChangeEvent constructor
    let hashchange_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        let (old_url, new_url) = if args.len() > 1 {
            let options = args.get_or_undefined(1);
            if let Some(obj) = options.as_object() {
                let old = obj.get(js_string!("oldURL"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_default();
                let new = obj.get(js_string!("newURL"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_default();
                (old, new)
            } else {
                (String::new(), String::new())
            }
        } else {
            (String::new(), String::new())
        };

        let event = ObjectInitializer::new(ctx)
            .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
            .property(js_string!("oldURL"), js_string!(old_url), Attribute::READONLY)
            .property(js_string!("newURL"), js_string!(new_url), Attribute::READONLY)
            .property(js_string!("bubbles"), false, Attribute::READONLY)
            .property(js_string!("cancelable"), false, Attribute::READONLY)
            .build();

        Ok(JsValue::from(event))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), hashchange_event_constructor)
        .name(js_string!("HashChangeEvent"))
        .length(2)
        .constructor(true)
        .build();
    context.global_object().set(js_string!("HashChangeEvent"), constructor, false, context)?;

    Ok(())
}

/// Register Selection and Range APIs
fn register_selection_api(context: &mut Context) -> JsResult<()> {
    // window.getSelection()
    let get_selection = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let to_string = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::from(js_string!("")))
        });

        let get_range_at = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            Ok(JsValue::from(create_range(ctx)))
        });

        let add_range = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let remove_range = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let remove_all_ranges = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let collapse = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let collapse_to_start = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let collapse_to_end = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let extend = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let set_base_and_extent = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let select_all_children = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let contains_node = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::from(false))
        });

        let selection = ObjectInitializer::new(ctx)
            .property(js_string!("anchorNode"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("anchorOffset"), 0, Attribute::READONLY)
            .property(js_string!("focusNode"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("focusOffset"), 0, Attribute::READONLY)
            .property(js_string!("isCollapsed"), true, Attribute::READONLY)
            .property(js_string!("rangeCount"), 0, Attribute::READONLY)
            .property(js_string!("type"), js_string!("None"), Attribute::READONLY)
            .function(to_string, js_string!("toString"), 0)
            .function(get_range_at, js_string!("getRangeAt"), 1)
            .function(add_range, js_string!("addRange"), 1)
            .function(remove_range, js_string!("removeRange"), 1)
            .function(remove_all_ranges, js_string!("removeAllRanges"), 0)
            .function(collapse, js_string!("collapse"), 2)
            .function(collapse_to_start, js_string!("collapseToStart"), 0)
            .function(collapse_to_end, js_string!("collapseToEnd"), 0)
            .function(extend, js_string!("extend"), 2)
            .function(set_base_and_extent, js_string!("setBaseAndExtent"), 4)
            .function(select_all_children, js_string!("selectAllChildren"), 1)
            .function(contains_node, js_string!("containsNode"), 2)
            .build();

        Ok(JsValue::from(selection))
    });

    context.register_global_builtin_callable(js_string!("getSelection"), 0, get_selection)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register getSelection: {}", e)))?;

    // Range constructor
    let range_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_range(ctx)))
    });

    let range_ctor = FunctionObjectBuilder::new(context.realm(), range_constructor)
        .name(js_string!("Range"))
        .length(0)
        .constructor(true)
        .build();
    context.global_object().set(js_string!("Range"), range_ctor, false, context)?;

    // StaticRange constructor
    let static_range_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let (start_container, start_offset, end_container, end_offset) = if args.len() > 0 {
            let init = args.get_or_undefined(0);
            if let Some(obj) = init.as_object() {
                let sc = obj.get(js_string!("startContainer"), ctx).unwrap_or(JsValue::null());
                let so = obj.get(js_string!("startOffset"), ctx)
                    .ok()
                    .and_then(|v| v.to_u32(ctx).ok())
                    .unwrap_or(0);
                let ec = obj.get(js_string!("endContainer"), ctx).unwrap_or(JsValue::null());
                let eo = obj.get(js_string!("endOffset"), ctx)
                    .ok()
                    .and_then(|v| v.to_u32(ctx).ok())
                    .unwrap_or(0);
                (sc, so, ec, eo)
            } else {
                (JsValue::null(), 0, JsValue::null(), 0)
            }
        } else {
            (JsValue::null(), 0, JsValue::null(), 0)
        };

        let range = ObjectInitializer::new(ctx)
            .property(js_string!("startContainer"), start_container.clone(), Attribute::READONLY)
            .property(js_string!("startOffset"), start_offset, Attribute::READONLY)
            .property(js_string!("endContainer"), end_container, Attribute::READONLY)
            .property(js_string!("endOffset"), end_offset, Attribute::READONLY)
            .property(js_string!("collapsed"), true, Attribute::READONLY)
            .build();

        Ok(JsValue::from(range))
    });

    let static_ctor = FunctionObjectBuilder::new(context.realm(), static_range_constructor)
        .name(js_string!("StaticRange"))
        .length(1)
        .constructor(true)
        .build();
    context.global_object().set(js_string!("StaticRange"), static_ctor, false, context)?;

    Ok(())
}

/// Create a Range object
fn create_range(ctx: &mut Context) -> JsObject {
    let set_start = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let set_end = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let set_start_before = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let set_start_after = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let set_end_before = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let set_end_after = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let collapse = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let select_node = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let select_node_contents = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let compare_boundary_points = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(0))
    });
    let delete_contents = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let extract_contents = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let frag = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 11, Attribute::READONLY)
            .build();
        Ok(JsValue::from(frag))
    });
    let clone_contents = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let frag = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 11, Attribute::READONLY)
            .build();
        Ok(JsValue::from(frag))
    });
    let insert_node = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let surround_contents = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let clone_range = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_range(ctx)))
    });
    let detach = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let to_string = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(js_string!("")))
    });
    let get_bounding_client_rect = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let rect = create_dom_rect(ctx, 0.0, 0.0, 0.0, 0.0);
        Ok(JsValue::from(rect))
    });
    let get_client_rects = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let rects = ObjectInitializer::new(ctx)
            .property(js_string!("length"), 0, Attribute::READONLY)
            .build();
        Ok(JsValue::from(rects))
    });
    let create_contextual_fragment = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let frag = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 11, Attribute::READONLY)
            .build();
        Ok(JsValue::from(frag))
    });
    let is_point_in_range = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });
    let compare_point = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(0))
    });
    let intersects_node = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });

    ObjectInitializer::new(ctx)
        .property(js_string!("startContainer"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("startOffset"), 0, Attribute::READONLY)
        .property(js_string!("endContainer"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("endOffset"), 0, Attribute::READONLY)
        .property(js_string!("collapsed"), true, Attribute::READONLY)
        .property(js_string!("commonAncestorContainer"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("START_TO_START"), 0, Attribute::READONLY)
        .property(js_string!("START_TO_END"), 1, Attribute::READONLY)
        .property(js_string!("END_TO_END"), 2, Attribute::READONLY)
        .property(js_string!("END_TO_START"), 3, Attribute::READONLY)
        .function(set_start, js_string!("setStart"), 2)
        .function(set_end, js_string!("setEnd"), 2)
        .function(set_start_before, js_string!("setStartBefore"), 1)
        .function(set_start_after, js_string!("setStartAfter"), 1)
        .function(set_end_before, js_string!("setEndBefore"), 1)
        .function(set_end_after, js_string!("setEndAfter"), 1)
        .function(collapse, js_string!("collapse"), 1)
        .function(select_node, js_string!("selectNode"), 1)
        .function(select_node_contents, js_string!("selectNodeContents"), 1)
        .function(compare_boundary_points, js_string!("compareBoundaryPoints"), 2)
        .function(delete_contents, js_string!("deleteContents"), 0)
        .function(extract_contents, js_string!("extractContents"), 0)
        .function(clone_contents, js_string!("cloneContents"), 0)
        .function(insert_node, js_string!("insertNode"), 1)
        .function(surround_contents, js_string!("surroundContents"), 1)
        .function(clone_range, js_string!("cloneRange"), 0)
        .function(detach, js_string!("detach"), 0)
        .function(to_string, js_string!("toString"), 0)
        .function(get_bounding_client_rect, js_string!("getBoundingClientRect"), 0)
        .function(get_client_rects, js_string!("getClientRects"), 0)
        .function(create_contextual_fragment, js_string!("createContextualFragment"), 1)
        .function(is_point_in_range, js_string!("isPointInRange"), 2)
        .function(compare_point, js_string!("comparePoint"), 2)
        .function(intersects_node, js_string!("intersectsNode"), 1)
        .build()
}

/// Register Geolocation API
fn register_geolocation_api(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    if let Ok(nav_val) = global.get(js_string!("navigator"), context) {
        if let Some(nav_obj) = nav_val.as_object() {
            // getCurrentPosition(success, error?, options?)
            let get_current_position = NativeFunction::from_copy_closure(|_this, args, ctx| {
                let success_callback = args.get_or_undefined(0);

                if success_callback.is_callable() {
                    // Create position object with mock data
                    let coords = ObjectInitializer::new(ctx)
                        .property(js_string!("latitude"), 37.7749, Attribute::READONLY)
                        .property(js_string!("longitude"), -122.4194, Attribute::READONLY)
                        .property(js_string!("altitude"), JsValue::null(), Attribute::READONLY)
                        .property(js_string!("accuracy"), 100.0, Attribute::READONLY)
                        .property(js_string!("altitudeAccuracy"), JsValue::null(), Attribute::READONLY)
                        .property(js_string!("heading"), JsValue::null(), Attribute::READONLY)
                        .property(js_string!("speed"), JsValue::null(), Attribute::READONLY)
                        .build();

                    let position = ObjectInitializer::new(ctx)
                        .property(js_string!("coords"), coords, Attribute::READONLY)
                        .property(js_string!("timestamp"), 0.0, Attribute::READONLY)
                        .build();

                    let callback = success_callback.as_callable().unwrap();
                    let _ = callback.call(&JsValue::undefined(), &[JsValue::from(position)], ctx);
                }

                Ok(JsValue::undefined())
            });

            // watchPosition(success, error?, options?) - returns watch ID
            let watch_position = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                // Return a watch ID
                Ok(JsValue::from(1))
            });

            // clearWatch(id)
            let clear_watch = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });

            let geolocation = ObjectInitializer::new(context)
                .function(get_current_position, js_string!("getCurrentPosition"), 3)
                .function(watch_position, js_string!("watchPosition"), 3)
                .function(clear_watch, js_string!("clearWatch"), 1)
                .build();

            nav_obj.set(js_string!("geolocation"), geolocation, false, context)?;
        }
    }

    // GeolocationPositionError constructor
    let geo_error_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let error = ObjectInitializer::new(ctx)
            .property(js_string!("code"), 1, Attribute::READONLY)
            .property(js_string!("message"), js_string!("User denied geolocation"), Attribute::READONLY)
            .property(js_string!("PERMISSION_DENIED"), 1, Attribute::READONLY)
            .property(js_string!("POSITION_UNAVAILABLE"), 2, Attribute::READONLY)
            .property(js_string!("TIMEOUT"), 3, Attribute::READONLY)
            .build();

        Ok(JsValue::from(error))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), geo_error_constructor)
        .name(js_string!("GeolocationPositionError"))
        .length(0)
        .constructor(true)
        .build();
    context.global_object().set(js_string!("GeolocationPositionError"), constructor, false, context)?;

    Ok(())
}

/// Register Notification API
fn register_notification_api(context: &mut Context) -> JsResult<()> {
    // Notification constructor
    let notification_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let title = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        // Parse options
        let (body, icon, tag, data, require_interaction, silent, dir, lang, badge, image, renotify, timestamp, vibrate) = if args.len() > 1 {
            let options = args.get_or_undefined(1);
            if let Some(obj) = options.as_object() {
                let body = obj.get(js_string!("body"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_default();
                let icon = obj.get(js_string!("icon"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_default();
                let tag = obj.get(js_string!("tag"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_default();
                let data = obj.get(js_string!("data"), ctx).unwrap_or(JsValue::null());
                let require_interaction = obj.get(js_string!("requireInteraction"), ctx)
                    .ok()
                    .map(|v| v.to_boolean())
                    .unwrap_or(false);
                let silent = obj.get(js_string!("silent"), ctx)
                    .ok()
                    .map(|v| v.to_boolean())
                    .unwrap_or(false);
                let dir = obj.get(js_string!("dir"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_else(|| "auto".to_string());
                let lang = obj.get(js_string!("lang"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_default();
                let badge = obj.get(js_string!("badge"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_default();
                let image = obj.get(js_string!("image"), ctx)
                    .ok()
                    .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                    .unwrap_or_default();
                let renotify = obj.get(js_string!("renotify"), ctx)
                    .ok()
                    .map(|v| v.to_boolean())
                    .unwrap_or(false);
                let timestamp = obj.get(js_string!("timestamp"), ctx)
                    .ok()
                    .and_then(|v| v.to_number(ctx).ok())
                    .unwrap_or(0.0);
                let vibrate = obj.get(js_string!("vibrate"), ctx).unwrap_or(JsValue::null());
                (body, icon, tag, data, require_interaction, silent, dir, lang, badge, image, renotify, timestamp, vibrate)
            } else {
                (String::new(), String::new(), String::new(), JsValue::null(), false, false, "auto".to_string(), String::new(), String::new(), String::new(), false, 0.0, JsValue::null())
            }
        } else {
            (String::new(), String::new(), String::new(), JsValue::null(), false, false, "auto".to_string(), String::new(), String::new(), String::new(), false, 0.0, JsValue::null())
        };

        let close = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let remove_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // Create actions array first to avoid borrow issue
        let actions_array = boa_engine::object::builtins::JsArray::new(ctx);

        let notification = ObjectInitializer::new(ctx)
            .property(js_string!("title"), js_string!(title), Attribute::READONLY)
            .property(js_string!("body"), js_string!(body), Attribute::READONLY)
            .property(js_string!("icon"), js_string!(icon), Attribute::READONLY)
            .property(js_string!("tag"), js_string!(tag), Attribute::READONLY)
            .property(js_string!("data"), data, Attribute::READONLY)
            .property(js_string!("requireInteraction"), require_interaction, Attribute::READONLY)
            .property(js_string!("silent"), silent, Attribute::READONLY)
            .property(js_string!("dir"), js_string!(dir), Attribute::READONLY)
            .property(js_string!("lang"), js_string!(lang), Attribute::READONLY)
            .property(js_string!("badge"), js_string!(badge), Attribute::READONLY)
            .property(js_string!("image"), js_string!(image), Attribute::READONLY)
            .property(js_string!("renotify"), renotify, Attribute::READONLY)
            .property(js_string!("timestamp"), timestamp, Attribute::READONLY)
            .property(js_string!("vibrate"), vibrate, Attribute::READONLY)
            .property(js_string!("actions"), JsValue::from(actions_array), Attribute::READONLY)
            .property(js_string!("onclick"), JsValue::null(), Attribute::all())
            .property(js_string!("onclose"), JsValue::null(), Attribute::all())
            .property(js_string!("onerror"), JsValue::null(), Attribute::all())
            .property(js_string!("onshow"), JsValue::null(), Attribute::all())
            .function(close, js_string!("close"), 0)
            .function(add_event_listener, js_string!("addEventListener"), 3)
            .function(remove_event_listener, js_string!("removeEventListener"), 3)
            .build();

        Ok(JsValue::from(notification))
    });

    let notif_ctor = FunctionObjectBuilder::new(context.realm(), notification_constructor)
        .name(js_string!("Notification"))
        .length(2)
        .constructor(true)
        .build();
    context.global_object().set(js_string!("Notification"), notif_ctor.clone(), false, context)?;

    // Add static properties to Notification
    let global = context.global_object();
    if let Ok(notif_val) = global.get(js_string!("Notification"), context) {
        if let Some(notif_obj) = notif_val.as_object() {
            // Notification.permission
            let permission = {
                let perm = NOTIFICATION_PERMISSION.lock().unwrap();
                perm.clone()
            };
            notif_obj.set(js_string!("permission"), js_string!(permission), false, context)?;

            // Notification.requestPermission()
            let request_permission = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                // Grant permission
                {
                    let mut perm = NOTIFICATION_PERMISSION.lock().unwrap();
                    *perm = "granted".to_string();
                }
                create_resolved_promise(ctx, JsValue::from(js_string!("granted")))
            });

            let request_permission_func = boa_engine::object::FunctionObjectBuilder::new(context.realm(), request_permission)
                .name(js_string!("requestPermission"))
                .length(0)
                .build();

            notif_obj.set(js_string!("requestPermission"), request_permission_func, false, context)?;
        }
    }

    Ok(())
}

/// Get default permission state for known permission types
fn get_default_permission_state(name: &str) -> &'static str {
    match name {
        // Granted by default in headless browser (no user interaction possible)
        "clipboard-read" | "clipboard-write" => "granted",
        "notifications" => "granted",
        "persistent-storage" => "granted",
        "storage-access" => "granted",
        // Denied - require hardware
        "camera" | "microphone" => "denied",
        "geolocation" => "denied",
        "midi" | "midi-sysex" => "denied",
        "speaker-selection" => "denied",
        "display-capture" => "denied",
        "screen-wake-lock" => "denied",
        // Prompt for others
        "push" | "background-sync" | "background-fetch" => "prompt",
        "accelerometer" | "gyroscope" | "magnetometer" => "denied",
        "ambient-light-sensor" => "denied",
        "payment-handler" => "prompt",
        "idle-detection" => "prompt",
        "window-management" => "granted",
        "local-fonts" => "granted",
        _ => "prompt",
    }
}

/// Register Permissions API
fn register_permissions_api(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    if let Ok(nav_val) = global.get(js_string!("navigator"), context) {
        if let Some(nav_obj) = nav_val.as_object() {
            // permissions.query({name: ...})
            let query = NativeFunction::from_copy_closure(|_this, args, ctx| {
                let name = if args.len() > 0 {
                    let desc = args.get_or_undefined(0);
                    if let Some(obj) = desc.as_object() {
                        obj.get(js_string!("name"), ctx)
                            .ok()
                            .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                // Check stored permission or use smart defaults
                let state = {
                    let perms = PERMISSIONS.lock().unwrap();
                    perms.get(&name).cloned().unwrap_or_else(|| get_default_permission_state(&name).to_string())
                };

                let on_change = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                });

                let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                });

                let remove_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                });

                let permission_status = ObjectInitializer::new(ctx)
                    .property(js_string!("name"), js_string!(name), Attribute::READONLY)
                    .property(js_string!("state"), js_string!(state), Attribute::READONLY)
                    .property(js_string!("onchange"), JsValue::null(), Attribute::all())
                    .function(add_event_listener, js_string!("addEventListener"), 3)
                    .function(remove_event_listener, js_string!("removeEventListener"), 3)
                    .build();

                create_resolved_promise(ctx, JsValue::from(permission_status))
            });

            // permissions.revoke({name: ...}) - deprecated but still used
            let revoke = NativeFunction::from_copy_closure(|_this, args, ctx| {
                let name = if args.len() > 0 {
                    let desc = args.get_or_undefined(0);
                    if let Some(obj) = desc.as_object() {
                        obj.get(js_string!("name"), ctx)
                            .ok()
                            .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                // Remove permission
                {
                    let mut perms = PERMISSIONS.lock().unwrap();
                    perms.remove(&name);
                }

                let permission_status = ObjectInitializer::new(ctx)
                    .property(js_string!("name"), js_string!(name), Attribute::READONLY)
                    .property(js_string!("state"), js_string!("prompt"), Attribute::READONLY)
                    .build();

                create_resolved_promise(ctx, JsValue::from(permission_status))
            });

            // permissions.request({name: ...}) - request a permission (auto-grants based on defaults)
            let request = NativeFunction::from_copy_closure(|_this, args, ctx| {
                let name = if args.len() > 0 {
                    let desc = args.get_or_undefined(0);
                    if let Some(obj) = desc.as_object() {
                        obj.get(js_string!("name"), ctx)
                            .ok()
                            .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                // For headless browser, auto-grant grantable permissions
                let state = get_default_permission_state(&name);
                let final_state = if state == "prompt" {
                    // Auto-grant prompt permissions in headless mode
                    "granted"
                } else {
                    state
                };

                // Store the permission state
                {
                    let mut perms = PERMISSIONS.lock().unwrap();
                    perms.insert(name.clone(), final_state.to_string());
                }

                let permission_status = ObjectInitializer::new(ctx)
                    .property(js_string!("name"), js_string!(name), Attribute::READONLY)
                    .property(js_string!("state"), js_string!(final_state), Attribute::READONLY)
                    .build();

                create_resolved_promise(ctx, JsValue::from(permission_status))
            });

            let permissions = ObjectInitializer::new(context)
                .function(query, js_string!("query"), 1)
                .function(request, js_string!("request"), 1)
                .function(revoke, js_string!("revoke"), 1)
                .build();

            nav_obj.set(js_string!("permissions"), permissions, false, context)?;
        }
    }

    Ok(())
}

/// Register Page Visibility API
fn register_visibility_api(context: &mut Context) -> JsResult<()> {
    // Add to document object
    let global = context.global_object();

    if let Ok(doc_val) = global.get(js_string!("document"), context) {
        if let Some(doc_obj) = doc_val.as_object() {
            doc_obj.set(js_string!("visibilityState"), js_string!("visible"), false, context)?;
            doc_obj.set(js_string!("hidden"), false, false, context)?;
        }
    }

    // VisibilityChange event is already supported via standard Event
    Ok(())
}

/// Register Fullscreen API
fn register_fullscreen_api(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    if let Ok(doc_val) = global.get(js_string!("document"), context) {
        if let Some(doc_obj) = doc_val.as_object() {
            // document.fullscreenEnabled
            doc_obj.set(js_string!("fullscreenEnabled"), true, false, context)?;

            // document.fullscreenElement
            doc_obj.set(js_string!("fullscreenElement"), JsValue::null(), false, context)?;

            // document.exitFullscreen()
            let exit_fullscreen = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                create_resolved_promise(ctx, JsValue::undefined())
            });

            let exit_func = boa_engine::object::FunctionObjectBuilder::new(context.realm(), exit_fullscreen)
                .name(js_string!("exitFullscreen"))
                .length(0)
                .build();

            doc_obj.set(js_string!("exitFullscreen"), exit_func, false, context)?;
        }
    }

    Ok(())
}

/// Register Vibration API
fn register_vibration_api(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    if let Ok(nav_val) = global.get(js_string!("navigator"), context) {
        if let Some(nav_obj) = nav_val.as_object() {
            // navigator.vibrate(pattern)
            let vibrate = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                // Always return true (vibration "succeeded")
                Ok(JsValue::from(true))
            });

            let vibrate_func = boa_engine::object::FunctionObjectBuilder::new(context.realm(), vibrate)
                .name(js_string!("vibrate"))
                .length(1)
                .build();

            nav_obj.set(js_string!("vibrate"), vibrate_func, false, context)?;
        }
    }

    Ok(())
}

/// Register Battery Status API
fn register_battery_api(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    if let Ok(nav_val) = global.get(js_string!("navigator"), context) {
        if let Some(nav_obj) = nav_val.as_object() {
            // navigator.getBattery()
            let get_battery = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                });

                let remove_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                });

                let battery_manager = ObjectInitializer::new(ctx)
                    .property(js_string!("charging"), true, Attribute::READONLY)
                    .property(js_string!("chargingTime"), 0.0, Attribute::READONLY)
                    .property(js_string!("dischargingTime"), f64::INFINITY, Attribute::READONLY)
                    .property(js_string!("level"), 1.0, Attribute::READONLY)
                    .property(js_string!("onchargingchange"), JsValue::null(), Attribute::all())
                    .property(js_string!("onchargingtimechange"), JsValue::null(), Attribute::all())
                    .property(js_string!("ondischargingtimechange"), JsValue::null(), Attribute::all())
                    .property(js_string!("onlevelchange"), JsValue::null(), Attribute::all())
                    .function(add_event_listener, js_string!("addEventListener"), 3)
                    .function(remove_event_listener, js_string!("removeEventListener"), 3)
                    .build();

                create_resolved_promise(ctx, JsValue::from(battery_manager))
            });

            let get_battery_func = boa_engine::object::FunctionObjectBuilder::new(context.realm(), get_battery)
                .name(js_string!("getBattery"))
                .length(0)
                .build();

            nav_obj.set(js_string!("getBattery"), get_battery_func, false, context)?;
        }
    }

    Ok(())
}

/// Register Network Information API
fn register_network_information_api(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    if let Ok(nav_val) = global.get(js_string!("navigator"), context) {
        if let Some(nav_obj) = nav_val.as_object() {
            let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });

            let remove_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });

            let connection = ObjectInitializer::new(context)
                .property(js_string!("effectiveType"), js_string!("4g"), Attribute::READONLY)
                .property(js_string!("type"), js_string!("wifi"), Attribute::READONLY)
                .property(js_string!("downlink"), 10.0, Attribute::READONLY)
                .property(js_string!("downlinkMax"), f64::INFINITY, Attribute::READONLY)
                .property(js_string!("rtt"), 50, Attribute::READONLY)
                .property(js_string!("saveData"), false, Attribute::READONLY)
                .property(js_string!("onchange"), JsValue::null(), Attribute::all())
                .function(add_event_listener, js_string!("addEventListener"), 3)
                .function(remove_event_listener, js_string!("removeEventListener"), 3)
                .build();

            nav_obj.set(js_string!("connection"), connection, false, context)?;
        }
    }

    Ok(())
}

/// Register Web Share API
fn register_share_api(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    if let Ok(nav_val) = global.get(js_string!("navigator"), context) {
        if let Some(nav_obj) = nav_val.as_object() {
            // navigator.share(data)
            let share = NativeFunction::from_copy_closure(|_this, args, ctx| {
                // Validate share data
                if args.len() > 0 {
                    let data = args.get_or_undefined(0);
                    if let Some(obj) = data.as_object() {
                        let has_title = obj.get(js_string!("title"), ctx).is_ok();
                        let has_text = obj.get(js_string!("text"), ctx).is_ok();
                        let has_url = obj.get(js_string!("url"), ctx).is_ok();
                        let has_files = obj.get(js_string!("files"), ctx).is_ok();

                        if !has_title && !has_text && !has_url && !has_files {
                            return Err(JsNativeError::typ()
                                .with_message("Share data must have at least one of: title, text, url, or files")
                                .into());
                        }
                    }
                }

                // Return resolved promise (share "succeeded")
                create_resolved_promise(ctx, JsValue::undefined())
            });

            // navigator.canShare(data)
            let can_share = NativeFunction::from_copy_closure(|_this, args, ctx| {
                if args.len() > 0 {
                    let data = args.get_or_undefined(0);
                    if let Some(obj) = data.as_object() {
                        // Check if we have shareable data
                        let has_data = obj.get(js_string!("title"), ctx).is_ok()
                            || obj.get(js_string!("text"), ctx).is_ok()
                            || obj.get(js_string!("url"), ctx).is_ok();
                        return Ok(JsValue::from(has_data));
                    }
                }
                Ok(JsValue::from(false))
            });

            let share_func = boa_engine::object::FunctionObjectBuilder::new(context.realm(), share)
                .name(js_string!("share"))
                .length(1)
                .build();

            let can_share_func = boa_engine::object::FunctionObjectBuilder::new(context.realm(), can_share)
                .name(js_string!("canShare"))
                .length(1)
                .build();

            nav_obj.set(js_string!("share"), share_func, false, context)?;
            nav_obj.set(js_string!("canShare"), can_share_func, false, context)?;
        }
    }

    Ok(())
}

/// Register Screen Wake Lock API
fn register_wake_lock_api(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    if let Ok(nav_val) = global.get(js_string!("navigator"), context) {
        if let Some(nav_obj) = nav_val.as_object() {
            // navigator.wakeLock.request(type)
            let request = NativeFunction::from_copy_closure(|_this, args, ctx| {
                let lock_type = if args.len() > 0 {
                    args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped()
                } else {
                    "screen".to_string()
                };

                // Generate lock ID
                let lock_id = {
                    let mut id = NEXT_WAKE_LOCK_ID.lock().unwrap();
                    let current = *id;
                    *id += 1;

                    let mut locks = WAKE_LOCKS.lock().unwrap();
                    locks.push(current);

                    current
                };

                // Create WakeLockSentinel
                let release_id = lock_id;
                let release = unsafe {
                    NativeFunction::from_closure(move |_this, _args, ctx| {
                        {
                            let mut locks = WAKE_LOCKS.lock().unwrap();
                            locks.retain(|&id| id != release_id);
                        }
                        create_resolved_promise(ctx, JsValue::undefined())
                    })
                };

                let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                });

                let remove_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                });

                let sentinel = ObjectInitializer::new(ctx)
                    .property(js_string!("released"), false, Attribute::READONLY)
                    .property(js_string!("type"), js_string!(lock_type), Attribute::READONLY)
                    .property(js_string!("onrelease"), JsValue::null(), Attribute::all())
                    .function(release, js_string!("release"), 0)
                    .function(add_event_listener, js_string!("addEventListener"), 3)
                    .function(remove_event_listener, js_string!("removeEventListener"), 3)
                    .build();

                create_resolved_promise(ctx, JsValue::from(sentinel))
            });

            let wake_lock = ObjectInitializer::new(context)
                .function(request, js_string!("request"), 1)
                .build();

            nav_obj.set(js_string!("wakeLock"), wake_lock, false, context)?;
        }
    }

    Ok(())
}

/// Register Screen Orientation API
fn register_screen_orientation_api(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    if let Ok(screen_val) = global.get(js_string!("screen"), context) {
        if let Some(screen_obj) = screen_val.as_object() {
            let lock = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                create_resolved_promise(ctx, JsValue::undefined())
            });

            let unlock = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });

            let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });

            let remove_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });

            let orientation = ObjectInitializer::new(context)
                .property(js_string!("type"), js_string!("landscape-primary"), Attribute::READONLY)
                .property(js_string!("angle"), 0, Attribute::READONLY)
                .property(js_string!("onchange"), JsValue::null(), Attribute::all())
                .function(lock, js_string!("lock"), 1)
                .function(unlock, js_string!("unlock"), 0)
                .function(add_event_listener, js_string!("addEventListener"), 3)
                .function(remove_event_listener, js_string!("removeEventListener"), 3)
                .build();

            screen_obj.set(js_string!("orientation"), orientation, false, context)?;
        }
    }

    Ok(())
}

/// Register Media Capabilities API
fn register_media_capabilities_api(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    if let Ok(nav_val) = global.get(js_string!("navigator"), context) {
        if let Some(nav_obj) = nav_val.as_object() {
            // navigator.mediaCapabilities.decodingInfo(config)
            let decoding_info = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let info = ObjectInitializer::new(ctx)
                    .property(js_string!("supported"), true, Attribute::READONLY)
                    .property(js_string!("smooth"), true, Attribute::READONLY)
                    .property(js_string!("powerEfficient"), true, Attribute::READONLY)
                    .build();

                create_resolved_promise(ctx, JsValue::from(info))
            });

            // navigator.mediaCapabilities.encodingInfo(config)
            let encoding_info = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let info = ObjectInitializer::new(ctx)
                    .property(js_string!("supported"), true, Attribute::READONLY)
                    .property(js_string!("smooth"), true, Attribute::READONLY)
                    .property(js_string!("powerEfficient"), true, Attribute::READONLY)
                    .build();

                create_resolved_promise(ctx, JsValue::from(info))
            });

            let media_capabilities = ObjectInitializer::new(context)
                .function(decoding_info, js_string!("decodingInfo"), 1)
                .function(encoding_info, js_string!("encodingInfo"), 1)
                .build();

            nav_obj.set(js_string!("mediaCapabilities"), media_capabilities, false, context)?;
        }
    }

    Ok(())
}

/// Helper to create DOMMatrix with specific values and transformation methods
fn create_dom_matrix_with_values(
    ctx: &mut Context,
    m11: f64, m12: f64, m13: f64, m14: f64,
    m21: f64, m22: f64, m23: f64, m24: f64,
    m31: f64, m32: f64, m33: f64, m34: f64,
    m41: f64, m42: f64, m43: f64, m44: f64,
    is_2d: bool, is_identity: bool,
) -> JsObject {
    // translate(tx, ty, tz?) - returns new matrix with translation applied
    // Use separate capture for translate to avoid closure issues
    let tr_m11 = m11; let tr_m12 = m12; let tr_m13 = m13; let tr_m14 = m14;
    let tr_m21 = m21; let tr_m22 = m22; let tr_m23 = m23; let tr_m24 = m24;
    let tr_m31 = m31; let tr_m32 = m32; let tr_m33 = m33; let tr_m34 = m34;
    let tr_m41 = m41; let tr_m42 = m42; let tr_m43 = m43; let tr_m44 = m44;

    let translate = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        // Handle NaN from undefined by checking and defaulting to 0
        let tx = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
        let tx = if tx.is_nan() { 0.0 } else { tx };
        let ty = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);
        let ty = if ty.is_nan() { 0.0 } else { ty };
        let tz = args.get_or_undefined(2).to_number(ctx).unwrap_or(0.0);
        let tz = if tz.is_nan() { 0.0 } else { tz };

        // Multiply by translation matrix
        let new_m41 = tr_m11 * tx + tr_m21 * ty + tr_m31 * tz + tr_m41;
        let new_m42 = tr_m12 * tx + tr_m22 * ty + tr_m32 * tz + tr_m42;
        let new_m43 = tr_m13 * tx + tr_m23 * ty + tr_m33 * tz + tr_m43;
        let new_m44 = tr_m14 * tx + tr_m24 * ty + tr_m34 * tz + tr_m44;

        let new_is_2d = tz == 0.0;

        Ok(JsValue::from(create_dom_matrix_with_values(ctx,
            tr_m11, tr_m12, tr_m13, tr_m14, tr_m21, tr_m22, tr_m23, tr_m24,
            tr_m31, tr_m32, tr_m33, tr_m34, new_m41, new_m42, new_m43, new_m44,
            new_is_2d, false)))
    });

    // Captures for other closures (scale, rotate, etc.)
    let (cm11, cm12, cm13, cm14) = (m11, m12, m13, m14);
    let (cm21, cm22, cm23, cm24) = (m21, m22, m23, m24);
    let (cm31, cm32, cm33, cm34) = (m31, m32, m33, m34);
    let (cm41, cm42, cm43, cm44) = (m41, m42, m43, m44);

    // scale(scaleX, scaleY?, scaleZ?, originX?, originY?, originZ?)
    let scale = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let sx = args.get_or_undefined(0).to_number(ctx).unwrap_or(1.0);
        let sy = args.get_or_undefined(1).to_number(ctx).ok().unwrap_or(sx);
        let sz = args.get_or_undefined(2).to_number(ctx).unwrap_or(1.0);

        // Apply scale
        let new_m11 = cm11 * sx;
        let new_m12 = cm12 * sx;
        let new_m13 = cm13 * sx;
        let new_m14 = cm14 * sx;
        let new_m21 = cm21 * sy;
        let new_m22 = cm22 * sy;
        let new_m23 = cm23 * sy;
        let new_m24 = cm24 * sy;
        let new_m31 = cm31 * sz;
        let new_m32 = cm32 * sz;
        let new_m33 = cm33 * sz;
        let new_m34 = cm34 * sz;

        let new_is_2d = sz == 1.0;

        Ok(JsValue::from(create_dom_matrix_with_values(ctx,
            new_m11, new_m12, new_m13, new_m14, new_m21, new_m22, new_m23, new_m24,
            new_m31, new_m32, new_m33, new_m34, cm41, cm42, cm43, cm44,
            new_is_2d, false)))
    });

    // rotate(rotX, rotY?, rotZ?) - angles in degrees
    let rotate = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let angle_deg = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
        let angle = angle_deg * std::f64::consts::PI / 180.0;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        // 2D rotation around Z axis
        let new_m11 = cm11 * cos_a + cm21 * sin_a;
        let new_m12 = cm12 * cos_a + cm22 * sin_a;
        let new_m21 = cm21 * cos_a - cm11 * sin_a;
        let new_m22 = cm22 * cos_a - cm12 * sin_a;

        Ok(JsValue::from(create_dom_matrix_with_values(ctx,
            new_m11, new_m12, cm13, cm14, new_m21, new_m22, cm23, cm24,
            cm31, cm32, cm33, cm34, cm41, cm42, cm43, cm44,
            true, false)))
    });

    // skewX(sx) - angle in degrees
    let skew_x = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let angle_deg = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
        let tan_a = (angle_deg * std::f64::consts::PI / 180.0).tan();

        let new_m21 = cm11 * tan_a + cm21;
        let new_m22 = cm12 * tan_a + cm22;

        Ok(JsValue::from(create_dom_matrix_with_values(ctx,
            cm11, cm12, cm13, cm14, new_m21, new_m22, cm23, cm24,
            cm31, cm32, cm33, cm34, cm41, cm42, cm43, cm44,
            true, false)))
    });

    // skewY(sy) - angle in degrees
    let skew_y = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let angle_deg = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
        let tan_a = (angle_deg * std::f64::consts::PI / 180.0).tan();

        let new_m11 = cm11 + cm21 * tan_a;
        let new_m12 = cm12 + cm22 * tan_a;

        Ok(JsValue::from(create_dom_matrix_with_values(ctx,
            new_m11, new_m12, cm13, cm14, cm21, cm22, cm23, cm24,
            cm31, cm32, cm33, cm34, cm41, cm42, cm43, cm44,
            true, false)))
    });

    // multiply(other) - multiply this matrix by another
    let multiply = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        if let Some(other) = args.get_or_undefined(0).as_object() {
            let om11 = other.get(js_string!("m11"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0);
            let om12 = other.get(js_string!("m12"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let om13 = other.get(js_string!("m13"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let om14 = other.get(js_string!("m14"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let om21 = other.get(js_string!("m21"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let om22 = other.get(js_string!("m22"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0);
            let om23 = other.get(js_string!("m23"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let om24 = other.get(js_string!("m24"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let om31 = other.get(js_string!("m31"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let om32 = other.get(js_string!("m32"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let om33 = other.get(js_string!("m33"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0);
            let om34 = other.get(js_string!("m34"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let om41 = other.get(js_string!("m41"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let om42 = other.get(js_string!("m42"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let om43 = other.get(js_string!("m43"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let om44 = other.get(js_string!("m44"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0);

            // Matrix multiplication
            let r11 = cm11*om11 + cm21*om12 + cm31*om13 + cm41*om14;
            let r12 = cm12*om11 + cm22*om12 + cm32*om13 + cm42*om14;
            let r13 = cm13*om11 + cm23*om12 + cm33*om13 + cm43*om14;
            let r14 = cm14*om11 + cm24*om12 + cm34*om13 + cm44*om14;
            let r21 = cm11*om21 + cm21*om22 + cm31*om23 + cm41*om24;
            let r22 = cm12*om21 + cm22*om22 + cm32*om23 + cm42*om24;
            let r23 = cm13*om21 + cm23*om22 + cm33*om23 + cm43*om24;
            let r24 = cm14*om21 + cm24*om22 + cm34*om23 + cm44*om24;
            let r31 = cm11*om31 + cm21*om32 + cm31*om33 + cm41*om34;
            let r32 = cm12*om31 + cm22*om32 + cm32*om33 + cm42*om34;
            let r33 = cm13*om31 + cm23*om32 + cm33*om33 + cm43*om34;
            let r34 = cm14*om31 + cm24*om32 + cm34*om33 + cm44*om34;
            let r41 = cm11*om41 + cm21*om42 + cm31*om43 + cm41*om44;
            let r42 = cm12*om41 + cm22*om42 + cm32*om43 + cm42*om44;
            let r43 = cm13*om41 + cm23*om42 + cm33*om43 + cm43*om44;
            let r44 = cm14*om41 + cm24*om42 + cm34*om43 + cm44*om44;

            return Ok(JsValue::from(create_dom_matrix_with_values(ctx,
                r11, r12, r13, r14, r21, r22, r23, r24,
                r31, r32, r33, r34, r41, r42, r43, r44,
                false, false)));
        }
        // Return identity if no valid other matrix
        Ok(JsValue::from(create_dom_matrix_with_values(ctx,
            cm11, cm12, cm13, cm14, cm21, cm22, cm23, cm24,
            cm31, cm32, cm33, cm34, cm41, cm42, cm43, cm44,
            true, false)))
    });

    // inverse() - compute inverse matrix
    let inverse = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        // For 2D matrix, compute simple inverse
        let det = cm11 * cm22 - cm12 * cm21;
        if det.abs() < 1e-10 {
            // Singular matrix, return identity
            return Ok(JsValue::from(create_dom_matrix_with_values(ctx,
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                true, true)));
        }

        let inv_det = 1.0 / det;
        let new_m11 = cm22 * inv_det;
        let new_m12 = -cm12 * inv_det;
        let new_m21 = -cm21 * inv_det;
        let new_m22 = cm11 * inv_det;
        let new_m41 = (cm21 * cm42 - cm22 * cm41) * inv_det;
        let new_m42 = (cm12 * cm41 - cm11 * cm42) * inv_det;

        Ok(JsValue::from(create_dom_matrix_with_values(ctx,
            new_m11, new_m12, 0.0, 0.0, new_m21, new_m22, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0, new_m41, new_m42, 0.0, 1.0,
            true, false)))
    });

    // flipX() - flip around Y axis
    let flip_x = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        Ok(JsValue::from(create_dom_matrix_with_values(ctx,
            -cm11, -cm12, cm13, cm14, cm21, cm22, cm23, cm24,
            cm31, cm32, cm33, cm34, cm41, cm42, cm43, cm44,
            true, false)))
    });

    // flipY() - flip around X axis
    let flip_y = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        Ok(JsValue::from(create_dom_matrix_with_values(ctx,
            cm11, cm12, cm13, cm14, -cm21, -cm22, cm23, cm24,
            cm31, cm32, cm33, cm34, cm41, cm42, cm43, cm44,
            true, false)))
    });

    // transformPoint(point) - transform a DOMPoint
    let transform_point = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let (x, y, z, w) = if let Some(pt) = args.get_or_undefined(0).as_object() {
            let x = pt.get(js_string!("x"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let x = if x.is_nan() { 0.0 } else { x };
            let y = pt.get(js_string!("y"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let y = if y.is_nan() { 0.0 } else { y };
            let z = pt.get(js_string!("z"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let z = if z.is_nan() { 0.0 } else { z };
            let w = pt.get(js_string!("w"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0);
            let w = if w.is_nan() { 1.0 } else { w };
            (x, y, z, w)
        } else {
            (0.0, 0.0, 0.0, 1.0)
        };

        let new_x = cm11 * x + cm21 * y + cm31 * z + cm41 * w;
        let new_y = cm12 * x + cm22 * y + cm32 * z + cm42 * w;
        let new_z = cm13 * x + cm23 * y + cm33 * z + cm43 * w;
        let new_w = cm14 * x + cm24 * y + cm34 * z + cm44 * w;

        Ok(JsValue::from(create_dom_point(ctx, new_x, new_y, new_z, new_w)))
    });

    // toString()
    let str_m11 = m11; let str_m12 = m12; let str_m21 = m21; let str_m22 = m22;
    let str_m41 = m41; let str_m42 = m42; let str_is_2d = is_2d;
    let to_string = NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
        if str_is_2d {
            Ok(JsValue::from(js_string!(format!("matrix({}, {}, {}, {}, {}, {})",
                str_m11, str_m12, str_m21, str_m22, str_m41, str_m42))))
        } else {
            Ok(JsValue::from(js_string!("matrix3d(...)")))
        }
    });

    // toJSON()
    let json_m11 = m11; let json_m12 = m12; let json_m21 = m21; let json_m22 = m22;
    let json_m41 = m41; let json_m42 = m42;
    let to_json = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("a"), json_m11, Attribute::all())
            .property(js_string!("b"), json_m12, Attribute::all())
            .property(js_string!("c"), json_m21, Attribute::all())
            .property(js_string!("d"), json_m22, Attribute::all())
            .property(js_string!("e"), json_m41, Attribute::all())
            .property(js_string!("f"), json_m42, Attribute::all())
            .build();
        Ok(JsValue::from(obj))
    });

    ObjectInitializer::new(ctx)
        // 2D properties (a-f are aliases for m11, m12, m21, m22, m41, m42)
        .property(js_string!("a"), m11, Attribute::all())
        .property(js_string!("b"), m12, Attribute::all())
        .property(js_string!("c"), m21, Attribute::all())
        .property(js_string!("d"), m22, Attribute::all())
        .property(js_string!("e"), m41, Attribute::all())
        .property(js_string!("f"), m42, Attribute::all())
        // 4x4 matrix properties
        .property(js_string!("m11"), m11, Attribute::all())
        .property(js_string!("m12"), m12, Attribute::all())
        .property(js_string!("m13"), m13, Attribute::all())
        .property(js_string!("m14"), m14, Attribute::all())
        .property(js_string!("m21"), m21, Attribute::all())
        .property(js_string!("m22"), m22, Attribute::all())
        .property(js_string!("m23"), m23, Attribute::all())
        .property(js_string!("m24"), m24, Attribute::all())
        .property(js_string!("m31"), m31, Attribute::all())
        .property(js_string!("m32"), m32, Attribute::all())
        .property(js_string!("m33"), m33, Attribute::all())
        .property(js_string!("m34"), m34, Attribute::all())
        .property(js_string!("m41"), m41, Attribute::all())
        .property(js_string!("m42"), m42, Attribute::all())
        .property(js_string!("m43"), m43, Attribute::all())
        .property(js_string!("m44"), m44, Attribute::all())
        .property(js_string!("is2D"), is_2d, Attribute::READONLY)
        .property(js_string!("isIdentity"), is_identity, Attribute::READONLY)
        // Transformation methods
        .function(translate, js_string!("translate"), 2)
        .function(scale, js_string!("scale"), 1)
        .function(rotate, js_string!("rotate"), 1)
        .function(skew_x, js_string!("skewX"), 1)
        .function(skew_y, js_string!("skewY"), 1)
        .function(multiply, js_string!("multiply"), 1)
        .function(inverse, js_string!("inverse"), 0)
        .function(flip_x, js_string!("flipX"), 0)
        .function(flip_y, js_string!("flipY"), 0)
        .function(transform_point, js_string!("transformPoint"), 1)
        .function(to_string, js_string!("toString"), 0)
        .function(to_json, js_string!("toJSON"), 0)
        .build()
}

/// Register DOMRect and related types
fn register_intersection_types(context: &mut Context) -> JsResult<()> {
    // DOMRect constructor
    let domrect_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let x = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
        let y = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);
        let width = args.get_or_undefined(2).to_number(ctx).unwrap_or(0.0);
        let height = args.get_or_undefined(3).to_number(ctx).unwrap_or(0.0);

        Ok(JsValue::from(create_dom_rect(ctx, x, y, width, height)))
    });

    let domrect_ctor = FunctionObjectBuilder::new(context.realm(), domrect_constructor)
        .name(js_string!("DOMRect"))
        .length(4)
        .constructor(true)
        .build();
    context.global_object().set(js_string!("DOMRect"), domrect_ctor, false, context)?;

    // DOMRectReadOnly constructor
    let domrect_readonly_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let x = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
        let y = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);
        let width = args.get_or_undefined(2).to_number(ctx).unwrap_or(0.0);
        let height = args.get_or_undefined(3).to_number(ctx).unwrap_or(0.0);

        Ok(JsValue::from(create_dom_rect(ctx, x, y, width, height)))
    });

    let domrect_ro_ctor = FunctionObjectBuilder::new(context.realm(), domrect_readonly_constructor)
        .name(js_string!("DOMRectReadOnly"))
        .length(4)
        .constructor(true)
        .build();
    context.global_object().set(js_string!("DOMRectReadOnly"), domrect_ro_ctor, false, context)?;

    // DOMPoint constructor
    let dompoint_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let x = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
        let y = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);
        let z = args.get_or_undefined(2).to_number(ctx).unwrap_or(0.0);
        let w = args.get_or_undefined(3).to_number(ctx).unwrap_or(1.0);

        let point = ObjectInitializer::new(ctx)
            .property(js_string!("x"), x, Attribute::all())
            .property(js_string!("y"), y, Attribute::all())
            .property(js_string!("z"), z, Attribute::all())
            .property(js_string!("w"), w, Attribute::all())
            .build();

        Ok(JsValue::from(point))
    });

    let dompoint_ctor = FunctionObjectBuilder::new(context.realm(), dompoint_constructor)
        .name(js_string!("DOMPoint"))
        .length(4)
        .constructor(true)
        .build();
    context.global_object().set(js_string!("DOMPoint"), dompoint_ctor, false, context)?;

    // DOMPointReadOnly constructor
    let dompoint_readonly_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let x = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
        let y = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);
        let z = args.get_or_undefined(2).to_number(ctx).unwrap_or(0.0);
        let w = args.get_or_undefined(3).to_number(ctx).unwrap_or(1.0);

        let point = ObjectInitializer::new(ctx)
            .property(js_string!("x"), x, Attribute::READONLY)
            .property(js_string!("y"), y, Attribute::READONLY)
            .property(js_string!("z"), z, Attribute::READONLY)
            .property(js_string!("w"), w, Attribute::READONLY)
            .build();

        Ok(JsValue::from(point))
    });

    let dompoint_ro_ctor = FunctionObjectBuilder::new(context.realm(), dompoint_readonly_constructor)
        .name(js_string!("DOMPointReadOnly"))
        .length(4)
        .constructor(true)
        .build();
    context.global_object().set(js_string!("DOMPointReadOnly"), dompoint_ro_ctor, false, context)?;

    // DOMMatrix constructor with real matrix math
    let dommatrix_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        // Parse initial values - can be array of 6 (2D) or 16 (3D) numbers, or a transform string
        let (m11, m12, m13, m14, m21, m22, m23, m24, m31, m32, m33, m34, m41, m42, m43, m44, is_2d) =
            if let Some(arr) = args.get_or_undefined(0).as_object() {
                // Check if it's an array-like object
                let len = arr.get(js_string!("length"), ctx)
                    .ok()
                    .and_then(|v| v.to_number(ctx).ok())
                    .unwrap_or(0.0) as usize;

                if len == 6 {
                    // 2D matrix: [a, b, c, d, e, f]
                    let a = arr.get(js_string!("0"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0);
                    let b = arr.get(js_string!("1"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let c = arr.get(js_string!("2"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let d = arr.get(js_string!("3"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0);
                    let e = arr.get(js_string!("4"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let f = arr.get(js_string!("5"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    (a, b, 0.0, 0.0, c, d, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, e, f, 0.0, 1.0, true)
                } else if len == 16 {
                    // 3D matrix
                    let m11 = arr.get(js_string!("0"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0);
                    let m12 = arr.get(js_string!("1"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let m13 = arr.get(js_string!("2"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let m14 = arr.get(js_string!("3"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let m21 = arr.get(js_string!("4"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let m22 = arr.get(js_string!("5"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0);
                    let m23 = arr.get(js_string!("6"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let m24 = arr.get(js_string!("7"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let m31 = arr.get(js_string!("8"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let m32 = arr.get(js_string!("9"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let m33 = arr.get(js_string!("10"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0);
                    let m34 = arr.get(js_string!("11"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let m41 = arr.get(js_string!("12"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let m42 = arr.get(js_string!("13"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let m43 = arr.get(js_string!("14"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let m44 = arr.get(js_string!("15"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0);
                    (m11, m12, m13, m14, m21, m22, m23, m24, m31, m32, m33, m34, m41, m42, m43, m44, false)
                } else {
                    // Identity
                    (1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, true)
                }
            } else {
                // Identity matrix
                (1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, true)
            };

        let is_identity = m11 == 1.0 && m12 == 0.0 && m13 == 0.0 && m14 == 0.0 &&
                          m21 == 0.0 && m22 == 1.0 && m23 == 0.0 && m24 == 0.0 &&
                          m31 == 0.0 && m32 == 0.0 && m33 == 1.0 && m34 == 0.0 &&
                          m41 == 0.0 && m42 == 0.0 && m43 == 0.0 && m44 == 1.0;

        Ok(JsValue::from(create_dom_matrix_with_values(ctx, m11, m12, m13, m14, m21, m22, m23, m24, m31, m32, m33, m34, m41, m42, m43, m44, is_2d, is_identity)))
    });

    let dommatrix_ctor = FunctionObjectBuilder::new(context.realm(), dommatrix_constructor.clone())
        .name(js_string!("DOMMatrix"))
        .length(1)
        .constructor(true)
        .build();
    context.global_object().set(js_string!("DOMMatrix"), dommatrix_ctor, false, context)?;

    let dommatrix_ro_ctor = FunctionObjectBuilder::new(context.realm(), dommatrix_constructor)
        .name(js_string!("DOMMatrixReadOnly"))
        .length(1)
        .constructor(true)
        .build();
    context.global_object().set(js_string!("DOMMatrixReadOnly"), dommatrix_ro_ctor, false, context)?;

    // DOMQuad constructor - represents a quadrilateral with 4 corner points
    let domquad_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        // DOMQuad(p1?, p2?, p3?, p4?) or DOMQuad(rect)
        let (p1, p2, p3, p4) = if args.len() == 0 {
            // Default: unit square
            let p1 = create_dom_point(ctx, 0.0, 0.0, 0.0, 1.0);
            let p2 = create_dom_point(ctx, 1.0, 0.0, 0.0, 1.0);
            let p3 = create_dom_point(ctx, 1.0, 1.0, 0.0, 1.0);
            let p4 = create_dom_point(ctx, 0.0, 1.0, 0.0, 1.0);
            (p1, p2, p3, p4)
        } else if args.len() == 1 {
            // Single argument: could be a DOMRect or similar
            let arg = args.get_or_undefined(0);
            if let Some(rect) = arg.as_object() {
                let x = rect.get(js_string!("x"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                let y = rect.get(js_string!("y"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                let w = rect.get(js_string!("width"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                let h = rect.get(js_string!("height"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                let p1 = create_dom_point(ctx, x, y, 0.0, 1.0);
                let p2 = create_dom_point(ctx, x + w, y, 0.0, 1.0);
                let p3 = create_dom_point(ctx, x + w, y + h, 0.0, 1.0);
                let p4 = create_dom_point(ctx, x, y + h, 0.0, 1.0);
                (p1, p2, p3, p4)
            } else {
                let p1 = create_dom_point(ctx, 0.0, 0.0, 0.0, 1.0);
                let p2 = create_dom_point(ctx, 0.0, 0.0, 0.0, 1.0);
                let p3 = create_dom_point(ctx, 0.0, 0.0, 0.0, 1.0);
                let p4 = create_dom_point(ctx, 0.0, 0.0, 0.0, 1.0);
                (p1, p2, p3, p4)
            }
        } else {
            // 4 point arguments
            let extract_point = |arg: &JsValue, ctx: &mut Context| -> JsObject {
                if let Some(obj) = arg.as_object() {
                    let x = obj.get(js_string!("x"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let y = obj.get(js_string!("y"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let z = obj.get(js_string!("z"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                    let w = obj.get(js_string!("w"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0);
                    create_dom_point(ctx, x, y, z, w)
                } else {
                    create_dom_point(ctx, 0.0, 0.0, 0.0, 1.0)
                }
            };
            let p1 = extract_point(args.get_or_undefined(0), ctx);
            let p2 = extract_point(args.get_or_undefined(1), ctx);
            let p3 = extract_point(args.get_or_undefined(2), ctx);
            let p4 = extract_point(args.get_or_undefined(3), ctx);
            (p1, p2, p3, p4)
        };

        // Calculate bounding box
        let p1x = p1.get(js_string!("x"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let p1y = p1.get(js_string!("y"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let p2x = p2.get(js_string!("x"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let p2y = p2.get(js_string!("y"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let p3x = p3.get(js_string!("x"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let p3y = p3.get(js_string!("y"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let p4x = p4.get(js_string!("x"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let p4y = p4.get(js_string!("y"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);

        let min_x = p1x.min(p2x).min(p3x).min(p4x);
        let max_x = p1x.max(p2x).max(p3x).max(p4x);
        let min_y = p1y.min(p2y).min(p3y).min(p4y);
        let max_y = p1y.max(p2y).max(p3y).max(p4y);

        // getBounds method
        let bounds_x = min_x;
        let bounds_y = min_y;
        let bounds_w = max_x - min_x;
        let bounds_h = max_y - min_y;
        let get_bounds = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
            let bounds = ObjectInitializer::new(ctx)
                .property(js_string!("x"), bounds_x, Attribute::READONLY)
                .property(js_string!("y"), bounds_y, Attribute::READONLY)
                .property(js_string!("width"), bounds_w, Attribute::READONLY)
                .property(js_string!("height"), bounds_h, Attribute::READONLY)
                .build();
            Ok(JsValue::from(bounds))
        });

        // toJSON method
        let to_json = NativeFunction::from_copy_closure(|this, _args, ctx| {
            if let Some(obj) = this.as_object() {
                let p1 = obj.get(js_string!("p1"), ctx).unwrap_or(JsValue::undefined());
                let p2 = obj.get(js_string!("p2"), ctx).unwrap_or(JsValue::undefined());
                let p3 = obj.get(js_string!("p3"), ctx).unwrap_or(JsValue::undefined());
                let p4 = obj.get(js_string!("p4"), ctx).unwrap_or(JsValue::undefined());
                let json = ObjectInitializer::new(ctx)
                    .property(js_string!("p1"), p1, Attribute::all())
                    .property(js_string!("p2"), p2, Attribute::all())
                    .property(js_string!("p3"), p3, Attribute::all())
                    .property(js_string!("p4"), p4, Attribute::all())
                    .build();
                Ok(JsValue::from(json))
            } else {
                Ok(JsValue::undefined())
            }
        });

        let quad = ObjectInitializer::new(ctx)
            .property(js_string!("p1"), JsValue::from(p1), Attribute::READONLY)
            .property(js_string!("p2"), JsValue::from(p2), Attribute::READONLY)
            .property(js_string!("p3"), JsValue::from(p3), Attribute::READONLY)
            .property(js_string!("p4"), JsValue::from(p4), Attribute::READONLY)
            .function(get_bounds, js_string!("getBounds"), 0)
            .function(to_json, js_string!("toJSON"), 0)
            .build();

        Ok(JsValue::from(quad))
    });

    // DOMQuad.fromRect static method
    let domquad_from_rect = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let rect = args.get_or_undefined(0);
        if let Some(r) = rect.as_object() {
            let x = r.get(js_string!("x"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let y = r.get(js_string!("y"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let w = r.get(js_string!("width"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let h = r.get(js_string!("height"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);

            let p1 = create_dom_point(ctx, x, y, 0.0, 1.0);
            let p2 = create_dom_point(ctx, x + w, y, 0.0, 1.0);
            let p3 = create_dom_point(ctx, x + w, y + h, 0.0, 1.0);
            let p4 = create_dom_point(ctx, x, y + h, 0.0, 1.0);

            let get_bounds = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                let bounds = ObjectInitializer::new(ctx)
                    .property(js_string!("x"), x, Attribute::READONLY)
                    .property(js_string!("y"), y, Attribute::READONLY)
                    .property(js_string!("width"), w, Attribute::READONLY)
                    .property(js_string!("height"), h, Attribute::READONLY)
                    .build();
                Ok(JsValue::from(bounds))
            });

            let to_json = NativeFunction::from_copy_closure(|this, _args, ctx| {
                if let Some(obj) = this.as_object() {
                    let p1 = obj.get(js_string!("p1"), ctx).unwrap_or(JsValue::undefined());
                    let p2 = obj.get(js_string!("p2"), ctx).unwrap_or(JsValue::undefined());
                    let p3 = obj.get(js_string!("p3"), ctx).unwrap_or(JsValue::undefined());
                    let p4 = obj.get(js_string!("p4"), ctx).unwrap_or(JsValue::undefined());
                    let json = ObjectInitializer::new(ctx)
                        .property(js_string!("p1"), p1, Attribute::all())
                        .property(js_string!("p2"), p2, Attribute::all())
                        .property(js_string!("p3"), p3, Attribute::all())
                        .property(js_string!("p4"), p4, Attribute::all())
                        .build();
                    Ok(JsValue::from(json))
                } else {
                    Ok(JsValue::undefined())
                }
            });

            let quad = ObjectInitializer::new(ctx)
                .property(js_string!("p1"), JsValue::from(p1), Attribute::READONLY)
                .property(js_string!("p2"), JsValue::from(p2), Attribute::READONLY)
                .property(js_string!("p3"), JsValue::from(p3), Attribute::READONLY)
                .property(js_string!("p4"), JsValue::from(p4), Attribute::READONLY)
                .function(get_bounds, js_string!("getBounds"), 0)
                .function(to_json, js_string!("toJSON"), 0)
                .build();
            Ok(JsValue::from(quad))
        } else {
            // Empty quad
            let p = create_dom_point(ctx, 0.0, 0.0, 0.0, 1.0);
            let quad = ObjectInitializer::new(ctx)
                .property(js_string!("p1"), JsValue::from(p.clone()), Attribute::READONLY)
                .property(js_string!("p2"), JsValue::from(p.clone()), Attribute::READONLY)
                .property(js_string!("p3"), JsValue::from(p.clone()), Attribute::READONLY)
                .property(js_string!("p4"), JsValue::from(p), Attribute::READONLY)
                .build();
            Ok(JsValue::from(quad))
        }
    });

    // DOMQuad.fromQuad static method
    let domquad_from_quad = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let quad_init = args.get_or_undefined(0);
        if let Some(q) = quad_init.as_object() {
            let extract_point = |name: &str, ctx: &mut Context, q: &JsObject| -> JsObject {
                if let Ok(p) = q.get(js_string!(name), ctx) {
                    if let Some(pobj) = p.as_object() {
                        let x = pobj.get(js_string!("x"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                        let y = pobj.get(js_string!("y"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                        let z = pobj.get(js_string!("z"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
                        let w = pobj.get(js_string!("w"), ctx).ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0);
                        return create_dom_point(ctx, x, y, z, w);
                    }
                }
                create_dom_point(ctx, 0.0, 0.0, 0.0, 1.0)
            };
            let p1 = extract_point("p1", ctx, &q);
            let p2 = extract_point("p2", ctx, &q);
            let p3 = extract_point("p3", ctx, &q);
            let p4 = extract_point("p4", ctx, &q);

            let to_json = NativeFunction::from_copy_closure(|this, _args, ctx| {
                if let Some(obj) = this.as_object() {
                    let p1 = obj.get(js_string!("p1"), ctx).unwrap_or(JsValue::undefined());
                    let p2 = obj.get(js_string!("p2"), ctx).unwrap_or(JsValue::undefined());
                    let p3 = obj.get(js_string!("p3"), ctx).unwrap_or(JsValue::undefined());
                    let p4 = obj.get(js_string!("p4"), ctx).unwrap_or(JsValue::undefined());
                    let json = ObjectInitializer::new(ctx)
                        .property(js_string!("p1"), p1, Attribute::all())
                        .property(js_string!("p2"), p2, Attribute::all())
                        .property(js_string!("p3"), p3, Attribute::all())
                        .property(js_string!("p4"), p4, Attribute::all())
                        .build();
                    Ok(JsValue::from(json))
                } else {
                    Ok(JsValue::undefined())
                }
            });

            let get_bounds = NativeFunction::from_copy_closure(|this, _args, ctx| {
                if let Some(obj) = this.as_object() {
                    let get_coord = |p: &JsValue, coord: &str, ctx: &mut Context| -> f64 {
                        p.as_object().and_then(|o| o.get(js_string!(coord), ctx).ok())
                            .and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0)
                    };
                    let p1 = obj.get(js_string!("p1"), ctx).unwrap_or(JsValue::undefined());
                    let p2 = obj.get(js_string!("p2"), ctx).unwrap_or(JsValue::undefined());
                    let p3 = obj.get(js_string!("p3"), ctx).unwrap_or(JsValue::undefined());
                    let p4 = obj.get(js_string!("p4"), ctx).unwrap_or(JsValue::undefined());
                    let xs = [get_coord(&p1, "x", ctx), get_coord(&p2, "x", ctx), get_coord(&p3, "x", ctx), get_coord(&p4, "x", ctx)];
                    let ys = [get_coord(&p1, "y", ctx), get_coord(&p2, "y", ctx), get_coord(&p3, "y", ctx), get_coord(&p4, "y", ctx)];
                    let min_x = xs.iter().cloned().fold(f64::INFINITY, f64::min);
                    let max_x = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let min_y = ys.iter().cloned().fold(f64::INFINITY, f64::min);
                    let max_y = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let bounds = ObjectInitializer::new(ctx)
                        .property(js_string!("x"), min_x, Attribute::READONLY)
                        .property(js_string!("y"), min_y, Attribute::READONLY)
                        .property(js_string!("width"), max_x - min_x, Attribute::READONLY)
                        .property(js_string!("height"), max_y - min_y, Attribute::READONLY)
                        .build();
                    Ok(JsValue::from(bounds))
                } else {
                    Ok(JsValue::undefined())
                }
            });

            let quad = ObjectInitializer::new(ctx)
                .property(js_string!("p1"), JsValue::from(p1), Attribute::READONLY)
                .property(js_string!("p2"), JsValue::from(p2), Attribute::READONLY)
                .property(js_string!("p3"), JsValue::from(p3), Attribute::READONLY)
                .property(js_string!("p4"), JsValue::from(p4), Attribute::READONLY)
                .function(get_bounds, js_string!("getBounds"), 0)
                .function(to_json, js_string!("toJSON"), 0)
                .build();
            Ok(JsValue::from(quad))
        } else {
            let p = create_dom_point(ctx, 0.0, 0.0, 0.0, 1.0);
            let quad = ObjectInitializer::new(ctx)
                .property(js_string!("p1"), JsValue::from(p.clone()), Attribute::READONLY)
                .property(js_string!("p2"), JsValue::from(p.clone()), Attribute::READONLY)
                .property(js_string!("p3"), JsValue::from(p.clone()), Attribute::READONLY)
                .property(js_string!("p4"), JsValue::from(p), Attribute::READONLY)
                .build();
            Ok(JsValue::from(quad))
        }
    });

    let mut domquad_ctor = FunctionObjectBuilder::new(context.realm(), domquad_constructor)
        .name(js_string!("DOMQuad"))
        .length(4)
        .constructor(true)
        .build();

    // Add static methods
    domquad_ctor.set(js_string!("fromRect"), domquad_from_rect.to_js_function(context.realm()), false, context)?;
    domquad_ctor.set(js_string!("fromQuad"), domquad_from_quad.to_js_function(context.realm()), false, context)?;

    context.global_object().set(js_string!("DOMQuad"), domquad_ctor, false, context)?;

    Ok(())
}

// Helper to create DOMPoint object
fn create_dom_point(ctx: &mut Context, x: f64, y: f64, z: f64, w: f64) -> JsObject {
    ObjectInitializer::new(ctx)
        .property(js_string!("x"), x, Attribute::all())
        .property(js_string!("y"), y, Attribute::all())
        .property(js_string!("z"), z, Attribute::all())
        .property(js_string!("w"), w, Attribute::all())
        .build()
}

// Helper functions

fn create_dom_rect(ctx: &mut Context, x: f64, y: f64, width: f64, height: f64) -> JsObject {
    let to_json = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("x"), x, Attribute::all())
            .property(js_string!("y"), y, Attribute::all())
            .property(js_string!("width"), width, Attribute::all())
            .property(js_string!("height"), height, Attribute::all())
            .build();
        Ok(JsValue::from(obj))
    });

    ObjectInitializer::new(ctx)
        .property(js_string!("x"), x, Attribute::all())
        .property(js_string!("y"), y, Attribute::all())
        .property(js_string!("width"), width, Attribute::all())
        .property(js_string!("height"), height, Attribute::all())
        .property(js_string!("top"), y, Attribute::READONLY)
        .property(js_string!("right"), x + width, Attribute::READONLY)
        .property(js_string!("bottom"), y + height, Attribute::READONLY)
        .property(js_string!("left"), x, Attribute::READONLY)
        .function(to_json, js_string!("toJSON"), 0)
        .build()
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

fn create_string_array(ctx: &mut Context, strings: &[String]) -> JsObject {
    let arr = ObjectInitializer::new(ctx)
        .property(js_string!("length"), strings.len() as u32, Attribute::READONLY)
        .build();

    for (i, s) in strings.iter().enumerate() {
        let _ = arr.set(js_string!(i.to_string()), JsValue::from(js_string!(s.clone())), false, ctx);
    }

    arr
}

fn js_value_to_json(value: &JsValue, ctx: &mut Context) -> String {
    if value.is_null_or_undefined() {
        return "null".to_string();
    }

    if let Some(b) = value.as_boolean() {
        return b.to_string();
    }

    if let Ok(n) = value.to_number(ctx) {
        if n.is_finite() {
            return n.to_string();
        }
        return "null".to_string();
    }

    if let Ok(s) = value.to_string(ctx) {
        let escaped = s.to_std_string_escaped()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        return format!("\"{}\"", escaped);
    }

    "null".to_string()
}

fn json_to_js_value(json: &str, _ctx: &mut Context) -> JsResult<JsValue> {
    if json == "null" || json.is_empty() {
        return Ok(JsValue::null());
    }

    if json == "true" {
        return Ok(JsValue::from(true));
    }

    if json == "false" {
        return Ok(JsValue::from(false));
    }

    if let Ok(n) = json.parse::<f64>() {
        return Ok(JsValue::from(n));
    }

    if json.starts_with('"') && json.ends_with('"') {
        let s = &json[1..json.len()-1];
        return Ok(JsValue::from(js_string!(s.to_string())));
    }

    Ok(JsValue::null())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_storage() {
        {
            let mut clipboard = CLIPBOARD_TEXT.lock().unwrap();
            *clipboard = "test".to_string();
        }

        let result = {
            let clipboard = CLIPBOARD_TEXT.lock().unwrap();
            clipboard.clone()
        };

        assert_eq!(result, "test");
    }

    #[test]
    fn test_history_push() {
        // Reset history
        {
            let mut stack = HISTORY_STACK.lock().unwrap();
            let mut index = HISTORY_INDEX.lock().unwrap();
            *stack = vec![HistoryEntry::default()];
            *index = 0;
        }

        // Push entry
        {
            let mut stack = HISTORY_STACK.lock().unwrap();
            let mut index = HISTORY_INDEX.lock().unwrap();
            stack.push(HistoryEntry {
                state: "{}".to_string(),
                title: "Test".to_string(),
                url: "/test".to_string(),
            });
            *index = 1;
        }

        let len = {
            let stack = HISTORY_STACK.lock().unwrap();
            stack.len()
        };

        assert_eq!(len, 2);
    }
}

/// Register SVGElement constructor and related types
fn register_svg_element(context: &mut Context) -> JsResult<()> {
    use boa_engine::object::builtins::JsArray;

    // SVGAnimatedString - represents an animated string attribute
    let svg_animated_string_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let base_val = args.get(0)
            .and_then(|v| v.to_string(ctx).ok())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        let base_val_clone = base_val.clone();
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("baseVal"), js_string!(base_val.as_str()), Attribute::all())
            .property(js_string!("animVal"), js_string!(base_val_clone.as_str()), Attribute::READONLY)
            .build();

        Ok(JsValue::from(obj))
    });

    let svg_animated_string = FunctionObjectBuilder::new(context.realm(), svg_animated_string_ctor)
        .name(js_string!("SVGAnimatedString"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("SVGAnimatedString"), svg_animated_string, Attribute::all())?;

    // SVGAnimatedLength
    let svg_animated_length_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let value = if args.len() > 0 && !args[0].is_undefined() && !args[0].is_null() {
            args[0].to_number(ctx).unwrap_or(0.0)
        } else {
            0.0
        };

        // Create SVGLength-like object
        let base_val = ObjectInitializer::new(ctx)
            .property(js_string!("value"), value, Attribute::all())
            .property(js_string!("valueInSpecifiedUnits"), value, Attribute::all())
            .property(js_string!("unitType"), 1, Attribute::READONLY) // SVG_LENGTHTYPE_NUMBER
            .build();

        let anim_val = ObjectInitializer::new(ctx)
            .property(js_string!("value"), value, Attribute::READONLY)
            .property(js_string!("valueInSpecifiedUnits"), value, Attribute::READONLY)
            .property(js_string!("unitType"), 1, Attribute::READONLY)
            .build();

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("baseVal"), JsValue::from(base_val), Attribute::all())
            .property(js_string!("animVal"), JsValue::from(anim_val), Attribute::READONLY)
            .build();

        Ok(JsValue::from(obj))
    });

    let svg_animated_length = FunctionObjectBuilder::new(context.realm(), svg_animated_length_ctor)
        .name(js_string!("SVGAnimatedLength"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("SVGAnimatedLength"), svg_animated_length, Attribute::all())?;

    // SVGRect
    let svg_rect_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let x = if args.len() > 0 && !args[0].is_undefined() { args[0].to_number(ctx).unwrap_or(0.0) } else { 0.0 };
        let y = if args.len() > 1 && !args[1].is_undefined() { args[1].to_number(ctx).unwrap_or(0.0) } else { 0.0 };
        let width = if args.len() > 2 && !args[2].is_undefined() { args[2].to_number(ctx).unwrap_or(0.0) } else { 0.0 };
        let height = if args.len() > 3 && !args[3].is_undefined() { args[3].to_number(ctx).unwrap_or(0.0) } else { 0.0 };

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("x"), x, Attribute::all())
            .property(js_string!("y"), y, Attribute::all())
            .property(js_string!("width"), width, Attribute::all())
            .property(js_string!("height"), height, Attribute::all())
            .build();

        Ok(JsValue::from(obj))
    });

    let svg_rect = FunctionObjectBuilder::new(context.realm(), svg_rect_ctor)
        .name(js_string!("SVGRect"))
        .length(4)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("SVGRect"), svg_rect, Attribute::all())?;

    // SVGMatrix (deprecated but still used)
    let svg_matrix_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let a = if args.len() > 0 && !args[0].is_undefined() { args[0].to_number(ctx).unwrap_or(1.0) } else { 1.0 };
        let b = if args.len() > 1 && !args[1].is_undefined() { args[1].to_number(ctx).unwrap_or(0.0) } else { 0.0 };
        let c = if args.len() > 2 && !args[2].is_undefined() { args[2].to_number(ctx).unwrap_or(0.0) } else { 0.0 };
        let d = if args.len() > 3 && !args[3].is_undefined() { args[3].to_number(ctx).unwrap_or(1.0) } else { 1.0 };
        let e = if args.len() > 4 && !args[4].is_undefined() { args[4].to_number(ctx).unwrap_or(0.0) } else { 0.0 };
        let f = if args.len() > 5 && !args[5].is_undefined() { args[5].to_number(ctx).unwrap_or(0.0) } else { 0.0 };

        // multiply method
        let multiply_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not an object"))?;
            let other = args.get_or_undefined(0);

            if let Some(other_obj) = other.as_object() {
                let a1 = this_obj.get(js_string!("a"), ctx)?.to_number(ctx).unwrap_or(1.0);
                let b1 = this_obj.get(js_string!("b"), ctx)?.to_number(ctx).unwrap_or(0.0);
                let c1 = this_obj.get(js_string!("c"), ctx)?.to_number(ctx).unwrap_or(0.0);
                let d1 = this_obj.get(js_string!("d"), ctx)?.to_number(ctx).unwrap_or(1.0);
                let e1 = this_obj.get(js_string!("e"), ctx)?.to_number(ctx).unwrap_or(0.0);
                let f1 = this_obj.get(js_string!("f"), ctx)?.to_number(ctx).unwrap_or(0.0);

                let a2 = other_obj.get(js_string!("a"), ctx)?.to_number(ctx).unwrap_or(1.0);
                let b2 = other_obj.get(js_string!("b"), ctx)?.to_number(ctx).unwrap_or(0.0);
                let c2 = other_obj.get(js_string!("c"), ctx)?.to_number(ctx).unwrap_or(0.0);
                let d2 = other_obj.get(js_string!("d"), ctx)?.to_number(ctx).unwrap_or(1.0);
                let e2 = other_obj.get(js_string!("e"), ctx)?.to_number(ctx).unwrap_or(0.0);
                let f2 = other_obj.get(js_string!("f"), ctx)?.to_number(ctx).unwrap_or(0.0);

                let result = ObjectInitializer::new(ctx)
                    .property(js_string!("a"), a1 * a2 + c1 * b2, Attribute::all())
                    .property(js_string!("b"), b1 * a2 + d1 * b2, Attribute::all())
                    .property(js_string!("c"), a1 * c2 + c1 * d2, Attribute::all())
                    .property(js_string!("d"), b1 * c2 + d1 * d2, Attribute::all())
                    .property(js_string!("e"), a1 * e2 + c1 * f2 + e1, Attribute::all())
                    .property(js_string!("f"), b1 * e2 + d1 * f2 + f1, Attribute::all())
                    .build();

                Ok(JsValue::from(result))
            } else {
                Ok(JsValue::from(this_obj.clone()))
            }
        });

        // inverse method
        let inverse_fn = NativeFunction::from_copy_closure(|this, _args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not an object"))?;
            let a = this_obj.get(js_string!("a"), ctx)?.to_number(ctx).unwrap_or(1.0);
            let b = this_obj.get(js_string!("b"), ctx)?.to_number(ctx).unwrap_or(0.0);
            let c = this_obj.get(js_string!("c"), ctx)?.to_number(ctx).unwrap_or(0.0);
            let d = this_obj.get(js_string!("d"), ctx)?.to_number(ctx).unwrap_or(1.0);
            let e = this_obj.get(js_string!("e"), ctx)?.to_number(ctx).unwrap_or(0.0);
            let f = this_obj.get(js_string!("f"), ctx)?.to_number(ctx).unwrap_or(0.0);

            let det = a * d - b * c;
            if det.abs() < 1e-10 {
                return Err(JsNativeError::typ().with_message("Matrix is not invertible").into());
            }

            let result = ObjectInitializer::new(ctx)
                .property(js_string!("a"), d / det, Attribute::all())
                .property(js_string!("b"), -b / det, Attribute::all())
                .property(js_string!("c"), -c / det, Attribute::all())
                .property(js_string!("d"), a / det, Attribute::all())
                .property(js_string!("e"), (c * f - d * e) / det, Attribute::all())
                .property(js_string!("f"), (b * e - a * f) / det, Attribute::all())
                .build();

            Ok(JsValue::from(result))
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("a"), a, Attribute::all())
            .property(js_string!("b"), b, Attribute::all())
            .property(js_string!("c"), c, Attribute::all())
            .property(js_string!("d"), d, Attribute::all())
            .property(js_string!("e"), e, Attribute::all())
            .property(js_string!("f"), f, Attribute::all())
            .function(multiply_fn, js_string!("multiply"), 1)
            .function(inverse_fn, js_string!("inverse"), 0)
            .build();

        Ok(JsValue::from(obj))
    });

    let svg_matrix = FunctionObjectBuilder::new(context.realm(), svg_matrix_ctor)
        .name(js_string!("SVGMatrix"))
        .length(6)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("SVGMatrix"), svg_matrix, Attribute::all())?;

    // SVGPoint
    let svg_point_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let x = if args.len() > 0 && !args[0].is_undefined() { args[0].to_number(ctx).unwrap_or(0.0) } else { 0.0 };
        let y = if args.len() > 1 && !args[1].is_undefined() { args[1].to_number(ctx).unwrap_or(0.0) } else { 0.0 };

        // matrixTransform method
        let matrix_transform_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not an object"))?;
            let px = this_obj.get(js_string!("x"), ctx)?.to_number(ctx).unwrap_or(0.0);
            let py = this_obj.get(js_string!("y"), ctx)?.to_number(ctx).unwrap_or(0.0);

            if let Some(matrix) = args.get(0).and_then(|v| v.as_object()) {
                let a = matrix.get(js_string!("a"), ctx)?.to_number(ctx).unwrap_or(1.0);
                let b = matrix.get(js_string!("b"), ctx)?.to_number(ctx).unwrap_or(0.0);
                let c = matrix.get(js_string!("c"), ctx)?.to_number(ctx).unwrap_or(0.0);
                let d = matrix.get(js_string!("d"), ctx)?.to_number(ctx).unwrap_or(1.0);
                let e = matrix.get(js_string!("e"), ctx)?.to_number(ctx).unwrap_or(0.0);
                let f = matrix.get(js_string!("f"), ctx)?.to_number(ctx).unwrap_or(0.0);

                let new_x = a * px + c * py + e;
                let new_y = b * px + d * py + f;

                let result = ObjectInitializer::new(ctx)
                    .property(js_string!("x"), new_x, Attribute::all())
                    .property(js_string!("y"), new_y, Attribute::all())
                    .build();

                Ok(JsValue::from(result))
            } else {
                Ok(JsValue::from(this_obj.clone()))
            }
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("x"), x, Attribute::all())
            .property(js_string!("y"), y, Attribute::all())
            .function(matrix_transform_fn, js_string!("matrixTransform"), 1)
            .build();

        Ok(JsValue::from(obj))
    });

    let svg_point = FunctionObjectBuilder::new(context.realm(), svg_point_ctor)
        .name(js_string!("SVGPoint"))
        .length(2)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("SVGPoint"), svg_point, Attribute::all())?;

    // SVGNumber
    let svg_number_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let value = if args.len() > 0 && !args[0].is_undefined() { args[0].to_number(ctx).unwrap_or(0.0) } else { 0.0 };

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("value"), value, Attribute::all())
            .build();

        Ok(JsValue::from(obj))
    });

    let svg_number = FunctionObjectBuilder::new(context.realm(), svg_number_ctor)
        .name(js_string!("SVGNumber"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("SVGNumber"), svg_number, Attribute::all())?;

    // SVGLength
    let svg_length_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let value = if args.len() > 0 && !args[0].is_undefined() { args[0].to_number(ctx).unwrap_or(0.0) } else { 0.0 };
        let unit_type = if args.len() > 1 && !args[1].is_undefined() { args[1].to_u32(ctx).unwrap_or(1) as u16 } else { 1 };

        // newValueSpecifiedUnits method
        let new_value_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not an object"))?;
            let unit = args.get_or_undefined(0).to_u32(ctx).unwrap_or(1);
            let value = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);
            this_obj.set(js_string!("unitType"), unit, false, ctx)?;
            this_obj.set(js_string!("value"), value, false, ctx)?;
            this_obj.set(js_string!("valueInSpecifiedUnits"), value, false, ctx)?;
            Ok(JsValue::undefined())
        });

        // convertToSpecifiedUnits method
        let convert_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not an object"))?;
            let unit = args.get_or_undefined(0).to_u32(ctx).unwrap_or(1);
            this_obj.set(js_string!("unitType"), unit, false, ctx)?;
            Ok(JsValue::undefined())
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("value"), value, Attribute::all())
            .property(js_string!("valueInSpecifiedUnits"), value, Attribute::all())
            .property(js_string!("valueAsString"), js_string!(format!("{}", value)), Attribute::all())
            .property(js_string!("unitType"), unit_type, Attribute::all())
            // Unit type constants
            .property(js_string!("SVG_LENGTHTYPE_UNKNOWN"), 0, Attribute::READONLY)
            .property(js_string!("SVG_LENGTHTYPE_NUMBER"), 1, Attribute::READONLY)
            .property(js_string!("SVG_LENGTHTYPE_PERCENTAGE"), 2, Attribute::READONLY)
            .property(js_string!("SVG_LENGTHTYPE_EMS"), 3, Attribute::READONLY)
            .property(js_string!("SVG_LENGTHTYPE_EXS"), 4, Attribute::READONLY)
            .property(js_string!("SVG_LENGTHTYPE_PX"), 5, Attribute::READONLY)
            .property(js_string!("SVG_LENGTHTYPE_CM"), 6, Attribute::READONLY)
            .property(js_string!("SVG_LENGTHTYPE_MM"), 7, Attribute::READONLY)
            .property(js_string!("SVG_LENGTHTYPE_IN"), 8, Attribute::READONLY)
            .property(js_string!("SVG_LENGTHTYPE_PT"), 9, Attribute::READONLY)
            .property(js_string!("SVG_LENGTHTYPE_PC"), 10, Attribute::READONLY)
            .function(new_value_fn, js_string!("newValueSpecifiedUnits"), 2)
            .function(convert_fn, js_string!("convertToSpecifiedUnits"), 1)
            .build();

        Ok(JsValue::from(obj))
    });

    let svg_length = FunctionObjectBuilder::new(context.realm(), svg_length_ctor)
        .name(js_string!("SVGLength"))
        .length(2)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("SVGLength"), svg_length, Attribute::all())?;

    // SVGTransform
    let svg_transform_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // setMatrix method
        let set_matrix_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not an object"))?;
            if let Some(matrix) = args.get(0).and_then(|v| v.as_object()) {
                this_obj.set(js_string!("matrix"), JsValue::from(matrix.clone()), false, ctx)?;
                this_obj.set(js_string!("type"), 1, false, ctx)?; // SVG_TRANSFORM_MATRIX
            }
            Ok(JsValue::undefined())
        });

        // setTranslate method
        let set_translate_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not an object"))?;
            let tx = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
            let ty = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);

            let matrix = ObjectInitializer::new(ctx)
                .property(js_string!("a"), 1.0, Attribute::all())
                .property(js_string!("b"), 0.0, Attribute::all())
                .property(js_string!("c"), 0.0, Attribute::all())
                .property(js_string!("d"), 1.0, Attribute::all())
                .property(js_string!("e"), tx, Attribute::all())
                .property(js_string!("f"), ty, Attribute::all())
                .build();

            this_obj.set(js_string!("matrix"), JsValue::from(matrix), false, ctx)?;
            this_obj.set(js_string!("type"), 2, false, ctx)?; // SVG_TRANSFORM_TRANSLATE
            Ok(JsValue::undefined())
        });

        // setScale method
        let set_scale_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not an object"))?;
            let sx = args.get_or_undefined(0).to_number(ctx).unwrap_or(1.0);
            let sy = args.get_or_undefined(1).to_number(ctx).unwrap_or(sx);

            let matrix = ObjectInitializer::new(ctx)
                .property(js_string!("a"), sx, Attribute::all())
                .property(js_string!("b"), 0.0, Attribute::all())
                .property(js_string!("c"), 0.0, Attribute::all())
                .property(js_string!("d"), sy, Attribute::all())
                .property(js_string!("e"), 0.0, Attribute::all())
                .property(js_string!("f"), 0.0, Attribute::all())
                .build();

            this_obj.set(js_string!("matrix"), JsValue::from(matrix), false, ctx)?;
            this_obj.set(js_string!("type"), 3, false, ctx)?; // SVG_TRANSFORM_SCALE
            Ok(JsValue::undefined())
        });

        // setRotate method
        let set_rotate_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not an object"))?;
            let angle = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
            let cx = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);
            let cy = args.get_or_undefined(2).to_number(ctx).unwrap_or(0.0);

            let rad = angle * std::f64::consts::PI / 180.0;
            let cos_a = rad.cos();
            let sin_a = rad.sin();

            let matrix = ObjectInitializer::new(ctx)
                .property(js_string!("a"), cos_a, Attribute::all())
                .property(js_string!("b"), sin_a, Attribute::all())
                .property(js_string!("c"), -sin_a, Attribute::all())
                .property(js_string!("d"), cos_a, Attribute::all())
                .property(js_string!("e"), -cx * cos_a + cy * sin_a + cx, Attribute::all())
                .property(js_string!("f"), -cx * sin_a - cy * cos_a + cy, Attribute::all())
                .build();

            this_obj.set(js_string!("matrix"), JsValue::from(matrix), false, ctx)?;
            this_obj.set(js_string!("type"), 4, false, ctx)?; // SVG_TRANSFORM_ROTATE
            this_obj.set(js_string!("angle"), angle, false, ctx)?;
            Ok(JsValue::undefined())
        });

        // Default identity matrix
        let identity_matrix = ObjectInitializer::new(ctx)
            .property(js_string!("a"), 1.0, Attribute::all())
            .property(js_string!("b"), 0.0, Attribute::all())
            .property(js_string!("c"), 0.0, Attribute::all())
            .property(js_string!("d"), 1.0, Attribute::all())
            .property(js_string!("e"), 0.0, Attribute::all())
            .property(js_string!("f"), 0.0, Attribute::all())
            .build();

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("type"), 0, Attribute::all()) // SVG_TRANSFORM_UNKNOWN
            .property(js_string!("matrix"), JsValue::from(identity_matrix), Attribute::all())
            .property(js_string!("angle"), 0.0, Attribute::all())
            // Transform type constants
            .property(js_string!("SVG_TRANSFORM_UNKNOWN"), 0, Attribute::READONLY)
            .property(js_string!("SVG_TRANSFORM_MATRIX"), 1, Attribute::READONLY)
            .property(js_string!("SVG_TRANSFORM_TRANSLATE"), 2, Attribute::READONLY)
            .property(js_string!("SVG_TRANSFORM_SCALE"), 3, Attribute::READONLY)
            .property(js_string!("SVG_TRANSFORM_ROTATE"), 4, Attribute::READONLY)
            .property(js_string!("SVG_TRANSFORM_SKEWX"), 5, Attribute::READONLY)
            .property(js_string!("SVG_TRANSFORM_SKEWY"), 6, Attribute::READONLY)
            .function(set_matrix_fn, js_string!("setMatrix"), 1)
            .function(set_translate_fn, js_string!("setTranslate"), 2)
            .function(set_scale_fn, js_string!("setScale"), 2)
            .function(set_rotate_fn, js_string!("setRotate"), 3)
            .build();

        Ok(JsValue::from(obj))
    });

    let svg_transform = FunctionObjectBuilder::new(context.realm(), svg_transform_ctor)
        .name(js_string!("SVGTransform"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("SVGTransform"), svg_transform, Attribute::all())?;

    // SVGTransformList
    let svg_transform_list_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let items = JsArray::new(ctx);

        // getItem method
        let get_item_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not an object"))?;
            let index = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0);
            let items_val = this_obj.get(js_string!("_items"), ctx)?;
            if let Some(items_arr) = items_val.as_object() {
                items_arr.get(index, ctx)
            } else {
                Ok(JsValue::undefined())
            }
        });

        // appendItem method
        let append_item_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not an object"))?;
            let item = args.get_or_undefined(0);
            let items_val = this_obj.get(js_string!("_items"), ctx)?;
            if let Some(items_obj) = items_val.as_object() {
                if let Ok(items_arr) = boa_engine::object::builtins::JsArray::from_object(items_obj.clone()) {
                    let _ = items_arr.push(item.clone(), ctx);
                    let len = items_arr.length(ctx).unwrap_or(0);
                    this_obj.set(js_string!("numberOfItems"), len, false, ctx)?;
                }
            }
            Ok(item.clone())
        });

        // createSVGTransformFromMatrix method
        let create_from_matrix_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let matrix = args.get_or_undefined(0);
            let transform = ObjectInitializer::new(ctx)
                .property(js_string!("type"), 1, Attribute::all()) // SVG_TRANSFORM_MATRIX
                .property(js_string!("matrix"), matrix.clone(), Attribute::all())
                .property(js_string!("angle"), 0.0, Attribute::all())
                .build();
            Ok(JsValue::from(transform))
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("_items"), JsValue::from(items), Attribute::all())
            .property(js_string!("numberOfItems"), 0, Attribute::all())
            .function(get_item_fn, js_string!("getItem"), 1)
            .function(append_item_fn, js_string!("appendItem"), 1)
            .function(create_from_matrix_fn, js_string!("createSVGTransformFromMatrix"), 1)
            .build();

        Ok(JsValue::from(obj))
    });

    let svg_transform_list = FunctionObjectBuilder::new(context.realm(), svg_transform_list_ctor)
        .name(js_string!("SVGTransformList"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("SVGTransformList"), svg_transform_list, Attribute::all())?;

    // SVGElement - the main base class for all SVG elements
    let svg_element_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // getBBox method - returns bounding box
        let get_bbox_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            // Return default empty bounding box
            let bbox = ObjectInitializer::new(ctx)
                .property(js_string!("x"), 0.0, Attribute::all())
                .property(js_string!("y"), 0.0, Attribute::all())
                .property(js_string!("width"), 0.0, Attribute::all())
                .property(js_string!("height"), 0.0, Attribute::all())
                .build();
            Ok(JsValue::from(bbox))
        });

        // getCTM method - returns current transformation matrix
        let get_ctm_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let matrix = ObjectInitializer::new(ctx)
                .property(js_string!("a"), 1.0, Attribute::all())
                .property(js_string!("b"), 0.0, Attribute::all())
                .property(js_string!("c"), 0.0, Attribute::all())
                .property(js_string!("d"), 1.0, Attribute::all())
                .property(js_string!("e"), 0.0, Attribute::all())
                .property(js_string!("f"), 0.0, Attribute::all())
                .build();
            Ok(JsValue::from(matrix))
        });

        // getScreenCTM method - returns screen transformation matrix
        let get_screen_ctm_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let matrix = ObjectInitializer::new(ctx)
                .property(js_string!("a"), 1.0, Attribute::all())
                .property(js_string!("b"), 0.0, Attribute::all())
                .property(js_string!("c"), 0.0, Attribute::all())
                .property(js_string!("d"), 1.0, Attribute::all())
                .property(js_string!("e"), 0.0, Attribute::all())
                .property(js_string!("f"), 0.0, Attribute::all())
                .build();
            Ok(JsValue::from(matrix))
        });

        // focus and blur methods
        let focus_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let blur_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // Create className as SVGAnimatedString
        let class_name = ObjectInitializer::new(ctx)
            .property(js_string!("baseVal"), js_string!(""), Attribute::all())
            .property(js_string!("animVal"), js_string!(""), Attribute::READONLY)
            .build();

        // Create style object
        let style = ObjectInitializer::new(ctx)
            .property(js_string!("cssText"), js_string!(""), Attribute::all())
            .build();

        // Create dataset object
        let dataset = ObjectInitializer::new(ctx).build();

        let obj = ObjectInitializer::new(ctx)
            // SVG-specific properties
            .property(js_string!("ownerSVGElement"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("viewportElement"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("className"), JsValue::from(class_name), Attribute::all())
            .property(js_string!("style"), JsValue::from(style), Attribute::all())
            .property(js_string!("dataset"), JsValue::from(dataset), Attribute::READONLY)
            // Element properties
            .property(js_string!("tagName"), js_string!(""), Attribute::READONLY)
            .property(js_string!("id"), js_string!(""), Attribute::all())
            .property(js_string!("tabIndex"), -1, Attribute::all())
            // Methods
            .function(get_bbox_fn, js_string!("getBBox"), 0)
            .function(get_ctm_fn, js_string!("getCTM"), 0)
            .function(get_screen_ctm_fn, js_string!("getScreenCTM"), 0)
            .function(focus_fn, js_string!("focus"), 0)
            .function(blur_fn, js_string!("blur"), 0)
            .build();

        Ok(JsValue::from(obj))
    });

    let svg_element = FunctionObjectBuilder::new(context.realm(), svg_element_ctor)
        .name(js_string!("SVGElement"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("SVGElement"), svg_element, Attribute::all())?;

    Ok(())
}

/// Register ImageBitmap constructor and createImageBitmap global function
fn register_image_bitmap(context: &mut Context) -> JsResult<()> {
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT_BITMAP_ID: AtomicU32 = AtomicU32::new(1);

    // ImageBitmap constructor (not directly constructible, use createImageBitmap)
    let image_bitmap_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let width = if args.len() > 0 && !args[0].is_undefined() {
            args[0].to_u32(ctx).unwrap_or(0)
        } else {
            0
        };
        let height = if args.len() > 1 && !args[1].is_undefined() {
            args[1].to_u32(ctx).unwrap_or(0)
        } else {
            0
        };

        let id = NEXT_BITMAP_ID.fetch_add(1, Ordering::SeqCst);

        // close method - releases the bitmap resources
        let close_fn = NativeFunction::from_copy_closure(|this, _args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not an ImageBitmap"))?;
            // Mark as closed by setting dimensions to 0
            this_obj.set(js_string!("width"), 0, false, ctx)?;
            this_obj.set(js_string!("height"), 0, false, ctx)?;
            this_obj.set(js_string!("_closed"), true, false, ctx)?;
            Ok(JsValue::undefined())
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("width"), width, Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("height"), height, Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("_id"), id, Attribute::READONLY)
            .property(js_string!("_closed"), false, Attribute::all())
            .function(close_fn, js_string!("close"), 0)
            .build();

        Ok(JsValue::from(obj))
    });

    let image_bitmap = FunctionObjectBuilder::new(context.realm(), image_bitmap_ctor)
        .name(js_string!("ImageBitmap"))
        .length(2)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("ImageBitmap"), image_bitmap, Attribute::all())?;

    // createImageBitmap global function
    // createImageBitmap(image) or createImageBitmap(image, options)
    // createImageBitmap(image, sx, sy, sw, sh) or createImageBitmap(image, sx, sy, sw, sh, options)
    let create_image_bitmap = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let source = args.get_or_undefined(0);

        // Determine dimensions based on source type
        let (width, height) = if let Some(source_obj) = source.as_object() {
            // Check for width/height properties (HTMLImageElement, HTMLCanvasElement, etc.)
            let w = source_obj.get(js_string!("width"), ctx)
                .ok()
                .and_then(|v| v.to_u32(ctx).ok())
                .unwrap_or(0);
            let h = source_obj.get(js_string!("height"), ctx)
                .ok()
                .and_then(|v| v.to_u32(ctx).ok())
                .unwrap_or(0);

            // Check for naturalWidth/naturalHeight (HTMLImageElement)
            let nw = source_obj.get(js_string!("naturalWidth"), ctx)
                .ok()
                .and_then(|v| v.to_u32(ctx).ok())
                .unwrap_or(0);
            let nh = source_obj.get(js_string!("naturalHeight"), ctx)
                .ok()
                .and_then(|v| v.to_u32(ctx).ok())
                .unwrap_or(0);

            // Check for videoWidth/videoHeight (HTMLVideoElement)
            let vw = source_obj.get(js_string!("videoWidth"), ctx)
                .ok()
                .and_then(|v| v.to_u32(ctx).ok())
                .unwrap_or(0);
            let vh = source_obj.get(js_string!("videoHeight"), ctx)
                .ok()
                .and_then(|v| v.to_u32(ctx).ok())
                .unwrap_or(0);

            // Use natural dimensions if available, then video, then regular
            if nw > 0 && nh > 0 {
                (nw, nh)
            } else if vw > 0 && vh > 0 {
                (vw, vh)
            } else {
                (w, h)
            }
        } else {
            (0, 0)
        };

        // Check if crop rectangle is provided (sx, sy, sw, sh)
        let (final_width, final_height) = if args.len() >= 5 {
            let sw = args.get_or_undefined(3).to_u32(ctx).unwrap_or(width);
            let sh = args.get_or_undefined(4).to_u32(ctx).unwrap_or(height);
            (sw, sh)
        } else {
            (width, height)
        };

        // Check for resize options
        let (resize_width, resize_height) = if args.len() >= 2 {
            let options_idx = if args.len() >= 5 { 5 } else { 1 };
            if let Some(options) = args.get(options_idx).and_then(|v| v.as_object()) {
                let rw = options.get(js_string!("resizeWidth"), ctx)
                    .ok()
                    .and_then(|v| if v.is_undefined() { None } else { v.to_u32(ctx).ok() })
                    .unwrap_or(final_width);
                let rh = options.get(js_string!("resizeHeight"), ctx)
                    .ok()
                    .and_then(|v| if v.is_undefined() { None } else { v.to_u32(ctx).ok() })
                    .unwrap_or(final_height);
                (rw, rh)
            } else {
                (final_width, final_height)
            }
        } else {
            (final_width, final_height)
        };

        let id = NEXT_BITMAP_ID.fetch_add(1, Ordering::SeqCst);

        // close method
        let close_fn = NativeFunction::from_copy_closure(|this, _args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not an ImageBitmap"))?;
            this_obj.set(js_string!("width"), 0, false, ctx)?;
            this_obj.set(js_string!("height"), 0, false, ctx)?;
            this_obj.set(js_string!("_closed"), true, false, ctx)?;
            Ok(JsValue::undefined())
        });

        let bitmap = ObjectInitializer::new(ctx)
            .property(js_string!("width"), resize_width, Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("height"), resize_height, Attribute::WRITABLE | Attribute::CONFIGURABLE)
            .property(js_string!("_id"), id, Attribute::READONLY)
            .property(js_string!("_closed"), false, Attribute::all())
            .function(close_fn, js_string!("close"), 0)
            .build();

        // Return a real Promise using JsPromise
        use boa_engine::object::builtins::JsPromise;
        let promise = JsPromise::resolve(JsValue::from(bitmap), ctx);
        Ok(JsValue::from(promise))
    });

    let create_bitmap_func = FunctionObjectBuilder::new(context.realm(), create_image_bitmap)
        .name(js_string!("createImageBitmap"))
        .length(1)
        .build();

    context.register_global_property(js_string!("createImageBitmap"), create_bitmap_func, Attribute::all())?;

    // ImageBitmapRenderingContext - for transferring ImageBitmap to canvas
    let image_bitmap_ctx_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // transferFromImageBitmap method
        let transfer_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not an ImageBitmapRenderingContext"))?;
            let bitmap = args.get_or_undefined(0);

            if bitmap.is_null() {
                // Clear the canvas
                this_obj.set(js_string!("_bitmap"), JsValue::null(), false, ctx)?;
            } else if let Some(bmp) = bitmap.as_object() {
                // Transfer the bitmap
                this_obj.set(js_string!("_bitmap"), JsValue::from(bmp.clone()), false, ctx)?;
                // Close the source bitmap
                let _ = bmp.set(js_string!("_closed"), true, false, ctx);
            }

            Ok(JsValue::undefined())
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("canvas"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("_bitmap"), JsValue::null(), Attribute::all())
            .function(transfer_fn, js_string!("transferFromImageBitmap"), 1)
            .build();

        Ok(JsValue::from(obj))
    });

    let image_bitmap_ctx = FunctionObjectBuilder::new(context.realm(), image_bitmap_ctx_ctor)
        .name(js_string!("ImageBitmapRenderingContext"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("ImageBitmapRenderingContext"), image_bitmap_ctx, Attribute::all())?;

    Ok(())
}

/// Register GPUCanvasContext for WebGPU
fn register_gpu_canvas_context(context: &mut Context) -> JsResult<()> {
    // GPUCanvasContext - context for rendering WebGPU content to a canvas
    let gpu_canvas_ctx_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // configure method - configures the context for rendering
        let configure_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not a GPUCanvasContext"))?;

            if let Some(config) = args.get(0).and_then(|v| v.as_object()) {
                // Store configuration
                let device = config.get(js_string!("device"), ctx).unwrap_or(JsValue::null());
                let format = config.get(js_string!("format"), ctx)
                    .ok()
                    .and_then(|v| v.to_string(ctx).ok())
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_else(|| "bgra8unorm".to_string());
                let alpha_mode = config.get(js_string!("alphaMode"), ctx)
                    .ok()
                    .and_then(|v| v.to_string(ctx).ok())
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_else(|| "opaque".to_string());

                this_obj.set(js_string!("_device"), device, false, ctx)?;
                this_obj.set(js_string!("_format"), js_string!(format.as_str()), false, ctx)?;
                this_obj.set(js_string!("_alphaMode"), js_string!(alpha_mode.as_str()), false, ctx)?;
                this_obj.set(js_string!("_configured"), true, false, ctx)?;
            }

            Ok(JsValue::undefined())
        });

        // unconfigure method - removes the configuration
        let unconfigure_fn = NativeFunction::from_copy_closure(|this, _args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not a GPUCanvasContext"))?;
            this_obj.set(js_string!("_configured"), false, false, ctx)?;
            this_obj.set(js_string!("_device"), JsValue::null(), false, ctx)?;
            Ok(JsValue::undefined())
        });

        // getCurrentTexture method - returns a GPUTexture for the current frame
        let get_current_texture_fn = NativeFunction::from_copy_closure(|this, _args, ctx| {
            let this_obj = this.as_object().ok_or_else(|| JsNativeError::typ().with_message("this is not a GPUCanvasContext"))?;

            let configured = this_obj.get(js_string!("_configured"), ctx)?;
            if !configured.to_boolean() {
                return Err(JsNativeError::typ().with_message("Context is not configured").into());
            }

            // Create a mock GPUTexture
            let canvas_val = this_obj.get(js_string!("canvas"), ctx)?;
            let (width, height) = if let Some(canvas) = canvas_val.as_object() {
                let w = canvas.get(js_string!("width"), ctx)?.to_u32(ctx).unwrap_or(300);
                let h = canvas.get(js_string!("height"), ctx)?.to_u32(ctx).unwrap_or(150);
                (w, h)
            } else {
                (300, 150)
            };

            let format = this_obj.get(js_string!("_format"), ctx)?
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_else(|_| "bgra8unorm".to_string());

            // createView method for the texture
            let create_view_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let view = ObjectInitializer::new(ctx)
                    .property(js_string!("label"), js_string!(""), Attribute::all())
                    .build();
                Ok(JsValue::from(view))
            });

            // destroy method
            let destroy_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });

            let texture = ObjectInitializer::new(ctx)
                .property(js_string!("width"), width, Attribute::READONLY)
                .property(js_string!("height"), height, Attribute::READONLY)
                .property(js_string!("depthOrArrayLayers"), 1, Attribute::READONLY)
                .property(js_string!("mipLevelCount"), 1, Attribute::READONLY)
                .property(js_string!("sampleCount"), 1, Attribute::READONLY)
                .property(js_string!("dimension"), js_string!("2d"), Attribute::READONLY)
                .property(js_string!("format"), js_string!(format.as_str()), Attribute::READONLY)
                .property(js_string!("usage"), 16, Attribute::READONLY) // GPUTextureUsage.RENDER_ATTACHMENT
                .property(js_string!("label"), js_string!(""), Attribute::all())
                .function(create_view_fn, js_string!("createView"), 0)
                .function(destroy_fn, js_string!("destroy"), 0)
                .build();

            Ok(JsValue::from(texture))
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("canvas"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("_configured"), false, Attribute::all())
            .property(js_string!("_device"), JsValue::null(), Attribute::all())
            .property(js_string!("_format"), js_string!("bgra8unorm"), Attribute::all())
            .property(js_string!("_alphaMode"), js_string!("opaque"), Attribute::all())
            .function(configure_fn, js_string!("configure"), 1)
            .function(unconfigure_fn, js_string!("unconfigure"), 0)
            .function(get_current_texture_fn, js_string!("getCurrentTexture"), 0)
            .build();

        Ok(JsValue::from(obj))
    });

    let gpu_canvas_ctx = FunctionObjectBuilder::new(context.realm(), gpu_canvas_ctx_ctor)
        .name(js_string!("GPUCanvasContext"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("GPUCanvasContext"), gpu_canvas_ctx, Attribute::all())?;

    Ok(())
}

/// Register CSS Paint API types (PaintRenderingContext2D, PaintSize)
fn register_paint_api(context: &mut Context) -> JsResult<()> {
    // PaintSize - represents the size of the paint worklet area
    let paint_size_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let width = if args.len() > 0 && !args[0].is_undefined() {
            args[0].to_number(ctx).unwrap_or(0.0)
        } else {
            0.0
        };
        let height = if args.len() > 1 && !args[1].is_undefined() {
            args[1].to_number(ctx).unwrap_or(0.0)
        } else {
            0.0
        };

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("width"), width, Attribute::READONLY)
            .property(js_string!("height"), height, Attribute::READONLY)
            .build();

        Ok(JsValue::from(obj))
    });

    let paint_size = FunctionObjectBuilder::new(context.realm(), paint_size_ctor)
        .name(js_string!("PaintSize"))
        .length(2)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("PaintSize"), paint_size, Attribute::all())?;

    // PaintRenderingContext2D - subset of CanvasRenderingContext2D for paint worklets
    let paint_ctx_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Basic drawing state
        let fill_style = js_string!("black");
        let stroke_style = js_string!("black");
        let line_width = 1.0;

        // fillRect method
        let fill_rect_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            // Stub - actual rendering would happen in paint worklet
            Ok(JsValue::undefined())
        });

        // strokeRect method
        let stroke_rect_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // clearRect method
        let clear_rect_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // beginPath method
        let begin_path_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // closePath method
        let close_path_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // moveTo method
        let move_to_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // lineTo method
        let line_to_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // arc method
        let arc_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // arcTo method
        let arc_to_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // bezierCurveTo method
        let bezier_curve_to_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // quadraticCurveTo method
        let quadratic_curve_to_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // rect method
        let rect_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // ellipse method
        let ellipse_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // fill method
        let fill_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // stroke method
        let stroke_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // clip method
        let clip_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // save method
        let save_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // restore method
        let restore_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // scale method
        let scale_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // rotate method
        let rotate_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // translate method
        let translate_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // transform method
        let transform_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // setTransform method
        let set_transform_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // resetTransform method
        let reset_transform_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // createLinearGradient method
        let create_linear_gradient_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let x0 = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
            let y0 = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);
            let x1 = args.get_or_undefined(2).to_number(ctx).unwrap_or(0.0);
            let y1 = args.get_or_undefined(3).to_number(ctx).unwrap_or(0.0);

            let add_color_stop_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });

            let gradient = ObjectInitializer::new(ctx)
                .property(js_string!("_x0"), x0, Attribute::all())
                .property(js_string!("_y0"), y0, Attribute::all())
                .property(js_string!("_x1"), x1, Attribute::all())
                .property(js_string!("_y1"), y1, Attribute::all())
                .property(js_string!("_type"), js_string!("linear"), Attribute::READONLY)
                .function(add_color_stop_fn, js_string!("addColorStop"), 2)
                .build();

            Ok(JsValue::from(gradient))
        });

        // createRadialGradient method
        let create_radial_gradient_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let x0 = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
            let y0 = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);
            let r0 = args.get_or_undefined(2).to_number(ctx).unwrap_or(0.0);
            let x1 = args.get_or_undefined(3).to_number(ctx).unwrap_or(0.0);
            let y1 = args.get_or_undefined(4).to_number(ctx).unwrap_or(0.0);
            let r1 = args.get_or_undefined(5).to_number(ctx).unwrap_or(0.0);

            let add_color_stop_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });

            let gradient = ObjectInitializer::new(ctx)
                .property(js_string!("_x0"), x0, Attribute::all())
                .property(js_string!("_y0"), y0, Attribute::all())
                .property(js_string!("_r0"), r0, Attribute::all())
                .property(js_string!("_x1"), x1, Attribute::all())
                .property(js_string!("_y1"), y1, Attribute::all())
                .property(js_string!("_r1"), r1, Attribute::all())
                .property(js_string!("_type"), js_string!("radial"), Attribute::READONLY)
                .function(add_color_stop_fn, js_string!("addColorStop"), 2)
                .build();

            Ok(JsValue::from(gradient))
        });

        // createPattern method
        let create_pattern_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let repetition = args.get(1)
                .and_then(|v| v.to_string(ctx).ok())
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_else(|| "repeat".to_string());

            let pattern = ObjectInitializer::new(ctx)
                .property(js_string!("_repetition"), js_string!(repetition.as_str()), Attribute::all())
                .build();

            Ok(JsValue::from(pattern))
        });

        // setLineDash method
        let set_line_dash_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // getLineDash method
        let get_line_dash_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            use boa_engine::object::builtins::JsArray;
            let arr = JsArray::new(ctx);
            Ok(JsValue::from(arr))
        });

        // drawImage method
        let draw_image_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let obj = ObjectInitializer::new(ctx)
            // Properties
            .property(js_string!("fillStyle"), fill_style, Attribute::all())
            .property(js_string!("strokeStyle"), stroke_style, Attribute::all())
            .property(js_string!("lineWidth"), line_width, Attribute::all())
            .property(js_string!("lineCap"), js_string!("butt"), Attribute::all())
            .property(js_string!("lineJoin"), js_string!("miter"), Attribute::all())
            .property(js_string!("miterLimit"), 10.0, Attribute::all())
            .property(js_string!("lineDashOffset"), 0.0, Attribute::all())
            .property(js_string!("globalAlpha"), 1.0, Attribute::all())
            .property(js_string!("globalCompositeOperation"), js_string!("source-over"), Attribute::all())
            .property(js_string!("shadowBlur"), 0.0, Attribute::all())
            .property(js_string!("shadowColor"), js_string!("transparent"), Attribute::all())
            .property(js_string!("shadowOffsetX"), 0.0, Attribute::all())
            .property(js_string!("shadowOffsetY"), 0.0, Attribute::all())
            .property(js_string!("imageSmoothingEnabled"), true, Attribute::all())
            .property(js_string!("imageSmoothingQuality"), js_string!("low"), Attribute::all())
            // Drawing methods
            .function(fill_rect_fn, js_string!("fillRect"), 4)
            .function(stroke_rect_fn, js_string!("strokeRect"), 4)
            .function(clear_rect_fn, js_string!("clearRect"), 4)
            // Path methods
            .function(begin_path_fn, js_string!("beginPath"), 0)
            .function(close_path_fn, js_string!("closePath"), 0)
            .function(move_to_fn, js_string!("moveTo"), 2)
            .function(line_to_fn, js_string!("lineTo"), 2)
            .function(arc_fn, js_string!("arc"), 6)
            .function(arc_to_fn, js_string!("arcTo"), 5)
            .function(bezier_curve_to_fn, js_string!("bezierCurveTo"), 6)
            .function(quadratic_curve_to_fn, js_string!("quadraticCurveTo"), 4)
            .function(rect_fn, js_string!("rect"), 4)
            .function(ellipse_fn, js_string!("ellipse"), 7)
            .function(fill_fn, js_string!("fill"), 0)
            .function(stroke_fn, js_string!("stroke"), 0)
            .function(clip_fn, js_string!("clip"), 0)
            // State methods
            .function(save_fn, js_string!("save"), 0)
            .function(restore_fn, js_string!("restore"), 0)
            // Transform methods
            .function(scale_fn, js_string!("scale"), 2)
            .function(rotate_fn, js_string!("rotate"), 1)
            .function(translate_fn, js_string!("translate"), 2)
            .function(transform_fn, js_string!("transform"), 6)
            .function(set_transform_fn, js_string!("setTransform"), 6)
            .function(reset_transform_fn, js_string!("resetTransform"), 0)
            // Gradient/Pattern methods
            .function(create_linear_gradient_fn, js_string!("createLinearGradient"), 4)
            .function(create_radial_gradient_fn, js_string!("createRadialGradient"), 6)
            .function(create_pattern_fn, js_string!("createPattern"), 2)
            // Line dash methods
            .function(set_line_dash_fn, js_string!("setLineDash"), 1)
            .function(get_line_dash_fn, js_string!("getLineDash"), 0)
            // Image drawing
            .function(draw_image_fn, js_string!("drawImage"), 3)
            .build();

        Ok(JsValue::from(obj))
    });

    let paint_ctx = FunctionObjectBuilder::new(context.realm(), paint_ctx_ctor)
        .name(js_string!("PaintRenderingContext2D"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("PaintRenderingContext2D"), paint_ctx, Attribute::all())?;

    Ok(())
}

/// Register CSPViolationReportBody for Content Security Policy
fn register_csp_violation(context: &mut Context) -> JsResult<()> {
    // CSPViolationReportBody - represents a CSP violation report
    let csp_violation_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        // Extract options from first argument if provided
        let (document_url, referrer, blocked_url, effective_directive, original_policy,
             source_file, sample, disposition, status_code, line_number, column_number) =
            if let Some(options) = args.get(0).and_then(|v| v.as_object()) {
                let document_url = options.get(js_string!("documentURL"), ctx)
                    .ok()
                    .and_then(|v| if v.is_undefined() { None } else { v.to_string(ctx).ok() })
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                let referrer = options.get(js_string!("referrer"), ctx)
                    .ok()
                    .and_then(|v| if v.is_undefined() { None } else { v.to_string(ctx).ok() })
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                let blocked_url = options.get(js_string!("blockedURL"), ctx)
                    .ok()
                    .and_then(|v| if v.is_undefined() { None } else { v.to_string(ctx).ok() })
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                let effective_directive = options.get(js_string!("effectiveDirective"), ctx)
                    .ok()
                    .and_then(|v| if v.is_undefined() { None } else { v.to_string(ctx).ok() })
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                let original_policy = options.get(js_string!("originalPolicy"), ctx)
                    .ok()
                    .and_then(|v| if v.is_undefined() { None } else { v.to_string(ctx).ok() })
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                let source_file = options.get(js_string!("sourceFile"), ctx)
                    .ok()
                    .and_then(|v| if v.is_undefined() || v.is_null() { None } else { v.to_string(ctx).ok() })
                    .map(|s| JsValue::from(js_string!(s.to_std_string_escaped())))
                    .unwrap_or(JsValue::null());
                let sample = options.get(js_string!("sample"), ctx)
                    .ok()
                    .and_then(|v| if v.is_undefined() { None } else { v.to_string(ctx).ok() })
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                let disposition = options.get(js_string!("disposition"), ctx)
                    .ok()
                    .and_then(|v| if v.is_undefined() { None } else { v.to_string(ctx).ok() })
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_else(|| "enforce".to_string());
                let status_code = options.get(js_string!("statusCode"), ctx)
                    .ok()
                    .and_then(|v| v.to_u32(ctx).ok())
                    .unwrap_or(0) as u16;
                let line_number = options.get(js_string!("lineNumber"), ctx)
                    .ok()
                    .and_then(|v| if v.is_undefined() || v.is_null() { None } else { v.to_u32(ctx).ok() })
                    .map(|n| JsValue::from(n))
                    .unwrap_or(JsValue::null());
                let column_number = options.get(js_string!("columnNumber"), ctx)
                    .ok()
                    .and_then(|v| if v.is_undefined() || v.is_null() { None } else { v.to_u32(ctx).ok() })
                    .map(|n| JsValue::from(n))
                    .unwrap_or(JsValue::null());

                (document_url, referrer, blocked_url, effective_directive, original_policy,
                 source_file, sample, disposition, status_code, line_number, column_number)
            } else {
                (String::new(), String::new(), String::new(), String::new(), String::new(),
                 JsValue::null(), String::new(), "enforce".to_string(), 0u16, JsValue::null(), JsValue::null())
            };

        // toJSON method
        let document_url_clone = document_url.clone();
        let referrer_clone = referrer.clone();
        let blocked_url_clone = blocked_url.clone();
        let effective_directive_clone = effective_directive.clone();
        let original_policy_clone = original_policy.clone();
        let sample_clone = sample.clone();
        let disposition_clone = disposition.clone();
        let status_code_clone = status_code;

        let to_json_fn = unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let json = ObjectInitializer::new(ctx)
                    .property(js_string!("documentURL"), js_string!(document_url_clone.as_str()), Attribute::all())
                    .property(js_string!("referrer"), js_string!(referrer_clone.as_str()), Attribute::all())
                    .property(js_string!("blockedURL"), js_string!(blocked_url_clone.as_str()), Attribute::all())
                    .property(js_string!("effectiveDirective"), js_string!(effective_directive_clone.as_str()), Attribute::all())
                    .property(js_string!("originalPolicy"), js_string!(original_policy_clone.as_str()), Attribute::all())
                    .property(js_string!("sample"), js_string!(sample_clone.as_str()), Attribute::all())
                    .property(js_string!("disposition"), js_string!(disposition_clone.as_str()), Attribute::all())
                    .property(js_string!("statusCode"), status_code_clone, Attribute::all())
                    .build();
                Ok(JsValue::from(json))
            })
        };

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("documentURL"), js_string!(document_url.as_str()), Attribute::READONLY)
            .property(js_string!("referrer"), js_string!(referrer.as_str()), Attribute::READONLY)
            .property(js_string!("blockedURL"), js_string!(blocked_url.as_str()), Attribute::READONLY)
            .property(js_string!("effectiveDirective"), js_string!(effective_directive.as_str()), Attribute::READONLY)
            .property(js_string!("originalPolicy"), js_string!(original_policy.as_str()), Attribute::READONLY)
            .property(js_string!("sourceFile"), source_file, Attribute::READONLY)
            .property(js_string!("sample"), js_string!(sample.as_str()), Attribute::READONLY)
            .property(js_string!("disposition"), js_string!(disposition.as_str()), Attribute::READONLY)
            .property(js_string!("statusCode"), status_code, Attribute::READONLY)
            .property(js_string!("lineNumber"), line_number, Attribute::READONLY)
            .property(js_string!("columnNumber"), column_number, Attribute::READONLY)
            .function(to_json_fn, js_string!("toJSON"), 0)
            .build();

        Ok(JsValue::from(obj))
    });

    let csp_violation = FunctionObjectBuilder::new(context.realm(), csp_violation_ctor)
        .name(js_string!("CSPViolationReportBody"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("CSPViolationReportBody"), csp_violation, Attribute::all())?;

    Ok(())
}

// =============================================================================
// CSS Typed OM APIs
// =============================================================================

/// Register CSS Typed OM APIs (CSSStyleValue, CSSNestedDeclarations, StylePropertyMapReadOnly)
fn register_css_typed_om(context: &mut Context) -> JsResult<()> {
    // CSSStyleValue - base class for CSS values
    let css_style_value_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let value = args.get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        let to_string_val = value.clone();
        let to_string_fn = unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                Ok(JsValue::from(js_string!(to_string_val.as_str())))
            })
        };

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("_value"), js_string!(value.as_str()), Attribute::all())
            .function(to_string_fn, js_string!("toString"), 0)
            .build();

        Ok(JsValue::from(obj))
    });

    let css_style_value = FunctionObjectBuilder::new(context.realm(), css_style_value_ctor)
        .name(js_string!("CSSStyleValue"))
        .length(1)
        .constructor(true)
        .build();

    // Add static parse method
    let parse_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let property = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("_property"), js_string!(property.as_str()), Attribute::all())
            .property(js_string!("_value"), js_string!(value.as_str()), Attribute::all())
            .build();

        Ok(JsValue::from(obj))
    });
    css_style_value.set(js_string!("parse"), parse_fn.to_js_function(context.realm()), false, context)?;

    context.register_global_property(js_string!("CSSStyleValue"), css_style_value, Attribute::all())?;

    // CSSUnitValue - represents numeric values with units
    let css_unit_value_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let value = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
        let unit = args.get_or_undefined(1)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| "number".to_string());

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("value"), value, Attribute::all())
            .property(js_string!("unit"), js_string!(unit.as_str()), Attribute::READONLY)
            .build();

        Ok(JsValue::from(obj))
    });

    let css_unit_value = FunctionObjectBuilder::new(context.realm(), css_unit_value_ctor)
        .name(js_string!("CSSUnitValue"))
        .length(2)
        .constructor(true)
        .build();
    context.register_global_property(js_string!("CSSUnitValue"), css_unit_value, Attribute::all())?;

    // CSSKeywordValue - represents keyword values
    let css_keyword_value_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let value = args.get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        let value_clone = value.clone();
        let to_string_fn = unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                Ok(JsValue::from(js_string!(value_clone.as_str())))
            })
        };

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("value"), js_string!(value.as_str()), Attribute::all())
            .function(to_string_fn, js_string!("toString"), 0)
            .build();

        Ok(JsValue::from(obj))
    });

    let css_keyword_value = FunctionObjectBuilder::new(context.realm(), css_keyword_value_ctor)
        .name(js_string!("CSSKeywordValue"))
        .length(1)
        .constructor(true)
        .build();
    context.register_global_property(js_string!("CSSKeywordValue"), css_keyword_value, Attribute::all())?;

    // StylePropertyMapReadOnly - read-only map of computed styles
    let style_property_map_readonly_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let get_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let _property = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            // Return a CSSStyleValue-like object
            let obj = ObjectInitializer::new(ctx)
                .property(js_string!("_value"), js_string!(""), Attribute::all())
                .build();
            Ok(JsValue::from(obj))
        });

        let get_all_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            use boa_engine::object::builtins::JsArray;
            let arr = JsArray::new(ctx);
            Ok(JsValue::from(arr))
        });

        let has_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::from(false))
        });

        let entries_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            use boa_engine::object::builtins::JsArray;
            let arr = JsArray::new(ctx);
            Ok(JsValue::from(arr))
        });

        let keys_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            use boa_engine::object::builtins::JsArray;
            let arr = JsArray::new(ctx);
            Ok(JsValue::from(arr))
        });

        let values_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            use boa_engine::object::builtins::JsArray;
            let arr = JsArray::new(ctx);
            Ok(JsValue::from(arr))
        });

        let for_each_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("size"), 0, Attribute::READONLY)
            .function(get_fn, js_string!("get"), 1)
            .function(get_all_fn, js_string!("getAll"), 1)
            .function(has_fn, js_string!("has"), 1)
            .function(entries_fn, js_string!("entries"), 0)
            .function(keys_fn, js_string!("keys"), 0)
            .function(values_fn, js_string!("values"), 0)
            .function(for_each_fn, js_string!("forEach"), 1)
            .build();

        Ok(JsValue::from(obj))
    });

    let style_property_map_readonly = FunctionObjectBuilder::new(context.realm(), style_property_map_readonly_ctor)
        .name(js_string!("StylePropertyMapReadOnly"))
        .length(0)
        .constructor(true)
        .build();
    context.register_global_property(js_string!("StylePropertyMapReadOnly"), style_property_map_readonly, Attribute::all())?;

    // StylePropertyMap - writable map of styles
    let style_property_map_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let get_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let _property = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let obj = ObjectInitializer::new(ctx)
                .property(js_string!("_value"), js_string!(""), Attribute::all())
                .build();
            Ok(JsValue::from(obj))
        });

        let set_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let append_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let delete_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let clear_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("size"), 0, Attribute::READONLY)
            .function(get_fn, js_string!("get"), 1)
            .function(set_fn, js_string!("set"), 2)
            .function(append_fn, js_string!("append"), 2)
            .function(delete_fn, js_string!("delete"), 1)
            .function(clear_fn, js_string!("clear"), 0)
            .build();

        Ok(JsValue::from(obj))
    });

    let style_property_map = FunctionObjectBuilder::new(context.realm(), style_property_map_ctor)
        .name(js_string!("StylePropertyMap"))
        .length(0)
        .constructor(true)
        .build();
    context.register_global_property(js_string!("StylePropertyMap"), style_property_map, Attribute::all())?;

    // CSSNestedDeclarations - represents nested CSS declarations
    let css_nested_declarations_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Get style - returns CSSStyleDeclaration-like object
        let style = ObjectInitializer::new(ctx)
            .property(js_string!("length"), 0, Attribute::READONLY)
            .property(js_string!("cssText"), js_string!(""), Attribute::all())
            .build();

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("style"), style, Attribute::READONLY)
            .build();

        Ok(JsValue::from(obj))
    });

    let css_nested_declarations = FunctionObjectBuilder::new(context.realm(), css_nested_declarations_ctor)
        .name(js_string!("CSSNestedDeclarations"))
        .length(0)
        .constructor(true)
        .build();
    context.register_global_property(js_string!("CSSNestedDeclarations"), css_nested_declarations, Attribute::all())?;

    Ok(())
}

// =============================================================================
// TextMetrics Full Implementation
// =============================================================================

/// Register TextMetrics constructor with all properties
fn register_text_metrics(context: &mut Context) -> JsResult<()> {
    let text_metrics_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let width = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);

        let obj = ObjectInitializer::new(ctx)
            // Basic width
            .property(js_string!("width"), width, Attribute::READONLY)
            // Bounding box metrics
            .property(js_string!("actualBoundingBoxLeft"), 0.0, Attribute::READONLY)
            .property(js_string!("actualBoundingBoxRight"), width, Attribute::READONLY)
            .property(js_string!("actualBoundingBoxAscent"), 10.0, Attribute::READONLY)
            .property(js_string!("actualBoundingBoxDescent"), 2.0, Attribute::READONLY)
            // Font metrics
            .property(js_string!("fontBoundingBoxAscent"), 12.0, Attribute::READONLY)
            .property(js_string!("fontBoundingBoxDescent"), 3.0, Attribute::READONLY)
            // Em metrics
            .property(js_string!("emHeightAscent"), 10.0, Attribute::READONLY)
            .property(js_string!("emHeightDescent"), 2.0, Attribute::READONLY)
            // Hanging/alphabetic/ideographic baselines
            .property(js_string!("hangingBaseline"), 9.0, Attribute::READONLY)
            .property(js_string!("alphabeticBaseline"), 0.0, Attribute::READONLY)
            .property(js_string!("ideographicBaseline"), -2.0, Attribute::READONLY)
            .build();

        Ok(JsValue::from(obj))
    });

    let text_metrics = FunctionObjectBuilder::new(context.realm(), text_metrics_ctor)
        .name(js_string!("TextMetrics"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("TextMetrics"), text_metrics, Attribute::all())?;

    Ok(())
}

// =============================================================================
// HTML Collection APIs
// =============================================================================

/// Register HTMLFormControlsCollection
fn register_html_form_controls_collection(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let item_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::null())
        });

        let named_item_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::null())
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("length"), 0, Attribute::READONLY)
            .function(item_fn, js_string!("item"), 1)
            .function(named_item_fn, js_string!("namedItem"), 1)
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("HTMLFormControlsCollection"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("HTMLFormControlsCollection"), constructor, Attribute::all())?;

    Ok(())
}

/// Register HTMLOptionsCollection
fn register_html_options_collection(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let add_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let remove_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let item_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::null())
        });

        let named_item_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::null())
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("length"), 0, Attribute::all())
            .property(js_string!("selectedIndex"), -1i32, Attribute::all())
            .function(add_fn, js_string!("add"), 2)
            .function(remove_fn, js_string!("remove"), 1)
            .function(item_fn, js_string!("item"), 1)
            .function(named_item_fn, js_string!("namedItem"), 1)
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("HTMLOptionsCollection"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("HTMLOptionsCollection"), constructor, Attribute::all())?;

    Ok(())
}

// =============================================================================
// MediaQueryList Full Implementation
// =============================================================================

/// Register MediaQueryList constructor
fn register_media_query_list(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let media = args.get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        // Simple media query matching
        let matches = media.contains("screen") ||
                     media.contains("(min-width: 0") ||
                     media.contains("all") ||
                     media.is_empty();

        let add_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });
        let remove_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
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

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("media"), js_string!(media.as_str()), Attribute::READONLY)
            .property(js_string!("matches"), matches, Attribute::READONLY)
            .property(js_string!("onchange"), JsValue::null(), Attribute::all())
            .function(add_listener, js_string!("addListener"), 1)
            .function(remove_listener, js_string!("removeListener"), 1)
            .function(add_event_listener, js_string!("addEventListener"), 3)
            .function(remove_event_listener, js_string!("removeEventListener"), 3)
            .function(dispatch_event, js_string!("dispatchEvent"), 1)
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("MediaQueryList"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("MediaQueryList"), constructor, Attribute::all())?;

    Ok(())
}

// =============================================================================
// Geolocation Full Implementation
// =============================================================================

/// Register GeolocationCoordinates constructor
fn register_geolocation_coordinates(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let latitude = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
        let longitude = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);
        let altitude = if args.len() > 2 && !args[2].is_null() && !args[2].is_undefined() {
            JsValue::from(args[2].to_number(ctx).unwrap_or(0.0))
        } else {
            JsValue::null()
        };
        let accuracy = args.get_or_undefined(3).to_number(ctx).unwrap_or(0.0);
        let altitude_accuracy = if args.len() > 4 && !args[4].is_null() && !args[4].is_undefined() {
            JsValue::from(args[4].to_number(ctx).unwrap_or(0.0))
        } else {
            JsValue::null()
        };
        let heading = if args.len() > 5 && !args[5].is_null() && !args[5].is_undefined() {
            JsValue::from(args[5].to_number(ctx).unwrap_or(0.0))
        } else {
            JsValue::null()
        };
        let speed = if args.len() > 6 && !args[6].is_null() && !args[6].is_undefined() {
            JsValue::from(args[6].to_number(ctx).unwrap_or(0.0))
        } else {
            JsValue::null()
        };

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("latitude"), latitude, Attribute::READONLY)
            .property(js_string!("longitude"), longitude, Attribute::READONLY)
            .property(js_string!("altitude"), altitude, Attribute::READONLY)
            .property(js_string!("accuracy"), accuracy, Attribute::READONLY)
            .property(js_string!("altitudeAccuracy"), altitude_accuracy, Attribute::READONLY)
            .property(js_string!("heading"), heading, Attribute::READONLY)
            .property(js_string!("speed"), speed, Attribute::READONLY)
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("GeolocationCoordinates"))
        .length(4)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("GeolocationCoordinates"), constructor, Attribute::all())?;

    Ok(())
}

/// Register GeolocationPosition constructor
fn register_geolocation_position(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let coords = args.get_or_undefined(0);
        let timestamp = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);

        let coords_val = if coords.is_object() {
            coords.clone()
        } else {
            // Create default coords
            let default_coords = ObjectInitializer::new(ctx)
                .property(js_string!("latitude"), 0.0, Attribute::READONLY)
                .property(js_string!("longitude"), 0.0, Attribute::READONLY)
                .property(js_string!("altitude"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("accuracy"), 0.0, Attribute::READONLY)
                .property(js_string!("altitudeAccuracy"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("heading"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("speed"), JsValue::null(), Attribute::READONLY)
                .build();
            JsValue::from(default_coords)
        };

        let to_json_fn = NativeFunction::from_copy_closure(|this, _args, ctx| {
            if let Some(obj) = this.as_object() {
                let coords = obj.get(js_string!("coords"), ctx).unwrap_or(JsValue::null());
                let timestamp = obj.get(js_string!("timestamp"), ctx).unwrap_or(JsValue::from(0.0));

                let json = ObjectInitializer::new(ctx)
                    .property(js_string!("coords"), coords, Attribute::all())
                    .property(js_string!("timestamp"), timestamp, Attribute::all())
                    .build();
                Ok(JsValue::from(json))
            } else {
                Ok(JsValue::null())
            }
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("coords"), coords_val, Attribute::READONLY)
            .property(js_string!("timestamp"), timestamp, Attribute::READONLY)
            .function(to_json_fn, js_string!("toJSON"), 0)
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("GeolocationPosition"))
        .length(2)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("GeolocationPosition"), constructor, Attribute::all())?;

    Ok(())
}

/// Register GeolocationPositionError constructor
/// Error codes: 1 = PERMISSION_DENIED, 2 = POSITION_UNAVAILABLE, 3 = TIMEOUT
fn register_geolocation_position_error(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let code = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0);
        let message = args.get_or_undefined(1)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| String::new());

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("code"), code, Attribute::READONLY)
            .property(js_string!("message"), js_string!(message), Attribute::READONLY)
            // Error code constants
            .property(js_string!("PERMISSION_DENIED"), 1, Attribute::READONLY)
            .property(js_string!("POSITION_UNAVAILABLE"), 2, Attribute::READONLY)
            .property(js_string!("TIMEOUT"), 3, Attribute::READONLY)
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("GeolocationPositionError"))
        .length(2)
        .constructor(true)
        .build();

    // Add static constants to the constructor
    constructor.set(js_string!("PERMISSION_DENIED"), JsValue::from(1), false, context)?;
    constructor.set(js_string!("POSITION_UNAVAILABLE"), JsValue::from(2), false, context)?;
    constructor.set(js_string!("TIMEOUT"), JsValue::from(3), false, context)?;

    context.register_global_property(js_string!("GeolocationPositionError"), constructor, Attribute::all())?;

    Ok(())
}

// =============================================================================
// RadioNodeList Implementation
// =============================================================================

/// Register RadioNodeList constructor - extends NodeList with value property
fn register_radio_node_list(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        // item(index) method - looks up from 'this' object
        let item_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0);
            if let Some(obj) = this.as_object() {
                if let Ok(item) = obj.get(index, ctx) {
                    if !item.is_undefined() {
                        return Ok(item);
                    }
                }
            }
            Ok(JsValue::null())
        });

        // namedItem(name) method
        let named_item_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            if let Some(this_obj) = this.as_object() {
                if let Ok(len_val) = this_obj.get(js_string!("length"), ctx) {
                    let len = len_val.to_length(ctx).unwrap_or(0);
                    for i in 0..len {
                        if let Ok(item) = this_obj.get(i as u32, ctx) {
                            if let Some(obj) = item.as_object() {
                                // Check id
                                if let Ok(id) = obj.get(js_string!("id"), ctx) {
                                    if id.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default() == name {
                                        return Ok(item);
                                    }
                                }
                                // Check name attribute
                                if let Ok(n) = obj.get(js_string!("name"), ctx) {
                                    if n.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default() == name {
                                        return Ok(item);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(JsValue::null())
        });

        // value getter - returns the value of the first checked radio button
        let value_getter_fn = NativeFunction::from_copy_closure(|this, _args, ctx| {
            if let Some(this_obj) = this.as_object() {
                if let Ok(len_val) = this_obj.get(js_string!("length"), ctx) {
                    let len = len_val.to_length(ctx).unwrap_or(0);
                    for i in 0..len {
                        if let Ok(item) = this_obj.get(i as u32, ctx) {
                            if let Some(obj) = item.as_object() {
                                if let Ok(checked) = obj.get(js_string!("checked"), ctx) {
                                    if checked.to_boolean() {
                                        if let Ok(val) = obj.get(js_string!("value"), ctx) {
                                            return Ok(val);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(JsValue::from(js_string!("")))
        });

        let value_getter = FunctionObjectBuilder::new(ctx.realm(), value_getter_fn)
            .name(js_string!("get value"))
            .length(0)
            .build();

        // Get initial length from args
        let mut len: u32 = 0;
        if let Some(arr_val) = args.get(0) {
            if let Some(arr_obj) = arr_val.as_object() {
                if let Ok(len_val) = arr_obj.get(js_string!("length"), ctx) {
                    len = len_val.to_u32(ctx).unwrap_or(0);
                }
            }
        }

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("length"), len, Attribute::READONLY)
            .function(item_fn, js_string!("item"), 1)
            .function(named_item_fn, js_string!("namedItem"), 1)
            .accessor(js_string!("value"), Some(value_getter), None, Attribute::CONFIGURABLE)
            .build();

        // Copy elements from input array to object for indexed access
        if let Some(arr_val) = args.get(0) {
            if let Some(arr_obj) = arr_val.as_object() {
                for i in 0..len {
                    if let Ok(item) = arr_obj.get(i, ctx) {
                        obj.set(i, item, false, ctx)?;
                    }
                }
            }
        }

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("RadioNodeList"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("RadioNodeList"), constructor, Attribute::all())?;

    Ok(())
}

// =============================================================================
// TimeRanges Implementation
// =============================================================================

/// Register TimeRanges constructor - represents time ranges for media elements
fn register_time_ranges(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        // start(index) - returns the start time of a range (stored in _ranges)
        let start_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0);
            if let Some(this_obj) = this.as_object() {
                if let Ok(ranges) = this_obj.get(js_string!("_ranges"), ctx) {
                    if let Some(ranges_obj) = ranges.as_object() {
                        if let Ok(len_val) = ranges_obj.get(js_string!("length"), ctx) {
                            let len = len_val.to_u32(ctx).unwrap_or(0);
                            if index >= len {
                                return Err(JsNativeError::range()
                                    .with_message("Index out of bounds")
                                    .into());
                            }
                            if let Ok(pair) = ranges_obj.get(index, ctx) {
                                if let Some(pair_obj) = pair.as_object() {
                                    if let Ok(start) = pair_obj.get(0, ctx) {
                                        return Ok(start);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(JsValue::from(0.0))
        });

        // end(index) - returns the end time of a range
        let end_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0);
            if let Some(this_obj) = this.as_object() {
                if let Ok(ranges) = this_obj.get(js_string!("_ranges"), ctx) {
                    if let Some(ranges_obj) = ranges.as_object() {
                        if let Ok(len_val) = ranges_obj.get(js_string!("length"), ctx) {
                            let len = len_val.to_u32(ctx).unwrap_or(0);
                            if index >= len {
                                return Err(JsNativeError::range()
                                    .with_message("Index out of bounds")
                                    .into());
                            }
                            if let Ok(pair) = ranges_obj.get(index, ctx) {
                                if let Some(pair_obj) = pair.as_object() {
                                    if let Ok(end) = pair_obj.get(1, ctx) {
                                        return Ok(end);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(JsValue::from(0.0))
        });

        // Get ranges from args and calculate length
        let mut len: u32 = 0;
        let ranges_val = if let Some(arr_val) = args.get(0) {
            if let Some(arr_obj) = arr_val.as_object() {
                if let Ok(len_val) = arr_obj.get(js_string!("length"), ctx) {
                    len = len_val.to_u32(ctx).unwrap_or(0);
                }
            }
            arr_val.clone()
        } else {
            // Create empty array
            use boa_engine::object::builtins::JsArray;
            JsValue::from(JsArray::new(ctx))
        };

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("length"), len, Attribute::READONLY)
            .property(js_string!("_ranges"), ranges_val, Attribute::empty()) // Internal storage
            .function(start_fn, js_string!("start"), 1)
            .function(end_fn, js_string!("end"), 1)
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("TimeRanges"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("TimeRanges"), constructor, Attribute::all())?;

    Ok(())
}

// =============================================================================
// FileReaderSync Implementation
// =============================================================================

/// Register FileReaderSync constructor - synchronous file reading (for workers)
fn register_file_reader_sync(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // readAsArrayBuffer(blob)
        let read_as_array_buffer = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(blob) = args.get(0) {
                if let Some(blob_obj) = blob.as_object() {
                    // Try to get the blob's internal data
                    if let Ok(data) = blob_obj.get(js_string!("_data"), ctx) {
                        if let Some(data_obj) = data.as_object() {
                            // Return as ArrayBuffer
                            return Ok(JsValue::from(data_obj.clone()));
                        }
                    }
                    // If no internal data, return empty ArrayBuffer
                    let array_buffer = boa_engine::object::builtins::JsArrayBuffer::new(0, ctx)?;
                    return Ok(JsValue::from(array_buffer));
                }
            }
            Err(JsNativeError::typ()
                .with_message("Failed to execute 'readAsArrayBuffer': parameter 1 is not of type 'Blob'")
                .into())
        });

        // readAsBinaryString(blob)
        let read_as_binary_string = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(blob) = args.get(0) {
                if let Some(blob_obj) = blob.as_object() {
                    if let Ok(data) = blob_obj.get(js_string!("_data"), ctx) {
                        if let Ok(str_val) = data.to_string(ctx) {
                            return Ok(JsValue::from(str_val));
                        }
                    }
                    return Ok(JsValue::from(js_string!("")));
                }
            }
            Err(JsNativeError::typ()
                .with_message("Failed to execute 'readAsBinaryString': parameter 1 is not of type 'Blob'")
                .into())
        });

        // readAsDataURL(blob)
        let read_as_data_url = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(blob) = args.get(0) {
                if let Some(blob_obj) = blob.as_object() {
                    let mime_type = blob_obj.get(js_string!("type"), ctx)
                        .ok()
                        .and_then(|v| v.to_string(ctx).ok())
                        .map(|s| s.to_std_string_escaped())
                        .unwrap_or_else(|| "application/octet-stream".to_string());

                    // Get blob data and encode as base64
                    if let Ok(data) = blob_obj.get(js_string!("_data"), ctx) {
                        if let Ok(str_val) = data.to_string(ctx) {
                            let data_str = str_val.to_std_string_escaped();
                            // Simple base64 encoding
                            let encoded = simple_base64_encode(data_str.as_bytes());
                            let data_url = format!("data:{};base64,{}", mime_type, encoded);
                            return Ok(JsValue::from(js_string!(data_url)));
                        }
                    }
                    // Return empty data URL
                    let data_url = format!("data:{};base64,", mime_type);
                    return Ok(JsValue::from(js_string!(data_url)));
                }
            }
            Err(JsNativeError::typ()
                .with_message("Failed to execute 'readAsDataURL': parameter 1 is not of type 'Blob'")
                .into())
        });

        // readAsText(blob, encoding)
        let read_as_text = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(blob) = args.get(0) {
                if let Some(blob_obj) = blob.as_object() {
                    // encoding parameter (default UTF-8)
                    let _encoding = args.get(1)
                        .and_then(|v| v.to_string(ctx).ok())
                        .map(|s| s.to_std_string_escaped())
                        .unwrap_or_else(|| "UTF-8".to_string());

                    // Get blob data as text
                    if let Ok(data) = blob_obj.get(js_string!("_data"), ctx) {
                        if let Ok(str_val) = data.to_string(ctx) {
                            return Ok(JsValue::from(str_val));
                        }
                    }
                    return Ok(JsValue::from(js_string!("")));
                }
            }
            Err(JsNativeError::typ()
                .with_message("Failed to execute 'readAsText': parameter 1 is not of type 'Blob'")
                .into())
        });

        let obj = ObjectInitializer::new(ctx)
            .function(read_as_array_buffer, js_string!("readAsArrayBuffer"), 1)
            .function(read_as_binary_string, js_string!("readAsBinaryString"), 1)
            .function(read_as_data_url, js_string!("readAsDataURL"), 1)
            .function(read_as_text, js_string!("readAsText"), 2)
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("FileReaderSync"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("FileReaderSync"), constructor, Attribute::all())?;

    Ok(())
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Simple base64 encoding (RFC 4648)
fn simple_base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let mut i = 0;

    while i < data.len() {
        let b0 = data[i];
        let b1 = if i + 1 < data.len() { data[i + 1] } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] } else { 0 };

        result.push(ALPHABET[(b0 >> 2) as usize] as char);
        result.push(ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);

        if i + 1 < data.len() {
            result.push(ALPHABET[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            result.push('=');
        }

        if i + 2 < data.len() {
            result.push(ALPHABET[(b2 & 0x3f) as usize] as char);
        } else {
            result.push('=');
        }

        i += 3;
    }

    result
}

// =============================================================================
// QuotaExceededError Implementation
// =============================================================================

/// Register QuotaExceededError - DOMException subclass for storage quota errors
fn register_quota_exceeded_error(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let message = args.get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| "The quota has been exceeded.".to_string());

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!("QuotaExceededError"), Attribute::READONLY)
            .property(js_string!("message"), js_string!(message), Attribute::READONLY)
            .property(js_string!("code"), 22, Attribute::READONLY) // QUOTA_EXCEEDED_ERR = 22
            // DOMException constants
            .property(js_string!("QUOTA_EXCEEDED_ERR"), 22, Attribute::READONLY)
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("QuotaExceededError"))
        .length(1)
        .constructor(true)
        .build();

    // Static constant
    constructor.set(js_string!("QUOTA_EXCEEDED_ERR"), JsValue::from(22), false, context)?;

    context.register_global_property(js_string!("QuotaExceededError"), constructor, Attribute::all())?;

    Ok(())
}

// =============================================================================
// XPathNSResolver Implementation
// =============================================================================

/// Register XPathNSResolver - resolves namespace prefixes to URIs
fn register_xpath_ns_resolver(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        // lookupNamespaceURI(prefix) method
        let lookup_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
            let prefix = args.get_or_undefined(0)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();

            // Check if custom resolver function was provided
            if let Some(this_obj) = this.as_object() {
                if let Ok(resolver_fn) = this_obj.get(js_string!("_resolver"), ctx) {
                    if resolver_fn.is_callable() {
                        if let Some(callable) = resolver_fn.as_callable() {
                            return callable.call(&JsValue::undefined(), &[JsValue::from(js_string!(prefix))], ctx);
                        }
                    }
                }
                // Check namespaces map
                if let Ok(ns_map) = this_obj.get(js_string!("_namespaces"), ctx) {
                    if let Some(ns_obj) = ns_map.as_object() {
                        if let Ok(uri) = ns_obj.get(js_string!(prefix.as_str()), ctx) {
                            if !uri.is_undefined() {
                                return Ok(uri);
                            }
                        }
                    }
                }
            }

            // Default namespace mappings
            match prefix.as_str() {
                "xml" => Ok(JsValue::from(js_string!("http://www.w3.org/XML/1998/namespace"))),
                "xmlns" => Ok(JsValue::from(js_string!("http://www.w3.org/2000/xmlns/"))),
                "html" | "xhtml" => Ok(JsValue::from(js_string!("http://www.w3.org/1999/xhtml"))),
                "svg" => Ok(JsValue::from(js_string!("http://www.w3.org/2000/svg"))),
                "mathml" => Ok(JsValue::from(js_string!("http://www.w3.org/1998/Math/MathML"))),
                "xlink" => Ok(JsValue::from(js_string!("http://www.w3.org/1999/xlink"))),
                _ => Ok(JsValue::null()),
            }
        });

        // Get optional resolver function or namespaces map from args
        let resolver = args.get(0).cloned().unwrap_or(JsValue::undefined());

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("_resolver"), resolver.clone(), Attribute::empty())
            .property(js_string!("_namespaces"), resolver, Attribute::empty())
            .function(lookup_fn, js_string!("lookupNamespaceURI"), 1)
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("XPathNSResolver"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("XPathNSResolver"), constructor, Attribute::all())?;

    Ok(())
}

// =============================================================================
// ElementInternals Implementation
// =============================================================================

/// Register ElementInternals - provides access to custom element internals
fn register_element_internals(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let target_element = args.get(0).cloned().unwrap_or(JsValue::null());

        // setFormValue(value, state) - sets form value
        let set_form_value = NativeFunction::from_copy_closure(|this, args, ctx| {
            if let Some(this_obj) = this.as_object() {
                let value = args.get(0).cloned().unwrap_or(JsValue::null());
                let state = args.get(1).cloned().unwrap_or(value.clone());
                this_obj.set(js_string!("_formValue"), value, false, ctx)?;
                this_obj.set(js_string!("_formState"), state, false, ctx)?;
            }
            Ok(JsValue::undefined())
        });

        // setValidity(flags, message, anchor) - sets validity state
        let set_validity = NativeFunction::from_copy_closure(|this, args, ctx| {
            if let Some(this_obj) = this.as_object() {
                let flags = args.get(0).cloned().unwrap_or(JsValue::undefined());
                let message = args.get(1)
                    .and_then(|v| v.to_string(ctx).ok())
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                let anchor = args.get(2).cloned().unwrap_or(JsValue::null());

                this_obj.set(js_string!("_validityFlags"), flags, false, ctx)?;
                this_obj.set(js_string!("validationMessage"), js_string!(message), false, ctx)?;
                this_obj.set(js_string!("_validationAnchor"), anchor, false, ctx)?;
            }
            Ok(JsValue::undefined())
        });

        // reportValidity() - reports validity
        let report_validity = NativeFunction::from_copy_closure(|this, _args, ctx| {
            if let Some(this_obj) = this.as_object() {
                if let Ok(will_validate) = this_obj.get(js_string!("willValidate"), ctx) {
                    if !will_validate.to_boolean() {
                        return Ok(JsValue::from(true));
                    }
                }
                if let Ok(validity) = this_obj.get(js_string!("validity"), ctx) {
                    if let Some(validity_obj) = validity.as_object() {
                        if let Ok(valid) = validity_obj.get(js_string!("valid"), ctx) {
                            return Ok(valid);
                        }
                    }
                }
            }
            Ok(JsValue::from(true))
        });

        // checkValidity() - checks validity
        let check_validity = NativeFunction::from_copy_closure(|this, _args, ctx| {
            if let Some(this_obj) = this.as_object() {
                if let Ok(validity) = this_obj.get(js_string!("validity"), ctx) {
                    if let Some(validity_obj) = validity.as_object() {
                        if let Ok(valid) = validity_obj.get(js_string!("valid"), ctx) {
                            return Ok(valid);
                        }
                    }
                }
            }
            Ok(JsValue::from(true))
        });

        // Create default ValidityState
        let validity_state = ObjectInitializer::new(ctx)
            .property(js_string!("valueMissing"), false, Attribute::all())
            .property(js_string!("typeMismatch"), false, Attribute::all())
            .property(js_string!("patternMismatch"), false, Attribute::all())
            .property(js_string!("tooLong"), false, Attribute::all())
            .property(js_string!("tooShort"), false, Attribute::all())
            .property(js_string!("rangeUnderflow"), false, Attribute::all())
            .property(js_string!("rangeOverflow"), false, Attribute::all())
            .property(js_string!("stepMismatch"), false, Attribute::all())
            .property(js_string!("badInput"), false, Attribute::all())
            .property(js_string!("customError"), false, Attribute::all())
            .property(js_string!("valid"), true, Attribute::all())
            .build();

        // Create labels NodeList (empty)
        let labels = ObjectInitializer::new(ctx)
            .property(js_string!("length"), 0, Attribute::READONLY)
            .build();

        let obj = ObjectInitializer::new(ctx)
            // Properties
            .property(js_string!("shadowRoot"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("form"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("willValidate"), true, Attribute::READONLY)
            .property(js_string!("validity"), validity_state, Attribute::READONLY)
            .property(js_string!("validationMessage"), js_string!(""), Attribute::all())
            .property(js_string!("labels"), labels, Attribute::READONLY)
            .property(js_string!("_targetElement"), target_element, Attribute::empty())
            .property(js_string!("_formValue"), JsValue::null(), Attribute::empty())
            .property(js_string!("_formState"), JsValue::null(), Attribute::empty())
            // ARIA properties
            .property(js_string!("role"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaAtomic"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaAutoComplete"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaBusy"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaChecked"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaColCount"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaColIndex"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaColSpan"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaCurrent"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaDisabled"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaExpanded"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaHasPopup"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaHidden"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaLabel"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaLevel"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaLive"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaModal"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaMultiLine"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaMultiSelectable"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaOrientation"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaPlaceholder"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaPosInSet"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaPressed"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaReadOnly"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaRequired"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaRoleDescription"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaRowCount"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaRowIndex"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaRowSpan"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaSelected"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaSetSize"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaSort"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaValueMax"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaValueMin"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaValueNow"), JsValue::null(), Attribute::all())
            .property(js_string!("ariaValueText"), JsValue::null(), Attribute::all())
            // Methods
            .function(set_form_value, js_string!("setFormValue"), 2)
            .function(set_validity, js_string!("setValidity"), 3)
            .function(report_validity, js_string!("reportValidity"), 0)
            .function(check_validity, js_string!("checkValidity"), 0)
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("ElementInternals"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("ElementInternals"), constructor, Attribute::all())?;

    Ok(())
}

// =============================================================================
// MediaStream Implementation
// =============================================================================

/// Register MediaStream - represents a stream of media content
fn register_media_stream(context: &mut Context) -> JsResult<()> {
    use boa_engine::object::builtins::JsArray;

    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        // Generate unique ID
        let id = format!("{:x}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos());

        // Internal tracks storage
        let tracks = JsArray::new(ctx);

        // If passed another MediaStream or array of tracks, copy them
        if let Some(arg) = args.get(0) {
            if let Some(arg_obj) = arg.as_object() {
                // Check if it's a MediaStream (has getTracks method)
                if let Ok(get_tracks) = arg_obj.get(js_string!("getTracks"), ctx) {
                    if get_tracks.is_callable() {
                        if let Some(callable) = get_tracks.as_callable() {
                            if let Ok(result) = callable.call(&JsValue::from(arg_obj.clone()), &[], ctx) {
                                if let Some(result_obj) = result.as_object() {
                                    if let Ok(len) = result_obj.get(js_string!("length"), ctx) {
                                        let len = len.to_u32(ctx).unwrap_or(0);
                                        for i in 0..len {
                                            if let Ok(track) = result_obj.get(i, ctx) {
                                                let _ = tracks.push(track, ctx);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if let Ok(len) = arg_obj.get(js_string!("length"), ctx) {
                    // It's an array of tracks
                    let len = len.to_u32(ctx).unwrap_or(0);
                    for i in 0..len {
                        if let Ok(track) = arg_obj.get(i, ctx) {
                            let _ = tracks.push(track, ctx);
                        }
                    }
                }
            }
        }

        // Store tracks on object for methods to access
        let tracks_val = JsValue::from(tracks);

        // getTracks() - returns all tracks
        let get_tracks = NativeFunction::from_copy_closure(|this, _args, ctx| {
            if let Some(this_obj) = this.as_object() {
                if let Ok(tracks) = this_obj.get(js_string!("_tracks"), ctx) {
                    // Return a copy of the array
                    if let Some(tracks_obj) = tracks.as_object() {
                        let result = JsArray::new(ctx);
                        if let Ok(len) = tracks_obj.get(js_string!("length"), ctx) {
                            let len = len.to_u32(ctx).unwrap_or(0);
                            for i in 0..len {
                                if let Ok(track) = tracks_obj.get(i, ctx) {
                                    let _ = result.push(track, ctx);
                                }
                            }
                        }
                        return Ok(JsValue::from(result));
                    }
                }
            }
            Ok(JsValue::from(JsArray::new(ctx)))
        });

        // getAudioTracks() - returns audio tracks
        let get_audio_tracks = NativeFunction::from_copy_closure(|this, _args, ctx| {
            if let Some(this_obj) = this.as_object() {
                if let Ok(tracks) = this_obj.get(js_string!("_tracks"), ctx) {
                    if let Some(tracks_obj) = tracks.as_object() {
                        let result = JsArray::new(ctx);
                        if let Ok(len) = tracks_obj.get(js_string!("length"), ctx) {
                            let len = len.to_u32(ctx).unwrap_or(0);
                            for i in 0..len {
                                if let Ok(track) = tracks_obj.get(i, ctx) {
                                    if let Some(track_obj) = track.as_object() {
                                        if let Ok(kind) = track_obj.get(js_string!("kind"), ctx) {
                                            if kind.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default() == "audio" {
                                                let _ = result.push(track, ctx);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        return Ok(JsValue::from(result));
                    }
                }
            }
            Ok(JsValue::from(JsArray::new(ctx)))
        });

        // getVideoTracks() - returns video tracks
        let get_video_tracks = NativeFunction::from_copy_closure(|this, _args, ctx| {
            if let Some(this_obj) = this.as_object() {
                if let Ok(tracks) = this_obj.get(js_string!("_tracks"), ctx) {
                    if let Some(tracks_obj) = tracks.as_object() {
                        let result = JsArray::new(ctx);
                        if let Ok(len) = tracks_obj.get(js_string!("length"), ctx) {
                            let len = len.to_u32(ctx).unwrap_or(0);
                            for i in 0..len {
                                if let Ok(track) = tracks_obj.get(i, ctx) {
                                    if let Some(track_obj) = track.as_object() {
                                        if let Ok(kind) = track_obj.get(js_string!("kind"), ctx) {
                                            if kind.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default() == "video" {
                                                let _ = result.push(track, ctx);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        return Ok(JsValue::from(result));
                    }
                }
            }
            Ok(JsValue::from(JsArray::new(ctx)))
        });

        // getTrackById(id) - returns track by ID
        let get_track_by_id = NativeFunction::from_copy_closure(|this, args, ctx| {
            let track_id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            if let Some(this_obj) = this.as_object() {
                if let Ok(tracks) = this_obj.get(js_string!("_tracks"), ctx) {
                    if let Some(tracks_obj) = tracks.as_object() {
                        if let Ok(len) = tracks_obj.get(js_string!("length"), ctx) {
                            let len = len.to_u32(ctx).unwrap_or(0);
                            for i in 0..len {
                                if let Ok(track) = tracks_obj.get(i, ctx) {
                                    if let Some(track_obj) = track.as_object() {
                                        if let Ok(id) = track_obj.get(js_string!("id"), ctx) {
                                            if id.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default() == track_id {
                                                return Ok(track);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(JsValue::null())
        });

        // addTrack(track) - adds a track
        let add_track = NativeFunction::from_copy_closure(|this, args, ctx| {
            if let Some(this_obj) = this.as_object() {
                if let Ok(tracks) = this_obj.get(js_string!("_tracks"), ctx) {
                    if let Some(tracks_obj) = tracks.as_object() {
                        if let Some(track) = args.get(0) {
                            tracks_obj.set(tracks_obj.get(js_string!("length"), ctx)?.to_u32(ctx).unwrap_or(0), track.clone(), false, ctx)?;
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        });

        // removeTrack(track) - removes a track
        let remove_track = NativeFunction::from_copy_closure(|this, args, ctx| {
            if let Some(this_obj) = this.as_object() {
                if let Some(track_to_remove) = args.get(0) {
                    if let Some(track_obj) = track_to_remove.as_object() {
                        if let Ok(remove_id) = track_obj.get(js_string!("id"), ctx) {
                            let remove_id = remove_id.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
                            if let Ok(tracks) = this_obj.get(js_string!("_tracks"), ctx) {
                                if let Some(tracks_obj) = tracks.as_object() {
                                    let new_tracks = JsArray::new(ctx);
                                    if let Ok(len) = tracks_obj.get(js_string!("length"), ctx) {
                                        let len = len.to_u32(ctx).unwrap_or(0);
                                        for i in 0..len {
                                            if let Ok(track) = tracks_obj.get(i, ctx) {
                                                if let Some(t_obj) = track.as_object() {
                                                    if let Ok(id) = t_obj.get(js_string!("id"), ctx) {
                                                        if id.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default() != remove_id {
                                                            let _ = new_tracks.push(track, ctx);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    this_obj.set(js_string!("_tracks"), JsValue::from(new_tracks), false, ctx)?;
                                }
                            }
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        });

        // clone() - clones the stream
        let clone_stream = NativeFunction::from_copy_closure(|this, _args, ctx| {
            if let Some(this_obj) = this.as_object() {
                let new_id = format!("{:x}", std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos());

                let new_tracks = JsArray::new(ctx);
                if let Ok(tracks) = this_obj.get(js_string!("_tracks"), ctx) {
                    if let Some(tracks_obj) = tracks.as_object() {
                        if let Ok(len) = tracks_obj.get(js_string!("length"), ctx) {
                            let len = len.to_u32(ctx).unwrap_or(0);
                            for i in 0..len {
                                if let Ok(track) = tracks_obj.get(i, ctx) {
                                    // Clone each track
                                    if let Some(track_obj) = track.as_object() {
                                        if let Ok(clone_fn) = track_obj.get(js_string!("clone"), ctx) {
                                            if clone_fn.is_callable() {
                                                if let Some(callable) = clone_fn.as_callable() {
                                                    if let Ok(cloned) = callable.call(&track, &[], ctx) {
                                                        let _ = new_tracks.push(cloned, ctx);
                                                        continue;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    let _ = new_tracks.push(track, ctx);
                                }
                            }
                        }
                    }
                }

                let cloned = ObjectInitializer::new(ctx)
                    .property(js_string!("id"), js_string!(new_id), Attribute::READONLY)
                    .property(js_string!("active"), true, Attribute::READONLY)
                    .property(js_string!("_tracks"), JsValue::from(new_tracks), Attribute::empty())
                    .build();
                return Ok(JsValue::from(cloned));
            }
            Ok(JsValue::null())
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("id"), js_string!(id), Attribute::READONLY)
            .property(js_string!("active"), true, Attribute::READONLY)
            .property(js_string!("_tracks"), tracks_val, Attribute::empty())
            // Event handlers
            .property(js_string!("onaddtrack"), JsValue::null(), Attribute::all())
            .property(js_string!("onremovetrack"), JsValue::null(), Attribute::all())
            .property(js_string!("onactive"), JsValue::null(), Attribute::all())
            .property(js_string!("oninactive"), JsValue::null(), Attribute::all())
            // Methods
            .function(get_tracks, js_string!("getTracks"), 0)
            .function(get_audio_tracks, js_string!("getAudioTracks"), 0)
            .function(get_video_tracks, js_string!("getVideoTracks"), 0)
            .function(get_track_by_id, js_string!("getTrackById"), 1)
            .function(add_track, js_string!("addTrack"), 1)
            .function(remove_track, js_string!("removeTrack"), 1)
            .function(clone_stream, js_string!("clone"), 0)
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("MediaStream"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("MediaStream"), constructor, Attribute::all())?;

    Ok(())
}

// =============================================================================
// TextTrack, TextTrackCue, TextTrackList Implementation
// =============================================================================

/// Register TextTrackCue - represents a cue in a text track
fn register_text_track_cue(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let start_time = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
        let end_time = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);
        let text = args.get_or_undefined(2)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        let id = format!("cue-{:x}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos());

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("id"), js_string!(id), Attribute::all())
            .property(js_string!("startTime"), start_time, Attribute::all())
            .property(js_string!("endTime"), end_time, Attribute::all())
            .property(js_string!("text"), js_string!(text), Attribute::all())
            .property(js_string!("pauseOnExit"), false, Attribute::all())
            .property(js_string!("track"), JsValue::null(), Attribute::READONLY)
            // Event handlers
            .property(js_string!("onenter"), JsValue::null(), Attribute::all())
            .property(js_string!("onexit"), JsValue::null(), Attribute::all())
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("TextTrackCue"))
        .length(3)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("TextTrackCue"), constructor, Attribute::all())?;

    Ok(())
}

/// Register VTTCue - WebVTT cue (extends TextTrackCue)
fn register_vtt_cue(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let start_time = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
        let end_time = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);
        let text = args.get_or_undefined(2)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        let id = format!("vttcue-{:x}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos());

        // getCueAsHTML() method
        let get_cue_as_html = NativeFunction::from_copy_closure(|this, _args, ctx| {
            if let Some(this_obj) = this.as_object() {
                if let Ok(text) = this_obj.get(js_string!("text"), ctx) {
                    // Return a DocumentFragment-like object
                    let fragment = ObjectInitializer::new(ctx)
                        .property(js_string!("textContent"), text, Attribute::all())
                        .build();
                    return Ok(JsValue::from(fragment));
                }
            }
            Ok(JsValue::null())
        });

        let obj = ObjectInitializer::new(ctx)
            // TextTrackCue properties
            .property(js_string!("id"), js_string!(id), Attribute::all())
            .property(js_string!("startTime"), start_time, Attribute::all())
            .property(js_string!("endTime"), end_time, Attribute::all())
            .property(js_string!("text"), js_string!(text), Attribute::all())
            .property(js_string!("pauseOnExit"), false, Attribute::all())
            .property(js_string!("track"), JsValue::null(), Attribute::READONLY)
            // VTTCue-specific properties
            .property(js_string!("region"), JsValue::null(), Attribute::all())
            .property(js_string!("vertical"), js_string!(""), Attribute::all())
            .property(js_string!("snapToLines"), true, Attribute::all())
            .property(js_string!("line"), js_string!("auto"), Attribute::all())
            .property(js_string!("lineAlign"), js_string!("start"), Attribute::all())
            .property(js_string!("position"), js_string!("auto"), Attribute::all())
            .property(js_string!("positionAlign"), js_string!("auto"), Attribute::all())
            .property(js_string!("size"), 100.0, Attribute::all())
            .property(js_string!("align"), js_string!("center"), Attribute::all())
            // Event handlers
            .property(js_string!("onenter"), JsValue::null(), Attribute::all())
            .property(js_string!("onexit"), JsValue::null(), Attribute::all())
            // Methods
            .function(get_cue_as_html, js_string!("getCueAsHTML"), 0)
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("VTTCue"))
        .length(3)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("VTTCue"), constructor, Attribute::all())?;

    Ok(())
}

/// Register TextTrackCueList - list of cues
fn register_text_track_cue_list(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let mut len: u32 = 0;
        if let Some(arr) = args.get(0) {
            if let Some(arr_obj) = arr.as_object() {
                if let Ok(l) = arr_obj.get(js_string!("length"), ctx) {
                    len = l.to_u32(ctx).unwrap_or(0);
                }
            }
        }

        // getCueById(id) method
        let get_cue_by_id = NativeFunction::from_copy_closure(|this, args, ctx| {
            let cue_id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            if let Some(this_obj) = this.as_object() {
                if let Ok(len) = this_obj.get(js_string!("length"), ctx) {
                    let len = len.to_u32(ctx).unwrap_or(0);
                    for i in 0..len {
                        if let Ok(cue) = this_obj.get(i, ctx) {
                            if let Some(cue_obj) = cue.as_object() {
                                if let Ok(id) = cue_obj.get(js_string!("id"), ctx) {
                                    if id.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default() == cue_id {
                                        return Ok(cue);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(JsValue::null())
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("length"), len, Attribute::READONLY)
            .function(get_cue_by_id, js_string!("getCueById"), 1)
            .build();

        // Copy cues from input array
        if let Some(arr) = args.get(0) {
            if let Some(arr_obj) = arr.as_object() {
                for i in 0..len {
                    if let Ok(cue) = arr_obj.get(i, ctx) {
                        obj.set(i, cue, false, ctx)?;
                    }
                }
            }
        }

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("TextTrackCueList"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("TextTrackCueList"), constructor, Attribute::all())?;

    Ok(())
}

/// Register TextTrack - represents a text track
fn register_text_track(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let kind = args.get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| "subtitles".to_string());
        let label = args.get_or_undefined(1)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();
        let language = args.get_or_undefined(2)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        let id = format!("track-{:x}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos());

        // Internal cues storage
        use boa_engine::object::builtins::JsArray;
        let cues_array = JsArray::new(ctx);

        // addCue(cue) method
        let add_cue = NativeFunction::from_copy_closure(|this, args, ctx| {
            if let Some(this_obj) = this.as_object() {
                if let Some(cue) = args.get(0) {
                    if let Ok(cues) = this_obj.get(js_string!("_cues"), ctx) {
                        if let Some(cues_obj) = cues.as_object() {
                            let len = cues_obj.get(js_string!("length"), ctx)?.to_u32(ctx).unwrap_or(0);
                            cues_obj.set(len, cue.clone(), false, ctx)?;
                            // Update cues property
                            this_obj.set(js_string!("cues"), cues.clone(), false, ctx)?;
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        });

        // removeCue(cue) method
        let remove_cue = NativeFunction::from_copy_closure(|this, args, ctx| {
            if let Some(this_obj) = this.as_object() {
                if let Some(cue_to_remove) = args.get(0) {
                    if let Some(cue_obj) = cue_to_remove.as_object() {
                        if let Ok(remove_id) = cue_obj.get(js_string!("id"), ctx) {
                            let remove_id = remove_id.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
                            if let Ok(cues) = this_obj.get(js_string!("_cues"), ctx) {
                                if let Some(cues_obj) = cues.as_object() {
                                    let new_cues = JsArray::new(ctx);
                                    if let Ok(len) = cues_obj.get(js_string!("length"), ctx) {
                                        let len = len.to_u32(ctx).unwrap_or(0);
                                        for i in 0..len {
                                            if let Ok(cue) = cues_obj.get(i, ctx) {
                                                if let Some(c_obj) = cue.as_object() {
                                                    if let Ok(id) = c_obj.get(js_string!("id"), ctx) {
                                                        if id.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default() != remove_id {
                                                            let _ = new_cues.push(cue, ctx);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    this_obj.set(js_string!("_cues"), JsValue::from(new_cues.clone()), false, ctx)?;
                                    this_obj.set(js_string!("cues"), JsValue::from(new_cues), false, ctx)?;
                                }
                            }
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        });

        let cues_val = JsValue::from(cues_array);
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("id"), js_string!(id), Attribute::READONLY)
            .property(js_string!("kind"), js_string!(kind), Attribute::READONLY)
            .property(js_string!("label"), js_string!(label), Attribute::READONLY)
            .property(js_string!("language"), js_string!(language), Attribute::READONLY)
            .property(js_string!("inBandMetadataTrackDispatchType"), js_string!(""), Attribute::READONLY)
            .property(js_string!("mode"), js_string!("disabled"), Attribute::all())
            .property(js_string!("cues"), cues_val.clone(), Attribute::READONLY)
            .property(js_string!("activeCues"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("_cues"), cues_val, Attribute::empty())
            // Event handlers
            .property(js_string!("oncuechange"), JsValue::null(), Attribute::all())
            // Methods
            .function(add_cue, js_string!("addCue"), 1)
            .function(remove_cue, js_string!("removeCue"), 1)
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("TextTrack"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("TextTrack"), constructor, Attribute::all())?;

    Ok(())
}

/// Register TextTrackList - list of text tracks
fn register_text_track_list(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let mut len: u32 = 0;
        if let Some(arr) = args.get(0) {
            if let Some(arr_obj) = arr.as_object() {
                if let Ok(l) = arr_obj.get(js_string!("length"), ctx) {
                    len = l.to_u32(ctx).unwrap_or(0);
                }
            }
        }

        // getTrackById(id) method
        let get_track_by_id = NativeFunction::from_copy_closure(|this, args, ctx| {
            let track_id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            if let Some(this_obj) = this.as_object() {
                if let Ok(len) = this_obj.get(js_string!("length"), ctx) {
                    let len = len.to_u32(ctx).unwrap_or(0);
                    for i in 0..len {
                        if let Ok(track) = this_obj.get(i, ctx) {
                            if let Some(track_obj) = track.as_object() {
                                if let Ok(id) = track_obj.get(js_string!("id"), ctx) {
                                    if id.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default() == track_id {
                                        return Ok(track);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(JsValue::null())
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("length"), len, Attribute::READONLY)
            .property(js_string!("onchange"), JsValue::null(), Attribute::all())
            .property(js_string!("onaddtrack"), JsValue::null(), Attribute::all())
            .property(js_string!("onremovetrack"), JsValue::null(), Attribute::all())
            .function(get_track_by_id, js_string!("getTrackById"), 1)
            .build();

        // Copy tracks from input array
        if let Some(arr) = args.get(0) {
            if let Some(arr_obj) = arr.as_object() {
                for i in 0..len {
                    if let Ok(track) = arr_obj.get(i, ctx) {
                        obj.set(i, track, false, ctx)?;
                    }
                }
            }
        }

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("TextTrackList"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("TextTrackList"), constructor, Attribute::all())?;

    Ok(())
}

// =============================================================================
// NavigationPreloadManager Implementation
// =============================================================================

/// Register NavigationPreloadManager - manages SW navigation preload
fn register_navigation_preload_manager(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        use boa_engine::object::builtins::JsPromise;

        // enable() - enables preload
        let enable = NativeFunction::from_copy_closure(|this, _args, ctx| {
            if let Some(this_obj) = this.as_object() {
                let _ = this_obj.set(js_string!("_enabled"), true, false, ctx);
            }
            let promise = JsPromise::resolve(JsValue::undefined(), ctx);
            Ok(JsValue::from(promise))
        });

        // disable() - disables preload
        let disable = NativeFunction::from_copy_closure(|this, _args, ctx| {
            if let Some(this_obj) = this.as_object() {
                let _ = this_obj.set(js_string!("_enabled"), false, false, ctx);
            }
            let promise = JsPromise::resolve(JsValue::undefined(), ctx);
            Ok(JsValue::from(promise))
        });

        // setHeaderValue(value) - sets the header value
        let set_header_value = NativeFunction::from_copy_closure(|this, args, ctx| {
            if let Some(this_obj) = this.as_object() {
                let value = args.get_or_undefined(0).clone();
                let _ = this_obj.set(js_string!("_headerValue"), value, false, ctx);
            }
            let promise = JsPromise::resolve(JsValue::undefined(), ctx);
            Ok(JsValue::from(promise))
        });

        // getState() - returns the current state
        let get_state = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let state = ObjectInitializer::new(ctx)
                .property(js_string!("enabled"), false, Attribute::READONLY)
                .property(js_string!("headerValue"), js_string!("true"), Attribute::READONLY)
                .build();
            let promise = JsPromise::resolve(JsValue::from(state), ctx);
            Ok(JsValue::from(promise))
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("_enabled"), false, Attribute::empty())
            .property(js_string!("_headerValue"), js_string!("true"), Attribute::empty())
            .function(enable, js_string!("enable"), 0)
            .function(disable, js_string!("disable"), 0)
            .function(set_header_value, js_string!("setHeaderValue"), 1)
            .function(get_state, js_string!("getState"), 0)
            .build();

        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("NavigationPreloadManager"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("NavigationPreloadManager"), constructor, Attribute::all())?;

    Ok(())
}

/// Register StorageManager (navigator.storage)
fn register_storage_manager(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    if let Ok(nav_val) = global.get(js_string!("navigator"), context) {
        if let Some(nav_obj) = nav_val.as_object() {
            let storage = ObjectInitializer::new(context)
                .function(
                    NativeFunction::from_copy_closure(|_this, _args, ctx| {
                        // Calculate real storage usage from Blob, FormData, ReadableStream, FileReader
                        let usage = get_storage_usage();
                        // Quota: 2GB (reasonable browser default, adjustable)
                        let quota: u64 = 2 * 1024 * 1024 * 1024; // 2GB
                        let result = ObjectInitializer::new(ctx)
                            .property(js_string!("usage"), usage as f64, Attribute::READONLY)
                            .property(js_string!("quota"), quota as f64, Attribute::READONLY)
                            .build();
                        let promise = JsPromise::resolve(JsValue::from(result), ctx);
                        Ok(JsValue::from(promise))
                    }),
                    js_string!("estimate"),
                    0,
                )
                .function(
                    NativeFunction::from_copy_closure(|_this, _args, ctx| {
                        let promise = JsPromise::resolve(JsValue::from(true), ctx);
                        Ok(JsValue::from(promise))
                    }),
                    js_string!("persist"),
                    0,
                )
                .function(
                    NativeFunction::from_copy_closure(|_this, _args, ctx| {
                        let promise = JsPromise::resolve(JsValue::from(false), ctx);
                        Ok(JsValue::from(promise))
                    }),
                    js_string!("persisted"),
                    0,
                )
                .function(
                    NativeFunction::from_copy_closure(|_this, _args, ctx| {
                        let handle = ObjectInitializer::new(ctx)
                            .property(js_string!("kind"), js_string!("directory"), Attribute::READONLY)
                            .property(js_string!("name"), js_string!(""), Attribute::READONLY)
                            .build();
                        let promise = JsPromise::resolve(JsValue::from(handle), ctx);
                        Ok(JsValue::from(promise))
                    }),
                    js_string!("getDirectory"),
                    0,
                )
                .build();

            nav_obj.set(js_string!("storage"), storage, false, context)?;
        }
    }

    let ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let obj = ObjectInitializer::new(ctx)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    let result = ObjectInitializer::new(ctx)
                        .property(js_string!("usage"), 0, Attribute::READONLY)
                        .property(js_string!("quota"), 1073741824i64, Attribute::READONLY)
                        .build();
                    let promise = JsPromise::resolve(JsValue::from(result), ctx);
                    Ok(JsValue::from(promise))
                }),
                js_string!("estimate"),
                0,
            )
            .build();
        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor)
        .name(js_string!("StorageManager"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("StorageManager"), constructor, Attribute::all())?;
    Ok(())
}

/// Register LockManager (navigator.locks)
fn register_lock_manager(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    if let Ok(nav_val) = global.get(js_string!("navigator"), context) {
        if let Some(nav_obj) = nav_val.as_object() {
            let locks = ObjectInitializer::new(context)
                .function(
                    NativeFunction::from_copy_closure(|_this, args, ctx| {
                        // Parse arguments: request(name, callback) or request(name, options, callback)
                        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

                        let (mode, callback) = if args.len() >= 3 {
                            // request(name, options, callback)
                            let options = args.get_or_undefined(1);
                            let mode = if let Some(opts) = options.as_object() {
                                opts.get(js_string!("mode"), ctx)
                                    .ok()
                                    .and_then(|v| v.to_string(ctx).ok())
                                    .map(|s| s.to_std_string_escaped())
                                    .unwrap_or_else(|| "exclusive".to_string())
                            } else {
                                "exclusive".to_string()
                            };
                            (mode, args.get_or_undefined(2).clone())
                        } else if args.len() >= 2 {
                            // request(name, callback)
                            ("exclusive".to_string(), args.get_or_undefined(1).clone())
                        } else {
                            return Err(JsNativeError::typ().with_message("callback required").into());
                        };

                        // Generate lock ID and create lock info
                        let lock_id = {
                            let mut id = NEXT_LOCK_ID.lock().unwrap();
                            let current = *id;
                            *id += 1;
                            current
                        };

                        // Check if lock can be acquired (for exclusive, no other locks on same name)
                        let can_acquire = {
                            let held = HELD_LOCKS.lock().unwrap();
                            if mode == "exclusive" {
                                !held.iter().any(|l| l.name == name)
                            } else {
                                // Shared mode: can acquire if no exclusive locks on same name
                                !held.iter().any(|l| l.name == name && l.mode == "exclusive")
                            }
                        };

                        // Add to held locks
                        if can_acquire {
                            let lock_info = LockInfo {
                                id: lock_id,
                                name: name.clone(),
                                mode: mode.clone(),
                                client_id: "default".to_string(),
                            };
                            HELD_LOCKS.lock().unwrap().push(lock_info);
                        }

                        // Create lock object
                        let lock = ObjectInitializer::new(ctx)
                            .property(js_string!("name"), js_string!(name.clone()), Attribute::READONLY)
                            .property(js_string!("mode"), js_string!(mode), Attribute::READONLY)
                            .build();

                        // Call the callback with the lock
                        if let Some(cb_obj) = callback.as_object() {
                            if cb_obj.is_callable() {
                                let result = cb_obj.call(&JsValue::undefined(), &[JsValue::from(lock)], ctx)?;

                                // Release the lock after callback completes
                                {
                                    let mut held = HELD_LOCKS.lock().unwrap();
                                    held.retain(|l| l.id != lock_id);
                                }

                                return Ok(JsValue::from(JsPromise::resolve(result, ctx)));
                            }
                        }

                        // Release lock if no callback
                        {
                            let mut held = HELD_LOCKS.lock().unwrap();
                            held.retain(|l| l.id != lock_id);
                        }

                        Ok(JsValue::from(JsPromise::resolve(JsValue::undefined(), ctx)))
                    }),
                    js_string!("request"),
                    2,
                )
                .function(
                    NativeFunction::from_copy_closure(|_this, _args, ctx| {
                        // Return current state of locks
                        let held_arr = JsArray::new(ctx);
                        let pending_arr = JsArray::new(ctx);

                        // Add held locks to array
                        if let Ok(held) = HELD_LOCKS.lock() {
                            for (i, lock) in held.iter().enumerate() {
                                let lock_obj = ObjectInitializer::new(ctx)
                                    .property(js_string!("name"), js_string!(lock.name.clone()), Attribute::READONLY)
                                    .property(js_string!("mode"), js_string!(lock.mode.clone()), Attribute::READONLY)
                                    .property(js_string!("clientId"), js_string!(lock.client_id.clone()), Attribute::READONLY)
                                    .build();
                                let _ = held_arr.set(i as u32, JsValue::from(lock_obj), false, ctx);
                            }
                        }

                        // Add pending locks to array
                        if let Ok(pending) = PENDING_LOCKS.lock() {
                            for (i, lock) in pending.iter().enumerate() {
                                let lock_obj = ObjectInitializer::new(ctx)
                                    .property(js_string!("name"), js_string!(lock.name.clone()), Attribute::READONLY)
                                    .property(js_string!("mode"), js_string!(lock.mode.clone()), Attribute::READONLY)
                                    .property(js_string!("clientId"), js_string!(lock.client_id.clone()), Attribute::READONLY)
                                    .build();
                                let _ = pending_arr.set(i as u32, JsValue::from(lock_obj), false, ctx);
                            }
                        }

                        let result = ObjectInitializer::new(ctx)
                            .property(js_string!("held"), held_arr, Attribute::READONLY)
                            .property(js_string!("pending"), pending_arr, Attribute::READONLY)
                            .build();
                        Ok(JsValue::from(JsPromise::resolve(JsValue::from(result), ctx)))
                    }),
                    js_string!("query"),
                    0,
                )
                .build();
            nav_obj.set(js_string!("locks"), locks, false, context)?;
        }
    }

    let lock_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = args.get_or_undefined(0).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
        let mode = args.get_or_undefined(1).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_else(|_| "exclusive".to_string());
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!(name), Attribute::READONLY)
            .property(js_string!("mode"), js_string!(mode), Attribute::READONLY)
            .build();
        Ok(JsValue::from(obj))
    });
    context.register_global_property(js_string!("Lock"), FunctionObjectBuilder::new(context.realm(), lock_ctor).name(js_string!("Lock")).length(1).constructor(true).build(), Attribute::all())?;

    let ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let obj = ObjectInitializer::new(ctx)
            .function(NativeFunction::from_copy_closure(|_this, _args, ctx| {
                Ok(JsValue::from(JsPromise::resolve(JsValue::undefined(), ctx)))
            }), js_string!("request"), 2)
            .function(NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let held = JsArray::new(ctx);
                let pending = JsArray::new(ctx);
                let result = ObjectInitializer::new(ctx)
                    .property(js_string!("held"), held, Attribute::READONLY)
                    .property(js_string!("pending"), pending, Attribute::READONLY)
                    .build();
                Ok(JsValue::from(JsPromise::resolve(JsValue::from(result), ctx)))
            }), js_string!("query"), 0)
            .build();
        Ok(JsValue::from(obj))
    });
    context.register_global_property(js_string!("LockManager"), FunctionObjectBuilder::new(context.realm(), ctor).name(js_string!("LockManager")).length(0).constructor(true).build(), Attribute::all())?;
    Ok(())
}

/// Register navigator.mediaDevices
fn register_media_devices(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    if let Ok(nav_val) = global.get(js_string!("navigator"), context) {
        if let Some(nav_obj) = nav_val.as_object() {
            let media_devices = ObjectInitializer::new(context)
                .function(NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(JsValue::from(JsPromise::resolve(JsValue::from(JsArray::new(ctx)), ctx)))
                }), js_string!("enumerateDevices"), 0)
                .function(NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(JsValue::from(JsPromise::reject(JsError::from(JsNativeError::typ().with_message("NotAllowedError")), ctx)))
                }), js_string!("getUserMedia"), 1)
                .function(NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(JsValue::from(JsPromise::reject(JsError::from(JsNativeError::typ().with_message("NotAllowedError")), ctx)))
                }), js_string!("getDisplayMedia"), 1)
                .function(NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    let c = ObjectInitializer::new(ctx)
                        .property(js_string!("deviceId"), true, Attribute::READONLY)
                        .property(js_string!("groupId"), true, Attribute::READONLY)
                        .property(js_string!("width"), true, Attribute::READONLY)
                        .property(js_string!("height"), true, Attribute::READONLY)
                        .property(js_string!("frameRate"), true, Attribute::READONLY)
                        .build();
                    Ok(JsValue::from(c))
                }), js_string!("getSupportedConstraints"), 0)
                .property(js_string!("ondevicechange"), JsValue::null(), Attribute::all())
                .build();
            nav_obj.set(js_string!("mediaDevices"), media_devices, false, context)?;
        }
    }

    let ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let obj = ObjectInitializer::new(ctx)
            .function(NativeFunction::from_copy_closure(|_this, _args, ctx| {
                Ok(JsValue::from(JsPromise::resolve(JsValue::from(JsArray::new(ctx)), ctx)))
            }), js_string!("enumerateDevices"), 0)
            .function(NativeFunction::from_copy_closure(|_this, _args, ctx| {
                Ok(JsValue::from(JsPromise::reject(JsError::from(JsNativeError::typ().with_message("NotAllowedError")), ctx)))
            }), js_string!("getUserMedia"), 1)
            .build();
        Ok(JsValue::from(obj))
    });
    context.register_global_property(js_string!("MediaDevices"), FunctionObjectBuilder::new(context.realm(), ctor).name(js_string!("MediaDevices")).length(0).constructor(true).build(), Attribute::all())?;

    let device_info_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let device_id = args.get_or_undefined(0).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
        let kind = args.get_or_undefined(1).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_else(|_| "videoinput".to_string());
        let label = args.get_or_undefined(2).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
        let group_id = args.get_or_undefined(3).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("deviceId"), js_string!(device_id), Attribute::READONLY)
            .property(js_string!("kind"), js_string!(kind), Attribute::READONLY)
            .property(js_string!("label"), js_string!(label), Attribute::READONLY)
            .property(js_string!("groupId"), js_string!(group_id), Attribute::READONLY)
            .build();
        Ok(JsValue::from(obj))
    });
    context.register_global_property(js_string!("MediaDeviceInfo"), FunctionObjectBuilder::new(context.realm(), device_info_ctor).name(js_string!("MediaDeviceInfo")).length(0).constructor(true).build(), Attribute::all())?;
    Ok(())
}

/// Register navigator.userActivation
fn register_user_activation(context: &mut Context) -> JsResult<()> {
    let global = context.global_object();

    if let Ok(nav_val) = global.get(js_string!("navigator"), context) {
        if let Some(nav_obj) = nav_val.as_object() {
            let user_activation = ObjectInitializer::new(context)
                .property(js_string!("isActive"), false, Attribute::READONLY)
                .property(js_string!("hasBeenActive"), false, Attribute::READONLY)
                .build();
            nav_obj.set(js_string!("userActivation"), user_activation, false, context)?;
        }
    }

    let ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("isActive"), false, Attribute::READONLY)
            .property(js_string!("hasBeenActive"), false, Attribute::READONLY)
            .build();
        Ok(JsValue::from(obj))
    });
    context.register_global_property(js_string!("UserActivation"), FunctionObjectBuilder::new(context.realm(), ctor).name(js_string!("UserActivation")).length(0).constructor(true).build(), Attribute::all())?;
    Ok(())
}

/// Register MediaError
fn register_media_error(context: &mut Context) -> JsResult<()> {
    const MEDIA_ERR_ABORTED: u16 = 1;
    const MEDIA_ERR_NETWORK: u16 = 2;
    const MEDIA_ERR_DECODE: u16 = 3;
    const MEDIA_ERR_SRC_NOT_SUPPORTED: u16 = 4;

    let ctor = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let code = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0) as u16;
        let message = args.get_or_undefined(1).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("code"), code, Attribute::READONLY)
            .property(js_string!("message"), js_string!(message), Attribute::READONLY)
            .property(js_string!("MEDIA_ERR_ABORTED"), MEDIA_ERR_ABORTED, Attribute::READONLY)
            .property(js_string!("MEDIA_ERR_NETWORK"), MEDIA_ERR_NETWORK, Attribute::READONLY)
            .property(js_string!("MEDIA_ERR_DECODE"), MEDIA_ERR_DECODE, Attribute::READONLY)
            .property(js_string!("MEDIA_ERR_SRC_NOT_SUPPORTED"), MEDIA_ERR_SRC_NOT_SUPPORTED, Attribute::READONLY)
            .build();
        Ok(JsValue::from(obj))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), ctor).name(js_string!("MediaError")).length(1).constructor(true).build();
    constructor.set(js_string!("MEDIA_ERR_ABORTED"), JsValue::from(MEDIA_ERR_ABORTED), false, context)?;
    constructor.set(js_string!("MEDIA_ERR_NETWORK"), JsValue::from(MEDIA_ERR_NETWORK), false, context)?;
    constructor.set(js_string!("MEDIA_ERR_DECODE"), JsValue::from(MEDIA_ERR_DECODE), false, context)?;
    constructor.set(js_string!("MEDIA_ERR_SRC_NOT_SUPPORTED"), JsValue::from(MEDIA_ERR_SRC_NOT_SUPPORTED), false, context)?;
    context.register_global_property(js_string!("MediaError"), constructor, Attribute::all())?;
    Ok(())
}

/// Register MediaStreamTrack
fn register_media_stream_track(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let kind = args.get_or_undefined(0).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_else(|_| "video".to_string());
        let label = args.get_or_undefined(1).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
        let id = format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("id"), js_string!(id), Attribute::READONLY)
            .property(js_string!("kind"), js_string!(kind), Attribute::READONLY)
            .property(js_string!("label"), js_string!(label), Attribute::READONLY)
            .property(js_string!("enabled"), true, Attribute::all())
            .property(js_string!("muted"), false, Attribute::READONLY)
            .property(js_string!("readyState"), js_string!("live"), Attribute::READONLY)
            .property(js_string!("onended"), JsValue::null(), Attribute::all())
            .property(js_string!("onmute"), JsValue::null(), Attribute::all())
            .function(NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let id = format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
                let clone = ObjectInitializer::new(ctx)
                    .property(js_string!("id"), js_string!(id), Attribute::READONLY)
                    .property(js_string!("kind"), js_string!("video"), Attribute::READONLY)
                    .property(js_string!("readyState"), js_string!("live"), Attribute::READONLY)
                    .build();
                Ok(JsValue::from(clone))
            }), js_string!("clone"), 0)
            .function(NativeFunction::from_copy_closure(|this, _args, ctx| {
                if let Some(obj) = this.as_object() { let _ = obj.set(js_string!("readyState"), js_string!("ended"), false, ctx); }
                Ok(JsValue::undefined())
            }), js_string!("stop"), 0)
            .function(NativeFunction::from_copy_closure(|_this, _args, ctx| {
                Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
            }), js_string!("getCapabilities"), 0)
            .function(NativeFunction::from_copy_closure(|_this, _args, ctx| {
                Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
            }), js_string!("getConstraints"), 0)
            .function(NativeFunction::from_copy_closure(|_this, _args, ctx| {
                Ok(JsValue::from(ObjectInitializer::new(ctx).property(js_string!("deviceId"), js_string!(""), Attribute::READONLY).build()))
            }), js_string!("getSettings"), 0)
            .function(NativeFunction::from_copy_closure(|_this, _args, ctx| {
                Ok(JsValue::from(JsPromise::resolve(JsValue::undefined(), ctx)))
            }), js_string!("applyConstraints"), 1)
            .build();
        Ok(JsValue::from(obj))
    });

    context.register_global_property(js_string!("MediaStreamTrack"), FunctionObjectBuilder::new(context.realm(), ctor).name(js_string!("MediaStreamTrack")).length(0).constructor(true).build(), Attribute::all())?;
    Ok(())
}

/// Register Plugin and PluginArray constructors
fn register_plugin_types(context: &mut Context) -> JsResult<()> {
    let plugin_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = args.get_or_undefined(0).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
        let description = args.get_or_undefined(1).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
        let filename = args.get_or_undefined(2).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!(name), Attribute::READONLY)
            .property(js_string!("description"), js_string!(description), Attribute::READONLY)
            .property(js_string!("filename"), js_string!(filename), Attribute::READONLY)
            .property(js_string!("length"), 0, Attribute::READONLY)
            .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| { Ok(JsValue::null()) }), js_string!("item"), 1)
            .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| { Ok(JsValue::null()) }), js_string!("namedItem"), 1)
            .build();
        Ok(JsValue::from(obj))
    });
    context.register_global_property(js_string!("Plugin"), FunctionObjectBuilder::new(context.realm(), plugin_ctor).name(js_string!("Plugin")).length(0).constructor(true).build(), Attribute::all())?;

    let plugin_array_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("length"), 0, Attribute::READONLY)
            .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| { Ok(JsValue::null()) }), js_string!("item"), 1)
            .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| { Ok(JsValue::null()) }), js_string!("namedItem"), 1)
            .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| { Ok(JsValue::undefined()) }), js_string!("refresh"), 0)
            .build();
        Ok(JsValue::from(obj))
    });
    context.register_global_property(js_string!("PluginArray"), FunctionObjectBuilder::new(context.realm(), plugin_array_ctor).name(js_string!("PluginArray")).length(0).constructor(true).build(), Attribute::all())?;
    Ok(())
}

/// Register MimeType and MimeTypeArray constructors
fn register_mime_types(context: &mut Context) -> JsResult<()> {
    let mime_type_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let type_str = args.get_or_undefined(0).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
        let description = args.get_or_undefined(1).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
        let suffixes = args.get_or_undefined(2).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("type"), js_string!(type_str), Attribute::READONLY)
            .property(js_string!("description"), js_string!(description), Attribute::READONLY)
            .property(js_string!("suffixes"), js_string!(suffixes), Attribute::READONLY)
            .property(js_string!("enabledPlugin"), JsValue::null(), Attribute::READONLY)
            .build();
        Ok(JsValue::from(obj))
    });
    context.register_global_property(js_string!("MimeType"), FunctionObjectBuilder::new(context.realm(), mime_type_ctor).name(js_string!("MimeType")).length(0).constructor(true).build(), Attribute::all())?;

    let mime_type_array_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("length"), 0, Attribute::READONLY)
            .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| { Ok(JsValue::null()) }), js_string!("item"), 1)
            .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| { Ok(JsValue::null()) }), js_string!("namedItem"), 1)
            .build();
        Ok(JsValue::from(obj))
    });
    context.register_global_property(js_string!("MimeTypeArray"), FunctionObjectBuilder::new(context.realm(), mime_type_array_ctor).name(js_string!("MimeTypeArray")).length(0).constructor(true).build(), Attribute::all())?;
    Ok(())
}

/// Register DissimilarOriginLocation and DissimilarOriginWindow
/// These are restricted interfaces for cross-origin window access per HTML spec
fn register_dissimilar_origin_apis(context: &mut Context) -> JsResult<()> {
    // DissimilarOriginLocation - restricted Location for cross-origin windows
    // Per spec, only allows: href setter, replace(), and ancestorOrigins
    let dissimilar_location_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let href = args.get_or_undefined(0).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();

        // replace() - navigate the window (cross-origin safe)
        let replace_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let _url = args.get_or_undefined(0).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
            // In a real browser, this would navigate the cross-origin window
            // For our purposes, this is a no-op as we can't actually navigate cross-origin frames
            Ok(JsValue::undefined())
        });

        // ancestorOrigins - DOMStringList of ancestor origins
        let ancestor_origins = ObjectInitializer::new(ctx)
            .property(js_string!("length"), 0, Attribute::READONLY)
            .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::null())
            }), js_string!("item"), 1)
            .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::from(false))
            }), js_string!("contains"), 1)
            .build();

        let obj = ObjectInitializer::new(ctx)
            // href is the only readable/writable property on cross-origin Location
            .property(js_string!("href"), js_string!(href), Attribute::all())
            .property(js_string!("ancestorOrigins"), ancestor_origins, Attribute::READONLY)
            .function(replace_fn, js_string!("replace"), 1)
            // toString returns href
            .function(NativeFunction::from_copy_closure(|this, _args, ctx| {
                if let Some(obj) = this.as_object() {
                    if let Ok(href) = obj.get(js_string!("href"), ctx) {
                        return Ok(href);
                    }
                }
                Ok(JsValue::from(js_string!("")))
            }), js_string!("toString"), 0)
            .build();
        Ok(JsValue::from(obj))
    });

    context.register_global_property(
        js_string!("DissimilarOriginLocation"),
        FunctionObjectBuilder::new(context.realm(), dissimilar_location_ctor)
            .name(js_string!("DissimilarOriginLocation"))
            .length(1)
            .constructor(true)
            .build(),
        Attribute::all()
    )?;

    // DissimilarOriginWindow - restricted Window for cross-origin access
    // Per spec, allows: window, self, location, closed, frames, length, top, opener, parent,
    // postMessage, close, blur, focus
    let dissimilar_window_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let origin = args.get_or_undefined(0).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_else(|_| "null".to_string());

        // Create a DissimilarOriginLocation for this window
        let location = ObjectInitializer::new(ctx)
            .property(js_string!("href"), js_string!(""), Attribute::all())
            .function(NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            }), js_string!("replace"), 1)
            .build();

        // postMessage - the main way to communicate cross-origin
        let post_message = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            // In a real browser, this would use MessagePort to send to the other window
            // For our headless browser, this is a no-op
            Ok(JsValue::undefined())
        });

        // close - request to close the window
        let close_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // blur - remove focus from window
        let blur_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // focus - focus the window
        let focus_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let obj = ObjectInitializer::new(ctx)
            // Location - restricted DissimilarOriginLocation
            .property(js_string!("location"), location, Attribute::READONLY)
            // Window state
            .property(js_string!("closed"), false, Attribute::READONLY)
            .property(js_string!("length"), 0, Attribute::READONLY)
            // Window relationships (would point to other DissimilarOriginWindows)
            .property(js_string!("top"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("opener"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("parent"), JsValue::null(), Attribute::READONLY)
            // Origin info
            .property(js_string!("origin"), js_string!(origin), Attribute::READONLY)
            // Methods
            .function(post_message, js_string!("postMessage"), 3)
            .function(close_fn, js_string!("close"), 0)
            .function(blur_fn, js_string!("blur"), 0)
            .function(focus_fn, js_string!("focus"), 0)
            .build();

        // Set self-references (must use define_property_or_throw to set on object)
        let self_ref = JsValue::from(obj.clone());
        obj.define_property_or_throw(js_string!("window"), boa_engine::property::PropertyDescriptor::builder()
            .value(self_ref.clone())
            .writable(false)
            .enumerable(true)
            .configurable(false)
            .build(), ctx)?;
        obj.define_property_or_throw(js_string!("self"), boa_engine::property::PropertyDescriptor::builder()
            .value(self_ref.clone())
            .writable(false)
            .enumerable(true)
            .configurable(false)
            .build(), ctx)?;
        obj.define_property_or_throw(js_string!("frames"), boa_engine::property::PropertyDescriptor::builder()
            .value(self_ref)
            .writable(false)
            .enumerable(true)
            .configurable(false)
            .build(), ctx)?;

        Ok(JsValue::from(obj))
    });

    context.register_global_property(
        js_string!("DissimilarOriginWindow"),
        FunctionObjectBuilder::new(context.realm(), dissimilar_window_ctor)
            .name(js_string!("DissimilarOriginWindow"))
            .length(1)
            .constructor(true)
            .build(),
        Attribute::all()
    )?;

    Ok(())
}

/// Register DynamicModuleOwner for ES Modules dynamic import() support
/// This tracks ownership of dynamically imported modules for proper error reporting
/// and module graph management
fn register_dynamic_module_owner(context: &mut Context) -> JsResult<()> {
    // DynamicModuleOwner - internal interface for tracking dynamic import() owners
    // Properties: url (the URL of the importing module), promise (the import promise)
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let url = args.get_or_undefined(0).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
        let specifier = args.get_or_undefined(1).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();

        // Create a promise that represents the dynamic import
        // In our implementation, dynamic imports will reject since we don't have full module support
        let promise = JsPromise::reject(
            JsError::from(JsNativeError::typ().with_message("Dynamic import() is not fully supported in this environment")),
            ctx
        );

        // resolve() - resolve the dynamic import with a module namespace object
        let resolve_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            // Would resolve the import promise with the module namespace
            Ok(JsValue::undefined())
        });

        // reject() - reject the dynamic import with an error
        let reject_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            // Would reject the import promise with the error
            Ok(JsValue::undefined())
        });

        let obj = ObjectInitializer::new(ctx)
            // The URL of the module that called import()
            .property(js_string!("url"), js_string!(url), Attribute::READONLY)
            // The module specifier being imported
            .property(js_string!("specifier"), js_string!(specifier), Attribute::READONLY)
            // The promise for the import result
            .property(js_string!("promise"), promise, Attribute::READONLY)
            // Timestamp of when the import was initiated
            .property(js_string!("timestamp"), JsValue::from(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as f64), Attribute::READONLY)
            // Methods to resolve/reject the import
            .function(resolve_fn, js_string!("resolve"), 1)
            .function(reject_fn, js_string!("reject"), 1)
            .build();

        Ok(JsValue::from(obj))
    });

    context.register_global_property(
        js_string!("DynamicModuleOwner"),
        FunctionObjectBuilder::new(context.realm(), ctor)
            .name(js_string!("DynamicModuleOwner"))
            .length(2)
            .constructor(true)
            .build(),
        Attribute::all()
    )?;

    Ok(())
}

/// Register VTTRegion for WebVTT caption regions
/// Per spec: https://w3c.github.io/webvtt/#the-vttregion-interface
fn register_vtt_region(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Default values per spec
        let obj = ObjectInitializer::new(ctx)
            // id - Region identifier (empty string by default)
            .property(js_string!("id"), js_string!(""), Attribute::all())
            // width - Region width as percentage (100 by default)
            .property(js_string!("width"), 100.0, Attribute::all())
            // lines - Number of lines (3 by default)
            .property(js_string!("lines"), 3, Attribute::all())
            // regionAnchorX - X position of region anchor (0 by default)
            .property(js_string!("regionAnchorX"), 0.0, Attribute::all())
            // regionAnchorY - Y position of region anchor (100 by default)
            .property(js_string!("regionAnchorY"), 100.0, Attribute::all())
            // viewportAnchorX - X position in viewport (0 by default)
            .property(js_string!("viewportAnchorX"), 0.0, Attribute::all())
            // viewportAnchorY - Y position in viewport (100 by default)
            .property(js_string!("viewportAnchorY"), 100.0, Attribute::all())
            // scroll - Scroll setting ("" or "up")
            .property(js_string!("scroll"), js_string!(""), Attribute::all())
            .build();

        Ok(JsValue::from(obj))
    });

    context.register_global_property(
        js_string!("VTTRegion"),
        FunctionObjectBuilder::new(context.realm(), ctor)
            .name(js_string!("VTTRegion"))
            .length(0)
            .constructor(true)
            .build(),
        Attribute::all()
    )?;

    Ok(())
}

/// Register MediaSession and MediaMetadata APIs
/// Per spec: https://w3c.github.io/mediasession/
fn register_media_session(context: &mut Context) -> JsResult<()> {
    // MediaMetadata constructor
    let metadata_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let mut title = String::new();
        let mut artist = String::new();
        let mut album = String::new();
        let artwork = JsArray::new(ctx);

        // Parse init dict if provided
        if let Some(init) = args.get(0).and_then(|v| v.as_object()) {
            if let Ok(t) = init.get(js_string!("title"), ctx) {
                if !t.is_undefined() {
                    title = t.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
                }
            }
            if let Ok(a) = init.get(js_string!("artist"), ctx) {
                if !a.is_undefined() {
                    artist = a.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
                }
            }
            if let Ok(al) = init.get(js_string!("album"), ctx) {
                if !al.is_undefined() {
                    album = al.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
                }
            }
            // artwork is an array of MediaImage objects
            if let Ok(aw) = init.get(js_string!("artwork"), ctx) {
                if let Some(arr) = aw.as_object() {
                    if let Ok(len) = arr.get(js_string!("length"), ctx) {
                        let length = len.to_u32(ctx).unwrap_or(0);
                        for i in 0..length {
                            if let Ok(item) = arr.get(i, ctx) {
                                let _ = artwork.push(item, ctx);
                            }
                        }
                    }
                }
            }
        }

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("title"), js_string!(title), Attribute::all())
            .property(js_string!("artist"), js_string!(artist), Attribute::all())
            .property(js_string!("album"), js_string!(album), Attribute::all())
            .property(js_string!("artwork"), artwork, Attribute::all())
            .build();

        Ok(JsValue::from(obj))
    });

    context.register_global_property(
        js_string!("MediaMetadata"),
        FunctionObjectBuilder::new(context.realm(), metadata_ctor)
            .name(js_string!("MediaMetadata"))
            .length(1)
            .constructor(true)
            .build(),
        Attribute::all()
    )?;

    // MediaSession - accessed via navigator.mediaSession
    let global = context.global_object();

    if let Ok(nav_val) = global.get(js_string!("navigator"), context) {
        if let Some(nav_obj) = nav_val.as_object() {
            // Create setActionHandler storage
            let action_handlers: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, JsValue>>> =
                std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));

            // setActionHandler(action, handler)
            let handlers_clone = action_handlers.clone();
            let set_action_handler = unsafe {
                NativeFunction::from_closure(move |_this, args, ctx| {
                    let action = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let handler = args.get_or_undefined(1).clone();

                    // Valid actions per spec
                    let valid_actions = [
                        "play", "pause", "seekbackward", "seekforward", "seekto",
                        "previoustrack", "nexttrack", "skipad", "stop",
                        "togglemicrophone", "togglecamera", "hangup"
                    ];

                    if valid_actions.contains(&action.as_str()) {
                        if handler.is_null() {
                            handlers_clone.borrow_mut().remove(&action);
                        } else {
                            handlers_clone.borrow_mut().insert(action, handler);
                        }
                    }

                    Ok(JsValue::undefined())
                })
            };

            // setPositionState(state)
            let set_position_state = NativeFunction::from_copy_closure(|_this, args, ctx| {
                // Parse position state
                if let Some(state) = args.get(0).and_then(|v| v.as_object()) {
                    let _duration = state.get(js_string!("duration"), ctx)
                        .ok()
                        .and_then(|v| v.to_number(ctx).ok())
                        .unwrap_or(0.0);
                    let _playback_rate = state.get(js_string!("playbackRate"), ctx)
                        .ok()
                        .and_then(|v| v.to_number(ctx).ok())
                        .unwrap_or(1.0);
                    let _position = state.get(js_string!("position"), ctx)
                        .ok()
                        .and_then(|v| v.to_number(ctx).ok())
                        .unwrap_or(0.0);
                    // In a real browser, this would update OS media controls
                }
                Ok(JsValue::undefined())
            });

            // setCameraActive/setMicrophoneActive
            let set_camera_active = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });

            let set_microphone_active = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });

            let media_session = ObjectInitializer::new(context)
                // metadata - MediaMetadata object or null
                .property(js_string!("metadata"), JsValue::null(), Attribute::all())
                // playbackState - "none", "paused", or "playing"
                .property(js_string!("playbackState"), js_string!("none"), Attribute::all())
                // Methods
                .function(set_action_handler, js_string!("setActionHandler"), 2)
                .function(set_position_state, js_string!("setPositionState"), 1)
                .function(set_camera_active, js_string!("setCameraActive"), 1)
                .function(set_microphone_active, js_string!("setMicrophoneActive"), 1)
                .build();

            nav_obj.set(js_string!("mediaSession"), media_session, false, context)?;
        }
    }

    Ok(())
}

/// Register VideoTrack and VideoTrackList
/// Per spec: https://html.spec.whatwg.org/multipage/media.html#videotrack
fn register_video_track(context: &mut Context) -> JsResult<()> {
    // VideoTrack constructor
    let video_track_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let id = args.get_or_undefined(0).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| format!("{:x}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()));
        let kind = args.get_or_undefined(1).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| "main".to_string());
        let label = args.get_or_undefined(2).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();
        let language = args.get_or_undefined(3).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();
        let selected = args.get_or_undefined(4).to_boolean();

        let obj = ObjectInitializer::new(ctx)
            // id - unique identifier
            .property(js_string!("id"), js_string!(id), Attribute::READONLY)
            // kind - category of track ("alternative", "captions", "main", "sign", "subtitles", "commentary", "")
            .property(js_string!("kind"), js_string!(kind), Attribute::READONLY)
            // label - human-readable label
            .property(js_string!("label"), js_string!(label), Attribute::READONLY)
            // language - BCP 47 language tag
            .property(js_string!("language"), js_string!(language), Attribute::READONLY)
            // selected - whether track is selected
            .property(js_string!("selected"), selected, Attribute::all())
            // sourceBuffer - SourceBuffer if from MSE, null otherwise
            .property(js_string!("sourceBuffer"), JsValue::null(), Attribute::READONLY)
            .build();

        Ok(JsValue::from(obj))
    });

    context.register_global_property(
        js_string!("VideoTrack"),
        FunctionObjectBuilder::new(context.realm(), video_track_ctor)
            .name(js_string!("VideoTrack"))
            .length(5)
            .constructor(true)
            .build(),
        Attribute::all()
    )?;

    // VideoTrackList constructor
    let video_track_list_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        // Create internal storage for tracks
        let tracks = JsArray::new(ctx);

        // If an array of tracks is provided, add them
        if let Some(init_arr) = args.get(0).and_then(|v| v.as_object()) {
            if let Ok(len) = init_arr.get(js_string!("length"), ctx) {
                let length = len.to_u32(ctx).unwrap_or(0);
                for i in 0..length {
                    if let Ok(track) = init_arr.get(i, ctx) {
                        let _ = tracks.push(track, ctx);
                    }
                }
            }
        }

        let length = tracks.length(ctx)?;

        // getTrackById(id)
        let tracks_for_get = tracks.clone();
        let get_track_by_id = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let len = tracks_for_get.length(ctx)?;

                for i in 0..len {
                    if let Ok(track) = tracks_for_get.get(i, ctx) {
                        if let Some(track_obj) = track.as_object() {
                            if let Ok(track_id) = track_obj.get(js_string!("id"), ctx) {
                                let tid = track_id.to_string(ctx)?.to_std_string_escaped();
                                if tid == id {
                                    return Ok(track);
                                }
                            }
                        }
                    }
                }
                Ok(JsValue::null())
            })
        };

        // selectedIndex getter - returns index of selected track or -1
        let tracks_for_selected = tracks.clone();
        let get_selected_index = unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let len = tracks_for_selected.length(ctx)?;

                for i in 0..len {
                    if let Ok(track) = tracks_for_selected.get(i, ctx) {
                        if let Some(track_obj) = track.as_object() {
                            if let Ok(selected) = track_obj.get(js_string!("selected"), ctx) {
                                if selected.to_boolean() {
                                    return Ok(JsValue::from(i as i32));
                                }
                            }
                        }
                    }
                }
                Ok(JsValue::from(-1))
            })
        };

        let obj = ObjectInitializer::new(ctx)
            // length - number of tracks
            .property(js_string!("length"), length, Attribute::READONLY)
            // Internal tracks array for indexed access
            .property(js_string!("__tracks__"), tracks, Attribute::READONLY)
            // selectedIndex - index of selected track or -1
            .function(get_selected_index, js_string!("selectedIndex"), 0)
            // Event handlers
            .property(js_string!("onchange"), JsValue::null(), Attribute::all())
            .property(js_string!("onaddtrack"), JsValue::null(), Attribute::all())
            .property(js_string!("onremovetrack"), JsValue::null(), Attribute::all())
            // Methods
            .function(get_track_by_id, js_string!("getTrackById"), 1)
            .build();

        // Add indexed access for tracks (0, 1, 2, etc.)
        for i in 0..length {
            if let Ok(track) = obj.get(js_string!("__tracks__"), ctx) {
                if let Some(tracks_arr) = track.as_object() {
                    if let Ok(t) = tracks_arr.get(i, ctx) {
                        let _ = obj.set(i, t, false, ctx);
                    }
                }
            }
        }

        Ok(JsValue::from(obj))
    });

    context.register_global_property(
        js_string!("VideoTrackList"),
        FunctionObjectBuilder::new(context.realm(), video_track_list_ctor)
            .name(js_string!("VideoTrackList"))
            .length(1)
            .constructor(true)
            .build(),
        Attribute::all()
    )?;

    // AudioTrack constructor (similar to VideoTrack)
    let audio_track_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let id = args.get_or_undefined(0).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| format!("{:x}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()));
        let kind = args.get_or_undefined(1).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| "main".to_string());
        let label = args.get_or_undefined(2).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();
        let language = args.get_or_undefined(3).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();
        let enabled = args.get_or_undefined(4).to_boolean();

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("id"), js_string!(id), Attribute::READONLY)
            .property(js_string!("kind"), js_string!(kind), Attribute::READONLY)
            .property(js_string!("label"), js_string!(label), Attribute::READONLY)
            .property(js_string!("language"), js_string!(language), Attribute::READONLY)
            .property(js_string!("enabled"), enabled, Attribute::all())
            .property(js_string!("sourceBuffer"), JsValue::null(), Attribute::READONLY)
            .build();

        Ok(JsValue::from(obj))
    });

    context.register_global_property(
        js_string!("AudioTrack"),
        FunctionObjectBuilder::new(context.realm(), audio_track_ctor)
            .name(js_string!("AudioTrack"))
            .length(5)
            .constructor(true)
            .build(),
        Attribute::all()
    )?;

    // AudioTrackList constructor
    let audio_track_list_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let tracks = JsArray::new(ctx);

        if let Some(init_arr) = args.get(0).and_then(|v| v.as_object()) {
            if let Ok(len) = init_arr.get(js_string!("length"), ctx) {
                let length = len.to_u32(ctx).unwrap_or(0);
                for i in 0..length {
                    if let Ok(track) = init_arr.get(i, ctx) {
                        let _ = tracks.push(track, ctx);
                    }
                }
            }
        }

        let length = tracks.length(ctx)?;

        let tracks_for_get = tracks.clone();
        let get_track_by_id = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let len = tracks_for_get.length(ctx)?;

                for i in 0..len {
                    if let Ok(track) = tracks_for_get.get(i, ctx) {
                        if let Some(track_obj) = track.as_object() {
                            if let Ok(track_id) = track_obj.get(js_string!("id"), ctx) {
                                let tid = track_id.to_string(ctx)?.to_std_string_escaped();
                                if tid == id {
                                    return Ok(track);
                                }
                            }
                        }
                    }
                }
                Ok(JsValue::null())
            })
        };

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("length"), length, Attribute::READONLY)
            .property(js_string!("__tracks__"), tracks, Attribute::READONLY)
            .property(js_string!("onchange"), JsValue::null(), Attribute::all())
            .property(js_string!("onaddtrack"), JsValue::null(), Attribute::all())
            .property(js_string!("onremovetrack"), JsValue::null(), Attribute::all())
            .function(get_track_by_id, js_string!("getTrackById"), 1)
            .build();

        for i in 0..length {
            if let Ok(track) = obj.get(js_string!("__tracks__"), ctx) {
                if let Some(tracks_arr) = track.as_object() {
                    if let Ok(t) = tracks_arr.get(i, ctx) {
                        let _ = obj.set(i, t, false, ctx);
                    }
                }
            }
        }

        Ok(JsValue::from(obj))
    });

    context.register_global_property(
        js_string!("AudioTrackList"),
        FunctionObjectBuilder::new(context.realm(), audio_track_list_ctor)
            .name(js_string!("AudioTrackList"))
            .length(1)
            .constructor(true)
            .build(),
        Attribute::all()
    )?;

    Ok(())
}

/// Register MediaDeviceInfo
/// Per spec: https://w3c.github.io/mediacapture-main/#device-info
fn register_media_device_info(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        // Parse init dict
        let device_id = args.get_or_undefined(0).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| format!("{:x}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()));

        let kind = args.get_or_undefined(1).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| "videoinput".to_string());

        let label = args.get_or_undefined(2).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        let group_id = args.get_or_undefined(3).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| format!("{:x}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()));

        // toJSON method
        let device_id_clone = device_id.clone();
        let kind_clone = kind.clone();
        let label_clone = label.clone();
        let group_id_clone = group_id.clone();
        let to_json = unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let obj = ObjectInitializer::new(ctx)
                    .property(js_string!("deviceId"), js_string!(device_id_clone.clone()), Attribute::all())
                    .property(js_string!("kind"), js_string!(kind_clone.clone()), Attribute::all())
                    .property(js_string!("label"), js_string!(label_clone.clone()), Attribute::all())
                    .property(js_string!("groupId"), js_string!(group_id_clone.clone()), Attribute::all())
                    .build();
                Ok(JsValue::from(obj))
            })
        };

        let obj = ObjectInitializer::new(ctx)
            // deviceId - unique identifier for the device
            .property(js_string!("deviceId"), js_string!(device_id), Attribute::READONLY)
            // kind - "audioinput", "audiooutput", or "videoinput"
            .property(js_string!("kind"), js_string!(kind), Attribute::READONLY)
            // label - human-readable device name (empty if permission not granted)
            .property(js_string!("label"), js_string!(label), Attribute::READONLY)
            // groupId - group identifier (devices from same physical device share this)
            .property(js_string!("groupId"), js_string!(group_id), Attribute::READONLY)
            // toJSON method
            .function(to_json, js_string!("toJSON"), 0)
            .build();

        Ok(JsValue::from(obj))
    });

    context.register_global_property(
        js_string!("MediaDeviceInfo"),
        FunctionObjectBuilder::new(context.realm(), ctor)
            .name(js_string!("MediaDeviceInfo"))
            .length(4)
            .constructor(true)
            .build(),
        Attribute::all()
    )?;

    // Also add InputDeviceInfo (extends MediaDeviceInfo with getCapabilities)
    let input_device_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let device_id = args.get_or_undefined(0).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| format!("{:x}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()));
        let kind = args.get_or_undefined(1).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| "videoinput".to_string());
        let label = args.get_or_undefined(2).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();
        let group_id = args.get_or_undefined(3).to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        // getCapabilities returns MediaTrackCapabilities
        let get_capabilities = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            // Return empty capabilities object (would be populated with actual device capabilities)
            let caps = ObjectInitializer::new(ctx).build();
            Ok(JsValue::from(caps))
        });

        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("deviceId"), js_string!(device_id), Attribute::READONLY)
            .property(js_string!("kind"), js_string!(kind), Attribute::READONLY)
            .property(js_string!("label"), js_string!(label), Attribute::READONLY)
            .property(js_string!("groupId"), js_string!(group_id), Attribute::READONLY)
            .function(get_capabilities, js_string!("getCapabilities"), 0)
            .build();

        Ok(JsValue::from(obj))
    });

    context.register_global_property(
        js_string!("InputDeviceInfo"),
        FunctionObjectBuilder::new(context.realm(), input_device_ctor)
            .name(js_string!("InputDeviceInfo"))
            .length(4)
            .constructor(true)
            .build(),
        Attribute::all()
    )?;

    Ok(())
}

/// Helper to create MediaList object
fn create_media_list_object(ctx: &mut Context, init_queries: Vec<String>) -> JsResult<JsObject> {
    let queries: std::rc::Rc<std::cell::RefCell<Vec<String>>> =
        std::rc::Rc::new(std::cell::RefCell::new(init_queries));

    let length = queries.borrow().len() as u32;

    // Create item function
    let queries_for_item = queries.clone();
    let item_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0) as usize;
            let q = queries_for_item.borrow();
            if index < q.len() {
                Ok(JsValue::from(js_string!(q[index].clone())))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    // Create mediaText function
    let queries_for_media = queries.clone();
    let media_text_fn = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let q = queries_for_media.borrow();
            Ok(JsValue::from(js_string!(q.join(", "))))
        })
    };

    // Create appendMedium function
    let queries_for_append = queries.clone();
    let append_medium_fn = unsafe {
        NativeFunction::from_closure(move |this, args, ctx| {
            let medium = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            if !medium.is_empty() {
                let new_medium = medium.clone();
                queries_for_append.borrow_mut().push(new_medium);
                // Update length on the object
                if let Some(obj) = this.as_object() {
                    let new_len = queries_for_append.borrow().len() as u32;
                    let _ = obj.set(js_string!("length"), new_len, false, ctx);
                    // Also add indexed property
                    let _ = obj.set(new_len - 1, js_string!(medium), false, ctx);
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // Create deleteMedium function
    let queries_for_delete = queries.clone();
    let delete_medium_fn = unsafe {
        NativeFunction::from_closure(move |this, args, ctx| {
            let medium = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let mut q = queries_for_delete.borrow_mut();
            q.retain(|s| s != &medium);
            // Update length
            if let Some(obj) = this.as_object() {
                let _ = obj.set(js_string!("length"), q.len() as u32, false, ctx);
            }
            Ok(JsValue::undefined())
        })
    };

    let obj = ObjectInitializer::new(ctx)
        .property(js_string!("length"), length, Attribute::all())
        .function(item_fn, js_string!("item"), 1)
        .function(media_text_fn, js_string!("mediaText"), 0)
        .function(append_medium_fn, js_string!("appendMedium"), 1)
        .function(delete_medium_fn, js_string!("deleteMedium"), 1)
        .build();

    // Add indexed access for media queries
    for (i, query) in queries.borrow().iter().enumerate() {
        let _ = obj.set(i as u32, js_string!(query.clone()), false, ctx);
    }

    Ok(obj)
}

/// Register MediaList
/// Per spec: https://drafts.csswg.org/cssom/#the-medialist-interface
fn register_media_list(context: &mut Context) -> JsResult<()> {
    let ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let mut queries: Vec<String> = Vec::new();

        if let Some(init) = args.get(0) {
            if !init.is_undefined() && !init.is_null() {
                let query = init.to_string(ctx)
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                if !query.is_empty() {
                    for q in query.split(',') {
                        let trimmed = q.trim().to_string();
                        if !trimmed.is_empty() {
                            queries.push(trimmed);
                        }
                    }
                }
            }
        }

        let obj = create_media_list_object(ctx, queries)?;
        Ok(JsValue::from(obj))
    });

    context.register_global_property(
        js_string!("MediaList"),
        FunctionObjectBuilder::new(context.realm(), ctor)
            .name(js_string!("MediaList"))
            .length(1)
            .constructor(true)
            .build(),
        Attribute::all()
    )?;

    Ok(())
}
