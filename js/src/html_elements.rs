//! HTML Element specific implementations
//!
//! Adds element-specific properties and methods based on tag name.
//! Implements all HTML*Element interfaces.

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer,
    object::builtins::JsArray, property::Attribute,
    Context, JsArgs, JsObject, JsValue,
};
use std::cell::RefCell;
use std::rc::Rc;

/// Add element-specific properties based on tag name
pub fn add_element_specific_properties(
    obj: &JsObject,
    tag_name: &str,
    get_attr: impl Fn(&str) -> Option<String> + 'static,
    set_attr: impl Fn(&str, &str) + 'static,
    context: &mut Context,
) {
    let tag = tag_name.to_lowercase();
    let get_attr = Rc::new(get_attr);
    let set_attr = Rc::new(set_attr);

    match tag.as_str() {
        "input" => add_input_properties(obj, get_attr, set_attr, context),
        "button" => add_button_properties(obj, get_attr, context),
        "form" => add_form_properties(obj, get_attr, context),
        "select" => add_select_properties(obj, get_attr, context),
        "textarea" => add_textarea_properties(obj, get_attr, context),
        "label" => add_label_properties(obj, get_attr, context),
        "fieldset" => add_fieldset_properties(obj, get_attr, context),
        "legend" => add_legend_properties(obj, context),
        "option" => add_option_properties(obj, get_attr, context),
        "optgroup" => add_optgroup_properties(obj, get_attr, context),
        "output" => add_output_properties(obj, get_attr, context),
        "datalist" => add_datalist_properties(obj, context),
        "progress" => add_progress_properties(obj, get_attr, context),
        "meter" => add_meter_properties(obj, get_attr, context),
        "a" => add_anchor_properties(obj, get_attr, context),
        "area" => add_area_properties(obj, get_attr, context),
        "img" => add_image_properties(obj, get_attr, context),
        "video" => add_video_properties(obj, get_attr, context),
        "audio" => add_audio_properties(obj, get_attr, context),
        "source" => add_source_properties(obj, get_attr, context),
        "track" => add_track_properties(obj, get_attr, context),
        "map" => add_map_properties(obj, get_attr, context),
        "canvas" => add_canvas_properties(obj, get_attr.clone(), context),
        "base" => add_base_properties(obj, get_attr, context),
        "link" => add_link_properties(obj, get_attr, context),
        "meta" => add_meta_properties(obj, get_attr, context),
        "style" => add_style_properties(obj, get_attr, context),
        "script" => add_script_properties(obj, get_attr, context),
        "dialog" => add_dialog_properties(obj, get_attr.clone(), context),
        "details" => add_details_properties(obj, get_attr, context),
        "table" => add_table_properties(obj, context),
        "thead" | "tbody" | "tfoot" => add_table_section_properties(obj, context),
        "tr" => add_table_row_properties(obj, context),
        "td" | "th" => add_table_cell_properties(obj, get_attr, context),
        "colgroup" | "col" => add_colgroup_properties(obj, get_attr, context),
        "blockquote" | "q" => add_quote_properties(obj, get_attr, context),
        "ul" | "ol" => add_list_properties(obj, get_attr, context),
        "li" => add_li_properties(obj, get_attr, context),
        "time" => add_time_properties(obj, get_attr, context),
        "data" => add_data_properties(obj, get_attr, context),
        "iframe" => add_iframe_properties(obj, get_attr, context),
        "embed" => add_embed_properties(obj, get_attr, context),
        "object" => add_object_properties(obj, get_attr, context),
        "hr" => add_hr_properties(obj, context),
        "body" => add_body_properties(obj, get_attr, context),
        "head" => add_head_properties(obj, context),
        "html" => add_html_properties(obj, get_attr, context),
        "title" => add_title_properties(obj, context),
        "param" => add_param_properties(obj, get_attr, context),
        "picture" => add_picture_properties(obj, context),
        "menu" => add_menu_properties(obj, get_attr, context),
        "summary" => add_summary_properties(obj, context),
        "slot" => add_slot_properties(obj, get_attr, context),
        "template" => add_template_properties(obj, context),
        "noscript" => add_noscript_properties(obj, context),
        "marquee" => add_marquee_properties(obj, get_attr, context),
        "font" => add_font_properties(obj, get_attr, context),
        "frame" => add_frame_properties(obj, get_attr, context),
        "frameset" => add_frameset_properties(obj, get_attr, context),
        "span" | "div" | "p" | "section" | "article" | "nav" | "aside" |
        "header" | "footer" | "main" | "figure" | "figcaption" | "address" => {},
        _ => {}
    }
}

type GetAttr = Rc<dyn Fn(&str) -> Option<String>>;
type SetAttr = Rc<dyn Fn(&str, &str)>;

// =============================================================================
// Form Elements
// =============================================================================

fn add_input_properties(obj: &JsObject, get_attr: GetAttr, set_attr: SetAttr, ctx: &mut Context) {
    let value_state = Rc::new(RefCell::new(get_attr("value").unwrap_or_default()));
    let checked_state = Rc::new(RefCell::new(get_attr("checked").is_some()));

    // type
    let ga = get_attr.clone();
    let _ = obj.set(js_string!("type"), js_string!(ga("type").unwrap_or_else(|| "text".to_string())), false, ctx);

    // value getter/setter - syncs with DOM attribute for persistence across element lookups
    let vs = value_state.clone();
    let value_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(vs.borrow().clone())))
        })
    };
    let vs = value_state.clone();
    let sa = set_attr.clone();
    let value_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let val = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            *vs.borrow_mut() = val.clone();
            // Sync to DOM attribute so value persists when element is re-fetched
            sa("value", &val);
            Ok(JsValue::undefined())
        })
    };

    // checked getter/setter
    let cs = checked_state.clone();
    let checked_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*cs.borrow()))
        })
    };
    let cs = checked_state.clone();
    let checked_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            *cs.borrow_mut() = args.get_or_undefined(0).to_boolean();
            Ok(JsValue::undefined())
        })
    };

    let _ = obj.define_property_or_throw(
        js_string!("value"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(value_getter.to_js_function(ctx.realm()))
            .set(value_setter.to_js_function(ctx.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        ctx
    );

    let _ = obj.define_property_or_throw(
        js_string!("checked"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(checked_getter.to_js_function(ctx.realm()))
            .set(checked_setter.to_js_function(ctx.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        ctx
    );

    // Simple properties from attributes
    let _ = obj.set(js_string!("name"), js_string!(get_attr("name").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("placeholder"), js_string!(get_attr("placeholder").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("disabled"), JsValue::from(get_attr("disabled").is_some()), false, ctx);
    let _ = obj.set(js_string!("required"), JsValue::from(get_attr("required").is_some()), false, ctx);
    let _ = obj.set(js_string!("readOnly"), JsValue::from(get_attr("readonly").is_some()), false, ctx);
    let _ = obj.set(js_string!("multiple"), JsValue::from(get_attr("multiple").is_some()), false, ctx);
    let _ = obj.set(js_string!("autofocus"), JsValue::from(get_attr("autofocus").is_some()), false, ctx);
    let _ = obj.set(js_string!("maxLength"), JsValue::from(get_attr("maxlength").and_then(|s| s.parse::<i32>().ok()).unwrap_or(-1)), false, ctx);
    let _ = obj.set(js_string!("minLength"), JsValue::from(get_attr("minlength").and_then(|s| s.parse::<i32>().ok()).unwrap_or(-1)), false, ctx);
    let _ = obj.set(js_string!("pattern"), js_string!(get_attr("pattern").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("min"), js_string!(get_attr("min").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("max"), js_string!(get_attr("max").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("step"), js_string!(get_attr("step").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("accept"), js_string!(get_attr("accept").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("autocomplete"), js_string!(get_attr("autocomplete").unwrap_or_else(|| "on".to_string())), false, ctx);
    let _ = obj.set(js_string!("defaultValue"), js_string!(get_attr("value").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("defaultChecked"), JsValue::from(get_attr("checked").is_some()), false, ctx);
    let _ = obj.set(js_string!("size"), JsValue::from(get_attr("size").and_then(|s| s.parse::<i32>().ok()).unwrap_or(20)), false, ctx);

    // Validation
    let validity = create_validity_state(ctx);
    let _ = obj.set(js_string!("validity"), JsValue::from(validity), false, ctx);
    let _ = obj.set(js_string!("validationMessage"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("willValidate"), JsValue::from(true), false, ctx);

    // Other
    let _ = obj.set(js_string!("form"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("list"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("files"), JsValue::from(JsArray::new(ctx)), false, ctx);
    let _ = obj.set(js_string!("labels"), JsValue::from(JsArray::new(ctx)), false, ctx);
    let _ = obj.set(js_string!("selectionStart"), JsValue::from(0), false, ctx);
    let _ = obj.set(js_string!("selectionEnd"), JsValue::from(0), false, ctx);
    let _ = obj.set(js_string!("selectionDirection"), js_string!("none"), false, ctx);

    // Methods
    add_validation_methods(obj, ctx);
    add_stub_method(obj, "select", 0, ctx);
    add_stub_method(obj, "setSelectionRange", 3, ctx);
    add_stub_method(obj, "stepUp", 1, ctx);
    add_stub_method(obj, "stepDown", 1, ctx);
}

fn add_button_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("type"), js_string!(get_attr("type").unwrap_or_else(|| "submit".to_string())), false, ctx);
    let _ = obj.set(js_string!("name"), js_string!(get_attr("name").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("value"), js_string!(get_attr("value").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("disabled"), JsValue::from(get_attr("disabled").is_some()), false, ctx);
    let _ = obj.set(js_string!("formAction"), js_string!(get_attr("formaction").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("formMethod"), js_string!(get_attr("formmethod").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("formNoValidate"), JsValue::from(get_attr("formnovalidate").is_some()), false, ctx);
    let _ = obj.set(js_string!("formTarget"), js_string!(get_attr("formtarget").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("formEnctype"), js_string!(get_attr("formenctype").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("form"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("labels"), JsValue::from(JsArray::new(ctx)), false, ctx);

    let validity = create_validity_state(ctx);
    let _ = obj.set(js_string!("validity"), JsValue::from(validity), false, ctx);
    let _ = obj.set(js_string!("validationMessage"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("willValidate"), JsValue::from(true), false, ctx);

    add_validation_methods(obj, ctx);
}

fn add_form_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("action"), js_string!(get_attr("action").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("method"), js_string!(get_attr("method").unwrap_or_else(|| "get".to_string())), false, ctx);
    let _ = obj.set(js_string!("enctype"), js_string!(get_attr("enctype").unwrap_or_else(|| "application/x-www-form-urlencoded".to_string())), false, ctx);
    let _ = obj.set(js_string!("encoding"), js_string!(get_attr("enctype").unwrap_or_else(|| "application/x-www-form-urlencoded".to_string())), false, ctx);
    let _ = obj.set(js_string!("target"), js_string!(get_attr("target").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("name"), js_string!(get_attr("name").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("acceptCharset"), js_string!(get_attr("accept-charset").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("autocomplete"), js_string!(get_attr("autocomplete").unwrap_or_else(|| "on".to_string())), false, ctx);
    let _ = obj.set(js_string!("noValidate"), JsValue::from(get_attr("novalidate").is_some()), false, ctx);
    let _ = obj.set(js_string!("elements"), JsValue::from(JsArray::new(ctx)), false, ctx);
    let _ = obj.set(js_string!("length"), JsValue::from(0), false, ctx);

    add_stub_method(obj, "submit", 0, ctx);
    add_stub_method(obj, "reset", 0, ctx);
    add_stub_method(obj, "requestSubmit", 1, ctx);
    add_validation_methods(obj, ctx);
}

fn add_select_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let selected_index = Rc::new(RefCell::new(-1i32));

    let si = selected_index.clone();
    let selected_index_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*si.borrow()))
        })
    };
    let si = selected_index.clone();
    let selected_index_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            *si.borrow_mut() = args.get_or_undefined(0).to_i32(ctx)?;
            Ok(JsValue::undefined())
        })
    };

    let _ = obj.define_property_or_throw(
        js_string!("selectedIndex"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(selected_index_getter.to_js_function(ctx.realm()))
            .set(selected_index_setter.to_js_function(ctx.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        ctx
    );

    let _ = obj.set(js_string!("name"), js_string!(get_attr("name").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("disabled"), JsValue::from(get_attr("disabled").is_some()), false, ctx);
    let _ = obj.set(js_string!("multiple"), JsValue::from(get_attr("multiple").is_some()), false, ctx);
    let _ = obj.set(js_string!("required"), JsValue::from(get_attr("required").is_some()), false, ctx);
    let _ = obj.set(js_string!("size"), JsValue::from(get_attr("size").and_then(|s| s.parse::<i32>().ok()).unwrap_or(0)), false, ctx);
    let _ = obj.set(js_string!("type"), js_string!("select-one"), false, ctx);
    let _ = obj.set(js_string!("value"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("options"), JsValue::from(JsArray::new(ctx)), false, ctx);
    let _ = obj.set(js_string!("selectedOptions"), JsValue::from(JsArray::new(ctx)), false, ctx);
    let _ = obj.set(js_string!("length"), JsValue::from(0), false, ctx);
    let _ = obj.set(js_string!("form"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("labels"), JsValue::from(JsArray::new(ctx)), false, ctx);

    let validity = create_validity_state(ctx);
    let _ = obj.set(js_string!("validity"), JsValue::from(validity), false, ctx);
    let _ = obj.set(js_string!("validationMessage"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("willValidate"), JsValue::from(true), false, ctx);

    add_stub_method(obj, "add", 2, ctx);
    add_stub_method(obj, "remove", 1, ctx);
    add_stub_method(obj, "item", 1, ctx);
    add_stub_method(obj, "namedItem", 1, ctx);
    add_validation_methods(obj, ctx);
}

fn add_textarea_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    // Get initial value from textContent (would be passed as attribute in practice)
    let initial_value = get_attr("_textContent").unwrap_or_default();
    let value_state = Rc::new(RefCell::new(initial_value));

    let vs = value_state.clone();
    let value_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(vs.borrow().clone())))
        })
    };
    let vs = value_state.clone();
    let value_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            *vs.borrow_mut() = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            Ok(JsValue::undefined())
        })
    };

    let _ = obj.define_property_or_throw(
        js_string!("value"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(value_getter.to_js_function(ctx.realm()))
            .set(value_setter.to_js_function(ctx.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        ctx
    );

    let _ = obj.set(js_string!("name"), js_string!(get_attr("name").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("disabled"), JsValue::from(get_attr("disabled").is_some()), false, ctx);
    let _ = obj.set(js_string!("readOnly"), JsValue::from(get_attr("readonly").is_some()), false, ctx);
    let _ = obj.set(js_string!("required"), JsValue::from(get_attr("required").is_some()), false, ctx);
    let _ = obj.set(js_string!("placeholder"), js_string!(get_attr("placeholder").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("rows"), JsValue::from(get_attr("rows").and_then(|s| s.parse::<i32>().ok()).unwrap_or(2)), false, ctx);
    let _ = obj.set(js_string!("cols"), JsValue::from(get_attr("cols").and_then(|s| s.parse::<i32>().ok()).unwrap_or(20)), false, ctx);
    let _ = obj.set(js_string!("maxLength"), JsValue::from(get_attr("maxlength").and_then(|s| s.parse::<i32>().ok()).unwrap_or(-1)), false, ctx);
    let _ = obj.set(js_string!("minLength"), JsValue::from(get_attr("minlength").and_then(|s| s.parse::<i32>().ok()).unwrap_or(-1)), false, ctx);
    let _ = obj.set(js_string!("wrap"), js_string!(get_attr("wrap").unwrap_or_else(|| "soft".to_string())), false, ctx);
    let _ = obj.set(js_string!("type"), js_string!("textarea"), false, ctx);
    let _ = obj.set(js_string!("defaultValue"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("form"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("labels"), JsValue::from(JsArray::new(ctx)), false, ctx);
    let _ = obj.set(js_string!("selectionStart"), JsValue::from(0), false, ctx);
    let _ = obj.set(js_string!("selectionEnd"), JsValue::from(0), false, ctx);
    let _ = obj.set(js_string!("selectionDirection"), js_string!("none"), false, ctx);
    let _ = obj.set(js_string!("textLength"), JsValue::from(0), false, ctx);

    let validity = create_validity_state(ctx);
    let _ = obj.set(js_string!("validity"), JsValue::from(validity), false, ctx);
    let _ = obj.set(js_string!("validationMessage"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("willValidate"), JsValue::from(true), false, ctx);

    add_stub_method(obj, "select", 0, ctx);
    add_stub_method(obj, "setSelectionRange", 3, ctx);
    add_stub_method(obj, "setRangeText", 4, ctx);
    add_validation_methods(obj, ctx);
}

fn add_label_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("htmlFor"), js_string!(get_attr("for").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("control"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("form"), JsValue::null(), false, ctx);
}

fn add_fieldset_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("disabled"), JsValue::from(get_attr("disabled").is_some()), false, ctx);
    let _ = obj.set(js_string!("name"), js_string!(get_attr("name").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("form"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("type"), js_string!("fieldset"), false, ctx);
    let _ = obj.set(js_string!("elements"), JsValue::from(JsArray::new(ctx)), false, ctx);

    let validity = create_validity_state(ctx);
    let _ = obj.set(js_string!("validity"), JsValue::from(validity), false, ctx);
    let _ = obj.set(js_string!("validationMessage"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("willValidate"), JsValue::from(false), false, ctx);

    add_validation_methods(obj, ctx);
}

fn add_legend_properties(obj: &JsObject, ctx: &mut Context) {
    let _ = obj.set(js_string!("form"), JsValue::null(), false, ctx);
}

fn add_option_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let selected_state = Rc::new(RefCell::new(get_attr("selected").is_some()));

    let ss = selected_state.clone();
    let selected_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*ss.borrow()))
        })
    };
    let ss = selected_state.clone();
    let selected_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            *ss.borrow_mut() = args.get_or_undefined(0).to_boolean();
            Ok(JsValue::undefined())
        })
    };

    let _ = obj.define_property_or_throw(
        js_string!("selected"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(selected_getter.to_js_function(ctx.realm()))
            .set(selected_setter.to_js_function(ctx.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        ctx
    );

    let _ = obj.set(js_string!("value"), js_string!(get_attr("value").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("label"), js_string!(get_attr("label").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("disabled"), JsValue::from(get_attr("disabled").is_some()), false, ctx);
    let _ = obj.set(js_string!("defaultSelected"), JsValue::from(get_attr("selected").is_some()), false, ctx);
    let _ = obj.set(js_string!("text"), js_string!(get_attr("_textContent").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("form"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("index"), JsValue::from(0), false, ctx);
}

fn add_optgroup_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("disabled"), JsValue::from(get_attr("disabled").is_some()), false, ctx);
    let _ = obj.set(js_string!("label"), js_string!(get_attr("label").unwrap_or_default()), false, ctx);
}

fn add_output_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let value_state = Rc::new(RefCell::new(String::new()));

    let vs = value_state.clone();
    let value_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(vs.borrow().clone())))
        })
    };
    let vs = value_state.clone();
    let value_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            *vs.borrow_mut() = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            Ok(JsValue::undefined())
        })
    };

    let _ = obj.define_property_or_throw(
        js_string!("value"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(value_getter.to_js_function(ctx.realm()))
            .set(value_setter.to_js_function(ctx.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        ctx
    );

    let _ = obj.set(js_string!("name"), js_string!(get_attr("name").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("htmlFor"), js_string!(get_attr("for").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("type"), js_string!("output"), false, ctx);
    let _ = obj.set(js_string!("defaultValue"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("form"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("labels"), JsValue::from(JsArray::new(ctx)), false, ctx);

    let validity = create_validity_state(ctx);
    let _ = obj.set(js_string!("validity"), JsValue::from(validity), false, ctx);
    let _ = obj.set(js_string!("validationMessage"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("willValidate"), JsValue::from(false), false, ctx);

    add_validation_methods(obj, ctx);
}

fn add_datalist_properties(obj: &JsObject, ctx: &mut Context) {
    let _ = obj.set(js_string!("options"), JsValue::from(JsArray::new(ctx)), false, ctx);
}

fn add_progress_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("value"), JsValue::from(get_attr("value").and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)), false, ctx);
    let _ = obj.set(js_string!("max"), JsValue::from(get_attr("max").and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0)), false, ctx);
    let _ = obj.set(js_string!("position"), JsValue::from(-1.0), false, ctx);
    let _ = obj.set(js_string!("labels"), JsValue::from(JsArray::new(ctx)), false, ctx);
}

fn add_meter_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("value"), JsValue::from(get_attr("value").and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)), false, ctx);
    let _ = obj.set(js_string!("min"), JsValue::from(get_attr("min").and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)), false, ctx);
    let _ = obj.set(js_string!("max"), JsValue::from(get_attr("max").and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0)), false, ctx);
    let _ = obj.set(js_string!("low"), JsValue::from(get_attr("low").and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)), false, ctx);
    let _ = obj.set(js_string!("high"), JsValue::from(get_attr("high").and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0)), false, ctx);
    let _ = obj.set(js_string!("optimum"), JsValue::from(get_attr("optimum").and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.5)), false, ctx);
    let _ = obj.set(js_string!("labels"), JsValue::from(JsArray::new(ctx)), false, ctx);
}

// =============================================================================
// Links and Media
// =============================================================================

fn add_anchor_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("href"), js_string!(get_attr("href").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("target"), js_string!(get_attr("target").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("download"), js_string!(get_attr("download").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("rel"), js_string!(get_attr("rel").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("hreflang"), js_string!(get_attr("hreflang").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("type"), js_string!(get_attr("type").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("text"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("relList"), JsValue::from(JsArray::new(ctx)), false, ctx);
    let _ = obj.set(js_string!("referrerPolicy"), js_string!(get_attr("referrerpolicy").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("ping"), js_string!(get_attr("ping").unwrap_or_default()), false, ctx);

    // URL decomposition
    let _ = obj.set(js_string!("origin"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("protocol"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("host"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("hostname"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("port"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("pathname"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("search"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("hash"), js_string!(""), false, ctx);
}

fn add_area_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("alt"), js_string!(get_attr("alt").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("coords"), js_string!(get_attr("coords").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("shape"), js_string!(get_attr("shape").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("href"), js_string!(get_attr("href").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("target"), js_string!(get_attr("target").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("download"), js_string!(get_attr("download").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("rel"), js_string!(get_attr("rel").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("relList"), JsValue::from(JsArray::new(ctx)), false, ctx);
    let _ = obj.set(js_string!("referrerPolicy"), js_string!(get_attr("referrerpolicy").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("ping"), js_string!(get_attr("ping").unwrap_or_default()), false, ctx);

    // URL decomposition
    let _ = obj.set(js_string!("origin"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("protocol"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("host"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("hostname"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("port"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("pathname"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("search"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("hash"), js_string!(""), false, ctx);
}

fn add_image_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("src"), js_string!(get_attr("src").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("srcset"), js_string!(get_attr("srcset").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("sizes"), js_string!(get_attr("sizes").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("alt"), js_string!(get_attr("alt").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("width"), JsValue::from(get_attr("width").and_then(|s| s.parse::<i32>().ok()).unwrap_or(0)), false, ctx);
    let _ = obj.set(js_string!("height"), JsValue::from(get_attr("height").and_then(|s| s.parse::<i32>().ok()).unwrap_or(0)), false, ctx);
    let _ = obj.set(js_string!("naturalWidth"), JsValue::from(0), false, ctx);
    let _ = obj.set(js_string!("naturalHeight"), JsValue::from(0), false, ctx);
    let _ = obj.set(js_string!("complete"), JsValue::from(true), false, ctx);
    let _ = obj.set(js_string!("currentSrc"), js_string!(get_attr("src").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("loading"), js_string!(get_attr("loading").unwrap_or_else(|| "auto".to_string())), false, ctx);
    let _ = obj.set(js_string!("decoding"), js_string!(get_attr("decoding").unwrap_or_else(|| "auto".to_string())), false, ctx);
    let _ = obj.set(js_string!("isMap"), JsValue::from(get_attr("ismap").is_some()), false, ctx);
    let _ = obj.set(js_string!("useMap"), js_string!(get_attr("usemap").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("crossOrigin"), js_string!(get_attr("crossorigin").unwrap_or_default()), false, ctx);

    add_stub_method(obj, "decode", 0, ctx);
}

fn add_media_base_properties(obj: &JsObject, get_attr: &GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("src"), js_string!(get_attr("src").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("currentSrc"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("crossOrigin"), js_string!(get_attr("crossorigin").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("preload"), js_string!(get_attr("preload").unwrap_or_else(|| "auto".to_string())), false, ctx);
    let _ = obj.set(js_string!("autoplay"), JsValue::from(get_attr("autoplay").is_some()), false, ctx);
    let _ = obj.set(js_string!("loop"), JsValue::from(get_attr("loop").is_some()), false, ctx);
    let _ = obj.set(js_string!("controls"), JsValue::from(get_attr("controls").is_some()), false, ctx);
    let _ = obj.set(js_string!("muted"), JsValue::from(get_attr("muted").is_some()), false, ctx);
    let _ = obj.set(js_string!("defaultMuted"), JsValue::from(get_attr("muted").is_some()), false, ctx);
    let _ = obj.set(js_string!("volume"), JsValue::from(1.0), false, ctx);
    let _ = obj.set(js_string!("currentTime"), JsValue::from(0.0), false, ctx);
    let _ = obj.set(js_string!("duration"), JsValue::from(f64::NAN), false, ctx);
    let _ = obj.set(js_string!("paused"), JsValue::from(true), false, ctx);
    let _ = obj.set(js_string!("ended"), JsValue::from(false), false, ctx);
    let _ = obj.set(js_string!("seeking"), JsValue::from(false), false, ctx);
    let _ = obj.set(js_string!("readyState"), JsValue::from(0), false, ctx);
    let _ = obj.set(js_string!("networkState"), JsValue::from(0), false, ctx);
    let _ = obj.set(js_string!("playbackRate"), JsValue::from(1.0), false, ctx);
    let _ = obj.set(js_string!("defaultPlaybackRate"), JsValue::from(1.0), false, ctx);
    let _ = obj.set(js_string!("error"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("buffered"), JsValue::from(create_time_ranges(ctx)), false, ctx);
    let _ = obj.set(js_string!("seekable"), JsValue::from(create_time_ranges(ctx)), false, ctx);
    let _ = obj.set(js_string!("played"), JsValue::from(create_time_ranges(ctx)), false, ctx);
    let _ = obj.set(js_string!("mediaKeys"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("textTracks"), JsValue::from(JsArray::new(ctx)), false, ctx);
    let _ = obj.set(js_string!("audioTracks"), JsValue::from(JsArray::new(ctx)), false, ctx);
    let _ = obj.set(js_string!("videoTracks"), JsValue::from(JsArray::new(ctx)), false, ctx);

    add_stub_method(obj, "play", 0, ctx);
    add_stub_method(obj, "pause", 0, ctx);
    add_stub_method(obj, "load", 0, ctx);
    add_stub_method(obj, "canPlayType", 1, ctx);
    add_stub_method(obj, "fastSeek", 1, ctx);
    add_stub_method(obj, "getStartDate", 0, ctx);
    add_stub_method(obj, "setMediaKeys", 1, ctx);
    add_stub_method(obj, "setSinkId", 1, ctx);
    add_stub_method(obj, "captureStream", 0, ctx);
}

fn add_video_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    // Add base media properties
    add_media_base_properties(obj, &get_attr, ctx);

    // Video-specific properties
    let _ = obj.set(js_string!("width"), JsValue::from(get_attr("width").and_then(|s| s.parse::<i32>().ok()).unwrap_or(0)), false, ctx);
    let _ = obj.set(js_string!("height"), JsValue::from(get_attr("height").and_then(|s| s.parse::<i32>().ok()).unwrap_or(0)), false, ctx);
    let _ = obj.set(js_string!("videoWidth"), JsValue::from(0), false, ctx);
    let _ = obj.set(js_string!("videoHeight"), JsValue::from(0), false, ctx);
    let _ = obj.set(js_string!("poster"), js_string!(get_attr("poster").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("playsInline"), JsValue::from(get_attr("playsinline").is_some()), false, ctx);
    let _ = obj.set(js_string!("disablePictureInPicture"), JsValue::from(get_attr("disablepictureinpicture").is_some()), false, ctx);
    let _ = obj.set(js_string!("disableRemotePlayback"), JsValue::from(get_attr("disableremoteplayback").is_some()), false, ctx);

    // Video-specific methods
    add_stub_method(obj, "requestPictureInPicture", 0, ctx);
    add_stub_method(obj, "getVideoPlaybackQuality", 0, ctx);
    add_stub_method(obj, "requestVideoFrameCallback", 1, ctx);
    add_stub_method(obj, "cancelVideoFrameCallback", 1, ctx);
}

fn add_audio_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    // Add base media properties
    add_media_base_properties(obj, &get_attr, ctx);

    // Audio has no additional specific properties beyond HTMLMediaElement
    // But we can add mozCurrentSampleOffset for compatibility
    let _ = obj.set(js_string!("mozCurrentSampleOffset"), JsValue::from(0), false, ctx);
}

fn add_source_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("src"), js_string!(get_attr("src").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("type"), js_string!(get_attr("type").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("srcset"), js_string!(get_attr("srcset").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("sizes"), js_string!(get_attr("sizes").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("media"), js_string!(get_attr("media").unwrap_or_default()), false, ctx);
}

fn add_track_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("kind"), js_string!(get_attr("kind").unwrap_or_else(|| "subtitles".to_string())), false, ctx);
    let _ = obj.set(js_string!("src"), js_string!(get_attr("src").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("srclang"), js_string!(get_attr("srclang").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("label"), js_string!(get_attr("label").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("default"), JsValue::from(get_attr("default").is_some()), false, ctx);
    let _ = obj.set(js_string!("readyState"), JsValue::from(0), false, ctx);
    let _ = obj.set(js_string!("track"), JsValue::null(), false, ctx);
}

fn add_map_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("name"), js_string!(get_attr("name").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("areas"), JsValue::from(JsArray::new(ctx)), false, ctx);
}

fn add_canvas_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("width"), JsValue::from(get_attr("width").and_then(|s| s.parse::<i32>().ok()).unwrap_or(300)), false, ctx);
    let _ = obj.set(js_string!("height"), JsValue::from(get_attr("height").and_then(|s| s.parse::<i32>().ok()).unwrap_or(150)), false, ctx);

    add_stub_method(obj, "getContext", 2, ctx);
    add_stub_method(obj, "toDataURL", 2, ctx);
    add_stub_method(obj, "toBlob", 3, ctx);
    add_stub_method(obj, "transferControlToOffscreen", 0, ctx);
}

// =============================================================================
// Document structure elements
// =============================================================================

fn add_base_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("href"), js_string!(get_attr("href").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("target"), js_string!(get_attr("target").unwrap_or_default()), false, ctx);
}

fn add_link_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("href"), js_string!(get_attr("href").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("rel"), js_string!(get_attr("rel").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("type"), js_string!(get_attr("type").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("media"), js_string!(get_attr("media").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("hreflang"), js_string!(get_attr("hreflang").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("crossOrigin"), js_string!(get_attr("crossorigin").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("disabled"), JsValue::from(get_attr("disabled").is_some()), false, ctx);
    let _ = obj.set(js_string!("sheet"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("relList"), JsValue::from(JsArray::new(ctx)), false, ctx);
    let _ = obj.set(js_string!("as"), js_string!(get_attr("as").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("referrerPolicy"), js_string!(get_attr("referrerpolicy").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("integrity"), js_string!(get_attr("integrity").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("sizes"), js_string!(get_attr("sizes").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("imageSrcset"), js_string!(get_attr("imagesrcset").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("imageSizes"), js_string!(get_attr("imagesizes").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("fetchPriority"), js_string!(get_attr("fetchpriority").unwrap_or_else(|| "auto".to_string())), false, ctx);
}

fn add_meta_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("name"), js_string!(get_attr("name").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("content"), js_string!(get_attr("content").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("httpEquiv"), js_string!(get_attr("http-equiv").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("charset"), js_string!(get_attr("charset").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("media"), js_string!(get_attr("media").unwrap_or_default()), false, ctx);
}

fn add_style_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("type"), js_string!(get_attr("type").unwrap_or_else(|| "text/css".to_string())), false, ctx);
    let _ = obj.set(js_string!("media"), js_string!(get_attr("media").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("disabled"), JsValue::from(get_attr("disabled").is_some()), false, ctx);
    let _ = obj.set(js_string!("sheet"), JsValue::null(), false, ctx);
}

fn add_script_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("src"), js_string!(get_attr("src").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("type"), js_string!(get_attr("type").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("async"), JsValue::from(get_attr("async").is_some()), false, ctx);
    let _ = obj.set(js_string!("defer"), JsValue::from(get_attr("defer").is_some()), false, ctx);
    let _ = obj.set(js_string!("crossOrigin"), js_string!(get_attr("crossorigin").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("noModule"), JsValue::from(get_attr("nomodule").is_some()), false, ctx);
    let _ = obj.set(js_string!("integrity"), js_string!(get_attr("integrity").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("text"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("charset"), js_string!(get_attr("charset").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("referrerPolicy"), js_string!(get_attr("referrerpolicy").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("fetchPriority"), js_string!(get_attr("fetchpriority").unwrap_or_else(|| "auto".to_string())), false, ctx);
    let _ = obj.set(js_string!("blocking"), js_string!(get_attr("blocking").unwrap_or_default()), false, ctx);
}

// =============================================================================
// Interactive elements
// =============================================================================

fn add_dialog_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let open_state = Rc::new(RefCell::new(get_attr("open").is_some()));
    let return_value = Rc::new(RefCell::new(String::new()));

    let os = open_state.clone();
    let open_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*os.borrow()))
        })
    };
    let os = open_state.clone();
    let open_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            *os.borrow_mut() = args.get_or_undefined(0).to_boolean();
            Ok(JsValue::undefined())
        })
    };

    let rv = return_value.clone();
    let return_value_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(rv.borrow().clone())))
        })
    };
    let rv = return_value.clone();
    let return_value_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            *rv.borrow_mut() = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            Ok(JsValue::undefined())
        })
    };

    let _ = obj.define_property_or_throw(
        js_string!("open"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(open_getter.to_js_function(ctx.realm()))
            .set(open_setter.to_js_function(ctx.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        ctx
    );

    let _ = obj.define_property_or_throw(
        js_string!("returnValue"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(return_value_getter.to_js_function(ctx.realm()))
            .set(return_value_setter.to_js_function(ctx.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        ctx
    );

    // Methods
    let os = open_state.clone();
    let show = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            *os.borrow_mut() = true;
            Ok(JsValue::undefined())
        })
    };

    let os = open_state.clone();
    let show_modal = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            *os.borrow_mut() = true;
            Ok(JsValue::undefined())
        })
    };

    let os = open_state.clone();
    let close = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            *os.borrow_mut() = false;
            if let Some(rv_arg) = args.get(0) {
                if !rv_arg.is_undefined() {
                    // Could set return value here
                }
            }
            Ok(JsValue::undefined())
        })
    };

    let _ = obj.set(js_string!("show"), show.to_js_function(ctx.realm()), false, ctx);
    let _ = obj.set(js_string!("showModal"), show_modal.to_js_function(ctx.realm()), false, ctx);
    let _ = obj.set(js_string!("close"), close.to_js_function(ctx.realm()), false, ctx);
}

fn add_details_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let open_state = Rc::new(RefCell::new(get_attr("open").is_some()));

    let os = open_state.clone();
    let open_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*os.borrow()))
        })
    };
    let os = open_state.clone();
    let open_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            *os.borrow_mut() = args.get_or_undefined(0).to_boolean();
            Ok(JsValue::undefined())
        })
    };

    let _ = obj.define_property_or_throw(
        js_string!("open"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(open_getter.to_js_function(ctx.realm()))
            .set(open_setter.to_js_function(ctx.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        ctx
    );
}

// =============================================================================
// Table elements
// =============================================================================

fn add_table_properties(obj: &JsObject, ctx: &mut Context) {
    let _ = obj.set(js_string!("caption"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("tHead"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("tFoot"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("tBodies"), JsValue::from(JsArray::new(ctx)), false, ctx);
    let _ = obj.set(js_string!("rows"), JsValue::from(JsArray::new(ctx)), false, ctx);

    add_stub_method(obj, "createCaption", 0, ctx);
    add_stub_method(obj, "deleteCaption", 0, ctx);
    add_stub_method(obj, "createTHead", 0, ctx);
    add_stub_method(obj, "deleteTHead", 0, ctx);
    add_stub_method(obj, "createTFoot", 0, ctx);
    add_stub_method(obj, "deleteTFoot", 0, ctx);
    add_stub_method(obj, "createTBody", 0, ctx);
    add_stub_method(obj, "insertRow", 1, ctx);
    add_stub_method(obj, "deleteRow", 1, ctx);
}

fn add_table_section_properties(obj: &JsObject, ctx: &mut Context) {
    let _ = obj.set(js_string!("rows"), JsValue::from(JsArray::new(ctx)), false, ctx);

    add_stub_method(obj, "insertRow", 1, ctx);
    add_stub_method(obj, "deleteRow", 1, ctx);
}

fn add_table_row_properties(obj: &JsObject, ctx: &mut Context) {
    let _ = obj.set(js_string!("rowIndex"), JsValue::from(-1), false, ctx);
    let _ = obj.set(js_string!("sectionRowIndex"), JsValue::from(-1), false, ctx);
    let _ = obj.set(js_string!("cells"), JsValue::from(JsArray::new(ctx)), false, ctx);

    add_stub_method(obj, "insertCell", 1, ctx);
    add_stub_method(obj, "deleteCell", 1, ctx);
}

fn add_table_cell_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("colSpan"), JsValue::from(get_attr("colspan").and_then(|s| s.parse::<i32>().ok()).unwrap_or(1)), false, ctx);
    let _ = obj.set(js_string!("rowSpan"), JsValue::from(get_attr("rowspan").and_then(|s| s.parse::<i32>().ok()).unwrap_or(1)), false, ctx);
    let _ = obj.set(js_string!("headers"), js_string!(get_attr("headers").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("cellIndex"), JsValue::from(-1), false, ctx);
    let _ = obj.set(js_string!("scope"), js_string!(get_attr("scope").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("abbr"), js_string!(get_attr("abbr").unwrap_or_default()), false, ctx);
}

fn add_colgroup_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("span"), JsValue::from(get_attr("span").and_then(|s| s.parse::<i32>().ok()).unwrap_or(1)), false, ctx);
}

// =============================================================================
// Text content elements
// =============================================================================

fn add_quote_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("cite"), js_string!(get_attr("cite").unwrap_or_default()), false, ctx);
}

fn add_list_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("type"), js_string!(get_attr("type").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("start"), JsValue::from(get_attr("start").and_then(|s| s.parse::<i32>().ok()).unwrap_or(1)), false, ctx);
    let _ = obj.set(js_string!("reversed"), JsValue::from(get_attr("reversed").is_some()), false, ctx);
}

fn add_li_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("value"), JsValue::from(get_attr("value").and_then(|s| s.parse::<i32>().ok()).unwrap_or(0)), false, ctx);
    let _ = obj.set(js_string!("type"), js_string!(get_attr("type").unwrap_or_default()), false, ctx);
}

fn add_time_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("dateTime"), js_string!(get_attr("datetime").unwrap_or_default()), false, ctx);
}

fn add_data_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("value"), js_string!(get_attr("value").unwrap_or_default()), false, ctx);
}

fn add_hr_properties(obj: &JsObject, ctx: &mut Context) {
    // HR has no specific properties in modern HTML
    let _ = obj.set(js_string!("align"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("color"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("noShade"), JsValue::from(false), false, ctx);
    let _ = obj.set(js_string!("size"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("width"), js_string!(""), false, ctx);
}

// =============================================================================
// Document structure elements (body, head, html, title)
// =============================================================================

fn add_body_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    // Deprecated but still present properties
    let _ = obj.set(js_string!("aLink"), js_string!(get_attr("alink").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("background"), js_string!(get_attr("background").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("bgColor"), js_string!(get_attr("bgcolor").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("link"), js_string!(get_attr("link").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("text"), js_string!(get_attr("text").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("vLink"), js_string!(get_attr("vlink").unwrap_or_default()), false, ctx);

    // Event handlers (stubs)
    let _ = obj.set(js_string!("onafterprint"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onbeforeprint"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onbeforeunload"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onhashchange"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onlanguagechange"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onmessage"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onmessageerror"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onoffline"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("ononline"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onpagehide"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onpageshow"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onpopstate"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onrejectionhandled"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onstorage"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onunhandledrejection"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onunload"), JsValue::null(), false, ctx);
}

fn add_head_properties(obj: &JsObject, ctx: &mut Context) {
    // HTMLHeadElement has no specific properties beyond standard Element
    let _ = obj.set(js_string!("profile"), js_string!(""), false, ctx);
}

fn add_html_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("version"), js_string!(get_attr("version").unwrap_or_default()), false, ctx);
}

fn add_title_properties(obj: &JsObject, ctx: &mut Context) {
    let text_state = Rc::new(RefCell::new(String::new()));

    let ts = text_state.clone();
    let text_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(ts.borrow().clone())))
        })
    };
    let ts = text_state.clone();
    let text_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            *ts.borrow_mut() = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            Ok(JsValue::undefined())
        })
    };

    let _ = obj.define_property_or_throw(
        js_string!("text"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(text_getter.to_js_function(ctx.realm()))
            .set(text_setter.to_js_function(ctx.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        ctx
    );
}

fn add_param_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("name"), js_string!(get_attr("name").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("value"), js_string!(get_attr("value").unwrap_or_default()), false, ctx);
    // Deprecated
    let _ = obj.set(js_string!("type"), js_string!(get_attr("type").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("valueType"), js_string!(get_attr("valuetype").unwrap_or_default()), false, ctx);
}

fn add_picture_properties(obj: &JsObject, ctx: &mut Context) {
    // HTMLPictureElement has no specific properties beyond standard Element
    // It's a container for source and img elements
    let _ = obj;
    let _ = ctx;
}

fn add_menu_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    // Deprecated/non-standard properties
    let _ = obj.set(js_string!("type"), js_string!(get_attr("type").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("label"), js_string!(get_attr("label").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("compact"), JsValue::from(get_attr("compact").is_some()), false, ctx);
}

fn add_summary_properties(obj: &JsObject, ctx: &mut Context) {
    // HTMLSummaryElement has no specific properties beyond standard Element
    let _ = obj;
    let _ = ctx;
}

fn add_slot_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("name"), js_string!(get_attr("name").unwrap_or_default()), false, ctx);

    // assignedNodes and assignedElements methods
    let assigned_nodes = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    });
    let assigned_elements = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    });
    let assign = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let _ = obj.set(js_string!("assignedNodes"), assigned_nodes.to_js_function(ctx.realm()), false, ctx);
    let _ = obj.set(js_string!("assignedElements"), assigned_elements.to_js_function(ctx.realm()), false, ctx);
    let _ = obj.set(js_string!("assign"), assign.to_js_function(ctx.realm()), false, ctx);
}

fn add_template_properties(obj: &JsObject, ctx: &mut Context) {
    // content property returns a DocumentFragment
    let child_nodes = JsArray::new(ctx);
    let children = JsArray::new(ctx);

    let content = ObjectInitializer::new(ctx)
        .property(js_string!("nodeType"), 11, Attribute::READONLY)
        .property(js_string!("nodeName"), js_string!("#document-fragment"), Attribute::READONLY)
        .property(js_string!("childNodes"), child_nodes, Attribute::READONLY)
        .property(js_string!("children"), children, Attribute::READONLY)
        .property(js_string!("firstChild"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("lastChild"), JsValue::null(), Attribute::READONLY)
        .build();

    let _ = obj.set(js_string!("content"), JsValue::from(content), false, ctx);
}

fn add_noscript_properties(obj: &JsObject, ctx: &mut Context) {
    // HTMLNoScriptElement has no specific properties
    let _ = obj;
    let _ = ctx;
}

fn add_marquee_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    // Deprecated but some sites still use it
    let _ = obj.set(js_string!("behavior"), js_string!(get_attr("behavior").unwrap_or_else(|| "scroll".to_string())), false, ctx);
    let _ = obj.set(js_string!("bgColor"), js_string!(get_attr("bgcolor").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("direction"), js_string!(get_attr("direction").unwrap_or_else(|| "left".to_string())), false, ctx);
    let _ = obj.set(js_string!("height"), js_string!(get_attr("height").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("hspace"), JsValue::from(get_attr("hspace").and_then(|s| s.parse::<i32>().ok()).unwrap_or(0)), false, ctx);
    let _ = obj.set(js_string!("loop"), JsValue::from(get_attr("loop").and_then(|s| s.parse::<i32>().ok()).unwrap_or(-1)), false, ctx);
    let _ = obj.set(js_string!("scrollAmount"), JsValue::from(get_attr("scrollamount").and_then(|s| s.parse::<i32>().ok()).unwrap_or(6)), false, ctx);
    let _ = obj.set(js_string!("scrollDelay"), JsValue::from(get_attr("scrolldelay").and_then(|s| s.parse::<i32>().ok()).unwrap_or(85)), false, ctx);
    let _ = obj.set(js_string!("trueSpeed"), JsValue::from(get_attr("truespeed").is_some()), false, ctx);
    let _ = obj.set(js_string!("vspace"), JsValue::from(get_attr("vspace").and_then(|s| s.parse::<i32>().ok()).unwrap_or(0)), false, ctx);
    let _ = obj.set(js_string!("width"), js_string!(get_attr("width").unwrap_or_default()), false, ctx);

    // Methods
    add_stub_method(obj, "start", 0, ctx);
    add_stub_method(obj, "stop", 0, ctx);
}

fn add_font_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    // Deprecated but still present
    let _ = obj.set(js_string!("color"), js_string!(get_attr("color").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("face"), js_string!(get_attr("face").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("size"), js_string!(get_attr("size").unwrap_or_default()), false, ctx);
}

fn add_frame_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    // Deprecated frame element
    let _ = obj.set(js_string!("name"), js_string!(get_attr("name").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("src"), js_string!(get_attr("src").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("scrolling"), js_string!(get_attr("scrolling").unwrap_or_else(|| "auto".to_string())), false, ctx);
    let _ = obj.set(js_string!("frameBorder"), js_string!(get_attr("frameborder").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("longDesc"), js_string!(get_attr("longdesc").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("marginHeight"), js_string!(get_attr("marginheight").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("marginWidth"), js_string!(get_attr("marginwidth").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("noResize"), JsValue::from(get_attr("noresize").is_some()), false, ctx);
    let _ = obj.set(js_string!("contentDocument"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("contentWindow"), JsValue::null(), false, ctx);
}

fn add_frameset_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    // Deprecated frameset element
    let _ = obj.set(js_string!("cols"), js_string!(get_attr("cols").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("rows"), js_string!(get_attr("rows").unwrap_or_default()), false, ctx);

    // Event handlers
    let _ = obj.set(js_string!("onafterprint"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onbeforeprint"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onbeforeunload"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onhashchange"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onlanguagechange"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onmessage"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onoffline"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("ononline"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onpagehide"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onpageshow"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onpopstate"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onresize"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onstorage"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("onunload"), JsValue::null(), false, ctx);
}

// =============================================================================
// Embedding elements
// =============================================================================

fn add_iframe_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("src"), js_string!(get_attr("src").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("srcdoc"), js_string!(get_attr("srcdoc").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("name"), js_string!(get_attr("name").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("sandbox"), js_string!(get_attr("sandbox").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("allow"), js_string!(get_attr("allow").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("allowFullscreen"), JsValue::from(get_attr("allowfullscreen").is_some()), false, ctx);
    let _ = obj.set(js_string!("width"), js_string!(get_attr("width").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("height"), js_string!(get_attr("height").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("loading"), js_string!(get_attr("loading").unwrap_or_else(|| "eager".to_string())), false, ctx);
    let _ = obj.set(js_string!("contentDocument"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("contentWindow"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("referrerPolicy"), js_string!(get_attr("referrerpolicy").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("csp"), js_string!(get_attr("csp").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("fetchPriority"), js_string!(get_attr("fetchpriority").unwrap_or_else(|| "auto".to_string())), false, ctx);
}

fn add_embed_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("src"), js_string!(get_attr("src").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("type"), js_string!(get_attr("type").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("width"), js_string!(get_attr("width").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("height"), js_string!(get_attr("height").unwrap_or_default()), false, ctx);
}

fn add_object_properties(obj: &JsObject, get_attr: GetAttr, ctx: &mut Context) {
    let _ = obj.set(js_string!("data"), js_string!(get_attr("data").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("type"), js_string!(get_attr("type").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("name"), js_string!(get_attr("name").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("useMap"), js_string!(get_attr("usemap").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("form"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("width"), js_string!(get_attr("width").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("height"), js_string!(get_attr("height").unwrap_or_default()), false, ctx);
    let _ = obj.set(js_string!("contentDocument"), JsValue::null(), false, ctx);
    let _ = obj.set(js_string!("contentWindow"), JsValue::null(), false, ctx);

    let validity = create_validity_state(ctx);
    let _ = obj.set(js_string!("validity"), JsValue::from(validity), false, ctx);
    let _ = obj.set(js_string!("validationMessage"), js_string!(""), false, ctx);
    let _ = obj.set(js_string!("willValidate"), JsValue::from(false), false, ctx);

    add_validation_methods(obj, ctx);
}

// =============================================================================
// Helpers
// =============================================================================

fn create_time_ranges(ctx: &mut Context) -> JsObject {
    let start = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(0.0))
    });
    let end = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(0.0))
    });

    ObjectInitializer::new(ctx)
        .property(js_string!("length"), 0, Attribute::READONLY)
        .function(start, js_string!("start"), 1)
        .function(end, js_string!("end"), 1)
        .build()
}

fn create_validity_state(ctx: &mut Context) -> JsObject {
    ObjectInitializer::new(ctx)
        .property(js_string!("valueMissing"), false, Attribute::READONLY)
        .property(js_string!("typeMismatch"), false, Attribute::READONLY)
        .property(js_string!("patternMismatch"), false, Attribute::READONLY)
        .property(js_string!("tooLong"), false, Attribute::READONLY)
        .property(js_string!("tooShort"), false, Attribute::READONLY)
        .property(js_string!("rangeUnderflow"), false, Attribute::READONLY)
        .property(js_string!("rangeOverflow"), false, Attribute::READONLY)
        .property(js_string!("stepMismatch"), false, Attribute::READONLY)
        .property(js_string!("badInput"), false, Attribute::READONLY)
        .property(js_string!("customError"), false, Attribute::READONLY)
        .property(js_string!("valid"), true, Attribute::READONLY)
        .build()
}

fn add_validation_methods(obj: &JsObject, ctx: &mut Context) {
    let check_validity = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(true))
    });
    let report_validity = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(true))
    });
    let set_custom_validity = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let _ = obj.set(js_string!("checkValidity"), check_validity.to_js_function(ctx.realm()), false, ctx);
    let _ = obj.set(js_string!("reportValidity"), report_validity.to_js_function(ctx.realm()), false, ctx);
    let _ = obj.set(js_string!("setCustomValidity"), set_custom_validity.to_js_function(ctx.realm()), false, ctx);
}

fn add_stub_method(obj: &JsObject, name: &str, _arity: usize, ctx: &mut Context) {
    let method = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = obj.set(js_string!(name), method.to_js_function(ctx.realm()), false, ctx);
}
