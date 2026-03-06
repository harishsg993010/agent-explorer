//! CSS Object Model (CSSOM) implementation
//!
//! Implements the full CSSOM APIs including:
//! - CSS namespace object (CSS.supports, CSS.escape, etc.)
//! - CSSStyleSheet and StyleSheet
//! - CSSRule and all subtypes (CSSStyleRule, CSSMediaRule, etc.)
//! - CSSRuleList and StyleSheetList
//! - MediaList
//! - CSS Parsing using cssparser crate

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer,
    object::builtins::JsArray, property::Attribute,
    Context, JsArgs, JsObject, JsResult, JsValue,
};
use cssparser::{Parser, ParserInput, Token};
use std::cell::RefCell;
use std::rc::Rc;

// =============================================================================
// CSS Namespace Object
// =============================================================================

/// Register the CSS namespace object with static methods
pub fn register_css_namespace(context: &mut Context) -> JsResult<()> {
    // CSS.supports(property, value) or CSS.supports(conditionText)
    let supports = NativeFunction::from_copy_closure(|_this, args, ctx| {
        if args.len() >= 2 {
            // CSS.supports(property, value)
            let property = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let _value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            // For now, return true for common properties
            let supported = matches!(property.as_str(),
                "display" | "position" | "width" | "height" | "color" | "background" |
                "margin" | "padding" | "border" | "font" | "flex" | "grid" |
                "transform" | "transition" | "animation" | "opacity" | "visibility"
            );
            Ok(JsValue::from(supported))
        } else if args.len() == 1 {
            // CSS.supports(conditionText)
            let condition = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            // Basic parsing - check for common patterns
            let supported = condition.contains("display") ||
                           condition.contains("flex") ||
                           condition.contains("grid") ||
                           condition.contains("transform");
            Ok(JsValue::from(supported))
        } else {
            Ok(JsValue::from(false))
        }
    });

    // CSS.escape(ident)
    let escape = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let ident = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // Escape special CSS characters
        let escaped = ident
            .chars()
            .map(|c| match c {
                '!' | '"' | '#' | '$' | '%' | '&' | '\'' | '(' | ')' | '*' |
                '+' | ',' | '-' | '.' | '/' | ':' | ';' | '<' | '=' | '>' |
                '?' | '@' | '[' | '\\' | ']' | '^' | '`' | '{' | '|' | '}' | '~' => {
                    format!("\\{}", c)
                }
                c if c.is_ascii_control() => format!("\\{:x} ", c as u32),
                c => c.to_string(),
            })
            .collect::<String>();
        Ok(JsValue::from(js_string!(escaped)))
    });

    // CSS.registerProperty(definition) - for CSS Houdini
    let register_property = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        // Stub - CSS Houdini custom properties
        Ok(JsValue::undefined())
    });

    // CSS.px, CSS.em, etc. - CSS Typed OM factory functions
    let px = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let value = args.get_or_undefined(0).to_number(ctx)?;
        Ok(JsValue::from(js_string!(format!("{}px", value))))
    });

    let em = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let value = args.get_or_undefined(0).to_number(ctx)?;
        Ok(JsValue::from(js_string!(format!("{}em", value))))
    });

    let rem = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let value = args.get_or_undefined(0).to_number(ctx)?;
        Ok(JsValue::from(js_string!(format!("{}rem", value))))
    });

    let percent = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let value = args.get_or_undefined(0).to_number(ctx)?;
        Ok(JsValue::from(js_string!(format!("{}%", value))))
    });

    let vh = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let value = args.get_or_undefined(0).to_number(ctx)?;
        Ok(JsValue::from(js_string!(format!("{}vh", value))))
    });

    let vw = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let value = args.get_or_undefined(0).to_number(ctx)?;
        Ok(JsValue::from(js_string!(format!("{}vw", value))))
    });

    // Create paintWorklet - for CSS Paint API
    let add_module_paint = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Return a resolved promise (module loading simulated)
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

    let paint_worklet = ObjectInitializer::new(context)
        .function(add_module_paint, js_string!("addModule"), 1)
        .build();

    // Create animationWorklet - for Animation Worklet API
    let add_module_anim = NativeFunction::from_copy_closure(|_this, _args, ctx| {
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

    let animation_worklet = ObjectInitializer::new(context)
        .function(add_module_anim, js_string!("addModule"), 1)
        .build();

    let css_obj = ObjectInitializer::new(context)
        .function(supports, js_string!("supports"), 2)
        .function(escape, js_string!("escape"), 1)
        .function(register_property, js_string!("registerProperty"), 1)
        .function(px, js_string!("px"), 1)
        .function(em, js_string!("em"), 1)
        .function(rem, js_string!("rem"), 1)
        .function(percent, js_string!("percent"), 1)
        .function(vh, js_string!("vh"), 1)
        .function(vw, js_string!("vw"), 1)
        .property(js_string!("paintWorklet"), JsValue::from(paint_worklet), Attribute::all())
        .property(js_string!("animationWorklet"), JsValue::from(animation_worklet), Attribute::all())
        .build();

    context.register_global_property(
        js_string!("CSS"),
        css_obj,
        Attribute::WRITABLE | Attribute::CONFIGURABLE,
    )?;

    Ok(())
}

// =============================================================================
// CSSRule Types (constants)
// =============================================================================

pub const STYLE_RULE: u16 = 1;
pub const CHARSET_RULE: u16 = 2;
pub const IMPORT_RULE: u16 = 3;
pub const MEDIA_RULE: u16 = 4;
pub const FONT_FACE_RULE: u16 = 5;
pub const PAGE_RULE: u16 = 6;
pub const KEYFRAMES_RULE: u16 = 7;
pub const KEYFRAME_RULE: u16 = 8;
pub const NAMESPACE_RULE: u16 = 10;
pub const COUNTER_STYLE_RULE: u16 = 11;
pub const SUPPORTS_RULE: u16 = 12;
pub const FONT_FEATURE_VALUES_RULE: u16 = 14;
pub const LAYER_BLOCK_RULE: u16 = 16;
pub const LAYER_STATEMENT_RULE: u16 = 17;

// =============================================================================
// MediaList
// =============================================================================

/// Create a MediaList object
pub fn create_media_list(media_text: &str, context: &mut Context) -> JsObject {
    let media_queries: Vec<String> = if media_text.is_empty() {
        vec![]
    } else {
        media_text.split(',').map(|s| s.trim().to_string()).collect()
    };

    let queries = Rc::new(RefCell::new(media_queries));
    let queries_clone = queries.clone();

    // mediaText getter
    let mq = queries.clone();
    let media_text_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let text = mq.borrow().join(", ");
            Ok(JsValue::from(js_string!(text)))
        })
    };

    // mediaText setter
    let mq = queries.clone();
    let media_text_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let new_queries: Vec<String> = text.split(',').map(|s| s.trim().to_string()).collect();
            *mq.borrow_mut() = new_queries;
            Ok(JsValue::undefined())
        })
    };

    // length getter
    let mq = queries.clone();
    let length_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(mq.borrow().len() as i32))
        })
    };

    // item(index)
    let mq = queries.clone();
    let item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let queries = mq.borrow();
            if index < queries.len() {
                Ok(JsValue::from(js_string!(queries[index].clone())))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    // appendMedium(medium)
    let mq = queries.clone();
    let append_medium = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let medium = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            mq.borrow_mut().push(medium);
            Ok(JsValue::undefined())
        })
    };

    // deleteMedium(medium)
    let mq = queries_clone;
    let delete_medium = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let medium = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            mq.borrow_mut().retain(|m| m != &medium);
            Ok(JsValue::undefined())
        })
    };

    let obj = ObjectInitializer::new(context)
        .function(item, js_string!("item"), 1)
        .function(append_medium, js_string!("appendMedium"), 1)
        .function(delete_medium, js_string!("deleteMedium"), 1)
        .build();

    let _ = obj.define_property_or_throw(
        js_string!("mediaText"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(media_text_getter.to_js_function(context.realm()))
            .set(media_text_setter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    let _ = obj.define_property_or_throw(
        js_string!("length"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(length_getter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    obj
}

// =============================================================================
// CSSRuleList
// =============================================================================

/// Create a CSSRuleList object
pub fn create_css_rule_list(rules: Vec<JsObject>, context: &mut Context) -> JsObject {
    let rules_rc = Rc::new(RefCell::new(rules));

    // length getter
    let rules_clone = rules_rc.clone();
    let length_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(rules_clone.borrow().len() as i32))
        })
    };

    // item(index)
    let rules_clone = rules_rc.clone();
    let item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let rules = rules_clone.borrow();
            if index < rules.len() {
                Ok(JsValue::from(rules[index].clone()))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    let obj = ObjectInitializer::new(context)
        .function(item, js_string!("item"), 1)
        .build();

    let _ = obj.define_property_or_throw(
        js_string!("length"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(length_getter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    // Add indexed access
    let rules = rules_rc.borrow();
    for (i, rule) in rules.iter().enumerate() {
        let _ = obj.set(js_string!(i.to_string()), JsValue::from(rule.clone()), false, context);
    }

    obj
}

// =============================================================================
// StyleSheetList
// =============================================================================

/// Create a StyleSheetList object
pub fn create_style_sheet_list(sheets: Vec<JsObject>, context: &mut Context) -> JsObject {
    let sheets_rc = Rc::new(RefCell::new(sheets));

    // length getter
    let sheets_clone = sheets_rc.clone();
    let length_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(sheets_clone.borrow().len() as i32))
        })
    };

    // item(index)
    let sheets_clone = sheets_rc.clone();
    let item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let sheets = sheets_clone.borrow();
            if index < sheets.len() {
                Ok(JsValue::from(sheets[index].clone()))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    let obj = ObjectInitializer::new(context)
        .function(item, js_string!("item"), 1)
        .build();

    let _ = obj.define_property_or_throw(
        js_string!("length"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(length_getter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    // Add indexed access
    let sheets = sheets_rc.borrow();
    for (i, sheet) in sheets.iter().enumerate() {
        let _ = obj.set(js_string!(i.to_string()), JsValue::from(sheet.clone()), false, context);
    }

    obj
}

// =============================================================================
// CSSRule (base)
// =============================================================================

/// Create a base CSSRule object
pub fn create_css_rule(
    rule_type: u16,
    css_text: &str,
    parent_rule: Option<JsObject>,
    parent_stylesheet: Option<JsObject>,
    context: &mut Context,
) -> JsObject {
    let css_text_rc = Rc::new(RefCell::new(css_text.to_string()));

    // cssText getter/setter
    let ct = css_text_rc.clone();
    let css_text_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(ct.borrow().clone())))
        })
    };

    let ct = css_text_rc.clone();
    let css_text_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            *ct.borrow_mut() = text;
            Ok(JsValue::undefined())
        })
    };

    let obj = ObjectInitializer::new(context)
        .property(js_string!("type"), rule_type, Attribute::READONLY)
        .property(
            js_string!("parentRule"),
            parent_rule.map(JsValue::from).unwrap_or(JsValue::null()),
            Attribute::READONLY,
        )
        .property(
            js_string!("parentStyleSheet"),
            parent_stylesheet.map(JsValue::from).unwrap_or(JsValue::null()),
            Attribute::READONLY,
        )
        // CSSRule type constants
        .property(js_string!("STYLE_RULE"), STYLE_RULE, Attribute::READONLY)
        .property(js_string!("CHARSET_RULE"), CHARSET_RULE, Attribute::READONLY)
        .property(js_string!("IMPORT_RULE"), IMPORT_RULE, Attribute::READONLY)
        .property(js_string!("MEDIA_RULE"), MEDIA_RULE, Attribute::READONLY)
        .property(js_string!("FONT_FACE_RULE"), FONT_FACE_RULE, Attribute::READONLY)
        .property(js_string!("PAGE_RULE"), PAGE_RULE, Attribute::READONLY)
        .property(js_string!("KEYFRAMES_RULE"), KEYFRAMES_RULE, Attribute::READONLY)
        .property(js_string!("KEYFRAME_RULE"), KEYFRAME_RULE, Attribute::READONLY)
        .property(js_string!("NAMESPACE_RULE"), NAMESPACE_RULE, Attribute::READONLY)
        .property(js_string!("COUNTER_STYLE_RULE"), COUNTER_STYLE_RULE, Attribute::READONLY)
        .property(js_string!("SUPPORTS_RULE"), SUPPORTS_RULE, Attribute::READONLY)
        .property(js_string!("LAYER_BLOCK_RULE"), LAYER_BLOCK_RULE, Attribute::READONLY)
        .property(js_string!("LAYER_STATEMENT_RULE"), LAYER_STATEMENT_RULE, Attribute::READONLY)
        .build();

    let _ = obj.define_property_or_throw(
        js_string!("cssText"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(css_text_getter.to_js_function(context.realm()))
            .set(css_text_setter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    obj
}

// =============================================================================
// CSSStyleRule
// =============================================================================

/// Create a CSSStyleRule object
pub fn create_css_style_rule(
    selector_text: &str,
    style_declarations: &str,
    parent_stylesheet: Option<JsObject>,
    context: &mut Context,
) -> JsObject {
    let css_text = format!("{} {{ {} }}", selector_text, style_declarations);
    let rule = create_css_rule(STYLE_RULE, &css_text, None, parent_stylesheet, context);

    let selector_rc = Rc::new(RefCell::new(selector_text.to_string()));

    // selectorText getter/setter
    let sel = selector_rc.clone();
    let selector_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(sel.borrow().clone())))
        })
    };

    let sel = selector_rc.clone();
    let selector_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            *sel.borrow_mut() = text;
            Ok(JsValue::undefined())
        })
    };

    let _ = rule.define_property_or_throw(
        js_string!("selectorText"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(selector_getter.to_js_function(context.realm()))
            .set(selector_setter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    // Create style object (CSSStyleDeclaration)
    let style = create_rule_style_declaration(style_declarations, context);
    let _ = rule.set(js_string!("style"), JsValue::from(style), false, context);

    // styleMap for CSS Typed OM
    let style_map = ObjectInitializer::new(context).build();
    let _ = rule.set(js_string!("styleMap"), JsValue::from(style_map), false, context);

    rule
}

/// Create a CSSStyleDeclaration for a rule
fn create_rule_style_declaration(declarations: &str, context: &mut Context) -> JsObject {
    let props: std::collections::HashMap<String, String> = declarations
        .split(';')
        .filter_map(|decl| {
            let parts: Vec<&str> = decl.splitn(2, ':').collect();
            if parts.len() == 2 {
                Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
            } else {
                None
            }
        })
        .collect();

    let state = Rc::new(RefCell::new(props));

    // setProperty
    let state_clone = state.clone();
    let set_property = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            state_clone.borrow_mut().insert(name, value);
            Ok(JsValue::undefined())
        })
    };

    // getPropertyValue
    let state_clone = state.clone();
    let get_property_value = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let value = state_clone.borrow().get(&name).cloned().unwrap_or_default();
            Ok(JsValue::from(js_string!(value)))
        })
    };

    // removeProperty
    let state_clone = state.clone();
    let remove_property = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let old = state_clone.borrow_mut().remove(&name).unwrap_or_default();
            Ok(JsValue::from(js_string!(old)))
        })
    };

    // getPropertyPriority
    let get_property_priority = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(js_string!("")))
    });

    // item
    let state_clone = state.clone();
    let item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let props = state_clone.borrow();
            let keys: Vec<_> = props.keys().collect();
            if index < keys.len() {
                Ok(JsValue::from(js_string!(keys[index].clone())))
            } else {
                Ok(JsValue::from(js_string!("")))
            }
        })
    };

    // length getter
    let state_clone = state.clone();
    let length_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(state_clone.borrow().len() as i32))
        })
    };

    // cssText getter
    let state_clone = state.clone();
    let css_text_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let props = state_clone.borrow();
            let text = props
                .iter()
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect::<Vec<_>>()
                .join("; ");
            Ok(JsValue::from(js_string!(text)))
        })
    };

    let obj = ObjectInitializer::new(context)
        .function(set_property, js_string!("setProperty"), 3)
        .function(get_property_value, js_string!("getPropertyValue"), 1)
        .function(remove_property, js_string!("removeProperty"), 1)
        .function(get_property_priority, js_string!("getPropertyPriority"), 1)
        .function(item, js_string!("item"), 1)
        .build();

    let _ = obj.define_property_or_throw(
        js_string!("length"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(length_getter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    let _ = obj.define_property_or_throw(
        js_string!("cssText"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(css_text_getter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    obj
}

// =============================================================================
// CSSMediaRule
// =============================================================================

/// Create a CSSMediaRule object
pub fn create_css_media_rule(
    condition_text: &str,
    css_rules: Vec<JsObject>,
    parent_stylesheet: Option<JsObject>,
    context: &mut Context,
) -> JsObject {
    let css_text = format!("@media {} {{ ... }}", condition_text);
    let rule = create_css_rule(MEDIA_RULE, &css_text, None, parent_stylesheet.clone(), context);

    // conditionText
    let _ = rule.set(
        js_string!("conditionText"),
        JsValue::from(js_string!(condition_text)),
        false,
        context,
    );

    // media (MediaList)
    let media = create_media_list(condition_text, context);
    let _ = rule.set(js_string!("media"), JsValue::from(media), false, context);

    // cssRules
    let css_rule_list = create_css_rule_list(css_rules, context);
    let _ = rule.set(js_string!("cssRules"), JsValue::from(css_rule_list), false, context);

    // insertRule
    let insert_rule = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        // Return inserted index
        Ok(JsValue::from(0))
    });
    let _ = rule.set(js_string!("insertRule"), insert_rule.to_js_function(context.realm()), false, context);

    // deleteRule
    let delete_rule = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = rule.set(js_string!("deleteRule"), delete_rule.to_js_function(context.realm()), false, context);

    rule
}

// =============================================================================
// CSSSupportsRule
// =============================================================================

/// Create a CSSSupportsRule object
pub fn create_css_supports_rule(
    condition_text: &str,
    css_rules: Vec<JsObject>,
    parent_stylesheet: Option<JsObject>,
    context: &mut Context,
) -> JsObject {
    let css_text = format!("@supports {} {{ ... }}", condition_text);
    let rule = create_css_rule(SUPPORTS_RULE, &css_text, None, parent_stylesheet.clone(), context);

    // conditionText
    let _ = rule.set(
        js_string!("conditionText"),
        JsValue::from(js_string!(condition_text)),
        false,
        context,
    );

    // cssRules
    let css_rule_list = create_css_rule_list(css_rules, context);
    let _ = rule.set(js_string!("cssRules"), JsValue::from(css_rule_list), false, context);

    // insertRule
    let insert_rule = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(0))
    });
    let _ = rule.set(js_string!("insertRule"), insert_rule.to_js_function(context.realm()), false, context);

    // deleteRule
    let delete_rule = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = rule.set(js_string!("deleteRule"), delete_rule.to_js_function(context.realm()), false, context);

    rule
}

// =============================================================================
// CSSImportRule
// =============================================================================

/// Create a CSSImportRule object
pub fn create_css_import_rule(
    href: &str,
    media_text: &str,
    parent_stylesheet: Option<JsObject>,
    context: &mut Context,
) -> JsObject {
    let css_text = if media_text.is_empty() {
        format!("@import url(\"{}\");", href)
    } else {
        format!("@import url(\"{}\") {};", href, media_text)
    };
    let rule = create_css_rule(IMPORT_RULE, &css_text, None, parent_stylesheet, context);

    let _ = rule.set(js_string!("href"), JsValue::from(js_string!(href)), false, context);

    let media = create_media_list(media_text, context);
    let _ = rule.set(js_string!("media"), JsValue::from(media), false, context);

    // styleSheet (the imported stylesheet, null until loaded)
    let _ = rule.set(js_string!("styleSheet"), JsValue::null(), false, context);

    // layerName
    let _ = rule.set(js_string!("layerName"), JsValue::null(), false, context);

    // supportsText
    let _ = rule.set(js_string!("supportsText"), JsValue::null(), false, context);

    rule
}

// =============================================================================
// CSSFontFaceRule
// =============================================================================

/// Create a CSSFontFaceRule object
pub fn create_css_font_face_rule(
    declarations: &str,
    parent_stylesheet: Option<JsObject>,
    context: &mut Context,
) -> JsObject {
    let css_text = format!("@font-face {{ {} }}", declarations);
    let rule = create_css_rule(FONT_FACE_RULE, &css_text, None, parent_stylesheet, context);

    // style property (CSSStyleDeclaration)
    let style = create_rule_style_declaration(declarations, context);
    let _ = rule.set(js_string!("style"), JsValue::from(style), false, context);

    rule
}

// =============================================================================
// CSSKeyframesRule
// =============================================================================

/// Create a CSSKeyframesRule object
pub fn create_css_keyframes_rule(
    name: &str,
    keyframes: Vec<JsObject>,
    parent_stylesheet: Option<JsObject>,
    context: &mut Context,
) -> JsObject {
    let css_text = format!("@keyframes {} {{ ... }}", name);
    let rule = create_css_rule(KEYFRAMES_RULE, &css_text, None, parent_stylesheet, context);

    let name_rc = Rc::new(RefCell::new(name.to_string()));

    // name getter/setter
    let n = name_rc.clone();
    let name_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(n.borrow().clone())))
        })
    };

    let n = name_rc.clone();
    let name_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let new_name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            *n.borrow_mut() = new_name;
            Ok(JsValue::undefined())
        })
    };

    let _ = rule.define_property_or_throw(
        js_string!("name"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(name_getter.to_js_function(context.realm()))
            .set(name_setter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    // cssRules
    let css_rule_list = create_css_rule_list(keyframes, context);
    let _ = rule.set(js_string!("cssRules"), JsValue::from(css_rule_list), false, context);

    // appendRule
    let append_rule = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = rule.set(js_string!("appendRule"), append_rule.to_js_function(context.realm()), false, context);

    // deleteRule
    let delete_rule = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = rule.set(js_string!("deleteRule"), delete_rule.to_js_function(context.realm()), false, context);

    // findRule
    let find_rule = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });
    let _ = rule.set(js_string!("findRule"), find_rule.to_js_function(context.realm()), false, context);

    rule
}

// =============================================================================
// CSSKeyframeRule
// =============================================================================

/// Create a CSSKeyframeRule object
pub fn create_css_keyframe_rule(
    key_text: &str,
    style_declarations: &str,
    parent_rule: Option<JsObject>,
    context: &mut Context,
) -> JsObject {
    let css_text = format!("{} {{ {} }}", key_text, style_declarations);
    let rule = create_css_rule(KEYFRAME_RULE, &css_text, parent_rule, None, context);

    let key_rc = Rc::new(RefCell::new(key_text.to_string()));

    // keyText getter/setter
    let k = key_rc.clone();
    let key_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(k.borrow().clone())))
        })
    };

    let k = key_rc.clone();
    let key_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let new_key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            *k.borrow_mut() = new_key;
            Ok(JsValue::undefined())
        })
    };

    let _ = rule.define_property_or_throw(
        js_string!("keyText"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(key_getter.to_js_function(context.realm()))
            .set(key_setter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    // style
    let style = create_rule_style_declaration(style_declarations, context);
    let _ = rule.set(js_string!("style"), JsValue::from(style), false, context);

    rule
}

// =============================================================================
// CSSNamespaceRule
// =============================================================================

/// Create a CSSNamespaceRule object
pub fn create_css_namespace_rule(
    namespace_uri: &str,
    prefix: &str,
    parent_stylesheet: Option<JsObject>,
    context: &mut Context,
) -> JsObject {
    let css_text = if prefix.is_empty() {
        format!("@namespace url(\"{}\");", namespace_uri)
    } else {
        format!("@namespace {} url(\"{}\");", prefix, namespace_uri)
    };
    let rule = create_css_rule(NAMESPACE_RULE, &css_text, None, parent_stylesheet, context);

    let _ = rule.set(js_string!("namespaceURI"), JsValue::from(js_string!(namespace_uri)), false, context);
    let _ = rule.set(js_string!("prefix"), JsValue::from(js_string!(prefix)), false, context);

    rule
}

// =============================================================================
// CSSLayerBlockRule
// =============================================================================

/// Create a CSSLayerBlockRule object
pub fn create_css_layer_block_rule(
    name: &str,
    css_rules: Vec<JsObject>,
    parent_stylesheet: Option<JsObject>,
    context: &mut Context,
) -> JsObject {
    let css_text = format!("@layer {} {{ ... }}", name);
    let rule = create_css_rule(LAYER_BLOCK_RULE, &css_text, None, parent_stylesheet, context);

    let _ = rule.set(js_string!("name"), JsValue::from(js_string!(name)), false, context);

    // cssRules
    let css_rule_list = create_css_rule_list(css_rules, context);
    let _ = rule.set(js_string!("cssRules"), JsValue::from(css_rule_list), false, context);

    // insertRule
    let insert_rule = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(0))
    });
    let _ = rule.set(js_string!("insertRule"), insert_rule.to_js_function(context.realm()), false, context);

    // deleteRule
    let delete_rule = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = rule.set(js_string!("deleteRule"), delete_rule.to_js_function(context.realm()), false, context);

    rule
}

// =============================================================================
// CSSLayerStatementRule
// =============================================================================

/// Create a CSSLayerStatementRule object
pub fn create_css_layer_statement_rule(
    name_list: Vec<String>,
    parent_stylesheet: Option<JsObject>,
    context: &mut Context,
) -> JsObject {
    let css_text = format!("@layer {};", name_list.join(", "));
    let rule = create_css_rule(LAYER_STATEMENT_RULE, &css_text, None, parent_stylesheet, context);

    // nameList as a frozen array
    let names_array = JsArray::new(context);
    for (i, name) in name_list.iter().enumerate() {
        let _ = names_array.set(i as u32, JsValue::from(js_string!(name.clone())), false, context);
    }
    let _ = rule.set(js_string!("nameList"), JsValue::from(names_array), false, context);

    rule
}

// =============================================================================
// CSSPageRule
// =============================================================================

/// Create a CSSPageRule object
pub fn create_css_page_rule(
    selector_text: &str,
    style_declarations: &str,
    parent_stylesheet: Option<JsObject>,
    context: &mut Context,
) -> JsObject {
    let css_text = format!("@page {} {{ {} }}", selector_text, style_declarations);
    let rule = create_css_rule(PAGE_RULE, &css_text, None, parent_stylesheet, context);

    let _ = rule.set(js_string!("selectorText"), JsValue::from(js_string!(selector_text)), false, context);

    let style = create_rule_style_declaration(style_declarations, context);
    let _ = rule.set(js_string!("style"), JsValue::from(style), false, context);

    rule
}

// =============================================================================
// CSSStyleSheet
// =============================================================================

/// Create a CSSStyleSheet object
pub fn create_css_stylesheet(
    href: Option<&str>,
    title: Option<&str>,
    media_text: &str,
    owner_node: Option<JsObject>,
    disabled: bool,
    context: &mut Context,
) -> JsObject {
    let rules: Rc<RefCell<Vec<JsObject>>> = Rc::new(RefCell::new(Vec::new()));
    let disabled_rc = Rc::new(RefCell::new(disabled));

    // disabled getter/setter
    let d = disabled_rc.clone();
    let disabled_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*d.borrow()))
        })
    };

    let d = disabled_rc.clone();
    let disabled_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            let val = args.get_or_undefined(0).to_boolean();
            *d.borrow_mut() = val;
            Ok(JsValue::undefined())
        })
    };

    // cssRules getter
    let rules_clone = rules.clone();
    let css_rules_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let rules = rules_clone.borrow().clone();
            Ok(JsValue::from(create_css_rule_list(rules, ctx)))
        })
    };

    // insertRule(rule, index)
    let rules_clone = rules.clone();
    let insert_rule = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let rule_text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let index = if args.len() > 1 {
                args.get_or_undefined(1).to_u32(ctx)? as usize
            } else {
                rules_clone.borrow().len()
            };

            // Parse and create rule (simplified)
            let rule = if rule_text.starts_with("@media") {
                create_css_media_rule("all", vec![], None, ctx)
            } else if rule_text.starts_with("@import") {
                create_css_import_rule("", "", None, ctx)
            } else if rule_text.starts_with("@font-face") {
                create_css_font_face_rule("", None, ctx)
            } else if rule_text.starts_with("@keyframes") {
                create_css_keyframes_rule("", vec![], None, ctx)
            } else {
                // Style rule
                create_css_style_rule("*", "", None, ctx)
            };

            let mut rules = rules_clone.borrow_mut();
            if index <= rules.len() {
                rules.insert(index, rule);
            }
            Ok(JsValue::from(index as i32))
        })
    };

    // deleteRule(index)
    let rules_clone = rules.clone();
    let delete_rule = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let mut rules = rules_clone.borrow_mut();
            if index < rules.len() {
                rules.remove(index);
            }
            Ok(JsValue::undefined())
        })
    };

    // addRule (deprecated but still used)
    let rules_clone = rules.clone();
    let add_rule = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let selector = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let style = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let index = if args.len() > 2 {
                args.get_or_undefined(2).to_u32(ctx)? as usize
            } else {
                rules_clone.borrow().len()
            };

            let rule = create_css_style_rule(&selector, &style, None, ctx);
            let mut rules = rules_clone.borrow_mut();
            if index <= rules.len() {
                rules.insert(index, rule);
            }
            Ok(JsValue::from(index as i32))
        })
    };

    // removeRule (deprecated)
    let rules_clone = rules.clone();
    let remove_rule = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let mut rules = rules_clone.borrow_mut();
            if index < rules.len() {
                rules.remove(index);
            }
            Ok(JsValue::undefined())
        })
    };

    // replace(text) - async
    let replace = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Return a resolved promise with this stylesheet
        let promise = boa_engine::object::builtins::JsPromise::resolve(JsValue::undefined(), ctx);
        Ok(JsValue::from(promise))
    });

    // replaceSync(text)
    let replace_sync = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let media = create_media_list(media_text, context);

    let obj = ObjectInitializer::new(context)
        .property(js_string!("type"), js_string!("text/css"), Attribute::READONLY)
        .property(
            js_string!("href"),
            href.map(|h| JsValue::from(js_string!(h))).unwrap_or(JsValue::null()),
            Attribute::READONLY,
        )
        .property(
            js_string!("title"),
            title.map(|t| JsValue::from(js_string!(t))).unwrap_or(JsValue::null()),
            Attribute::READONLY,
        )
        .property(
            js_string!("ownerNode"),
            owner_node.map(JsValue::from).unwrap_or(JsValue::null()),
            Attribute::READONLY,
        )
        .property(js_string!("parentStyleSheet"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("media"), media, Attribute::READONLY)
        .property(js_string!("ownerRule"), JsValue::null(), Attribute::READONLY)
        .function(insert_rule, js_string!("insertRule"), 2)
        .function(delete_rule, js_string!("deleteRule"), 1)
        .function(add_rule, js_string!("addRule"), 3)
        .function(remove_rule, js_string!("removeRule"), 1)
        .function(replace, js_string!("replace"), 1)
        .function(replace_sync, js_string!("replaceSync"), 1)
        .build();

    let _ = obj.define_property_or_throw(
        js_string!("disabled"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(disabled_getter.to_js_function(context.realm()))
            .set(disabled_setter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    let _ = obj.define_property_or_throw(
        js_string!("cssRules"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(css_rules_getter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    // rules (deprecated alias)
    let rules_clone = rules.clone();
    let rules_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let rules = rules_clone.borrow().clone();
            Ok(JsValue::from(create_css_rule_list(rules, ctx)))
        })
    };

    let _ = obj.define_property_or_throw(
        js_string!("rules"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(rules_getter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    obj
}

// =============================================================================
// CSSStyleSheet Constructor
// =============================================================================

/// Register CSSStyleSheet constructor for constructable stylesheets
pub fn register_css_stylesheet_constructor(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let options = args.get_or_undefined(0);

        let media = if options.is_object() {
            options
                .as_object()
                .and_then(|o| o.get(js_string!("media"), ctx).ok())
                .and_then(|v| v.to_string(ctx).ok())
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default()
        } else {
            String::new()
        };

        let disabled = if options.is_object() {
            options
                .as_object()
                .and_then(|o| o.get(js_string!("disabled"), ctx).ok())
                .map(|v| v.to_boolean())
                .unwrap_or(false)
        } else {
            false
        };

        Ok(JsValue::from(create_css_stylesheet(None, None, &media, None, disabled, ctx)))
    });

    context.register_global_property(
        js_string!("CSSStyleSheet"),
        constructor.to_js_function(context.realm()),
        Attribute::WRITABLE | Attribute::CONFIGURABLE,
    )?;

    Ok(())
}

// =============================================================================
// Integration - document.styleSheets
// =============================================================================

/// Create document.styleSheets property
pub fn create_document_stylesheets(style_elements: Vec<JsObject>, context: &mut Context) -> JsObject {
    let sheets: Vec<JsObject> = style_elements
        .iter()
        .map(|_| create_css_stylesheet(None, None, "", None, false, context))
        .collect();

    create_style_sheet_list(sheets, context)
}

// =============================================================================
// Initialize all CSSOM APIs
// =============================================================================

/// Register all CSSOM-related global objects and constructors
pub fn register_cssom_apis(context: &mut Context) -> JsResult<()> {
    register_css_namespace(context)?;
    register_css_stylesheet_constructor(context)?;

    // Register CSSRule constructor (throws)
    let css_rule_ctor = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Err(boa_engine::JsError::from_opaque(JsValue::from(js_string!(
            "Illegal constructor"
        ))))
    });
    let css_rule_fn = css_rule_ctor.to_js_function(context.realm());

    // Add CSSRule constants to constructor
    let _ = css_rule_fn.set(js_string!("STYLE_RULE"), JsValue::from(STYLE_RULE), false, context);
    let _ = css_rule_fn.set(js_string!("CHARSET_RULE"), JsValue::from(CHARSET_RULE), false, context);
    let _ = css_rule_fn.set(js_string!("IMPORT_RULE"), JsValue::from(IMPORT_RULE), false, context);
    let _ = css_rule_fn.set(js_string!("MEDIA_RULE"), JsValue::from(MEDIA_RULE), false, context);
    let _ = css_rule_fn.set(js_string!("FONT_FACE_RULE"), JsValue::from(FONT_FACE_RULE), false, context);
    let _ = css_rule_fn.set(js_string!("PAGE_RULE"), JsValue::from(PAGE_RULE), false, context);
    let _ = css_rule_fn.set(js_string!("KEYFRAMES_RULE"), JsValue::from(KEYFRAMES_RULE), false, context);
    let _ = css_rule_fn.set(js_string!("KEYFRAME_RULE"), JsValue::from(KEYFRAME_RULE), false, context);
    let _ = css_rule_fn.set(js_string!("NAMESPACE_RULE"), JsValue::from(NAMESPACE_RULE), false, context);
    let _ = css_rule_fn.set(js_string!("COUNTER_STYLE_RULE"), JsValue::from(COUNTER_STYLE_RULE), false, context);
    let _ = css_rule_fn.set(js_string!("SUPPORTS_RULE"), JsValue::from(SUPPORTS_RULE), false, context);
    let _ = css_rule_fn.set(js_string!("LAYER_BLOCK_RULE"), JsValue::from(LAYER_BLOCK_RULE), false, context);
    let _ = css_rule_fn.set(js_string!("LAYER_STATEMENT_RULE"), JsValue::from(LAYER_STATEMENT_RULE), false, context);

    context.register_global_property(
        js_string!("CSSRule"),
        css_rule_fn,
        Attribute::WRITABLE | Attribute::CONFIGURABLE,
    )?;

    // Register FontFace and FontFaceSet constructors
    register_font_face_constructors(context)?;

    Ok(())
}

// =============================================================================
// CSS Parser Implementation
// =============================================================================

/// Parsed CSS rule types
#[derive(Debug, Clone)]
pub enum ParsedRule {
    Style {
        selector: String,
        declarations: String,
    },
    Media {
        condition: String,
        rules: Vec<ParsedRule>,
    },
    Import {
        url: String,
        media: String,
    },
    FontFace {
        declarations: String,
    },
    Keyframes {
        name: String,
        keyframes: Vec<ParsedKeyframe>,
    },
    Supports {
        condition: String,
        rules: Vec<ParsedRule>,
    },
    Namespace {
        prefix: String,
        uri: String,
    },
    Page {
        selector: String,
        declarations: String,
    },
    Layer {
        name: String,
        rules: Vec<ParsedRule>,
    },
    LayerStatement {
        names: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct ParsedKeyframe {
    pub key_text: String,
    pub declarations: String,
}

/// Parse @import url
fn parse_import_url<'i, 't>(input: &mut Parser<'i, 't>) -> String {
    // Try url() token first
    if let Ok(url) = input.try_parse(|i| {
        let result: Result<String, cssparser::ParseError<()>> = i.expect_url().map(|s| s.as_ref().to_string()).map_err(|e| e.into());
        result
    }) {
        return url;
    }

    // Try string token
    if let Ok(s) = input.try_parse(|i| {
        let result: Result<String, cssparser::ParseError<()>> = i.expect_string().map(|s| s.as_ref().to_string()).map_err(|e| e.into());
        result
    }) {
        return s;
    }

    // Try url() function
    if let Ok(Token::Function(name)) = input.next() {
        let name_str = name.as_ref().to_string();
        if name_str == "url" {
            let result: Result<String, cssparser::ParseError<()>> = input.parse_nested_block(|i| {
                if let Ok(s) = i.expect_string() {
                    return Ok(s.as_ref().to_string());
                }
                if let Ok(s) = i.expect_url_or_string() {
                    return Ok(s.as_ref().to_string());
                }
                // Return empty string on failure
                Ok(String::new())
            });
            if let Ok(url) = result {
                return url;
            }
        }
    }

    String::new()
}

/// Consume tokens until a block starts (preserving for selector)
fn consume_until_block<'i, 't>(input: &mut Parser<'i, 't>) -> String {
    let mut result = String::new();
    loop {
        let state = input.state();
        match input.next_including_whitespace() {
            Ok(token) => {
                match token {
                    Token::CurlyBracketBlock => {
                        input.reset(&state);
                        break;
                    }
                    _ => result.push_str(&token_to_string(token)),
                }
            }
            Err(_) => break,
        }
    }
    result.trim().to_string()
}

/// Consume tokens until semicolon
fn consume_until_semicolon<'i, 't>(input: &mut Parser<'i, 't>) -> String {
    let mut result = String::new();
    loop {
        match input.next_including_whitespace() {
            Ok(token) => {
                match token {
                    Token::Semicolon => break,
                    _ => result.push_str(&token_to_string(token)),
                }
            }
            Err(_) => break,
        }
    }
    result.trim().to_string()
}

/// Consume tokens until block or semicolon
fn consume_until_block_or_semicolon<'i, 't>(input: &mut Parser<'i, 't>) -> String {
    let mut result = String::new();
    loop {
        let state = input.state();
        match input.next_including_whitespace() {
            Ok(token) => {
                match token {
                    Token::CurlyBracketBlock | Token::Semicolon => {
                        input.reset(&state);
                        break;
                    }
                    _ => result.push_str(&token_to_string(token)),
                }
            }
            Err(_) => break,
        }
    }
    result.trim().to_string()
}

/// Consume all tokens inside a block
fn consume_block_contents<'i, 't>(input: &mut Parser<'i, 't>) -> Result<String, cssparser::ParseError<'i, ()>> {
    let mut result = String::new();
    loop {
        match input.next_including_whitespace_and_comments() {
            Ok(token) => result.push_str(&token_to_string(token)),
            Err(_) => break,
        }
    }
    Ok(result.trim().to_string())
}

/// Convert a token to its string representation
fn token_to_string(token: &Token) -> String {
    match token {
        Token::Ident(s) => s.as_ref().to_string(),
        Token::AtKeyword(s) => format!("@{}", s.as_ref()),
        Token::Hash(s) => format!("#{}", s.as_ref()),
        Token::IDHash(s) => format!("#{}", s.as_ref()),
        Token::QuotedString(s) => format!("\"{}\"", s.as_ref()),
        Token::UnquotedUrl(s) => format!("url({})", s.as_ref()),
        Token::Number { value, .. } => value.to_string(),
        Token::Percentage { unit_value, .. } => format!("{}%", unit_value * 100.0),
        Token::Dimension { value, unit, .. } => format!("{}{}", value, unit.as_ref()),
        Token::WhiteSpace(_) => " ".to_string(),
        Token::Colon => ":".to_string(),
        Token::Semicolon => ";".to_string(),
        Token::Comma => ",".to_string(),
        Token::IncludeMatch => "~=".to_string(),
        Token::DashMatch => "|=".to_string(),
        Token::PrefixMatch => "^=".to_string(),
        Token::SuffixMatch => "$=".to_string(),
        Token::SubstringMatch => "*=".to_string(),
        Token::Delim(c) => c.to_string(),
        Token::ParenthesisBlock => "(".to_string(),
        Token::SquareBracketBlock => "[".to_string(),
        Token::CurlyBracketBlock => "{".to_string(),
        Token::CloseParenthesis => ")".to_string(),
        Token::CloseSquareBracket => "]".to_string(),
        Token::CloseCurlyBracket => "}".to_string(),
        Token::Function(name) => format!("{}(", name.as_ref()),
        Token::BadString(_) => String::new(),
        Token::BadUrl(_) => String::new(),
        Token::Comment(_) => String::new(),
        Token::CDC => "-->".to_string(),
        Token::CDO => "<!--".to_string(),
    }
}

/// Parse rules from a block
fn parse_rules<'i, 't>(input: &mut Parser<'i, 't>) -> Result<Vec<ParsedRule>, cssparser::ParseError<'i, ()>> {
    let mut rules = Vec::new();

    loop {
        // Skip whitespace
        loop {
            match input.next_including_whitespace() {
                Ok(Token::WhiteSpace(_)) => continue,
                Ok(Token::Comment(_)) => continue,
                Ok(token) => {
                    // Put the token back by processing it
                    match token {
                        Token::AtKeyword(name) => {
                            let name_str = name.as_ref().to_string();
                            if let Some(rule) = parse_at_rule(&name_str, input) {
                                rules.push(rule);
                            }
                        }
                        _ => {
                            // This is the start of a selector - collect it
                            let mut selector = token_to_string(&token);
                            selector.push_str(&consume_until_block(input));
                            let selector = selector.trim().to_string();

                            if !selector.is_empty() {
                                if let Ok(Token::CurlyBracketBlock) = input.next() {
                                    let declarations = input.parse_nested_block(consume_block_contents)
                                        .unwrap_or_default();
                                    rules.push(ParsedRule::Style { selector, declarations });
                                }
                            }
                        }
                    }
                    break;
                }
                Err(_) => return Ok(rules),
            }
        }
    }
}

/// Parse an at-rule given its name
fn parse_at_rule<'i, 't>(name: &str, input: &mut Parser<'i, 't>) -> Option<ParsedRule> {
    match name {
        "media" => {
            let condition = consume_until_block(input);
            if let Ok(Token::CurlyBracketBlock) = input.next() {
                let result: Result<Vec<ParsedRule>, cssparser::ParseError<()>> =
                    input.parse_nested_block(parse_rules);
                let rules = result.unwrap_or_default();
                Some(ParsedRule::Media { condition, rules })
            } else {
                None
            }
        }
        "import" => {
            let url = parse_import_url(input);
            let media = consume_until_semicolon(input);
            Some(ParsedRule::Import { url, media })
        }
        "font-face" => {
            if let Ok(Token::CurlyBracketBlock) = input.next() {
                let result: Result<String, cssparser::ParseError<()>> =
                    input.parse_nested_block(consume_block_contents);
                let declarations = result.unwrap_or_default();
                Some(ParsedRule::FontFace { declarations })
            } else {
                None
            }
        }
        "keyframes" | "-webkit-keyframes" => {
            let kf_name = consume_until_block(input).trim().to_string();
            if let Ok(Token::CurlyBracketBlock) = input.next() {
                let result: Result<Vec<ParsedKeyframe>, cssparser::ParseError<()>> =
                    input.parse_nested_block(parse_keyframes);
                let keyframes = result.unwrap_or_default();
                Some(ParsedRule::Keyframes { name: kf_name, keyframes })
            } else {
                None
            }
        }
        "supports" => {
            let condition = consume_until_block(input);
            if let Ok(Token::CurlyBracketBlock) = input.next() {
                let result: Result<Vec<ParsedRule>, cssparser::ParseError<()>> =
                    input.parse_nested_block(parse_rules);
                let rules = result.unwrap_or_default();
                Some(ParsedRule::Supports { condition, rules })
            } else {
                None
            }
        }
        "namespace" => {
            let text = consume_until_semicolon(input);
            let parts: Vec<&str> = text.split_whitespace().collect();
            let (prefix, uri) = if parts.len() >= 2 {
                (parts[0].to_string(), parts[1].trim_matches(|c| c == '"' || c == '\'').to_string())
            } else if parts.len() == 1 {
                (String::new(), parts[0].trim_matches(|c| c == '"' || c == '\'').to_string())
            } else {
                (String::new(), String::new())
            };
            Some(ParsedRule::Namespace { prefix, uri })
        }
        "page" => {
            let selector = consume_until_block(input);
            if let Ok(Token::CurlyBracketBlock) = input.next() {
                let result: Result<String, cssparser::ParseError<()>> =
                    input.parse_nested_block(consume_block_contents);
                let declarations = result.unwrap_or_default();
                Some(ParsedRule::Page { selector, declarations })
            } else {
                None
            }
        }
        "layer" => {
            let text = consume_until_block_or_semicolon(input);
            match input.next() {
                Ok(Token::CurlyBracketBlock) => {
                    let result: Result<Vec<ParsedRule>, cssparser::ParseError<()>> =
                        input.parse_nested_block(parse_rules);
                    let rules = result.unwrap_or_default();
                    Some(ParsedRule::Layer { name: text.trim().to_string(), rules })
                }
                Ok(Token::Semicolon) | Err(_) => {
                    let names: Vec<String> = text.split(',').map(|s| s.trim().to_string()).collect();
                    Some(ParsedRule::LayerStatement { names })
                }
                _ => None,
            }
        }
        _ => {
            // Unknown at-rule - skip to semicolon or block
            loop {
                match input.next() {
                    Ok(Token::Semicolon) | Err(_) => break,
                    Ok(Token::CurlyBracketBlock) => {
                        let _: Result<(), cssparser::ParseError<()>> =
                            input.parse_nested_block(|_| Ok(()));
                        break;
                    }
                    _ => continue,
                }
            }
            None
        }
    }
}

/// Parse keyframes block
fn parse_keyframes<'i, 't>(input: &mut Parser<'i, 't>) -> Result<Vec<ParsedKeyframe>, cssparser::ParseError<'i, ()>> {
    let mut keyframes = Vec::new();

    loop {
        // Skip whitespace
        loop {
            match input.next_including_whitespace() {
                Ok(Token::WhiteSpace(_)) | Ok(Token::Comment(_)) => continue,
                Ok(token) => {
                    // Start of keyframe selector
                    let mut key_text = token_to_string(&token);
                    key_text.push_str(&consume_until_block(input));
                    let key_text = key_text.trim().to_string();

                    if key_text.is_empty() {
                        return Ok(keyframes);
                    }

                    // Parse the keyframe block
                    if let Ok(Token::CurlyBracketBlock) = input.next() {
                        let declarations = input.parse_nested_block(consume_block_contents)
                            .unwrap_or_default();
                        keyframes.push(ParsedKeyframe {
                            key_text,
                            declarations,
                        });
                    }
                    break;
                }
                Err(_) => return Ok(keyframes),
            }
        }
    }
}

/// Parse a complete CSS stylesheet text
pub fn parse_css(css_text: &str) -> Vec<ParsedRule> {
    let mut input = ParserInput::new(css_text);
    let mut parser = Parser::new(&mut input);
    parse_rules(&mut parser).unwrap_or_default()
}

/// Create a CSSStyleSheet from parsed CSS text
pub fn create_css_stylesheet_from_css(
    css_text: &str,
    href: Option<&str>,
    title: Option<&str>,
    media_text: &str,
    owner_node: Option<JsObject>,
    disabled: bool,
    context: &mut Context,
) -> JsObject {
    let parsed_rules = parse_css(css_text);

    let rules: Rc<RefCell<Vec<JsObject>>> = Rc::new(RefCell::new(Vec::new()));
    let disabled_rc = Rc::new(RefCell::new(disabled));

    // Convert parsed rules to JS objects
    {
        let mut js_rules = rules.borrow_mut();
        for parsed_rule in parsed_rules {
            if let Some(js_rule) = parsed_rule_to_js_object(&parsed_rule, None, context) {
                js_rules.push(js_rule);
            }
        }
    }

    // disabled getter/setter
    let d = disabled_rc.clone();
    let disabled_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*d.borrow()))
        })
    };

    let d = disabled_rc.clone();
    let disabled_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            let val = args.get_or_undefined(0).to_boolean();
            *d.borrow_mut() = val;
            Ok(JsValue::undefined())
        })
    };

    // cssRules getter
    let rules_clone = rules.clone();
    let css_rules_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let rules = rules_clone.borrow().clone();
            Ok(JsValue::from(create_css_rule_list(rules, ctx)))
        })
    };

    // insertRule(rule, index)
    let rules_clone = rules.clone();
    let insert_rule = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let rule_text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let index = if args.len() > 1 {
                args.get_or_undefined(1).to_u32(ctx)? as usize
            } else {
                rules_clone.borrow().len()
            };

            // Parse the rule text
            let parsed = parse_css(&rule_text);
            if let Some(parsed_rule) = parsed.first() {
                if let Some(js_rule) = parsed_rule_to_js_object(parsed_rule, None, ctx) {
                    let mut rules = rules_clone.borrow_mut();
                    if index <= rules.len() {
                        rules.insert(index, js_rule);
                    }
                }
            }
            Ok(JsValue::from(index as i32))
        })
    };

    // deleteRule(index)
    let rules_clone = rules.clone();
    let delete_rule = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let mut rules = rules_clone.borrow_mut();
            if index < rules.len() {
                rules.remove(index);
            }
            Ok(JsValue::undefined())
        })
    };

    // addRule (deprecated but still used)
    let rules_clone = rules.clone();
    let add_rule = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let selector = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let style = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let index = if args.len() > 2 {
                args.get_or_undefined(2).to_u32(ctx)? as usize
            } else {
                rules_clone.borrow().len()
            };

            let rule = create_css_style_rule(&selector, &style, None, ctx);
            let mut rules = rules_clone.borrow_mut();
            if index <= rules.len() {
                rules.insert(index, rule);
            }
            Ok(JsValue::from(index as i32))
        })
    };

    // removeRule (deprecated)
    let rules_clone = rules.clone();
    let remove_rule = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let mut rules = rules_clone.borrow_mut();
            if index < rules.len() {
                rules.remove(index);
            }
            Ok(JsValue::undefined())
        })
    };

    // replace(text) - async
    let rules_for_replace = rules.clone();
    let replace = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let parsed = parse_css(&text);
            let mut rules = rules_for_replace.borrow_mut();
            rules.clear();
            for parsed_rule in parsed {
                if let Some(js_rule) = parsed_rule_to_js_object(&parsed_rule, None, ctx) {
                    rules.push(js_rule);
                }
            }
            let promise = boa_engine::object::builtins::JsPromise::resolve(JsValue::undefined(), ctx);
            Ok(JsValue::from(promise))
        })
    };

    // replaceSync(text)
    let rules_for_replace_sync = rules.clone();
    let replace_sync = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let parsed = parse_css(&text);
            let mut rules = rules_for_replace_sync.borrow_mut();
            rules.clear();
            for parsed_rule in parsed {
                if let Some(js_rule) = parsed_rule_to_js_object(&parsed_rule, None, ctx) {
                    rules.push(js_rule);
                }
            }
            Ok(JsValue::undefined())
        })
    };

    let media = create_media_list(media_text, context);

    let obj = ObjectInitializer::new(context)
        .property(js_string!("type"), js_string!("text/css"), Attribute::READONLY)
        .property(
            js_string!("href"),
            href.map(|h| JsValue::from(js_string!(h))).unwrap_or(JsValue::null()),
            Attribute::READONLY,
        )
        .property(
            js_string!("title"),
            title.map(|t| JsValue::from(js_string!(t))).unwrap_or(JsValue::null()),
            Attribute::READONLY,
        )
        .property(
            js_string!("ownerNode"),
            owner_node.map(JsValue::from).unwrap_or(JsValue::null()),
            Attribute::READONLY,
        )
        .property(js_string!("parentStyleSheet"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("media"), media, Attribute::READONLY)
        .property(js_string!("ownerRule"), JsValue::null(), Attribute::READONLY)
        .function(insert_rule, js_string!("insertRule"), 2)
        .function(delete_rule, js_string!("deleteRule"), 1)
        .function(add_rule, js_string!("addRule"), 3)
        .function(remove_rule, js_string!("removeRule"), 1)
        .function(replace, js_string!("replace"), 1)
        .function(replace_sync, js_string!("replaceSync"), 1)
        .build();

    let _ = obj.define_property_or_throw(
        js_string!("disabled"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(disabled_getter.to_js_function(context.realm()))
            .set(disabled_setter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    let _ = obj.define_property_or_throw(
        js_string!("cssRules"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(css_rules_getter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    // rules (deprecated alias)
    let rules_clone = rules.clone();
    let rules_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let rules = rules_clone.borrow().clone();
            Ok(JsValue::from(create_css_rule_list(rules, ctx)))
        })
    };

    let _ = obj.define_property_or_throw(
        js_string!("rules"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(rules_getter.to_js_function(context.realm()))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );

    obj
}

/// Convert a parsed rule to a JS object
fn parsed_rule_to_js_object(
    rule: &ParsedRule,
    parent_stylesheet: Option<JsObject>,
    context: &mut Context,
) -> Option<JsObject> {
    match rule {
        ParsedRule::Style { selector, declarations } => {
            Some(create_css_style_rule(selector, declarations, parent_stylesheet, context))
        }
        ParsedRule::Media { condition, rules } => {
            let js_rules: Vec<JsObject> = rules
                .iter()
                .filter_map(|r| parsed_rule_to_js_object(r, parent_stylesheet.clone(), context))
                .collect();
            Some(create_css_media_rule(condition, js_rules, parent_stylesheet, context))
        }
        ParsedRule::Import { url, media } => {
            Some(create_css_import_rule(url, media, parent_stylesheet, context))
        }
        ParsedRule::FontFace { declarations } => {
            Some(create_css_font_face_rule(declarations, parent_stylesheet, context))
        }
        ParsedRule::Keyframes { name, keyframes } => {
            let js_keyframes: Vec<JsObject> = keyframes
                .iter()
                .map(|kf| create_css_keyframe_rule(&kf.key_text, &kf.declarations, None, context))
                .collect();
            Some(create_css_keyframes_rule(name, js_keyframes, parent_stylesheet, context))
        }
        ParsedRule::Supports { condition, rules } => {
            let js_rules: Vec<JsObject> = rules
                .iter()
                .filter_map(|r| parsed_rule_to_js_object(r, parent_stylesheet.clone(), context))
                .collect();
            Some(create_css_supports_rule(condition, js_rules, parent_stylesheet, context))
        }
        ParsedRule::Namespace { prefix, uri } => {
            Some(create_css_namespace_rule(uri, prefix, parent_stylesheet, context))
        }
        ParsedRule::Page { selector, declarations } => {
            Some(create_css_page_rule(selector, declarations, parent_stylesheet, context))
        }
        ParsedRule::Layer { name, rules } => {
            let js_rules: Vec<JsObject> = rules
                .iter()
                .filter_map(|r| parsed_rule_to_js_object(r, parent_stylesheet.clone(), context))
                .collect();
            Some(create_css_layer_block_rule(name, js_rules, parent_stylesheet, context))
        }
        ParsedRule::LayerStatement { names } => {
            Some(create_css_layer_statement_rule(names.clone(), parent_stylesheet, context))
        }
    }
}

// =============================================================================
// FontFace API
// =============================================================================

/// Create a resolved promise for font loading
fn create_font_resolved_promise(ctx: &mut Context, value: JsValue) -> JsResult<JsValue> {
    let value_clone = value.clone();
    let then = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            if let Some(callback) = args.first() {
                if callback.is_callable() {
                    let callback_obj = callback.as_callable().unwrap();
                    return callback_obj.call(&JsValue::undefined(), &[value_clone.clone()], ctx);
                }
            }
            Ok(value_clone.clone())
        })
    };

    let catch_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let finally_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        if let Some(callback) = args.first() {
            if callback.is_callable() {
                let _ = callback.as_callable().unwrap().call(&JsValue::undefined(), &[], ctx);
            }
        }
        Ok(JsValue::undefined())
    });

    let promise = ObjectInitializer::new(ctx)
        .function(then, js_string!("then"), 1)
        .function(catch_fn, js_string!("catch"), 1)
        .function(finally_fn, js_string!("finally"), 1)
        .build();

    Ok(promise.into())
}

/// Create FontFace constructor
fn create_fontface_constructor(context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        // FontFace(family, source, descriptors?)
        let family = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let source = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
        let descriptors = args.get_or_undefined(2);

        // Default descriptor values
        let mut style = "normal".to_string();
        let mut weight = "normal".to_string();
        let mut stretch = "normal".to_string();
        let mut unicode_range = "U+0-10FFFF".to_string();
        let mut variant = "normal".to_string();
        let mut feature_settings = "normal".to_string();
        let mut variation_settings = "normal".to_string();
        let mut display = "auto".to_string();
        let mut ascent_override = "normal".to_string();
        let mut descent_override = "normal".to_string();
        let mut line_gap_override = "normal".to_string();

        // Parse descriptors if provided
        if let Some(desc_obj) = descriptors.as_object() {
            if let Ok(val) = desc_obj.get(js_string!("style"), ctx) {
                if !val.is_undefined() {
                    style = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
            if let Ok(val) = desc_obj.get(js_string!("weight"), ctx) {
                if !val.is_undefined() {
                    weight = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
            if let Ok(val) = desc_obj.get(js_string!("stretch"), ctx) {
                if !val.is_undefined() {
                    stretch = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
            if let Ok(val) = desc_obj.get(js_string!("unicodeRange"), ctx) {
                if !val.is_undefined() {
                    unicode_range = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
            if let Ok(val) = desc_obj.get(js_string!("variant"), ctx) {
                if !val.is_undefined() {
                    variant = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
            if let Ok(val) = desc_obj.get(js_string!("featureSettings"), ctx) {
                if !val.is_undefined() {
                    feature_settings = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
            if let Ok(val) = desc_obj.get(js_string!("variationSettings"), ctx) {
                if !val.is_undefined() {
                    variation_settings = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
            if let Ok(val) = desc_obj.get(js_string!("display"), ctx) {
                if !val.is_undefined() {
                    display = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
            if let Ok(val) = desc_obj.get(js_string!("ascentOverride"), ctx) {
                if !val.is_undefined() {
                    ascent_override = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
            if let Ok(val) = desc_obj.get(js_string!("descentOverride"), ctx) {
                if !val.is_undefined() {
                    descent_override = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
            if let Ok(val) = desc_obj.get(js_string!("lineGapOverride"), ctx) {
                if !val.is_undefined() {
                    line_gap_override = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
        }

        // load() method - returns a Promise that resolves to this FontFace
        let load = NativeFunction::from_copy_closure(|this, _args, ctx| {
            // Mark as loaded
            if let Some(obj) = this.as_object() {
                let _ = obj.set(js_string!("status"), js_string!("loaded"), false, ctx);
            }
            // Return a promise that resolves to this FontFace
            create_font_resolved_promise(ctx, this.clone())
        });

        // Create the loaded promise (resolves to this FontFace when loaded)
        // For our stub, fonts are immediately "loaded"
        let font_face = ObjectInitializer::new(ctx)
            // Writable properties
            .property(js_string!("family"), js_string!(family), Attribute::all())
            .property(js_string!("style"), js_string!(style), Attribute::all())
            .property(js_string!("weight"), js_string!(weight), Attribute::all())
            .property(js_string!("stretch"), js_string!(stretch), Attribute::all())
            .property(js_string!("unicodeRange"), js_string!(unicode_range), Attribute::all())
            .property(js_string!("variant"), js_string!(variant), Attribute::all())
            .property(js_string!("featureSettings"), js_string!(feature_settings), Attribute::all())
            .property(js_string!("variationSettings"), js_string!(variation_settings), Attribute::all())
            .property(js_string!("display"), js_string!(display), Attribute::all())
            .property(js_string!("ascentOverride"), js_string!(ascent_override), Attribute::all())
            .property(js_string!("descentOverride"), js_string!(descent_override), Attribute::all())
            .property(js_string!("lineGapOverride"), js_string!(line_gap_override), Attribute::all())
            // Source (readonly after construction)
            .property(js_string!("_source"), js_string!(source), Attribute::READONLY)
            // Status (readonly): "unloaded", "loading", "loaded", "error"
            .property(js_string!("status"), js_string!("unloaded"), Attribute::all())
            // Methods
            .function(load, js_string!("load"), 0)
            .build();

        // Create the loaded promise that resolves to this FontFace
        let loaded_promise = create_font_resolved_promise(ctx, JsValue::from(font_face.clone()))?;
        font_face.set(js_string!("loaded"), loaded_promise, false, ctx)?;

        Ok(JsValue::from(font_face))
    })
}

/// Create FontFaceSet constructor
fn create_fontfaceset_constructor(context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, _args, ctx| {
        create_fontfaceset_object(ctx)
    })
}

/// Create a FontFaceSet object (used by constructor and document.fonts)
pub fn create_fontfaceset_object(ctx: &mut Context) -> JsResult<JsValue> {
    // Internal storage for font faces
    let fonts_storage: Rc<RefCell<Vec<JsObject>>> = Rc::new(RefCell::new(Vec::new()));

    // add(fontFace) - adds a FontFace to the set
    let fonts_add = fonts_storage.clone();
    let add = unsafe {
        NativeFunction::from_closure(move |this, args, ctx| {
            let font_face = args.get_or_undefined(0);
            if let Some(ff_obj) = font_face.as_object() {
                fonts_add.borrow_mut().push(ff_obj.clone());
                // Update size
                if let Some(this_obj) = this.as_object() {
                    let _ = this_obj.set(js_string!("size"), JsValue::from(fonts_add.borrow().len() as i32), false, ctx);
                }
            }
            Ok(this.clone())
        })
    };

    // delete(fontFace) - removes a FontFace from the set
    let fonts_delete = fonts_storage.clone();
    let delete = unsafe {
        NativeFunction::from_closure(move |this, args, ctx| {
            let font_face = args.get_or_undefined(0);
            if let Some(ff_obj) = font_face.as_object() {
                let mut storage = fonts_delete.borrow_mut();
                let initial_len = storage.len();
                storage.retain(|f| !JsObject::equals(f, &ff_obj));
                let removed = storage.len() < initial_len;
                // Update size
                if let Some(this_obj) = this.as_object() {
                    let _ = this_obj.set(js_string!("size"), JsValue::from(storage.len() as i32), false, ctx);
                }
                return Ok(JsValue::from(removed));
            }
            Ok(JsValue::from(false))
        })
    };

    // clear() - removes all FontFaces from the set
    let fonts_clear = fonts_storage.clone();
    let clear = unsafe {
        NativeFunction::from_closure(move |this, _args, ctx| {
            fonts_clear.borrow_mut().clear();
            // Update size
            if let Some(this_obj) = this.as_object() {
                let _ = this_obj.set(js_string!("size"), JsValue::from(0), false, ctx);
            }
            Ok(JsValue::undefined())
        })
    };

    // has(fontFace) - checks if a FontFace is in the set
    let fonts_has = fonts_storage.clone();
    let has = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            let font_face = args.get_or_undefined(0);
            if let Some(ff_obj) = font_face.as_object() {
                let has_it = fonts_has.borrow().iter().any(|f| JsObject::equals(f, &ff_obj));
                return Ok(JsValue::from(has_it));
            }
            Ok(JsValue::from(false))
        })
    };

    // check(font, text?) - checks if fonts are loaded for the specified font
    let check = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let _font = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // In our stub, fonts are always considered available/loaded
        Ok(JsValue::from(true))
    });

    // load(font, text?) - loads fonts and returns a Promise
    let fonts_load = fonts_storage.clone();
    let load = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let _font = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            // Return a promise that resolves to an array of matching FontFaces
            // For our stub, return all loaded fonts
            let result = JsArray::new(ctx);
            for (i, font) in fonts_load.borrow().iter().enumerate() {
                result.set(i, JsValue::from(font.clone()), false, ctx)?;
            }
            create_font_resolved_promise(ctx, JsValue::from(result))
        })
    };

    // forEach(callback, thisArg?)
    let fonts_foreach = fonts_storage.clone();
    let for_each = unsafe {
        NativeFunction::from_closure(move |this, args, ctx| {
            let callback = args.get_or_undefined(0);
            let this_arg = args.get_or_undefined(1);
            if let Some(cb) = callback.as_callable() {
                for font in fonts_foreach.borrow().iter() {
                    let font_val = JsValue::from(font.clone());
                    cb.call(&this_arg, &[font_val.clone(), font_val.clone(), this.clone()], ctx)?;
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // entries() - returns an iterator of [fontFace, fontFace] pairs
    let fonts_entries = fonts_storage.clone();
    let entries = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let arr = JsArray::new(ctx);
            for (i, font) in fonts_entries.borrow().iter().enumerate() {
                let pair = JsArray::new(ctx);
                pair.set(0_usize, JsValue::from(font.clone()), false, ctx)?;
                pair.set(1_usize, JsValue::from(font.clone()), false, ctx)?;
                arr.set(i, JsValue::from(pair), false, ctx)?;
            }
            Ok(JsValue::from(arr))
        })
    };

    // keys() - returns an iterator of fontFaces
    let fonts_keys = fonts_storage.clone();
    let keys = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let arr = JsArray::new(ctx);
            for (i, font) in fonts_keys.borrow().iter().enumerate() {
                arr.set(i, JsValue::from(font.clone()), false, ctx)?;
            }
            Ok(JsValue::from(arr))
        })
    };

    // values() - returns an iterator of fontFaces
    let fonts_values = fonts_storage.clone();
    let values = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let arr = JsArray::new(ctx);
            for (i, font) in fonts_values.borrow().iter().enumerate() {
                arr.set(i, JsValue::from(font.clone()), false, ctx)?;
            }
            Ok(JsValue::from(arr))
        })
    };

    // Create the ready promise (resolves to this FontFaceSet when all fonts are loaded)
    let font_face_set = ObjectInitializer::new(ctx)
        // Properties
        .property(js_string!("status"), js_string!("loaded"), Attribute::READONLY)
        .property(js_string!("size"), JsValue::from(0), Attribute::all())
        // Event handlers
        .property(js_string!("onloading"), JsValue::null(), Attribute::all())
        .property(js_string!("onloadingdone"), JsValue::null(), Attribute::all())
        .property(js_string!("onloadingerror"), JsValue::null(), Attribute::all())
        // Set methods
        .function(add, js_string!("add"), 1)
        .function(delete, js_string!("delete"), 1)
        .function(clear, js_string!("clear"), 0)
        .function(has, js_string!("has"), 1)
        // Font loading methods
        .function(check, js_string!("check"), 1)
        .function(load, js_string!("load"), 1)
        // Iterator methods
        .function(for_each, js_string!("forEach"), 1)
        .function(entries, js_string!("entries"), 0)
        .function(keys, js_string!("keys"), 0)
        .function(values, js_string!("values"), 0)
        .build();

    // Create and set the ready promise
    let ready_promise = create_font_resolved_promise(ctx, JsValue::from(font_face_set.clone()))?;
    font_face_set.set(js_string!("ready"), ready_promise, false, ctx)?;

    Ok(JsValue::from(font_face_set))
}

/// Register FontFace and FontFaceSet constructors
pub fn register_font_face_constructors(context: &mut Context) -> JsResult<()> {
    use boa_engine::object::FunctionObjectBuilder;

    // Register FontFace constructor
    let fontface_native = create_fontface_constructor(context);
    let fontface_ctor = FunctionObjectBuilder::new(context.realm(), fontface_native)
        .name(js_string!("FontFace"))
        .length(2)
        .constructor(true)
        .build();
    context.register_global_property(js_string!("FontFace"), fontface_ctor, Attribute::all())?;

    // Register FontFaceSet constructor
    let fontfaceset_native = create_fontfaceset_constructor(context);
    let fontfaceset_ctor = FunctionObjectBuilder::new(context.realm(), fontfaceset_native)
        .name(js_string!("FontFaceSet"))
        .length(0)
        .constructor(true)
        .build();
    context.register_global_property(js_string!("FontFaceSet"), fontfaceset_ctor, Attribute::all())?;

    Ok(())
}
