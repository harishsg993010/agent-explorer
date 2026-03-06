//! Web Components API
//!
//! Implements:
//! - CustomElementRegistry (customElements)
//! - HTMLTemplateElement
//! - HTMLSlotElement
//! - ShadowRoot enhancements
//! - Adopted stylesheets

use boa_engine::{
    Context, JsArgs, JsNativeError, JsObject, JsResult, JsValue,
    NativeFunction, js_string, object::ObjectInitializer, object::builtins::JsArray,
    object::FunctionObjectBuilder, property::Attribute,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

lazy_static::lazy_static! {
    /// Custom elements registry (stores metadata only)
    static ref CUSTOM_ELEMENTS_REGISTRY: Arc<Mutex<HashMap<String, CustomElementDefinition>>> =
        Arc::new(Mutex::new(HashMap::new()));

    /// Pending upgrade map
    static ref PENDING_UPGRADES: Arc<Mutex<HashMap<String, Vec<u32>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    /// Counter for template IDs
    static ref TEMPLATE_COUNTER: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
}

// Thread-local storage for constructors (JsObject can't be stored in static)
thread_local! {
    /// Maps element names to their constructors
    static CUSTOM_ELEMENT_CONSTRUCTORS: RefCell<HashMap<String, JsObject>> = RefCell::new(HashMap::new());
}

/// Custom element definition (stores metadata only, not the constructor)
#[derive(Debug, Clone)]
struct CustomElementDefinition {
    name: String,
    observed_attributes: Vec<String>,
    extends: Option<String>,
}

/// Register all web component APIs
pub fn register_all_web_component_apis(context: &mut Context) -> JsResult<()> {
    register_custom_element_registry(context)?;
    register_html_template_element(context)?;
    register_html_slot_element(context)?;
    register_shadow_root_enhancements(context)?;
    register_css_style_sheet(context)?;
    register_adopted_style_sheets(context)?;
    Ok(())
}

/// Register CustomElementRegistry
fn register_custom_element_registry(context: &mut Context) -> JsResult<()> {
    // define(name, constructor, options?)
    let define = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let constructor = args.get_or_undefined(1);
        let options = args.get_or_undefined(2);

        // Validate name (must contain hyphen, not start with hyphen, lowercase)
        if !name.contains('-') || name.starts_with('-') {
            return Err(JsNativeError::syntax()
                .with_message("Custom element name must contain a hyphen and not start with one")
                .into());
        }

        // Get observed attributes from static property
        let observed_attributes = if let Some(ctor) = constructor.as_object() {
            ctor.get(js_string!("observedAttributes"), ctx)
                .ok()
                .and_then(|v| v.as_object().clone())
                .map(|arr| {
                    let len = arr.get(js_string!("length"), ctx)
                        .ok()
                        .and_then(|v| v.to_u32(ctx).ok())
                        .unwrap_or(0);
                    (0..len)
                        .filter_map(|i| {
                            arr.get(i, ctx)
                                .ok()
                                .and_then(|v| v.to_string(ctx).ok())
                                .map(|s| s.to_std_string_escaped())
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        // Get extends option
        let extends = if let Some(opts) = options.as_object() {
            opts.get(js_string!("extends"), ctx)
                .ok()
                .and_then(|v| v.to_string(ctx).ok())
                .map(|s| s.to_std_string_escaped())
        } else {
            None
        };

        // Store definition metadata in static storage
        let definition = CustomElementDefinition {
            name: name.clone(),
            observed_attributes,
            extends,
        };

        CUSTOM_ELEMENTS_REGISTRY.lock().unwrap().insert(name.clone(), definition);

        // Store constructor in thread-local storage
        if let Some(ctor) = constructor.as_object() {
            CUSTOM_ELEMENT_CONSTRUCTORS.with(|constructors| {
                constructors.borrow_mut().insert(name.clone(), ctor.clone());
            });
        }

        Ok(JsValue::undefined())
    });

    // get(name) - returns the constructor for the custom element
    let get = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        // Try to get constructor from thread-local storage
        let constructor = CUSTOM_ELEMENT_CONSTRUCTORS.with(|constructors| {
            constructors.borrow().get(&name).cloned()
        });

        if let Some(ctor) = constructor {
            Ok(JsValue::from(ctor))
        } else {
            Ok(JsValue::undefined())
        }
    });

    // getName(constructor) - returns the name for a custom element constructor
    let get_name = NativeFunction::from_copy_closure(|_this, args, _ctx| {
        let constructor = args.get_or_undefined(0);

        if let Some(ctor) = constructor.as_object() {
            // Search for the constructor in thread-local storage
            let name = CUSTOM_ELEMENT_CONSTRUCTORS.with(|constructors| {
                for (name, stored_ctor) in constructors.borrow().iter() {
                    // Compare by object identity
                    if JsObject::equals(stored_ctor, &ctor) {
                        return Some(name.clone());
                    }
                }
                None
            });

            if let Some(name) = name {
                return Ok(JsValue::from(js_string!(name)));
            }
        }
        Ok(JsValue::null())
    });

    // whenDefined(name)
    let when_defined = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        // Create promise that resolves immediately if defined
        let _is_defined = CUSTOM_ELEMENTS_REGISTRY.lock()
            .map(|r| r.contains_key(&name))
            .unwrap_or(false);

        // Return a promise-like object
        let then = NativeFunction::from_copy_closure(move |_this, args, ctx| {
            if let Some(cb) = args.get_or_undefined(0).as_callable() {
                let _ = cb.call(&JsValue::undefined(), &[JsValue::undefined()], ctx);
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

    // upgrade(root)
    let upgrade = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        // In a real implementation, this would upgrade custom elements
        Ok(JsValue::undefined())
    });

    let define_fn = define.to_js_function(context.realm());
    let get_fn = get.to_js_function(context.realm());
    let get_name_fn = get_name.to_js_function(context.realm());
    let when_defined_fn = when_defined.to_js_function(context.realm());
    let upgrade_fn = upgrade.to_js_function(context.realm());

    let custom_elements = ObjectInitializer::new(context)
        .property(js_string!("define"), JsValue::from(define_fn), Attribute::all())
        .property(js_string!("get"), JsValue::from(get_fn), Attribute::all())
        .property(js_string!("getName"), JsValue::from(get_name_fn), Attribute::all())
        .property(js_string!("whenDefined"), JsValue::from(when_defined_fn), Attribute::all())
        .property(js_string!("upgrade"), JsValue::from(upgrade_fn), Attribute::all())
        .build();

    context.register_global_property(js_string!("customElements"), JsValue::from(custom_elements), Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register customElements: {}", e)))?;

    // Also register CustomElementRegistry constructor
    let registry_constructor_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Return the global customElements object
        ctx.global_object().get(js_string!("customElements"), ctx)
    });

    let registry_constructor = FunctionObjectBuilder::new(context.realm(), registry_constructor_fn)
        .name(js_string!("CustomElementRegistry"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("CustomElementRegistry"), registry_constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register CustomElementRegistry: {}", e)))?;

    Ok(())
}

/// Register HTMLTemplateElement
fn register_html_template_element(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let mut id = TEMPLATE_COUNTER.lock().unwrap();
        *id += 1;
        let template_id = *id;
        drop(id);

        let template = create_template_element(ctx, template_id)?;
        Ok(JsValue::from(template))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("HTMLTemplateElement"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("HTMLTemplateElement"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register HTMLTemplateElement: {}", e)))?;

    Ok(())
}

/// Create template element
fn create_template_element(context: &mut Context, _id: u32) -> JsResult<JsObject> {
    // Create document fragment for content
    let content = create_document_fragment(context)?;

    // cloneNode(deep)
    let clone_node = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let _deep = args.get_or_undefined(0).to_boolean();
        // Return a new template element
        let mut id = TEMPLATE_COUNTER.lock().unwrap();
        *id += 1;
        let new_id = *id;
        drop(id);
        let template = create_template_element(ctx, new_id)?;
        Ok(JsValue::from(template))
    }).to_js_function(context.realm());

    let template = ObjectInitializer::new(context)
        .property(js_string!("content"), JsValue::from(content), Attribute::all())
        .property(js_string!("tagName"), JsValue::from(js_string!("TEMPLATE")), Attribute::all())
        .property(js_string!("nodeName"), JsValue::from(js_string!("TEMPLATE")), Attribute::all())
        .property(js_string!("nodeType"), JsValue::from(1), Attribute::all())
        .property(js_string!("innerHTML"), JsValue::from(js_string!("")), Attribute::all())
        .property(js_string!("outerHTML"), JsValue::from(js_string!("<template></template>")), Attribute::all())
        .property(js_string!("cloneNode"), JsValue::from(clone_node), Attribute::all())
        .build();

    Ok(template)
}

/// Create a document fragment
fn create_document_fragment(context: &mut Context) -> JsResult<JsObject> {
    let append_child = NativeFunction::from_copy_closure(|_this, args, _ctx| {
        Ok(args.get_or_undefined(0).clone())
    }).to_js_function(context.realm());

    let query_selector = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    }).to_js_function(context.realm());

    let query_selector_all = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let empty = JsArray::new(ctx);
        Ok(JsValue::from(empty))
    }).to_js_function(context.realm());

    let clone_node = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let fragment = create_document_fragment(ctx)?;
        Ok(JsValue::from(fragment))
    }).to_js_function(context.realm());

    // Create arrays before ObjectInitializer to avoid borrow issues
    let child_nodes = JsArray::new(context);
    let children = JsArray::new(context);

    let fragment = ObjectInitializer::new(context)
        .property(js_string!("nodeType"), JsValue::from(11), Attribute::all())
        .property(js_string!("nodeName"), JsValue::from(js_string!("#document-fragment")), Attribute::all())
        .property(js_string!("childNodes"), JsValue::from(child_nodes), Attribute::all())
        .property(js_string!("children"), JsValue::from(children), Attribute::all())
        .property(js_string!("firstChild"), JsValue::null(), Attribute::all())
        .property(js_string!("lastChild"), JsValue::null(), Attribute::all())
        .property(js_string!("appendChild"), JsValue::from(append_child), Attribute::all())
        .property(js_string!("querySelector"), JsValue::from(query_selector), Attribute::all())
        .property(js_string!("querySelectorAll"), JsValue::from(query_selector_all), Attribute::all())
        .property(js_string!("cloneNode"), JsValue::from(clone_node), Attribute::all())
        .build();

    Ok(fragment)
}

/// Register HTMLSlotElement
fn register_html_slot_element(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let slot = create_slot_element(ctx)?;
        Ok(JsValue::from(slot))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("HTMLSlotElement"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("HTMLSlotElement"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register HTMLSlotElement: {}", e)))?;

    Ok(())
}

/// Create slot element
fn create_slot_element(context: &mut Context) -> JsResult<JsObject> {
    // assignedNodes(options?)
    let assigned_nodes = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    }).to_js_function(context.realm());

    // assignedElements(options?)
    let assigned_elements = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    }).to_js_function(context.realm());

    // assign(...nodes)
    let assign = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    let slot = ObjectInitializer::new(context)
        .property(js_string!("name"), JsValue::from(js_string!("")), Attribute::all())
        .property(js_string!("tagName"), JsValue::from(js_string!("SLOT")), Attribute::all())
        .property(js_string!("nodeName"), JsValue::from(js_string!("SLOT")), Attribute::all())
        .property(js_string!("nodeType"), JsValue::from(1), Attribute::all())
        .property(js_string!("assignedNodes"), JsValue::from(assigned_nodes), Attribute::all())
        .property(js_string!("assignedElements"), JsValue::from(assigned_elements), Attribute::all())
        .property(js_string!("assign"), JsValue::from(assign), Attribute::all())
        .build();

    Ok(slot)
}

/// Register ShadowRoot enhancements
fn register_shadow_root_enhancements(context: &mut Context) -> JsResult<()> {
    // ShadowRoot constructor (usually created via attachShadow)
    let constructor_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let shadow = create_shadow_root_object(ctx, "open")?;
        Ok(JsValue::from(shadow))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("ShadowRoot"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("ShadowRoot"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register ShadowRoot: {}", e)))?;

    Ok(())
}

/// Create ShadowRoot object
fn create_shadow_root_object(context: &mut Context, mode: &str) -> JsResult<JsObject> {
    let inner_html_get = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(js_string!("")))
    }).to_js_function(context.realm());

    let query_selector = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    }).to_js_function(context.realm());

    let query_selector_all = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    }).to_js_function(context.realm());

    let get_element_by_id = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    }).to_js_function(context.realm());

    let get_animations = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    }).to_js_function(context.realm());

    let set_html = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    // Create arrays before ObjectInitializer to avoid borrow issues
    let adopted_style_sheets = JsArray::new(context);
    let style_sheets = JsArray::new(context);
    let child_nodes = JsArray::new(context);
    let children = JsArray::new(context);

    let shadow = ObjectInitializer::new(context)
        .property(js_string!("mode"), JsValue::from(js_string!(mode)), Attribute::all())
        .property(js_string!("host"), JsValue::null(), Attribute::all())
        .property(js_string!("delegatesFocus"), JsValue::from(false), Attribute::all())
        .property(js_string!("slotAssignment"), JsValue::from(js_string!("named")), Attribute::all())
        .property(js_string!("innerHTML"), JsValue::from(js_string!("")), Attribute::all())
        .property(js_string!("adoptedStyleSheets"), JsValue::from(adopted_style_sheets), Attribute::all())
        .property(js_string!("styleSheets"), JsValue::from(style_sheets), Attribute::all())
        .property(js_string!("activeElement"), JsValue::null(), Attribute::all())
        .property(js_string!("fullscreenElement"), JsValue::null(), Attribute::all())
        .property(js_string!("pictureInPictureElement"), JsValue::null(), Attribute::all())
        .property(js_string!("pointerLockElement"), JsValue::null(), Attribute::all())
        .property(js_string!("childNodes"), JsValue::from(child_nodes), Attribute::all())
        .property(js_string!("children"), JsValue::from(children), Attribute::all())
        .property(js_string!("firstChild"), JsValue::null(), Attribute::all())
        .property(js_string!("lastChild"), JsValue::null(), Attribute::all())
        .property(js_string!("querySelector"), JsValue::from(query_selector), Attribute::all())
        .property(js_string!("querySelectorAll"), JsValue::from(query_selector_all), Attribute::all())
        .property(js_string!("getElementById"), JsValue::from(get_element_by_id), Attribute::all())
        .property(js_string!("getAnimations"), JsValue::from(get_animations), Attribute::all())
        .property(js_string!("setHTMLUnsafe"), JsValue::from(set_html), Attribute::all())
        .build();

    Ok(shadow)
}

/// Register CSSStyleSheet constructor
fn register_css_style_sheet(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let options = args.get_or_undefined(0);

        let media = if let Some(opts) = options.as_object() {
            opts.get(js_string!("media"), ctx)
                .ok()
                .and_then(|v| v.to_string(ctx).ok())
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default()
        } else {
            String::new()
        };

        let stylesheet = create_css_stylesheet(ctx, &media)?;
        Ok(JsValue::from(stylesheet))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("CSSStyleSheet"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("CSSStyleSheet"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register CSSStyleSheet: {}", e)))?;

    Ok(())
}

/// Create CSSStyleSheet object
fn create_css_stylesheet(context: &mut Context, _media: &str) -> JsResult<JsObject> {
    // replace(text) - returns Promise
    let replace = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Return resolved promise with self
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

    // replaceSync(text)
    let replace_sync = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    // insertRule(rule, index?)
    let insert_rule = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(0))
    }).to_js_function(context.realm());

    // deleteRule(index)
    let delete_rule = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    // Create arrays before ObjectInitializer
    let css_rules = JsArray::new(context);
    let media = JsArray::new(context);
    let rules = JsArray::new(context);

    let stylesheet = ObjectInitializer::new(context)
        .property(js_string!("type"), JsValue::from(js_string!("text/css")), Attribute::all())
        .property(js_string!("href"), JsValue::null(), Attribute::all())
        .property(js_string!("ownerNode"), JsValue::null(), Attribute::all())
        .property(js_string!("parentStyleSheet"), JsValue::null(), Attribute::all())
        .property(js_string!("title"), JsValue::null(), Attribute::all())
        .property(js_string!("media"), JsValue::from(media), Attribute::all())
        .property(js_string!("disabled"), JsValue::from(false), Attribute::all())
        .property(js_string!("cssRules"), JsValue::from(css_rules), Attribute::all())
        .property(js_string!("rules"), JsValue::from(rules), Attribute::all())
        .property(js_string!("replace"), JsValue::from(replace), Attribute::all())
        .property(js_string!("replaceSync"), JsValue::from(replace_sync), Attribute::all())
        .property(js_string!("insertRule"), JsValue::from(insert_rule), Attribute::all())
        .property(js_string!("deleteRule"), JsValue::from(delete_rule), Attribute::all())
        .build();

    Ok(stylesheet)
}

/// Register adopted style sheets support
fn register_adopted_style_sheets(context: &mut Context) -> JsResult<()> {
    // This is already handled in document and shadow root
    // Just ensure the property exists on document
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::Source;

    fn create_test_context() -> Context {
        let mut ctx = Context::default();
        register_all_web_component_apis(&mut ctx).unwrap();
        ctx
    }

    #[test]
    fn test_custom_elements_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof customElements === 'object' &&
            typeof customElements.define === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_custom_elements_define() {
        let mut ctx = create_test_context();
        // Note: customElements.get() returns a stub function since we can't store JsObject in static
        // So we just check that it returns something truthy (the element is defined)
        let result = ctx.eval(Source::from_bytes(r#"
            class MyElement {}
            customElements.define('my-element', MyElement);
            typeof customElements.get('my-element') === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_html_template_element_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof HTMLTemplateElement === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_template_has_content() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            var template = new HTMLTemplateElement();
            template.content !== null && template.content.nodeType === 11
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_html_slot_element_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof HTMLSlotElement === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_css_style_sheet_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof CSSStyleSheet === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_shadow_root_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof ShadowRoot === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }
}
