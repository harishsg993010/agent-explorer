//! JS crate - Boa JavaScript runtime with Web APIs for the semantic browser.
//!
//! Provides a JavaScript execution environment with DOM API implementations
//! that interact with the Rust-owned DOM through a façade pattern.

mod cookies;
mod crypto;
mod document;
mod dom_bindings;
mod dom_traversal;
mod element;
mod encoding;
mod events;
mod event_system;
mod fetch;
mod jquery;
mod modern;
mod module_loader;
mod network;
mod observers;
mod script_loader;
mod storage;
mod timers;
mod workers;
mod service_workers;
mod streams;
mod intl;
mod canvas;
mod forms;
mod drag_drop;
mod touch_pointer;
mod animation;
mod web_components;
mod visibility_fullscreen;
mod dom_core;
mod html_elements;
mod cssom;
mod xhr;
mod credentials;

// Re-export cookie functions for use by net crate
pub use cookies::{add_cookies_from_headers, get_cookie_header, set_current_url, clear_cookies, add_cookie, get_document_cookies, extract_domain, extract_path};

// Re-export script loader functions
pub use script_loader::{set_base_url, has_pending_scripts, drain_pending_scripts, queue_script, fetch_script};

// Re-export module loader for ES module support
pub use module_loader::{HttpModuleLoader, clear_module_cache};

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer, property::Attribute,
    object::FunctionObjectBuilder, object::builtins::JsArray, Context, JsArgs, JsNativeError,
    JsObject, JsResult, JsValue, Source, Module,
};
use boa_gc::{Finalize, Trace};
use dom::{Dom, DomSnapshot};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

lazy_static::lazy_static! {
    /// Performance timing origin (when the runtime was initialized)
    static ref PERFORMANCE_ORIGIN: Instant = Instant::now();
}

thread_local! {
    /// Thread-local StyleStore for CSSOM integration
    static STYLE_STORE: RefCell<Option<markdown::StyleStore>> = RefCell::new(None);
}

/// Set the StyleStore for the current thread (called during JsRuntime initialization)
pub fn set_style_store(store: markdown::StyleStore) {
    STYLE_STORE.with(|s| {
        *s.borrow_mut() = Some(store);
    });
}

/// Get a computed style property value from the StyleStore
pub fn get_computed_style_property(element_id: u64, property: &str) -> String {
    STYLE_STORE.with(|s| {
        if let Some(ref store) = *s.borrow() {
            store.get_property_value(element_id, property)
        } else {
            // Return defaults if no store is available
            match property {
                "display" => "block".to_string(),
                "visibility" => "visible".to_string(),
                "position" => "static".to_string(),
                _ => String::new(),
            }
        }
    })
}

/// Set a style property value in the StyleStore (for element.style mutations)
pub fn set_style_property(element_id: u64, property: &str, value: &str) {
    STYLE_STORE.with(|s| {
        if let Some(ref store) = *s.borrow() {
            store.set_property_value(element_id, property, value);
        }
    });
}

/// Check if the StyleStore needs relayout
pub fn style_store_needs_relayout() -> bool {
    STYLE_STORE.with(|s| {
        s.borrow().as_ref().map(|store| store.needs_relayout()).unwrap_or(false)
    })
}

use thiserror::Error;

pub use dom_bindings::DomWrapper;

/// Helper to register a constructor with a proper prototype chain for instanceof support.
/// This creates Constructor.prototype and sets prototype.constructor = Constructor.
pub fn register_constructor_with_prototype(
    context: &mut Context,
    name: &str,
    native_fn: NativeFunction,
) -> JsResult<()> {
    let constructor = FunctionObjectBuilder::new(context.realm(), native_fn)
        .name(js_string!(name))
        .length(1)
        .constructor(true)
        .build();

    // Create a prototype object for instanceof checks
    let prototype = ObjectInitializer::new(context)
        .property(js_string!("constructor"), constructor.clone(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
        .build();

    // Set Constructor.prototype = prototype (critical for instanceof)
    constructor.set(js_string!("prototype"), prototype, false, context)?;

    // Register globally
    context.register_global_property(js_string!(name), constructor, Attribute::all())?;

    Ok(())
}

/// Errors that can occur during JavaScript execution
#[derive(Error, Debug)]
pub enum JsError {
    #[error("JavaScript execution error: {0}")]
    ExecutionError(String),

    #[error("Runtime initialization error: {0}")]
    InitError(String),
}

/// Result type for JS operations
pub type Result<T> = std::result::Result<T, JsError>;

/// Wrapper for console output that can be stored in JS context
#[derive(Clone, Trace, Finalize)]
struct ConsoleOutput {
    #[unsafe_ignore_trace]
    inner: Rc<RefCell<Vec<String>>>,
}

/// JavaScript runtime bound to a live DOM
pub struct JsRuntime {
    context: Context,
    dom: Rc<Dom>,
    dom_wrapper: DomWrapper,
    console_output: Rc<RefCell<Vec<String>>>,
    base_url: String,
    style_store: Option<markdown::StyleStore>,
}

impl JsRuntime {
    /// Create a new JavaScript runtime bound to the given DOM
    pub fn new(dom: Rc<Dom>) -> Result<Self> {
        Self::new_with_url_and_styles(dom, "", None)
    }

    /// Create a new JavaScript runtime bound to the given DOM with URL for cookie scoping
    pub fn new_with_url(dom: Rc<Dom>, url: &str) -> Result<Self> {
        Self::new_with_url_and_styles(dom, url, None)
    }

    /// Create a new JavaScript runtime with StyleStore for CSSOM integration
    pub fn new_with_styles(dom: Rc<Dom>, style_store: markdown::StyleStore) -> Result<Self> {
        Self::new_with_url_and_styles(dom, "", Some(style_store))
    }

    /// Create a new JavaScript runtime with full configuration
    pub fn new_with_url_and_styles(dom: Rc<Dom>, url: &str, style_store: Option<markdown::StyleStore>) -> Result<Self> {
        // Create the module loader for ES module support
        let module_loader = Rc::new(module_loader::HttpModuleLoader::new(url));

        // Build context with module loader
        let mut context = Context::builder()
            .module_loader(module_loader)
            .build()
            .map_err(|e| JsError::InitError(format!("Failed to build context: {}", e)))?;

        let console_output = Rc::new(RefCell::new(Vec::new()));

        // Set runtime limits to prevent infinite loops and runaway scripts
        context.runtime_limits_mut().set_loop_iteration_limit(10_000_000);
        context.runtime_limits_mut().set_recursion_limit(1000);

        // Set base URL for dynamic script loading
        script_loader::set_base_url(url);

        // Set up StyleStore for CSSOM integration if provided
        if let Some(ref store) = style_store {
            set_style_store(store.clone());
        }

        // Initialize all Web APIs
        Self::init_console(&mut context, Rc::clone(&console_output))?;
        Self::init_window(&mut context)?;
        dom_bindings::init_document(&mut context, Rc::clone(&dom))?;

        // Extend document with full API (including cookie scoping via URL)
        let dom_wrapper = dom_bindings::DomWrapper {
            inner: Rc::clone(&dom),
            registry: dom_bindings::ElementRegistry::new(),
        };
        if let Ok(doc_val) = context.global_object().get(js_string!("document"), &mut context) {
            if let Some(doc_obj) = doc_val.as_object() {
                let _ = document::extend_document(&doc_obj, &mut context, &dom_wrapper, url);
            }
        }

        Self::init_btoa_atob(&mut context)?;
        timers::register_timer_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register timer APIs: {}", e)))?;
        timers::register_abort_controller(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register AbortController: {}", e)))?;
        fetch::register_fetch_api(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register fetch API: {}", e)))?;
        Self::init_location(&mut context)?;
        Self::init_navigator(&mut context)?;
        Self::init_performance(&mut context)?;
        Self::init_history(&mut context)?;
        storage::register_all_storage_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register storage APIs: {}", e)))?;
        network::register_all_network_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register network APIs: {}", e)))?;
        network::register_close_event(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register CloseEvent: {}", e)))?;
        network::register_message_event(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register MessageEvent: {}", e)))?;
        network::register_error_event(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register ErrorEvent: {}", e)))?;
        modern::register_all_modern_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register modern APIs: {}", e)))?;
        encoding::register_all_encoding_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register encoding APIs: {}", e)))?;
        crypto::register_all_crypto_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register crypto APIs: {}", e)))?;
        workers::register_all_worker_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register worker APIs: {}", e)))?;
        service_workers::register_all_service_worker_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register service worker APIs: {}", e)))?;
        streams::register_all_streams_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register streams APIs: {}", e)))?;
        intl::register_all_intl_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register Intl APIs: {}", e)))?;
        canvas::register_all_canvas_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register Canvas APIs: {}", e)))?;
        forms::register_all_form_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register Form APIs: {}", e)))?;
        drag_drop::register_all_drag_drop_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register Drag/Drop APIs: {}", e)))?;
        touch_pointer::register_all_touch_pointer_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register Touch/Pointer APIs: {}", e)))?;
        animation::register_all_animation_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register Animation APIs: {}", e)))?;
        web_components::register_all_web_component_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register Web Component APIs: {}", e)))?;
        visibility_fullscreen::register_all_visibility_fullscreen_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register Visibility/Fullscreen APIs: {}", e)))?;
        Self::init_screen(&mut context)?;
        events::register_event_constructors(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register events: {}", e)))?;
        element::register_element_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register element APIs: {}", e)))?;
        Self::init_observers(&mut context)?;
        dom_traversal::register_all_dom_traversal_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register DOM traversal APIs: {}", e)))?;
        dom_core::register_all_dom_core_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register DOM Core APIs: {}", e)))?;
        cssom::register_cssom_apis(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register CSSOM APIs: {}", e)))?;
        Self::init_misc_apis(&mut context)?;
        Self::init_error_polyfills(&mut context)?;

        // Register jQuery (must be after DOM and document APIs are set up)
        jquery::register_jquery(&mut context)
            .map_err(|e| JsError::InitError(format!("Failed to register jQuery: {}", e)))?;

        // Add NodeList iterator polyfill (works around Boa panic when adding Symbol.iterator from Rust)
        Self::add_nodelist_iterator_polyfill(&mut context)?;

        Ok(JsRuntime {
            context,
            dom: Rc::clone(&dom),
            dom_wrapper,
            console_output,
            base_url: url.to_string(),
            style_store,
        })
    }

    /// Get reference to the style store (for external relayout triggers)
    pub fn style_store(&self) -> Option<&markdown::StyleStore> {
        self.style_store.as_ref()
    }

    /// Check if styles have been modified and relayout is needed
    pub fn needs_relayout(&self) -> bool {
        self.style_store.as_ref().map(|s| s.needs_relayout()).unwrap_or(false)
    }

    /// Execute a JavaScript script
    pub fn execute(&mut self, script: &str) -> Result<()> {
        let source = Source::from_bytes(script);

        self.context
            .eval(source)
            .map_err(|e| JsError::ExecutionError(format!("{}", e)))?;

        // Run event loop tick to process microtasks (Promise callbacks, etc.)
        self.run_event_loop_tick();

        // Run garbage collection after each script to free memory
        boa_gc::force_collect();

        Ok(())
    }

    /// Evaluate a JavaScript expression and return the result as a string
    pub fn evaluate_to_string(&mut self, expression: &str) -> Result<String> {
        let source = Source::from_bytes(expression);

        let result = self.context
            .eval(source)
            .map_err(|e| JsError::ExecutionError(format!("{}", e)))?;

        // Run event loop tick to process microtasks (Promise callbacks, etc.)
        self.run_event_loop_tick();

        // Convert JsValue to string
        let string_result = result.to_string(&mut self.context)
            .map_err(|e| JsError::ExecutionError(format!("{}", e)))?;

        Ok(string_result.to_std_string_escaped())
    }

    /// Execute a JavaScript script and run the event loop
    pub fn execute_with_event_loop(&mut self, script: &str) -> Result<()> {
        self.execute(script)?;
        self.run_event_loop_tick();
        Ok(())
    }

    /// Maximum script size for execution (1MB) - prevents OOM in Boa parser
    /// Reduced from 2MB because Boa's parser has pathological cases with large minified scripts
    const MAX_EXECUTE_SIZE: usize = 5 * 1024 * 1024;

    /// Check if a script might trigger Boa's parser OOM bug
    /// DISABLED: Was blocking critical polyfills needed for React/Next.js
    fn is_problematic_script(_script: &str) -> bool {
        false // Allow all scripts to run
    }

    /// Execute a script, catching errors without propagating
    /// Calls window.onerror if defined when an error occurs
    pub fn execute_safe(&mut self, script: &str) -> Option<String> {
        self.execute_safe_with_src(script, None)
    }

    /// Execute a script with optional src, catching errors without propagating
    /// Sets document.currentScript during execution
    pub fn execute_safe_with_src(&mut self, script: &str, src: Option<&str>) -> Option<String> {
        // Skip scripts that are too large - they cause OOM in the parser
        if script.len() > Self::MAX_EXECUTE_SIZE {
            return Some(format!(
                "Script too large: {} bytes (max {} bytes)",
                script.len(),
                Self::MAX_EXECUTE_SIZE
            ));
        }

        // Skip scripts that might trigger Boa's parser OOM bug
        if Self::is_problematic_script(script) {
            return Some("Script skipped: matches pattern known to cause parser OOM".to_string());
        }

        // Set document.currentScript before execution
        self.set_current_script(src);

        let result = match self.execute(script) {
            Ok(()) => None,
            Err(e) => {
                let error_msg = e.to_string();

                // Try to invoke window.onerror if defined
                self.invoke_onerror(&error_msg);

                Some(error_msg)
            }
        };

        // Clear document.currentScript after execution
        self.clear_current_script();

        result
    }

    /// Invoke window.onerror handler if defined
    fn invoke_onerror(&mut self, message: &str) {
        let global = self.context.global_object();

        // Get onerror property from global/window
        if let Ok(onerror_val) = global.get(js_string!("onerror"), &mut self.context) {
            if let Some(onerror_fn) = onerror_val.as_callable() {
                // Create error event arguments: (message, source, lineno, colno, error)
                let args = [
                    JsValue::from(js_string!(message)),           // message
                    JsValue::from(js_string!("script")),          // source (filename)
                    JsValue::from(0),                              // lineno
                    JsValue::from(0),                              // colno
                    JsValue::undefined(),                          // error object
                ];

                // Call the handler, ignoring any errors from the handler itself
                let _ = onerror_fn.call(&JsValue::undefined(), &args, &mut self.context);
            }
        }
    }

    /// Set document.currentScript to a script element with the given src
    /// This is called before executing each script - uses the DOM to find or create the actual script element
    pub fn set_current_script(&mut self, src: Option<&str>) {
        // Try to find the script element in the DOM
        let script_element = if let Some(src_url) = src {
            // Look for script with matching src attribute
            self.dom.query_selector(&format!("script[src=\"{}\"]", src_url))
                .or_else(|| {
                    // Try with just the path portion for relative URLs
                    if let Some(path) = src_url.rsplit('/').next() {
                        self.dom.query_selector(&format!("script[src*=\"{}\"]", path))
                    } else {
                        None
                    }
                })
        } else {
            // For inline scripts, we can't easily identify which one, so create a synthetic element
            None
        };

        let global = self.context.global_object();
        if let Ok(doc_val) = global.get(js_string!("document"), &mut self.context) {
            if let Some(doc) = doc_val.as_object() {
                let script_obj = if let Some(el) = script_element {
                    // Use the actual DOM element through create_element_object
                    dom_bindings::create_element_object(el, &mut self.context, &self.dom_wrapper)
                } else {
                    // Create a synthetic script element for inline scripts or when not found
                    Self::create_synthetic_script_element(&mut self.context, src)
                };
                let _ = doc.set(js_string!("currentScript"), script_obj, false, &mut self.context);
            }
        }
    }

    /// Create a synthetic HTMLScriptElement object when the actual DOM element is not available
    fn create_synthetic_script_element(context: &mut Context, src: Option<&str>) -> JsValue {
        use std::cell::RefCell;
        use std::collections::HashMap;

        // Use RefCell for interior mutability in closures
        let src_value = src.map(|s| s.to_string()).unwrap_or_default();
        let attrs = Rc::new(RefCell::new({
            let mut m = HashMap::new();
            if !src_value.is_empty() {
                m.insert("src".to_string(), src_value.clone());
            }
            m.insert("type".to_string(), "text/javascript".to_string());
            m
        }));
        let listeners: Rc<RefCell<HashMap<String, Vec<JsObject>>>> = Rc::new(RefCell::new(HashMap::new()));

        // getAttribute(name) -> string | null
        // For "src" attribute, return "" instead of null for Turbopack compatibility
        // (Turbopack calls .replace() on the result without null checking)
        let attrs_clone = Rc::clone(&attrs);
        let get_attribute = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let attr_name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase();
                let attrs = attrs_clone.borrow();
                match attrs.get(&attr_name) {
                    Some(val) => Ok(JsValue::from(js_string!(val.clone()))),
                    None if attr_name == "src" => Ok(JsValue::from(js_string!(""))),
                    None => Ok(JsValue::null()),
                }
            })
        };

        // setAttribute(name, value)
        let attrs_clone = Rc::clone(&attrs);
        let set_attribute = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let attr_name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase();
                let attr_value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
                let mut attrs = attrs_clone.borrow_mut();
                attrs.insert(attr_name, attr_value);
                Ok(JsValue::undefined())
            })
        };

        // hasAttribute(name) -> boolean
        let attrs_clone = Rc::clone(&attrs);
        let has_attribute = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let attr_name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase();
                let attrs = attrs_clone.borrow();
                Ok(JsValue::from(attrs.contains_key(&attr_name)))
            })
        };

        // removeAttribute(name)
        let attrs_clone = Rc::clone(&attrs);
        let remove_attribute = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let attr_name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase();
                let mut attrs = attrs_clone.borrow_mut();
                attrs.remove(&attr_name);
                Ok(JsValue::undefined())
            })
        };

        // getAttributeNode(name) -> Attr | null
        let attrs_clone = Rc::clone(&attrs);
        let get_attribute_node = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let attr_name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase();
                let attrs = attrs_clone.borrow();
                match attrs.get(&attr_name) {
                    Some(val) => {
                        let attr_obj = ObjectInitializer::new(ctx)
                            .property(js_string!("name"), js_string!(attr_name.clone()), Attribute::READONLY)
                            .property(js_string!("value"), js_string!(val.clone()), Attribute::all())
                            .property(js_string!("specified"), true, Attribute::READONLY)
                            .property(js_string!("nodeType"), 2, Attribute::READONLY)
                            .build();
                        Ok(JsValue::from(attr_obj))
                    }
                    None => Ok(JsValue::null()),
                }
            })
        };

        // addEventListener(type, listener, options)
        let listeners_clone = Rc::clone(&listeners);
        let add_event_listener = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                if let Some(listener) = args.get_or_undefined(1).as_object() {
                    let mut listeners = listeners_clone.borrow_mut();
                    listeners.entry(event_type).or_insert_with(Vec::new).push(listener.clone());
                }
                Ok(JsValue::undefined())
            })
        };

        // removeEventListener(type, listener, options)
        let listeners_clone = Rc::clone(&listeners);
        let remove_event_listener = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                if args.get_or_undefined(1).as_object().is_some() {
                    let mut listeners = listeners_clone.borrow_mut();
                    listeners.remove(&event_type);
                }
                Ok(JsValue::undefined())
            })
        };

        // Simple stateless methods using from_copy_closure
        let dispatch_event = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::from(true)));
        let clone_node = NativeFunction::from_copy_closure(|this, _args, _ctx| Ok(this.clone()));
        let contains = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::from(false)));
        let has_child_nodes = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::from(false)));
        let compare_document_position = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::from(0)));
        let get_root_node = NativeFunction::from_copy_closure(|this, _args, _ctx| Ok(this.clone()));
        let is_equal_node = NativeFunction::from_copy_closure(|this, args, _ctx| {
            Ok(JsValue::from(this == args.get_or_undefined(0)))
        });
        let is_same_node = NativeFunction::from_copy_closure(|this, args, _ctx| {
            Ok(JsValue::from(this == args.get_or_undefined(0)))
        });
        let normalize = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let matches_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let selector = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let matches = selector == "script" || selector.starts_with("script");
            Ok(JsValue::from(matches))
        });
        let closest = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::null()));
        let get_bounding_client_rect = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let rect = ObjectInitializer::new(ctx)
                .property(js_string!("x"), 0, Attribute::READONLY)
                .property(js_string!("y"), 0, Attribute::READONLY)
                .property(js_string!("width"), 0, Attribute::READONLY)
                .property(js_string!("height"), 0, Attribute::READONLY)
                .property(js_string!("top"), 0, Attribute::READONLY)
                .property(js_string!("right"), 0, Attribute::READONLY)
                .property(js_string!("bottom"), 0, Attribute::READONLY)
                .property(js_string!("left"), 0, Attribute::READONLY)
                .build();
            Ok(JsValue::from(rect))
        });
        let get_client_rects = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            Ok(JsValue::from(JsArray::new(ctx)))
        });
        let focus = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let blur = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let insert_adjacent_element = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::null()));
        let insert_adjacent_html = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let insert_adjacent_text = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let append_child = NativeFunction::from_copy_closure(|_this, args, _ctx| {
            Ok(args.get_or_undefined(0).clone())
        });
        let remove_child = NativeFunction::from_copy_closure(|_this, args, _ctx| {
            Ok(args.get_or_undefined(0).clone())
        });
        let replace_child = NativeFunction::from_copy_closure(|_this, args, _ctx| {
            Ok(args.get_or_undefined(1).clone())
        });
        let insert_before = NativeFunction::from_copy_closure(|_this, args, _ctx| {
            Ok(args.get_or_undefined(0).clone())
        });
        let query_selector = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::null()));
        let query_selector_all = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            Ok(JsValue::from(JsArray::new(ctx)))
        });
        let get_elements_by_tag_name = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            Ok(JsValue::from(JsArray::new(ctx)))
        });
        let get_elements_by_class_name = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            Ok(JsValue::from(JsArray::new(ctx)))
        });

        // Pre-create nested objects to avoid borrow issues
        let child_nodes = JsArray::new(context);
        let class_list = ObjectInitializer::new(context).build();
        let attributes_obj = ObjectInitializer::new(context).build();
        let children_arr = JsArray::new(context);
        let style_obj = ObjectInitializer::new(context).build();
        let dataset_obj = ObjectInitializer::new(context).build();

        // Build the full script element object
        let script = ObjectInitializer::new(context)
            // Node properties
            .property(js_string!("nodeName"), js_string!("SCRIPT"), Attribute::READONLY)
            .property(js_string!("nodeType"), 1, Attribute::READONLY)
            .property(js_string!("nodeValue"), JsValue::null(), Attribute::all())
            .property(js_string!("textContent"), js_string!(""), Attribute::all())
            .property(js_string!("parentNode"), JsValue::null(), Attribute::all())
            .property(js_string!("parentElement"), JsValue::null(), Attribute::all())
            .property(js_string!("firstChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("lastChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("previousSibling"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("nextSibling"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("childNodes"), child_nodes, Attribute::READONLY)
            .property(js_string!("isConnected"), true, Attribute::READONLY)
            .property(js_string!("ownerDocument"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("baseURI"), js_string!(""), Attribute::READONLY)

            // Element properties
            .property(js_string!("tagName"), js_string!("SCRIPT"), Attribute::READONLY)
            .property(js_string!("localName"), js_string!("script"), Attribute::READONLY)
            .property(js_string!("id"), js_string!(""), Attribute::all())
            .property(js_string!("className"), js_string!(""), Attribute::all())
            .property(js_string!("classList"), class_list, Attribute::READONLY)
            .property(js_string!("innerHTML"), js_string!(""), Attribute::all())
            .property(js_string!("outerHTML"), js_string!("<script></script>"), Attribute::all())
            .property(js_string!("namespaceURI"), js_string!("http://www.w3.org/1999/xhtml"), Attribute::READONLY)
            .property(js_string!("prefix"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("slot"), js_string!(""), Attribute::all())
            .property(js_string!("attributes"), attributes_obj, Attribute::READONLY)
            .property(js_string!("children"), children_arr, Attribute::READONLY)
            .property(js_string!("firstElementChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("lastElementChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("previousElementSibling"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("nextElementSibling"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("childElementCount"), 0, Attribute::READONLY)

            // HTMLElement properties
            .property(js_string!("title"), js_string!(""), Attribute::all())
            .property(js_string!("lang"), js_string!(""), Attribute::all())
            .property(js_string!("dir"), js_string!(""), Attribute::all())
            .property(js_string!("hidden"), false, Attribute::all())
            .property(js_string!("tabIndex"), -1, Attribute::all())
            .property(js_string!("draggable"), false, Attribute::all())
            .property(js_string!("spellcheck"), true, Attribute::all())
            .property(js_string!("contentEditable"), js_string!("inherit"), Attribute::all())
            .property(js_string!("isContentEditable"), false, Attribute::READONLY)
            .property(js_string!("offsetParent"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("offsetTop"), 0, Attribute::READONLY)
            .property(js_string!("offsetLeft"), 0, Attribute::READONLY)
            .property(js_string!("offsetWidth"), 0, Attribute::READONLY)
            .property(js_string!("offsetHeight"), 0, Attribute::READONLY)
            .property(js_string!("clientTop"), 0, Attribute::READONLY)
            .property(js_string!("clientLeft"), 0, Attribute::READONLY)
            .property(js_string!("clientWidth"), 0, Attribute::READONLY)
            .property(js_string!("clientHeight"), 0, Attribute::READONLY)
            .property(js_string!("scrollTop"), 0, Attribute::all())
            .property(js_string!("scrollLeft"), 0, Attribute::all())
            .property(js_string!("scrollWidth"), 0, Attribute::READONLY)
            .property(js_string!("scrollHeight"), 0, Attribute::READONLY)
            .property(js_string!("style"), style_obj, Attribute::READONLY)
            .property(js_string!("dataset"), dataset_obj, Attribute::READONLY)

            // HTMLScriptElement specific properties
            .property(js_string!("src"), js_string!(src_value.clone()), Attribute::all())
            .property(js_string!("type"), js_string!("text/javascript"), Attribute::all())
            .property(js_string!("async"), false, Attribute::all())
            .property(js_string!("defer"), false, Attribute::all())
            .property(js_string!("crossOrigin"), JsValue::null(), Attribute::all())
            .property(js_string!("text"), js_string!(""), Attribute::all())
            .property(js_string!("charset"), js_string!(""), Attribute::all())
            .property(js_string!("noModule"), false, Attribute::all())
            .property(js_string!("integrity"), js_string!(""), Attribute::all())
            .property(js_string!("referrerPolicy"), js_string!(""), Attribute::all())
            .property(js_string!("fetchPriority"), js_string!("auto"), Attribute::all())
            .property(js_string!("blocking"), js_string!(""), Attribute::all())

            // Node methods
            .function(clone_node, js_string!("cloneNode"), 1)
            .function(contains, js_string!("contains"), 1)
            .function(has_child_nodes, js_string!("hasChildNodes"), 0)
            .function(compare_document_position, js_string!("compareDocumentPosition"), 1)
            .function(get_root_node, js_string!("getRootNode"), 1)
            .function(is_equal_node, js_string!("isEqualNode"), 1)
            .function(is_same_node, js_string!("isSameNode"), 1)
            .function(normalize, js_string!("normalize"), 0)
            .function(append_child, js_string!("appendChild"), 1)
            .function(remove_child, js_string!("removeChild"), 1)
            .function(replace_child, js_string!("replaceChild"), 2)
            .function(insert_before, js_string!("insertBefore"), 2)

            // Element methods
            .function(get_attribute, js_string!("getAttribute"), 1)
            .function(set_attribute, js_string!("setAttribute"), 2)
            .function(has_attribute, js_string!("hasAttribute"), 1)
            .function(remove_attribute, js_string!("removeAttribute"), 1)
            .function(get_attribute_node, js_string!("getAttributeNode"), 1)
            .function(matches_fn, js_string!("matches"), 1)
            .function(closest, js_string!("closest"), 1)
            .function(get_bounding_client_rect, js_string!("getBoundingClientRect"), 0)
            .function(get_client_rects, js_string!("getClientRects"), 0)
            .function(insert_adjacent_element, js_string!("insertAdjacentElement"), 2)
            .function(insert_adjacent_html, js_string!("insertAdjacentHTML"), 2)
            .function(insert_adjacent_text, js_string!("insertAdjacentText"), 2)
            .function(query_selector, js_string!("querySelector"), 1)
            .function(query_selector_all, js_string!("querySelectorAll"), 1)
            .function(get_elements_by_tag_name, js_string!("getElementsByTagName"), 1)
            .function(get_elements_by_class_name, js_string!("getElementsByClassName"), 1)

            // EventTarget methods
            .function(add_event_listener, js_string!("addEventListener"), 3)
            .function(remove_event_listener, js_string!("removeEventListener"), 3)
            .function(dispatch_event, js_string!("dispatchEvent"), 1)

            // HTMLElement methods
            .function(focus, js_string!("focus"), 0)
            .function(blur, js_string!("blur"), 0)
            .build();

        JsValue::from(script)
    }

    /// Clear document.currentScript (set to null)
    /// This is called after executing each script
    pub fn clear_current_script(&mut self) {
        let global = self.context.global_object();
        if let Ok(doc_val) = global.get(js_string!("document"), &mut self.context) {
            if let Some(doc) = doc_val.as_object() {
                let _ = doc.set(js_string!("currentScript"), JsValue::null(), false, &mut self.context);
            }
        }
    }

    /// Execute a script with currentScript properly set
    pub fn execute_with_script_element(&mut self, script: &str, src: Option<&str>) -> Result<()> {
        self.set_current_script(src);
        let result = self.execute(script);
        self.clear_current_script();
        result
    }

    /// Run one iteration of the event loop (process all pending tasks)
    pub fn run_event_loop_tick(&mut self) {
        timers::run_event_loop_tick(&mut self.context);
    }

    /// Run the event loop until all tasks are complete or max_iterations is reached
    pub fn run_event_loop(&mut self, max_iterations: u32) {
        timers::run_event_loop(&mut self.context, max_iterations);
    }

    /// Check if there are pending tasks in the event loop
    pub fn has_pending_tasks(&self) -> bool {
        timers::has_pending_tasks()
    }

    /// Execute all pending dynamically loaded scripts
    pub fn execute_pending_scripts(&mut self) -> Vec<String> {
        let mut errors = Vec::new();
        let scripts = script_loader::drain_pending_scripts();

        for script in scripts {
            log::debug!("Executing dynamic script from: {} (module: {})", script.url, script.is_module);

            if script.is_module {
                // Execute as ES module
                if let Err(err) = self.execute_module(&script.content) {
                    log::warn!("Dynamic module {} failed: {}", script.url, err);
                    errors.push(format!("{}: {}", script.url, err));
                }
            } else {
                // Execute as regular script
                if let Some(err) = self.execute_safe(&script.content) {
                    log::warn!("Dynamic script {} failed: {}", script.url, err);
                    errors.push(format!("{}: {}", script.url, err));
                }
            }
            // Run event loop after each script
            self.run_event_loop_tick();
        }

        errors
    }

    /// Execute an ES module from source code
    pub fn execute_module(&mut self, source: &str) -> Result<()> {
        // Check size limits
        if source.len() > Self::MAX_EXECUTE_SIZE {
            return Err(JsError::ExecutionError(format!(
                "Module too large: {} bytes (max {} bytes)",
                source.len(),
                Self::MAX_EXECUTE_SIZE
            )));
        }

        if Self::is_problematic_script(source) {
            return Err(JsError::ExecutionError(
                "Module skipped: matches pattern known to cause parser OOM".to_string()
            ));
        }

        let src = Source::from_bytes(source);

        // Parse the module
        let module = Module::parse(src, None, &mut self.context)
            .map_err(|e| JsError::ExecutionError(format!("Module parse error: {}", e)))?;

        // Load and link the module (resolves imports)
        let _ = module.load_link_evaluate(&mut self.context);

        // Run the job queue to execute the module
        let _ = self.context.run_jobs();

        // Run garbage collection
        boa_gc::force_collect();

        Ok(())
    }

    /// Execute an ES module with the event loop
    pub fn execute_module_with_event_loop(&mut self, source: &str) -> Result<()> {
        self.execute_module(source)?;
        self.run_event_loop_tick();
        Ok(())
    }

    /// Execute an ES module safely, returning error as Option
    /// Calls window.onerror if defined when an error occurs
    pub fn execute_module_safe(&mut self, source: &str) -> Option<String> {
        match self.execute_module(source) {
            Ok(()) => None,
            Err(e) => {
                let error_msg = e.to_string();
                self.invoke_onerror(&error_msg);
                Some(error_msg)
            }
        }
    }

    /// Check if there are pending dynamically loaded scripts
    pub fn has_pending_scripts(&self) -> bool {
        script_loader::has_pending_scripts()
    }

    /// Get the DOM
    pub fn dom(&self) -> Rc<Dom> {
        Rc::clone(&self.dom)
    }

    /// Get a snapshot of the current DOM state
    pub fn snapshot(&self) -> Rc<DomSnapshot> {
        self.dom.snapshot()
    }

    /// Get console output collected during script execution
    pub fn console_output(&self) -> Vec<String> {
        self.console_output.borrow().clone()
    }

    /// Fire the DOMContentLoaded event on the document
    /// This should be called after the HTML is parsed but before executing scripts
    pub fn fire_dom_content_loaded(&mut self) {
        let script = r#"
            (function() {
                var event = new Event('DOMContentLoaded', {bubbles: true, cancelable: false});
                document.dispatchEvent(event);
            })();
        "#;
        if let Some(err) = self.execute_safe(script) {
            log::warn!("Failed to fire DOMContentLoaded: {}", err);
        }
        self.run_event_loop_tick();
    }

    /// Fire the load event on the window
    /// This should be called after all resources (scripts, etc.) have been loaded
    pub fn fire_load(&mut self) {
        let script = r#"
            (function() {
                var event = new Event('load', {bubbles: false, cancelable: false});
                window.dispatchEvent(event);
                if (typeof window.onload === 'function') {
                    window.onload(event);
                }
            })();
        "#;
        if let Some(err) = self.execute_safe(script) {
            log::warn!("Failed to fire load event: {}", err);
        }
        self.run_event_loop_tick();
    }

    /// Initialize the console object
    fn init_console(context: &mut Context, output: Rc<RefCell<Vec<String>>>) -> Result<()> {
        let output_wrapper = ConsoleOutput {
            inner: Rc::clone(&output),
        };

        let console_log = {
            let output = output_wrapper.clone();
            unsafe {
                NativeFunction::from_closure(move |_this, args, ctx| {
                    let message = args
                        .iter()
                        .map(|arg| arg.to_string(ctx).map(|s| s.to_std_string_escaped()))
                        .collect::<JsResult<Vec<_>>>()?
                        .join(" ");

                    log::debug!("[console.log] {}", message);
                    output.inner.borrow_mut().push(message);
                    Ok(JsValue::undefined())
                })
            }
        };

        let console_warn = {
            let output = output_wrapper.clone();
            unsafe {
                NativeFunction::from_closure(move |_this, args, ctx| {
                    let message = args
                        .iter()
                        .map(|arg| arg.to_string(ctx).map(|s| s.to_std_string_escaped()))
                        .collect::<JsResult<Vec<_>>>()?
                        .join(" ");

                    log::warn!("[console.warn] {}", message);
                    output
                        .inner
                        .borrow_mut()
                        .push(format!("[warn] {}", message));
                    Ok(JsValue::undefined())
                })
            }
        };

        let console_error = {
            let output = output_wrapper.clone();
            unsafe {
                NativeFunction::from_closure(move |_this, args, ctx| {
                    let message = args
                        .iter()
                        .map(|arg| arg.to_string(ctx).map(|s| s.to_std_string_escaped()))
                        .collect::<JsResult<Vec<_>>>()?
                        .join(" ");

                    log::error!("[console.error] {}", message);
                    output
                        .inner
                        .borrow_mut()
                        .push(format!("[error] {}", message));
                    Ok(JsValue::undefined())
                })
            }
        };

        let console_info = {
            let output = output_wrapper.clone();
            unsafe {
                NativeFunction::from_closure(move |_this, args, ctx| {
                    let message = args
                        .iter()
                        .map(|arg| arg.to_string(ctx).map(|s| s.to_std_string_escaped()))
                        .collect::<JsResult<Vec<_>>>()?
                        .join(" ");
                    output
                        .inner
                        .borrow_mut()
                        .push(format!("[info] {}", message));
                    Ok(JsValue::undefined())
                })
            }
        };

        let console_debug = {
            let output = output_wrapper.clone();
            unsafe {
                NativeFunction::from_closure(move |_this, args, ctx| {
                    let message = args
                        .iter()
                        .map(|arg| arg.to_string(ctx).map(|s| s.to_std_string_escaped()))
                        .collect::<JsResult<Vec<_>>>()?
                        .join(" ");
                    output
                        .inner
                        .borrow_mut()
                        .push(format!("[debug] {}", message));
                    Ok(JsValue::undefined())
                })
            }
        };

        let console = ObjectInitializer::new(context)
            .function(console_log, js_string!("log"), 0)
            .function(console_warn, js_string!("warn"), 0)
            .function(console_error, js_string!("error"), 0)
            .function(console_info, js_string!("info"), 0)
            .function(console_debug, js_string!("debug"), 0)
            .build();

        context
            .register_global_property(js_string!("console"), console, Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register console: {}", e)))?;

        Ok(())
    }

    /// Initialize the window object with dimension properties and methods
    fn init_window(context: &mut Context) -> Result<()> {
        let global = context.global_object();

        // Register window aliases
        context
            .register_global_property(js_string!("window"), global.clone(), Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register window: {}", e)))?;
        context
            .register_global_property(js_string!("self"), global.clone(), Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register self: {}", e)))?;
        context
            .register_global_property(js_string!("globalThis"), global.clone(), Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register globalThis: {}", e)))?;

        // Window dimensions (standard desktop values)
        context
            .register_global_property(js_string!("innerWidth"), 1920, Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register innerWidth: {}", e)))?;
        context
            .register_global_property(js_string!("innerHeight"), 1080, Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register innerHeight: {}", e)))?;
        context
            .register_global_property(js_string!("outerWidth"), 1920, Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register outerWidth: {}", e)))?;
        context
            .register_global_property(js_string!("outerHeight"), 1080, Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register outerHeight: {}", e)))?;

        // Scroll position
        context
            .register_global_property(js_string!("scrollX"), 0, Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register scrollX: {}", e)))?;
        context
            .register_global_property(js_string!("scrollY"), 0, Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register scrollY: {}", e)))?;
        context
            .register_global_property(js_string!("pageXOffset"), 0, Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register pageXOffset: {}", e)))?;
        context
            .register_global_property(js_string!("pageYOffset"), 0, Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register pageYOffset: {}", e)))?;

        // Device pixel ratio
        context
            .register_global_property(js_string!("devicePixelRatio"), 1.0, Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register devicePixelRatio: {}", e)))?;

        // trustedTypes API (required by Google and other sites)
        let create_policy = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            // Create a policy object with createHTML, createScript, createScriptURL
            let create_html = NativeFunction::from_copy_closure(|_this, args, _ctx| {
                Ok(args.get_or_undefined(0).clone())
            });
            let create_script = NativeFunction::from_copy_closure(|_this, args, _ctx| {
                Ok(args.get_or_undefined(0).clone())
            });
            let create_script_url = NativeFunction::from_copy_closure(|_this, args, _ctx| {
                Ok(args.get_or_undefined(0).clone())
            });

            let policy = ObjectInitializer::new(ctx)
                .property(js_string!("name"), js_string!(name), Attribute::READONLY)
                .function(create_html, js_string!("createHTML"), 1)
                .function(create_script, js_string!("createScript"), 1)
                .function(create_script_url, js_string!("createScriptURL"), 1)
                .build();

            Ok(JsValue::from(policy))
        });

        let trusted_types = ObjectInitializer::new(context)
            .function(create_policy, js_string!("createPolicy"), 2)
            .build();

        context
            .register_global_property(js_string!("trustedTypes"), trusted_types, Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register trustedTypes: {}", e)))?;

        // Scroll methods
        let scroll = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });
        let scroll_to = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });
        let scroll_by = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        context
            .register_global_builtin_callable(js_string!("scroll"), 2, scroll)
            .map_err(|e| JsError::InitError(format!("Failed to register scroll: {}", e)))?;
        context
            .register_global_builtin_callable(js_string!("scrollTo"), 2, scroll_to)
            .map_err(|e| JsError::InitError(format!("Failed to register scrollTo: {}", e)))?;
        context
            .register_global_builtin_callable(js_string!("scrollBy"), 2, scroll_by)
            .map_err(|e| JsError::InitError(format!("Failed to register scrollBy: {}", e)))?;

        // getComputedStyle - returns CSSStyleDeclaration backed by StyleStore
        let get_computed_style = NativeFunction::from_copy_closure(|_this, args, ctx| {
            // Extract element ID from the element argument
            let element_id = if let Some(element) = args.get(0) {
                if let Some(obj) = element.as_object() {
                    // Try to get __element_id__ from the element
                    obj.get(js_string!("__element_id__"), ctx)
                        .ok()
                        .and_then(|v| v.to_u32(ctx).ok())
                        .map(|id| id as u64)
                        .unwrap_or(0)
                } else {
                    0
                }
            } else {
                0
            };

            // Create getPropertyValue function that queries StyleStore
            let get_property_value = NativeFunction::from_copy_closure(move |_this, args, ctx| {
                let prop = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let value = get_computed_style_property(element_id, &prop);
                Ok(JsValue::from(js_string!(value)))
            });

            // Get common property values from StyleStore
            let display = get_computed_style_property(element_id, "display");
            let visibility = get_computed_style_property(element_id, "visibility");
            let position = get_computed_style_property(element_id, "position");
            let width = get_computed_style_property(element_id, "width");
            let height = get_computed_style_property(element_id, "height");
            let margin_top = get_computed_style_property(element_id, "margin-top");
            let margin_right = get_computed_style_property(element_id, "margin-right");
            let margin_bottom = get_computed_style_property(element_id, "margin-bottom");
            let margin_left = get_computed_style_property(element_id, "margin-left");
            let padding_top = get_computed_style_property(element_id, "padding-top");
            let padding_right = get_computed_style_property(element_id, "padding-right");
            let padding_bottom = get_computed_style_property(element_id, "padding-bottom");
            let padding_left = get_computed_style_property(element_id, "padding-left");

            // Build CSSStyleDeclaration object with computed values
            let style = ObjectInitializer::new(ctx)
                .function(get_property_value, js_string!("getPropertyValue"), 1)
                .property(js_string!("display"), js_string!(display), Attribute::READONLY)
                .property(js_string!("visibility"), js_string!(visibility), Attribute::READONLY)
                .property(js_string!("position"), js_string!(position), Attribute::READONLY)
                .property(js_string!("width"), js_string!(width), Attribute::READONLY)
                .property(js_string!("height"), js_string!(height), Attribute::READONLY)
                .property(js_string!("marginTop"), js_string!(margin_top.clone()), Attribute::READONLY)
                .property(js_string!("marginRight"), js_string!(margin_right.clone()), Attribute::READONLY)
                .property(js_string!("marginBottom"), js_string!(margin_bottom.clone()), Attribute::READONLY)
                .property(js_string!("marginLeft"), js_string!(margin_left.clone()), Attribute::READONLY)
                .property(js_string!("margin-top"), js_string!(margin_top), Attribute::READONLY)
                .property(js_string!("margin-right"), js_string!(margin_right), Attribute::READONLY)
                .property(js_string!("margin-bottom"), js_string!(margin_bottom), Attribute::READONLY)
                .property(js_string!("margin-left"), js_string!(margin_left), Attribute::READONLY)
                .property(js_string!("paddingTop"), js_string!(padding_top.clone()), Attribute::READONLY)
                .property(js_string!("paddingRight"), js_string!(padding_right.clone()), Attribute::READONLY)
                .property(js_string!("paddingBottom"), js_string!(padding_bottom.clone()), Attribute::READONLY)
                .property(js_string!("paddingLeft"), js_string!(padding_left.clone()), Attribute::READONLY)
                .property(js_string!("padding-top"), js_string!(padding_top), Attribute::READONLY)
                .property(js_string!("padding-right"), js_string!(padding_right), Attribute::READONLY)
                .property(js_string!("padding-bottom"), js_string!(padding_bottom), Attribute::READONLY)
                .property(js_string!("padding-left"), js_string!(padding_left), Attribute::READONLY)
                .build();
            Ok(JsValue::from(style))
        });

        context
            .register_global_builtin_callable(js_string!("getComputedStyle"), 1, get_computed_style)
            .map_err(|e| JsError::InitError(format!("Failed to register getComputedStyle: {}", e)))?;

        // matchMedia
        let match_media = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let query = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            // Simple media query matching
            let matches = query.contains("screen") ||
                         query.contains("(min-width: 0") ||
                         query.contains("all");

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

            let media_query_list = ObjectInitializer::new(ctx)
                .property(js_string!("matches"), matches, Attribute::READONLY)
                .property(js_string!("media"), js_string!(query), Attribute::READONLY)
                .function(add_listener, js_string!("addListener"), 1)
                .function(remove_listener, js_string!("removeListener"), 1)
                .function(add_event_listener, js_string!("addEventListener"), 2)
                .function(remove_event_listener, js_string!("removeEventListener"), 2)
                .build();

            Ok(JsValue::from(media_query_list))
        });

        context
            .register_global_builtin_callable(js_string!("matchMedia"), 1, match_media)
            .map_err(|e| JsError::InitError(format!("Failed to register matchMedia: {}", e)))?;

        // Window open/close/print stubs
        let open = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::null())
        });
        let close = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });
        let print = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });
        let focus = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });
        let blur = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });
        let alert = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });
        let confirm = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::from(true))
        });
        let prompt = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::null())
        });

        context
            .register_global_builtin_callable(js_string!("open"), 3, open)
            .map_err(|e| JsError::InitError(format!("Failed to register open: {}", e)))?;
        context
            .register_global_builtin_callable(js_string!("close"), 0, close)
            .map_err(|e| JsError::InitError(format!("Failed to register close: {}", e)))?;
        context
            .register_global_builtin_callable(js_string!("print"), 0, print)
            .map_err(|e| JsError::InitError(format!("Failed to register print: {}", e)))?;
        context
            .register_global_builtin_callable(js_string!("focus"), 0, focus)
            .map_err(|e| JsError::InitError(format!("Failed to register focus: {}", e)))?;
        context
            .register_global_builtin_callable(js_string!("blur"), 0, blur)
            .map_err(|e| JsError::InitError(format!("Failed to register blur: {}", e)))?;
        context
            .register_global_builtin_callable(js_string!("alert"), 1, alert)
            .map_err(|e| JsError::InitError(format!("Failed to register alert: {}", e)))?;
        context
            .register_global_builtin_callable(js_string!("confirm"), 1, confirm)
            .map_err(|e| JsError::InitError(format!("Failed to register confirm: {}", e)))?;
        context
            .register_global_builtin_callable(js_string!("prompt"), 2, prompt)
            .map_err(|e| JsError::InitError(format!("Failed to register prompt: {}", e)))?;

        // Parent/top/frames
        context
            .register_global_property(js_string!("parent"), global.clone(), Attribute::READONLY)
            .map_err(|e| JsError::InitError(format!("Failed to register parent: {}", e)))?;
        context
            .register_global_property(js_string!("top"), global.clone(), Attribute::READONLY)
            .map_err(|e| JsError::InitError(format!("Failed to register top: {}", e)))?;
        context
            .register_global_property(js_string!("frames"), global, Attribute::READONLY)
            .map_err(|e| JsError::InitError(format!("Failed to register frames: {}", e)))?;
        context
            .register_global_property(js_string!("frameElement"), JsValue::null(), Attribute::READONLY)
            .map_err(|e| JsError::InitError(format!("Failed to register frameElement: {}", e)))?;
        context
            .register_global_property(js_string!("length"), 0, Attribute::READONLY)
            .map_err(|e| JsError::InitError(format!("Failed to register length: {}", e)))?;

        // Window event handling
        let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            // Store event listener (stub for now - events fire synchronously)
            Ok(JsValue::undefined())
        });
        let remove_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });
        let dispatch_event = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::from(true))
        });

        context
            .register_global_builtin_callable(js_string!("addEventListener"), 3, add_event_listener)
            .map_err(|e| JsError::InitError(format!("Failed to register addEventListener: {}", e)))?;
        context
            .register_global_builtin_callable(js_string!("removeEventListener"), 3, remove_event_listener)
            .map_err(|e| JsError::InitError(format!("Failed to register removeEventListener: {}", e)))?;
        context
            .register_global_builtin_callable(js_string!("dispatchEvent"), 1, dispatch_event)
            .map_err(|e| JsError::InitError(format!("Failed to register dispatchEvent: {}", e)))?;

        // Additional window properties
        context
            .register_global_property(js_string!("name"), js_string!(""), Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register name: {}", e)))?;
        context
            .register_global_property(js_string!("closed"), false, Attribute::READONLY)
            .map_err(|e| JsError::InitError(format!("Failed to register closed: {}", e)))?;
        context
            .register_global_property(js_string!("opener"), JsValue::null(), Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register opener: {}", e)))?;
        context
            .register_global_property(js_string!("status"), js_string!(""), Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register status: {}", e)))?;
        context
            .register_global_property(js_string!("isSecureContext"), true, Attribute::READONLY)
            .map_err(|e| JsError::InitError(format!("Failed to register isSecureContext: {}", e)))?;
        context
            .register_global_property(js_string!("origin"), js_string!("null"), Attribute::READONLY)
            .map_err(|e| JsError::InitError(format!("Failed to register origin: {}", e)))?;
        context
            .register_global_property(js_string!("crossOriginIsolated"), false, Attribute::READONLY)
            .map_err(|e| JsError::InitError(format!("Failed to register crossOriginIsolated: {}", e)))?;

        // postMessage
        let post_message = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            // In a single-window context, postMessage is a no-op
            Ok(JsValue::undefined())
        });
        context
            .register_global_builtin_callable(js_string!("postMessage"), 3, post_message)
            .map_err(|e| JsError::InitError(format!("Failed to register postMessage: {}", e)))?;

        // Event handler properties (all null by default)
        for handler in &[
            "onload", "onerror", "onunload", "onbeforeunload", "onhashchange",
            "onpopstate", "onresize", "onscroll", "onfocus", "onblur",
            "onmessage", "onmessageerror", "ononline", "onoffline",
            "onpagehide", "onpageshow", "onstorage", "onlanguagechange",
        ] {
            context
                .register_global_property(js_string!(*handler), JsValue::null(), Attribute::all())
                .map_err(|e| JsError::InitError(format!("Failed to register {}: {}", handler, e)))?;
        }

        // Note: crypto is now registered via crypto::register_all_crypto_apis

        Ok(())
    }

    /// Initialize btoa and atob functions
    fn init_btoa_atob(context: &mut Context) -> Result<()> {
        let btoa = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let input = args.get_or_undefined(0).to_string(ctx)?;
            let input_str = input.to_std_string_escaped();

            let mut bytes = Vec::with_capacity(input_str.len());
            for ch in input_str.chars() {
                let code = ch as u32;
                if code > 255 {
                    return Err(JsNativeError::range()
                        .with_message(
                            "The string to be encoded contains characters outside of the Latin1 range",
                        )
                        .into());
                }
                bytes.push(code as u8);
            }

            let encoded = base64_encode(&bytes);
            Ok(JsValue::from(js_string!(encoded)))
        });

        let atob = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let input = args.get_or_undefined(0).to_string(ctx)?;
            let input_str = input.to_std_string_escaped();

            let decoded = base64_decode(&input_str).map_err(|_| {
                JsNativeError::error()
                    .with_message("The string to be decoded is not correctly encoded")
            })?;

            let result: String = decoded.iter().map(|&b| b as char).collect();
            Ok(JsValue::from(js_string!(result)))
        });

        context
            .register_global_builtin_callable(js_string!("btoa"), 1, btoa)
            .map_err(|e| JsError::InitError(format!("Failed to register btoa: {}", e)))?;

        context
            .register_global_builtin_callable(js_string!("atob"), 1, atob)
            .map_err(|e| JsError::InitError(format!("Failed to register atob: {}", e)))?;

        Ok(())
    }

    /// Initialize timers
    fn init_timers(context: &mut Context) -> Result<()> {
        let set_timeout = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(callback) = args.first() {
                if callback.is_callable() {
                    let callback_obj = callback.as_callable().unwrap();
                    let callback_args: Vec<JsValue> = args.iter().skip(2).cloned().collect();
                    let _ = callback_obj.call(&JsValue::undefined(), &callback_args, ctx);
                }
            }
            Ok(JsValue::from(1))
        });

        let clear_timeout = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let set_interval = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(callback) = args.first() {
                if callback.is_callable() {
                    let callback_obj = callback.as_callable().unwrap();
                    let callback_args: Vec<JsValue> = args.iter().skip(2).cloned().collect();
                    let _ = callback_obj.call(&JsValue::undefined(), &callback_args, ctx);
                }
            }
            Ok(JsValue::from(1))
        });

        let clear_interval = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let request_animation_frame = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(callback) = args.first() {
                if callback.is_callable() {
                    let callback_obj = callback.as_callable().unwrap();
                    let _ = callback_obj.call(&JsValue::undefined(), &[JsValue::from(0)], ctx);
                }
            }
            Ok(JsValue::from(1))
        });

        let cancel_animation_frame = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        context
            .register_global_builtin_callable(js_string!("setTimeout"), 2, set_timeout)
            .map_err(|e| JsError::InitError(format!("Failed to register setTimeout: {}", e)))?;
        context
            .register_global_builtin_callable(js_string!("clearTimeout"), 1, clear_timeout)
            .map_err(|e| JsError::InitError(format!("Failed to register clearTimeout: {}", e)))?;
        context
            .register_global_builtin_callable(js_string!("setInterval"), 2, set_interval)
            .map_err(|e| JsError::InitError(format!("Failed to register setInterval: {}", e)))?;
        context
            .register_global_builtin_callable(js_string!("clearInterval"), 1, clear_interval)
            .map_err(|e| JsError::InitError(format!("Failed to register clearInterval: {}", e)))?;
        context
            .register_global_builtin_callable(
                js_string!("requestAnimationFrame"),
                1,
                request_animation_frame,
            )
            .map_err(|e| {
                JsError::InitError(format!("Failed to register requestAnimationFrame: {}", e))
            })?;
        context
            .register_global_builtin_callable(
                js_string!("cancelAnimationFrame"),
                1,
                cancel_animation_frame,
            )
            .map_err(|e| {
                JsError::InitError(format!("Failed to register cancelAnimationFrame: {}", e))
            })?;

        Ok(())
    }

    /// Initialize fetch as a stub
    fn init_fetch_stub(context: &mut Context) -> Result<()> {
        let fetch = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Err(JsNativeError::error()
                .with_message("fetch() is not implemented in this browser")
                .into())
        });

        context
            .register_global_builtin_callable(js_string!("fetch"), 1, fetch)
            .map_err(|e| JsError::InitError(format!("Failed to register fetch: {}", e)))?;

        Ok(())
    }

    /// Initialize location object
    fn init_location(context: &mut Context) -> Result<()> {
        let location = ObjectInitializer::new(context)
            .property(js_string!("href"), js_string!(""), Attribute::all())
            .property(js_string!("protocol"), js_string!("https:"), Attribute::all())
            .property(js_string!("host"), js_string!(""), Attribute::all())
            .property(js_string!("hostname"), js_string!(""), Attribute::all())
            .property(js_string!("pathname"), js_string!("/"), Attribute::all())
            .property(js_string!("search"), js_string!(""), Attribute::all())
            .property(js_string!("hash"), js_string!(""), Attribute::all())
            .property(js_string!("origin"), js_string!(""), Attribute::all())
            .build();

        context
            .register_global_property(js_string!("location"), location, Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register location: {}", e)))?;

        Ok(())
    }

    /// Initialize navigator object
    fn init_navigator(context: &mut Context) -> Result<()> {
        // Create languages array
        let languages = JsArray::from_iter(
            [
                JsValue::from(js_string!("en-US")),
                JsValue::from(js_string!("en")),
            ],
            context,
        );

        // Create plugins array (PluginArray with proper methods)
        let plugins_item = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::null()));
        let plugins_named_item = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::null()));
        let plugins_refresh = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let plugins = ObjectInitializer::new(context)
            .property(js_string!("length"), 0, Attribute::READONLY)
            .function(plugins_item, js_string!("item"), 1)
            .function(plugins_named_item, js_string!("namedItem"), 1)
            .function(plugins_refresh, js_string!("refresh"), 0)
            .build();

        // Create mimeTypes array (MimeTypeArray with proper methods)
        let mime_types_item = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::null()));
        let mime_types_named_item = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::null()));
        let mime_types = ObjectInitializer::new(context)
            .property(js_string!("length"), 0, Attribute::READONLY)
            .function(mime_types_item, js_string!("item"), 1)
            .function(mime_types_named_item, js_string!("namedItem"), 1)
            .build();

        let navigator = ObjectInitializer::new(context)
            .property(
                js_string!("userAgent"),
                js_string!("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"),
                Attribute::READONLY,
            )
            .property(js_string!("language"), js_string!("en-US"), Attribute::READONLY)
            .property(js_string!("languages"), languages, Attribute::READONLY)
            .property(js_string!("platform"), js_string!("Win32"), Attribute::READONLY)
            .property(js_string!("cookieEnabled"), true, Attribute::READONLY)
            .property(js_string!("onLine"), true, Attribute::READONLY)
            .property(js_string!("vendor"), js_string!("Google Inc."), Attribute::READONLY)
            .property(js_string!("vendorSub"), js_string!(""), Attribute::READONLY)
            .property(js_string!("product"), js_string!("Gecko"), Attribute::READONLY)
            .property(js_string!("productSub"), js_string!("20030107"), Attribute::READONLY)
            .property(js_string!("appCodeName"), js_string!("Mozilla"), Attribute::READONLY)
            .property(js_string!("appName"), js_string!("Netscape"), Attribute::READONLY)
            .property(js_string!("appVersion"), js_string!("5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"), Attribute::READONLY)
            .property(js_string!("hardwareConcurrency"), 8, Attribute::READONLY)
            .property(js_string!("maxTouchPoints"), 0, Attribute::READONLY)
            .property(js_string!("webdriver"), false, Attribute::READONLY)
            .property(js_string!("plugins"), plugins, Attribute::READONLY)
            .property(js_string!("mimeTypes"), mime_types, Attribute::READONLY)
            .property(js_string!("doNotTrack"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("deviceMemory"), 8, Attribute::READONLY)
            .build();

        context
            .register_global_property(js_string!("navigator"), navigator, Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register navigator: {}", e)))?;

        Ok(())
    }

    /// Initialize performance object
    fn init_performance(context: &mut Context) -> Result<()> {
        let now = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            // Return milliseconds since performance origin (high-resolution timestamp)
            let elapsed = PERFORMANCE_ORIGIN.elapsed();
            let ms = elapsed.as_secs_f64() * 1000.0;
            Ok(JsValue::from(ms))
        });

        let get_entries = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::from(js_string!("[]")))
        });

        let get_entries_by_type = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::from(js_string!("[]")))
        });

        let get_entries_by_name = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::from(js_string!("[]")))
        });

        let mark = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let measure = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let clear_marks = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let clear_measures = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        // Create timing object
        let timing = ObjectInitializer::new(context)
            .property(js_string!("navigationStart"), 0, Attribute::READONLY)
            .property(js_string!("domLoading"), 0, Attribute::READONLY)
            .property(js_string!("domInteractive"), 0, Attribute::READONLY)
            .property(js_string!("domContentLoadedEventStart"), 0, Attribute::READONLY)
            .property(js_string!("domContentLoadedEventEnd"), 0, Attribute::READONLY)
            .property(js_string!("domComplete"), 0, Attribute::READONLY)
            .property(js_string!("loadEventStart"), 0, Attribute::READONLY)
            .property(js_string!("loadEventEnd"), 0, Attribute::READONLY)
            .build();

        let performance = ObjectInitializer::new(context)
            .function(now, js_string!("now"), 0)
            .function(get_entries, js_string!("getEntries"), 0)
            .function(get_entries_by_type, js_string!("getEntriesByType"), 1)
            .function(get_entries_by_name, js_string!("getEntriesByName"), 1)
            .function(mark, js_string!("mark"), 1)
            .function(measure, js_string!("measure"), 1)
            .function(clear_marks, js_string!("clearMarks"), 0)
            .function(clear_measures, js_string!("clearMeasures"), 0)
            .property(js_string!("timing"), timing, Attribute::READONLY)
            .property(js_string!("timeOrigin"), std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64() * 1000.0)
                .unwrap_or(0.0), Attribute::READONLY)
            .build();

        context
            .register_global_property(js_string!("performance"), performance, Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register performance: {}", e)))?;

        Ok(())
    }

    /// Initialize history object
    fn init_history(context: &mut Context) -> Result<()> {
        let push_state = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let replace_state = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let go = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let back = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let forward = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let history = ObjectInitializer::new(context)
            .function(push_state, js_string!("pushState"), 3)
            .function(replace_state, js_string!("replaceState"), 3)
            .function(go, js_string!("go"), 1)
            .function(back, js_string!("back"), 0)
            .function(forward, js_string!("forward"), 0)
            .property(js_string!("length"), 1, Attribute::READONLY)
            .property(js_string!("state"), JsValue::null(), Attribute::READONLY)
            .build();

        context
            .register_global_property(js_string!("history"), history, Attribute::all())
            .map_err(|e| JsError::InitError(format!("Failed to register history: {}", e)))?;

        Ok(())
    }

    /// Initialize screen object
    fn init_screen(context: &mut Context) -> Result<()> {
        let screen = ObjectInitializer::new(context)
            .property(js_string!("width"), 1920, Attribute::READONLY)
            .property(js_string!("height"), 1080, Attribute::READONLY)
            .property(js_string!("availWidth"), 1920, Attribute::READONLY)
            .property(js_string!("availHeight"), 1040, Attribute::READONLY)
            .property(js_string!("colorDepth"), 24, Attribute::READONLY)
            .property(js_string!("pixelDepth"), 24, Attribute::READONLY)
            .property(js_string!("availLeft"), 0, Attribute::READONLY)
            .property(js_string!("availTop"), 0, Attribute::READONLY)
            .property(js_string!("orientation"), JsValue::undefined(), Attribute::READONLY)
            .build();

        context
            .register_global_property(js_string!("screen"), screen, Attribute::READONLY)
            .map_err(|e| JsError::InitError(format!("Failed to register screen: {}", e)))?;

        Ok(())
    }

    /// Initialize Event constructors and related APIs
    fn init_events(context: &mut Context) -> Result<()> {
        // Event constructor
        let event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let prevent_default = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });
            let stop_propagation = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });
            let stop_immediate_propagation = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });

            let event = ObjectInitializer::new(ctx)
                .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
                .property(js_string!("target"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("currentTarget"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("bubbles"), false, Attribute::READONLY)
                .property(js_string!("cancelable"), false, Attribute::READONLY)
                .property(js_string!("defaultPrevented"), false, Attribute::READONLY)
                .property(js_string!("eventPhase"), 0, Attribute::READONLY)
                .property(js_string!("timeStamp"), 0.0, Attribute::READONLY)
                .property(js_string!("isTrusted"), false, Attribute::READONLY)
                .function(prevent_default, js_string!("preventDefault"), 0)
                .function(stop_propagation, js_string!("stopPropagation"), 0)
                .function(stop_immediate_propagation, js_string!("stopImmediatePropagation"), 0)
                .build();

            Ok(JsValue::from(event))
        });

        let event_ctor = FunctionObjectBuilder::new(context.realm(), event_constructor)
            .name(js_string!("Event"))
            .length(2)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("Event"), event_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register Event: {}", e)))?;

        // CustomEvent constructor
        let custom_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let detail = if args.len() > 1 {
                let options = args.get_or_undefined(1);
                if let Some(obj) = options.as_object() {
                    obj.get(js_string!("detail"), ctx).unwrap_or(JsValue::null())
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
                .property(js_string!("detail"), detail, Attribute::READONLY)
                .property(js_string!("target"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("currentTarget"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("bubbles"), false, Attribute::READONLY)
                .property(js_string!("cancelable"), false, Attribute::READONLY)
                .property(js_string!("defaultPrevented"), false, Attribute::READONLY)
                .function(prevent_default, js_string!("preventDefault"), 0)
                .function(stop_propagation, js_string!("stopPropagation"), 0)
                .build();

            Ok(JsValue::from(event))
        });

        let custom_event_ctor = FunctionObjectBuilder::new(context.realm(), custom_event_constructor)
            .name(js_string!("CustomEvent"))
            .length(2)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("CustomEvent"), custom_event_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register CustomEvent: {}", e)))?;

        // MouseEvent constructor
        let mouse_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let event = ObjectInitializer::new(ctx)
                .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
                .property(js_string!("clientX"), 0, Attribute::READONLY)
                .property(js_string!("clientY"), 0, Attribute::READONLY)
                .property(js_string!("pageX"), 0, Attribute::READONLY)
                .property(js_string!("pageY"), 0, Attribute::READONLY)
                .property(js_string!("screenX"), 0, Attribute::READONLY)
                .property(js_string!("screenY"), 0, Attribute::READONLY)
                .property(js_string!("button"), 0, Attribute::READONLY)
                .property(js_string!("buttons"), 0, Attribute::READONLY)
                .property(js_string!("ctrlKey"), false, Attribute::READONLY)
                .property(js_string!("shiftKey"), false, Attribute::READONLY)
                .property(js_string!("altKey"), false, Attribute::READONLY)
                .property(js_string!("metaKey"), false, Attribute::READONLY)
                .build();

            Ok(JsValue::from(event))
        });

        let mouse_event_ctor = FunctionObjectBuilder::new(context.realm(), mouse_event_constructor)
            .name(js_string!("MouseEvent"))
            .length(2)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("MouseEvent"), mouse_event_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register MouseEvent: {}", e)))?;

        // KeyboardEvent constructor
        let keyboard_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let event = ObjectInitializer::new(ctx)
                .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
                .property(js_string!("key"), js_string!(""), Attribute::READONLY)
                .property(js_string!("code"), js_string!(""), Attribute::READONLY)
                .property(js_string!("keyCode"), 0, Attribute::READONLY)
                .property(js_string!("charCode"), 0, Attribute::READONLY)
                .property(js_string!("which"), 0, Attribute::READONLY)
                .property(js_string!("ctrlKey"), false, Attribute::READONLY)
                .property(js_string!("shiftKey"), false, Attribute::READONLY)
                .property(js_string!("altKey"), false, Attribute::READONLY)
                .property(js_string!("metaKey"), false, Attribute::READONLY)
                .property(js_string!("repeat"), false, Attribute::READONLY)
                .build();

            Ok(JsValue::from(event))
        });

        let keyboard_event_ctor = FunctionObjectBuilder::new(context.realm(), keyboard_event_constructor)
            .name(js_string!("KeyboardEvent"))
            .length(2)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("KeyboardEvent"), keyboard_event_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register KeyboardEvent: {}", e)))?;

        // FocusEvent constructor
        let focus_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let event = ObjectInitializer::new(ctx)
                .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
                .property(js_string!("relatedTarget"), JsValue::null(), Attribute::READONLY)
                .build();

            Ok(JsValue::from(event))
        });

        let focus_event_ctor = FunctionObjectBuilder::new(context.realm(), focus_event_constructor)
            .name(js_string!("FocusEvent"))
            .length(2)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("FocusEvent"), focus_event_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register FocusEvent: {}", e)))?;

        // InputEvent constructor
        let input_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let event = ObjectInitializer::new(ctx)
                .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
                .property(js_string!("data"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("inputType"), js_string!(""), Attribute::READONLY)
                .property(js_string!("isComposing"), false, Attribute::READONLY)
                .build();

            Ok(JsValue::from(event))
        });

        let input_event_ctor = FunctionObjectBuilder::new(context.realm(), input_event_constructor)
            .name(js_string!("InputEvent"))
            .length(2)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("InputEvent"), input_event_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register InputEvent: {}", e)))?;

        // TouchEvent constructor (stub)
        let touch_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let empty_touch_list = ObjectInitializer::new(ctx)
                .property(js_string!("length"), 0, Attribute::READONLY)
                .build();

            let event = ObjectInitializer::new(ctx)
                .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
                .property(js_string!("touches"), empty_touch_list.clone(), Attribute::READONLY)
                .property(js_string!("targetTouches"), empty_touch_list.clone(), Attribute::READONLY)
                .property(js_string!("changedTouches"), empty_touch_list, Attribute::READONLY)
                .build();

            Ok(JsValue::from(event))
        });

        let touch_event_ctor = FunctionObjectBuilder::new(context.realm(), touch_event_constructor)
            .name(js_string!("TouchEvent"))
            .length(2)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("TouchEvent"), touch_event_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register TouchEvent: {}", e)))?;

        // PointerEvent constructor
        let pointer_event_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let event = ObjectInitializer::new(ctx)
                .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
                .property(js_string!("pointerId"), 0, Attribute::READONLY)
                .property(js_string!("pointerType"), js_string!("mouse"), Attribute::READONLY)
                .property(js_string!("isPrimary"), true, Attribute::READONLY)
                .property(js_string!("width"), 1, Attribute::READONLY)
                .property(js_string!("height"), 1, Attribute::READONLY)
                .property(js_string!("pressure"), 0.0, Attribute::READONLY)
                .build();

            Ok(JsValue::from(event))
        });

        let pointer_event_ctor = FunctionObjectBuilder::new(context.realm(), pointer_event_constructor)
            .name(js_string!("PointerEvent"))
            .length(2)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("PointerEvent"), pointer_event_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register PointerEvent: {}", e)))?;

        Ok(())
    }

    /// Initialize Observer APIs (MutationObserver, IntersectionObserver, ResizeObserver, PerformanceObserver)
    fn init_observers(context: &mut Context) -> Result<()> {
        observers::register_all_observers(context)
            .map_err(|e| JsError::InitError(format!("Failed to register observers: {}", e)))?;
        Ok(())
    }

    /// Add NodeList iterator polyfill via JavaScript evaluation
    /// This works around a Boa panic when adding Symbol.iterator from Rust closures
    fn add_nodelist_iterator_polyfill(context: &mut Context) -> Result<()> {
        let polyfill = r#"
(function() {
    // Patch querySelectorAll to return iterable NodeLists
    var originalDocQSA = document.querySelectorAll.bind(document);
    var originalElementQSA = Element.prototype.querySelectorAll;
    
    function makeIterable(nodeList) {
        nodeList[Symbol.iterator] = function() {
            var index = 0;
            var list = this;
            return {
                next: function() {
                    if (index < list.length) {
                        return { value: list[index++], done: false };
                    }
                    return { done: true };
                }
            };
        };
        return nodeList;
    }
    
    document.querySelectorAll = function(selector) {
        return makeIterable(originalDocQSA(selector));
    };
    
    Element.prototype.querySelectorAll = function(selector) {
        return makeIterable(originalElementQSA.call(this, selector));
    };
    
    // Also patch getElementsByClassName, getElementsByTagName
    var originalGetsByClass = document.getElementsByClassName.bind(document);
    var originalGetsByTag = document.getElementsByTagName.bind(document);
    var originalElementGetsByClass = Element.prototype.getElementsByClassName;
    var originalElementGetsByTag = Element.prototype.getElementsByTagName;
    
    document.getElementsByClassName = function(name) {
        return makeIterable(originalGetsByClass(name));
    };
    document.getElementsByTagName = function(name) {
        return makeIterable(originalGetsByTag(name));
    };
    Element.prototype.getElementsByClassName = function(name) {
        return makeIterable(originalElementGetsByClass.call(this, name));
    };
    Element.prototype.getElementsByTagName = function(name) {
        return makeIterable(originalElementGetsByTag.call(this, name));
    };
})();
"#;
        
        let source = Source::from_bytes(polyfill);
        context.eval(source)
            .map_err(|e| JsError::InitError(format!("Failed to add NodeList polyfill: {}", e)))?;
        
        Ok(())
    }

    /// Initialize miscellaneous Web APIs
    fn init_misc_apis(context: &mut Context) -> Result<()> {
        // DOMParser
        let dom_parser_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let parse_from_string = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                // Return a minimal document-like object
                let doc = ObjectInitializer::new(ctx)
                    .property(js_string!("nodeType"), 9, Attribute::READONLY)
                    .property(js_string!("nodeName"), js_string!("#document"), Attribute::READONLY)
                    .property(js_string!("body"), JsValue::null(), Attribute::READONLY)
                    .property(js_string!("documentElement"), JsValue::null(), Attribute::READONLY)
                    .build();
                Ok(JsValue::from(doc))
            });

            let parser = ObjectInitializer::new(ctx)
                .function(parse_from_string, js_string!("parseFromString"), 2)
                .build();

            Ok(JsValue::from(parser))
        });

        let dom_parser_ctor = FunctionObjectBuilder::new(context.realm(), dom_parser_constructor)
            .name(js_string!("DOMParser"))
            .length(0)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("DOMParser"), dom_parser_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register DOMParser: {}", e)))?;

        // XMLSerializer
        let xml_serializer_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let serialize_to_string = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::from(js_string!("")))
            });

            let serializer = ObjectInitializer::new(ctx)
                .function(serialize_to_string, js_string!("serializeToString"), 1)
                .build();

            Ok(JsValue::from(serializer))
        });

        let xml_serializer_ctor = FunctionObjectBuilder::new(context.realm(), xml_serializer_constructor)
            .name(js_string!("XMLSerializer"))
            .length(0)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("XMLSerializer"), xml_serializer_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register XMLSerializer: {}", e)))?;

        // XMLHttpRequest (full implementation)
        xhr::register_xmlhttprequest(context)
            .map_err(|e| JsError::InitError(format!("Failed to register XMLHttpRequest: {}", e)))?;

        // FormData is registered in forms.rs with full implementation
        // (removed stub that was overwriting the real implementation)

        // URLSearchParams
        let url_search_params_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let append = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });
            let delete_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });
            let get = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::null())
            });
            let get_all = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let arr = ObjectInitializer::new(ctx)
                    .property(js_string!("length"), 0, Attribute::READONLY)
                    .build();
                Ok(JsValue::from(arr))
            });
            let has = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::from(false))
            });
            let set = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });
            let to_string = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::from(js_string!("")))
            });

            let params = ObjectInitializer::new(ctx)
                .function(append, js_string!("append"), 2)
                .function(delete_fn, js_string!("delete"), 1)
                .function(get, js_string!("get"), 1)
                .function(get_all, js_string!("getAll"), 1)
                .function(has, js_string!("has"), 1)
                .function(set, js_string!("set"), 2)
                .function(to_string, js_string!("toString"), 0)
                .build();

            Ok(JsValue::from(params))
        });

        let url_search_params_ctor = FunctionObjectBuilder::new(context.realm(), url_search_params_constructor)
            .name(js_string!("URLSearchParams"))
            .length(1)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("URLSearchParams"), url_search_params_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register URLSearchParams: {}", e)))?;

        // URL constructor
        let url_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let url_string = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            // Parse URL (simplified)
            let (protocol, rest) = if url_string.contains("://") {
                let parts: Vec<&str> = url_string.splitn(2, "://").collect();
                (format!("{}:", parts[0]), parts.get(1).unwrap_or(&"").to_string())
            } else {
                ("https:".to_string(), url_string.clone())
            };

            let (host, path_and_query) = if let Some(slash_pos) = rest.find('/') {
                (rest[..slash_pos].to_string(), rest[slash_pos..].to_string())
            } else {
                (rest.clone(), "/".to_string())
            };

            let (pathname, search) = if let Some(q_pos) = path_and_query.find('?') {
                (path_and_query[..q_pos].to_string(), path_and_query[q_pos..].to_string())
            } else {
                (path_and_query.clone(), "".to_string())
            };

            let (pathname, hash) = if let Some(h_pos) = pathname.find('#') {
                (pathname[..h_pos].to_string(), pathname[h_pos..].to_string())
            } else {
                (pathname, "".to_string())
            };

            let origin = format!("{}//{}", protocol, host);

            let url_obj = ObjectInitializer::new(ctx)
                .property(js_string!("href"), js_string!(url_string.clone()), Attribute::all())
                .property(js_string!("protocol"), js_string!(protocol.clone()), Attribute::READONLY)
                .property(js_string!("host"), js_string!(host.clone()), Attribute::READONLY)
                .property(js_string!("hostname"), js_string!(host.clone()), Attribute::READONLY)
                .property(js_string!("pathname"), js_string!(pathname), Attribute::READONLY)
                .property(js_string!("search"), js_string!(search), Attribute::READONLY)
                .property(js_string!("hash"), js_string!(hash), Attribute::READONLY)
                .property(js_string!("origin"), js_string!(origin), Attribute::READONLY)
                .property(js_string!("port"), js_string!(""), Attribute::READONLY)
                .property(js_string!("username"), js_string!(""), Attribute::READONLY)
                .property(js_string!("password"), js_string!(""), Attribute::READONLY)
                .build();

            Ok(JsValue::from(url_obj))
        });

        let url_ctor = FunctionObjectBuilder::new(context.realm(), url_constructor)
            .name(js_string!("URL"))
            .length(2)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("URL"), url_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register URL: {}", e)))?;

        // URLPattern - URL pattern matching API
        let urlpattern_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
            // URLPattern(input, baseURL?) or URLPattern(init)
            // input can be a string pattern or an object with pattern components

            // Extract pattern components
            let (protocol, username, password, hostname, port, pathname, search, hash) =
                if let Some(input) = args.get(0) {
                    if let Some(obj) = input.as_object() {
                        // Object with pattern components - check for undefined and default to "*"
                        let get_pattern = |name: &str, obj: &JsObject, ctx: &mut Context| -> String {
                            obj.get(js_string!(name), ctx)
                                .ok()
                                .and_then(|v| if v.is_undefined() || v.is_null() { None } else { v.to_string(ctx).ok() })
                                .map(|s| s.to_std_string_escaped())
                                .unwrap_or_else(|| "*".to_string())
                        };
                        let protocol = get_pattern("protocol", &obj, ctx);
                        let username = get_pattern("username", &obj, ctx);
                        let password = get_pattern("password", &obj, ctx);
                        let hostname = get_pattern("hostname", &obj, ctx);
                        let port = get_pattern("port", &obj, ctx);
                        let pathname = get_pattern("pathname", &obj, ctx);
                        let search = get_pattern("search", &obj, ctx);
                        let hash = get_pattern("hash", &obj, ctx);
                        (protocol, username, password, hostname, port, pathname, search, hash)
                    } else if let Ok(pattern_str) = input.to_string(ctx) {
                        // String pattern - parse as pathname pattern
                        let pattern = pattern_str.to_std_string_escaped();
                        (
                            "*".to_string(),
                            "*".to_string(),
                            "*".to_string(),
                            "*".to_string(),
                            "*".to_string(),
                            pattern,
                            "*".to_string(),
                            "*".to_string(),
                        )
                    } else {
                        ("*".to_string(), "*".to_string(), "*".to_string(), "*".to_string(),
                         "*".to_string(), "*".to_string(), "*".to_string(), "*".to_string())
                    }
                } else {
                    ("*".to_string(), "*".to_string(), "*".to_string(), "*".to_string(),
                     "*".to_string(), "*".to_string(), "*".to_string(), "*".to_string())
                };

            // Store patterns for matching
            let protocol_pattern = protocol.clone();
            let hostname_pattern = hostname.clone();
            let pathname_pattern = pathname.clone();
            let search_pattern = search.clone();
            let hash_pattern = hash.clone();

            // test(input, baseURL?) - returns boolean
            let test_fn = unsafe {
                NativeFunction::from_closure(move |_this, args, ctx| {
                    let input = args.get_or_undefined(0);
                    let url_str = if let Ok(s) = input.to_string(ctx) {
                        s.to_std_string_escaped()
                    } else {
                        return Ok(JsValue::from(false));
                    };

                    // Simple pattern matching (wildcards)
                    let matches_pattern = |value: &str, pattern: &str| -> bool {
                        if pattern == "*" {
                            return true;
                        }
                        // Simple glob-style matching
                        if pattern.contains('*') {
                            let parts: Vec<&str> = pattern.split('*').collect();
                            if parts.len() == 2 {
                                let starts = parts[0].is_empty() || value.starts_with(parts[0]);
                                let ends = parts[1].is_empty() || value.ends_with(parts[1]);
                                return starts && ends;
                            }
                        }
                        value == pattern
                    };

                    // Parse URL and check patterns
                    if let Ok(parsed) = url::Url::parse(&url_str) {
                        let protocol_match = matches_pattern(parsed.scheme(), &protocol_pattern);
                        let hostname_match = matches_pattern(parsed.host_str().unwrap_or(""), &hostname_pattern);
                        let pathname_match = matches_pattern(parsed.path(), &pathname_pattern);
                        let search_match = matches_pattern(parsed.query().unwrap_or(""), &search_pattern);
                        let hash_match = matches_pattern(parsed.fragment().unwrap_or(""), &hash_pattern);

                        Ok(JsValue::from(protocol_match && hostname_match && pathname_match && search_match && hash_match))
                    } else {
                        Ok(JsValue::from(false))
                    }
                })
            };

            // exec(input, baseURL?) - returns match result or null
            let protocol_exec = protocol.clone();
            let hostname_exec = hostname.clone();
            let pathname_exec = pathname.clone();
            let exec_fn = unsafe {
                NativeFunction::from_closure(move |_this, args, ctx| {
                    let input = args.get_or_undefined(0);
                    let url_str = if let Ok(s) = input.to_string(ctx) {
                        s.to_std_string_escaped()
                    } else {
                        return Ok(JsValue::null());
                    };

                    if let Ok(parsed) = url::Url::parse(&url_str) {
                        // Create result object with matched groups
                        let create_component_result = |input_val: &str, ctx: &mut Context| -> JsObject {
                            let groups = ObjectInitializer::new(ctx).build();
                            ObjectInitializer::new(ctx)
                                .property(js_string!("input"), js_string!(input_val), Attribute::all())
                                .property(js_string!("groups"), JsValue::from(groups), Attribute::all())
                                .build()
                        };

                        let protocol_result = create_component_result(parsed.scheme(), ctx);
                        let hostname_result = create_component_result(parsed.host_str().unwrap_or(""), ctx);
                        let pathname_result = create_component_result(parsed.path(), ctx);
                        let search_result = create_component_result(parsed.query().unwrap_or(""), ctx);
                        let hash_result = create_component_result(parsed.fragment().unwrap_or(""), ctx);
                        let username_result = create_component_result(parsed.username(), ctx);
                        let password_result = create_component_result(parsed.password().unwrap_or(""), ctx);
                        let port_result = create_component_result(&parsed.port().map(|p| p.to_string()).unwrap_or_default(), ctx);

                        let inputs = boa_engine::object::builtins::JsArray::new(ctx);
                        let _ = inputs.set(0u32, js_string!(url_str.clone()), false, ctx);

                        let result = ObjectInitializer::new(ctx)
                            .property(js_string!("inputs"), JsValue::from(inputs), Attribute::all())
                            .property(js_string!("protocol"), JsValue::from(protocol_result), Attribute::all())
                            .property(js_string!("username"), JsValue::from(username_result), Attribute::all())
                            .property(js_string!("password"), JsValue::from(password_result), Attribute::all())
                            .property(js_string!("hostname"), JsValue::from(hostname_result), Attribute::all())
                            .property(js_string!("port"), JsValue::from(port_result), Attribute::all())
                            .property(js_string!("pathname"), JsValue::from(pathname_result), Attribute::all())
                            .property(js_string!("search"), JsValue::from(search_result), Attribute::all())
                            .property(js_string!("hash"), JsValue::from(hash_result), Attribute::all())
                            .build();

                        Ok(JsValue::from(result))
                    } else {
                        Ok(JsValue::null())
                    }
                })
            };

            let pattern_obj = ObjectInitializer::new(ctx)
                .property(js_string!("protocol"), js_string!(protocol), Attribute::READONLY)
                .property(js_string!("username"), js_string!(username), Attribute::READONLY)
                .property(js_string!("password"), js_string!(password), Attribute::READONLY)
                .property(js_string!("hostname"), js_string!(hostname), Attribute::READONLY)
                .property(js_string!("port"), js_string!(port), Attribute::READONLY)
                .property(js_string!("pathname"), js_string!(pathname), Attribute::READONLY)
                .property(js_string!("search"), js_string!(search), Attribute::READONLY)
                .property(js_string!("hash"), js_string!(hash), Attribute::READONLY)
                .function(test_fn, js_string!("test"), 1)
                .function(exec_fn, js_string!("exec"), 1)
                .build();

            Ok(JsValue::from(pattern_obj))
        });

        let urlpattern_ctor = FunctionObjectBuilder::new(context.realm(), urlpattern_constructor)
            .name(js_string!("URLPattern"))
            .length(2)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("URLPattern"), urlpattern_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register URLPattern: {}", e)))?;

        // Blob, File, and FileReader are fully implemented in encoding.rs
        // (removed stubs that were overwriting the real implementations)

        // Image constructor (creates an HTMLImageElement-like object)
        let image_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let width = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0);
            let height = args.get_or_undefined(1).to_u32(ctx).unwrap_or(0);

            let img = ObjectInitializer::new(ctx)
                .property(js_string!("tagName"), js_string!("IMG"), Attribute::READONLY)
                .property(js_string!("nodeName"), js_string!("IMG"), Attribute::READONLY)
                .property(js_string!("nodeType"), 1, Attribute::READONLY)
                .property(js_string!("src"), js_string!(""), Attribute::all())
                .property(js_string!("alt"), js_string!(""), Attribute::all())
                .property(js_string!("width"), width, Attribute::all())
                .property(js_string!("height"), height, Attribute::all())
                .property(js_string!("naturalWidth"), 0, Attribute::READONLY)
                .property(js_string!("naturalHeight"), 0, Attribute::READONLY)
                .property(js_string!("complete"), true, Attribute::READONLY)
                .property(js_string!("onload"), JsValue::null(), Attribute::all())
                .property(js_string!("onerror"), JsValue::null(), Attribute::all())
                .build();

            Ok(JsValue::from(img))
        });

        let image_ctor = FunctionObjectBuilder::new(context.realm(), image_constructor)
            .name(js_string!("Image"))
            .length(2)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("Image"), image_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register Image: {}", e)))?;

        // Audio constructor (stub)
        let audio_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let play = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                // Return a resolved promise-like object
                let promise = ObjectInitializer::new(ctx)
                    .function(
                        NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                            Ok(JsValue::undefined())
                        }),
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
                    .build();
                Ok(JsValue::from(promise))
            });
            let pause = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });
            let load = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });

            let audio = ObjectInitializer::new(ctx)
                .property(js_string!("tagName"), js_string!("AUDIO"), Attribute::READONLY)
                .property(js_string!("src"), js_string!(""), Attribute::all())
                .property(js_string!("currentTime"), 0.0, Attribute::all())
                .property(js_string!("duration"), 0.0, Attribute::READONLY)
                .property(js_string!("paused"), true, Attribute::READONLY)
                .property(js_string!("volume"), 1.0, Attribute::all())
                .property(js_string!("muted"), false, Attribute::all())
                .property(js_string!("loop"), false, Attribute::all())
                .function(play, js_string!("play"), 0)
                .function(pause, js_string!("pause"), 0)
                .function(load, js_string!("load"), 0)
                .build();

            Ok(JsValue::from(audio))
        });

        let audio_ctor = FunctionObjectBuilder::new(context.realm(), audio_constructor)
            .name(js_string!("Audio"))
            .length(1)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("Audio"), audio_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register Audio: {}", e)))?;

        // AbortController
        let abort_controller_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let abort = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::undefined())
            });

            let signal = ObjectInitializer::new(ctx)
                .property(js_string!("aborted"), false, Attribute::READONLY)
                .property(js_string!("reason"), JsValue::undefined(), Attribute::READONLY)
                .build();

            let controller = ObjectInitializer::new(ctx)
                .property(js_string!("signal"), signal, Attribute::READONLY)
                .function(abort, js_string!("abort"), 1)
                .build();

            Ok(JsValue::from(controller))
        });

        let abort_controller_ctor = FunctionObjectBuilder::new(context.realm(), abort_controller_constructor)
            .name(js_string!("AbortController"))
            .length(0)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("AbortController"), abort_controller_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register AbortController: {}", e)))?;

        // WeakRef - holds a weak reference to an object
        let weak_ref_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let target = args.get_or_undefined(0).clone();

            // WeakRef requires an object or symbol target
            if !target.is_object() && !target.is_symbol() {
                return Err(JsNativeError::typ()
                    .with_message("WeakRef: target must be an object or symbol")
                    .into());
            }

            let deref = {
                let target = target.clone();
                unsafe {
                    NativeFunction::from_closure(move |_this, _args, _ctx| {
                        Ok(target.clone())
                    })
                }
            };

            let weak_ref = ObjectInitializer::new(ctx)
                .function(deref, js_string!("deref"), 0)
                .build();

            Ok(JsValue::from(weak_ref))
        });

        let weak_ref_ctor = FunctionObjectBuilder::new(context.realm(), weak_ref_constructor)
            .name(js_string!("WeakRef"))
            .length(1)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("WeakRef"), weak_ref_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register WeakRef: {}", e)))?;

        // FinalizationRegistry - registers cleanup callbacks for garbage-collected objects
        register_finalization_registry(context)
            .map_err(|e| JsError::InitError(format!("Failed to register FinalizationRegistry: {}", e)))?;

        // TextEncoder
        let text_encoder_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let encode = NativeFunction::from_copy_closure(|_this, args, ctx| {
                let input = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let bytes = input.as_bytes();

                // Create a Uint8Array-like object
                let arr = ObjectInitializer::new(ctx)
                    .property(js_string!("length"), bytes.len() as u32, Attribute::READONLY)
                    .property(js_string!("byteLength"), bytes.len() as u32, Attribute::READONLY)
                    .build();

                // Add byte values
                for (i, &byte) in bytes.iter().enumerate() {
                    let _ = arr.set(js_string!(i.to_string()), JsValue::from(byte as u32), false, ctx);
                }

                Ok(JsValue::from(arr))
            });

            let encode_into = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let result = ObjectInitializer::new(ctx)
                    .property(js_string!("read"), 0, Attribute::READONLY)
                    .property(js_string!("written"), 0, Attribute::READONLY)
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

        let text_encoder_ctor = FunctionObjectBuilder::new(context.realm(), text_encoder_constructor)
            .name(js_string!("TextEncoder"))
            .length(0)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("TextEncoder"), text_encoder_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register TextEncoder: {}", e)))?;

        // TextDecoder
        let text_decoder_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let encoding = if args.is_empty() {
                "utf-8".to_string()
            } else {
                args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped()
            };

            let decode = NativeFunction::from_copy_closure(|_this, args, ctx| {
                // Simple decode - get bytes from array-like object
                let input = args.get_or_undefined(0);
                if let Some(obj) = input.as_object() {
                    let length = obj.get(js_string!("length"), ctx)
                        .ok()
                        .and_then(|v| v.to_u32(ctx).ok())
                        .unwrap_or(0);

                    let mut bytes = Vec::with_capacity(length as usize);
                    for i in 0..length {
                        if let Ok(val) = obj.get(js_string!(i.to_string()), ctx) {
                            if let Ok(byte) = val.to_u32(ctx) {
                                bytes.push(byte as u8);
                            }
                        }
                    }

                    let text = String::from_utf8_lossy(&bytes).to_string();
                    return Ok(JsValue::from(js_string!(text)));
                }
                Ok(JsValue::from(js_string!("")))
            });

            let decoder = ObjectInitializer::new(ctx)
                .property(js_string!("encoding"), js_string!(encoding), Attribute::READONLY)
                .property(js_string!("fatal"), false, Attribute::READONLY)
                .property(js_string!("ignoreBOM"), false, Attribute::READONLY)
                .function(decode, js_string!("decode"), 1)
                .build();

            Ok(JsValue::from(decoder))
        });

        let text_decoder_ctor = FunctionObjectBuilder::new(context.realm(), text_decoder_constructor)
            .name(js_string!("TextDecoder"))
            .length(1)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("TextDecoder"), text_decoder_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register TextDecoder: {}", e)))?;

        // queueMicrotask
        let queue_microtask = NativeFunction::from_copy_closure(|_this, args, ctx| {
            // Execute the callback immediately (no true async in this environment)
            if let Some(callback) = args.first() {
                if callback.is_callable() {
                    let callback_obj = callback.as_callable().unwrap();
                    let _ = callback_obj.call(&JsValue::undefined(), &[], ctx);
                }
            }
            Ok(JsValue::undefined())
        });

        context
            .register_global_builtin_callable(js_string!("queueMicrotask"), 1, queue_microtask)
            .map_err(|e| JsError::InitError(format!("Failed to register queueMicrotask: {}", e)))?;

        // structuredClone (simplified)
        let structured_clone = NativeFunction::from_copy_closure(|_this, args, _ctx| {
            // Return the value directly (not a true deep clone, but works for primitives)
            Ok(args.get_or_undefined(0).clone())
        });

        context
            .register_global_builtin_callable(js_string!("structuredClone"), 1, structured_clone)
            .map_err(|e| JsError::InitError(format!("Failed to register structuredClone: {}", e)))?;

        // reportError
        let report_error = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            // Just log and continue - we don't have a true error reporting mechanism
            Ok(JsValue::undefined())
        });

        context
            .register_global_builtin_callable(js_string!("reportError"), 1, report_error)
            .map_err(|e| JsError::InitError(format!("Failed to register reportError: {}", e)))?;

        // escape() - Legacy URL encoding (deprecated but still used by some sites)
        let escape_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let input = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let mut result = String::with_capacity(input.len() * 3);

            for c in input.chars() {
                match c {
                    // Characters that are NOT escaped
                    'A'..='Z' | 'a'..='z' | '0'..='9' | '@' | '*' | '_' | '+' | '-' | '.' | '/' => {
                        result.push(c);
                    }
                    // Everything else gets %XX encoded
                    _ => {
                        if (c as u32) < 256 {
                            result.push_str(&format!("%{:02X}", c as u8));
                        } else {
                            // Unicode: %uXXXX format
                            result.push_str(&format!("%u{:04X}", c as u32));
                        }
                    }
                }
            }

            Ok(JsValue::from(js_string!(result)))
        });

        context
            .register_global_builtin_callable(js_string!("escape"), 1, escape_fn)
            .map_err(|e| JsError::InitError(format!("Failed to register escape: {}", e)))?;

        // unescape() - Legacy URL decoding (deprecated but still used by some sites)
        let unescape_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let input = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let mut result = String::with_capacity(input.len());
            let chars: Vec<char> = input.chars().collect();
            let mut i = 0;

            while i < chars.len() {
                if chars[i] == '%' {
                    // Check for %uXXXX (unicode)
                    if i + 5 < chars.len() && chars[i + 1] == 'u' {
                        let hex: String = chars[i + 2..i + 6].iter().collect();
                        if let Ok(code) = u32::from_str_radix(&hex, 16) {
                            if let Some(c) = char::from_u32(code) {
                                result.push(c);
                                i += 6;
                                continue;
                            }
                        }
                    }
                    // Check for %XX
                    if i + 2 < chars.len() {
                        let hex: String = chars[i + 1..i + 3].iter().collect();
                        if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                            result.push(byte as char);
                            i += 3;
                            continue;
                        }
                    }
                }
                result.push(chars[i]);
                i += 1;
            }

            Ok(JsValue::from(js_string!(result)))
        });

        context
            .register_global_builtin_callable(js_string!("unescape"), 1, unescape_fn)
            .map_err(|e| JsError::InitError(format!("Failed to register unescape: {}", e)))?;

        // Note: CompositionEvent, BeforeUnloadEvent, PageTransitionEvent, MediaQueryListEvent,
        // and SecurityPolicyViolationEvent are now properly implemented in events.rs
        // with full constructor support via register_event_constructors()

        // Register Credential Management APIs (Credential, PasswordCredential, FederatedCredential,
        // PublicKeyCredential, CredentialsContainer, navigator.credentials)
        credentials::register_credential_apis(context)
            .map_err(|e| JsError::InitError(format!("Failed to register Credential APIs: {:?}", e)))?;

        // PaymentRequest constructor (stub for Web Payments API)
        let payment_request_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            // Create abort method
            let abort = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                // Return rejected promise
                let reject = NativeFunction::from_copy_closure(|_this, args, ctx| {
                    if let Some(cb) = args.get(1).and_then(|v| v.as_callable()) {
                        let error = ObjectInitializer::new(ctx)
                            .property(js_string!("name"), js_string!("AbortError"), Attribute::READONLY)
                            .property(js_string!("message"), js_string!("Payment request aborted"), Attribute::READONLY)
                            .build();
                        let _ = cb.call(&JsValue::undefined(), &[JsValue::from(error)], ctx);
                    }
                    Ok(JsValue::undefined())
                }).to_js_function(ctx.realm());

                let promise = ObjectInitializer::new(ctx)
                    .property(js_string!("then"), reject.clone(), Attribute::all())
                    .property(js_string!("catch"), reject, Attribute::all())
                    .build();
                Ok(JsValue::from(promise))
            }).to_js_function(ctx.realm());

            // Create show method (returns rejected promise - not supported)
            let show = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let reject = NativeFunction::from_copy_closure(|_this, args, ctx| {
                    if let Some(cb) = args.get(1).and_then(|v| v.as_callable()) {
                        let error = ObjectInitializer::new(ctx)
                            .property(js_string!("name"), js_string!("NotSupportedError"), Attribute::READONLY)
                            .property(js_string!("message"), js_string!("PaymentRequest is not supported in this environment"), Attribute::READONLY)
                            .build();
                        let _ = cb.call(&JsValue::undefined(), &[JsValue::from(error)], ctx);
                    }
                    Ok(JsValue::undefined())
                }).to_js_function(ctx.realm());

                let promise = ObjectInitializer::new(ctx)
                    .property(js_string!("then"), reject.clone(), Attribute::all())
                    .property(js_string!("catch"), reject, Attribute::all())
                    .build();
                Ok(JsValue::from(promise))
            }).to_js_function(ctx.realm());

            // Create canMakePayment method (always returns false)
            let can_make_payment = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let then = NativeFunction::from_copy_closure(|_this, args, ctx| {
                    if let Some(cb) = args.get(0).and_then(|v| v.as_callable()) {
                        let _ = cb.call(&JsValue::undefined(), &[JsValue::from(false)], ctx);
                    }
                    Ok(JsValue::undefined())
                }).to_js_function(ctx.realm());

                let catch_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                }).to_js_function(ctx.realm());

                let promise = ObjectInitializer::new(ctx)
                    .property(js_string!("then"), then, Attribute::all())
                    .property(js_string!("catch"), catch_fn, Attribute::all())
                    .build();
                Ok(JsValue::from(promise))
            }).to_js_function(ctx.realm());

            // Create hasEnrolledInstrument method (always returns false)
            let has_enrolled_instrument = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let then = NativeFunction::from_copy_closure(|_this, args, ctx| {
                    if let Some(cb) = args.get(0).and_then(|v| v.as_callable()) {
                        let _ = cb.call(&JsValue::undefined(), &[JsValue::from(false)], ctx);
                    }
                    Ok(JsValue::undefined())
                }).to_js_function(ctx.realm());

                let catch_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                    Ok(JsValue::undefined())
                }).to_js_function(ctx.realm());

                let promise = ObjectInitializer::new(ctx)
                    .property(js_string!("then"), then, Attribute::all())
                    .property(js_string!("catch"), catch_fn, Attribute::all())
                    .build();
                Ok(JsValue::from(promise))
            }).to_js_function(ctx.realm());

            let payment_request = ObjectInitializer::new(ctx)
                .property(js_string!("id"), js_string!(uuid::Uuid::new_v4().to_string()), Attribute::READONLY)
                .property(js_string!("show"), show, Attribute::READONLY)
                .property(js_string!("abort"), abort, Attribute::READONLY)
                .property(js_string!("canMakePayment"), can_make_payment, Attribute::READONLY)
                .property(js_string!("hasEnrolledInstrument"), has_enrolled_instrument, Attribute::READONLY)
                .property(js_string!("onpaymentmethodchange"), JsValue::null(), Attribute::all())
                .property(js_string!("onshippingaddresschange"), JsValue::null(), Attribute::all())
                .property(js_string!("onshippingoptionchange"), JsValue::null(), Attribute::all())
                .build();

            Ok(JsValue::from(payment_request))
        });

        context
            .register_global_builtin_callable(js_string!("PaymentRequest"), 2, payment_request_ctor)
            .map_err(|e| JsError::InitError(format!("Failed to register PaymentRequest: {}", e)))?;

        // PaymentResponse constructor (stub)
        let payment_response_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let complete = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let then = NativeFunction::from_copy_closure(|_this, args, ctx| {
                    if let Some(cb) = args.get(0).and_then(|v| v.as_callable()) {
                        let _ = cb.call(&JsValue::undefined(), &[], ctx);
                    }
                    Ok(JsValue::undefined())
                }).to_js_function(ctx.realm());

                let promise = ObjectInitializer::new(ctx)
                    .property(js_string!("then"), then, Attribute::all())
                    .build();
                Ok(JsValue::from(promise))
            }).to_js_function(ctx.realm());

            let retry = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                let reject = NativeFunction::from_copy_closure(|_this, args, ctx| {
                    if let Some(cb) = args.get(1).and_then(|v| v.as_callable()) {
                        let error = ObjectInitializer::new(ctx)
                            .property(js_string!("name"), js_string!("InvalidStateError"), Attribute::READONLY)
                            .build();
                        let _ = cb.call(&JsValue::undefined(), &[JsValue::from(error)], ctx);
                    }
                    Ok(JsValue::undefined())
                }).to_js_function(ctx.realm());

                let promise = ObjectInitializer::new(ctx)
                    .property(js_string!("then"), reject.clone(), Attribute::all())
                    .property(js_string!("catch"), reject, Attribute::all())
                    .build();
                Ok(JsValue::from(promise))
            }).to_js_function(ctx.realm());

            let response = ObjectInitializer::new(ctx)
                .property(js_string!("requestId"), js_string!(""), Attribute::READONLY)
                .property(js_string!("methodName"), js_string!(""), Attribute::READONLY)
                .property(js_string!("details"), JsValue::undefined(), Attribute::READONLY)
                .property(js_string!("shippingAddress"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("shippingOption"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("payerName"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("payerEmail"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("payerPhone"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("complete"), complete, Attribute::READONLY)
                .property(js_string!("retry"), retry, Attribute::READONLY)
                .build();

            Ok(JsValue::from(response))
        });

        context
            .register_global_builtin_callable(js_string!("PaymentResponse"), 0, payment_response_ctor)
            .map_err(|e| JsError::InitError(format!("Failed to register PaymentResponse: {}", e)))?;

        // Trusted Types API - Full Implementation
        // Storage for policies
        use std::sync::{Arc, Mutex as StdMutex};
        use std::collections::HashMap as StdHashMap;

        lazy_static::lazy_static! {
            static ref TRUSTED_POLICIES: Arc<StdMutex<StdHashMap<String, u32>>> = Arc::new(StdMutex::new(StdHashMap::new()));
            static ref NEXT_POLICY_ID: Arc<StdMutex<u32>> = Arc::new(StdMutex::new(1));
        }

        // TrustedHTML constructor (throws - created by policy)
        let trusted_html_ctor = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Err(JsNativeError::typ()
                .with_message("TrustedHTML cannot be constructed directly, use policy.createHTML()")
                .into())
        });
        let trusted_html = FunctionObjectBuilder::new(context.realm(), trusted_html_ctor)
            .name(js_string!("TrustedHTML"))
            .length(0)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("TrustedHTML"), trusted_html, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register TrustedHTML: {}", e)))?;

        // TrustedScript constructor (throws - created by policy)
        let trusted_script_ctor = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Err(JsNativeError::typ()
                .with_message("TrustedScript cannot be constructed directly, use policy.createScript()")
                .into())
        });
        let trusted_script = FunctionObjectBuilder::new(context.realm(), trusted_script_ctor)
            .name(js_string!("TrustedScript"))
            .length(0)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("TrustedScript"), trusted_script, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register TrustedScript: {}", e)))?;

        // TrustedScriptURL constructor (throws - created by policy)
        let trusted_script_url_ctor = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Err(JsNativeError::typ()
                .with_message("TrustedScriptURL cannot be constructed directly, use policy.createScriptURL()")
                .into())
        });
        let trusted_script_url = FunctionObjectBuilder::new(context.realm(), trusted_script_url_ctor)
            .name(js_string!("TrustedScriptURL"))
            .length(0)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("TrustedScriptURL"), trusted_script_url, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register TrustedScriptURL: {}", e)))?;

        // TrustedTypePolicy constructor (throws - created by factory)
        let trusted_type_policy_constructor = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Err(JsNativeError::typ()
                .with_message("TrustedTypePolicy cannot be constructed directly, use trustedTypes.createPolicy()")
                .into())
        });
        let trusted_policy_ctor = FunctionObjectBuilder::new(context.realm(), trusted_type_policy_constructor)
            .name(js_string!("TrustedTypePolicy"))
            .length(0)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("TrustedTypePolicy"), trusted_policy_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register TrustedTypePolicy: {}", e)))?;

        // Helper to create TrustedHTML object
        fn create_trusted_html_obj(ctx: &mut Context, value: &str) -> JsObject {
            let val = value.to_string();
            let to_string = unsafe {
                NativeFunction::from_closure(move |_this, _args, _ctx| {
                    Ok(JsValue::from(js_string!(val.clone())))
                })
            };
            let val2 = value.to_string();
            let to_json = unsafe {
                NativeFunction::from_closure(move |_this, _args, _ctx| {
                    Ok(JsValue::from(js_string!(val2.clone())))
                })
            };
            ObjectInitializer::new(ctx)
                .property(js_string!("_trustedType"), js_string!("TrustedHTML"), Attribute::READONLY)
                .property(js_string!("_value"), js_string!(value), Attribute::READONLY)
                .function(to_string, js_string!("toString"), 0)
                .function(to_json, js_string!("toJSON"), 0)
                .build()
        }

        // Helper to create TrustedScript object
        fn create_trusted_script_obj(ctx: &mut Context, value: &str) -> JsObject {
            let val = value.to_string();
            let to_string = unsafe {
                NativeFunction::from_closure(move |_this, _args, _ctx| {
                    Ok(JsValue::from(js_string!(val.clone())))
                })
            };
            let val2 = value.to_string();
            let to_json = unsafe {
                NativeFunction::from_closure(move |_this, _args, _ctx| {
                    Ok(JsValue::from(js_string!(val2.clone())))
                })
            };
            ObjectInitializer::new(ctx)
                .property(js_string!("_trustedType"), js_string!("TrustedScript"), Attribute::READONLY)
                .property(js_string!("_value"), js_string!(value), Attribute::READONLY)
                .function(to_string, js_string!("toString"), 0)
                .function(to_json, js_string!("toJSON"), 0)
                .build()
        }

        // Helper to create TrustedScriptURL object
        fn create_trusted_script_url_obj(ctx: &mut Context, value: &str) -> JsObject {
            let val = value.to_string();
            let to_string = unsafe {
                NativeFunction::from_closure(move |_this, _args, _ctx| {
                    Ok(JsValue::from(js_string!(val.clone())))
                })
            };
            let val2 = value.to_string();
            let to_json = unsafe {
                NativeFunction::from_closure(move |_this, _args, _ctx| {
                    Ok(JsValue::from(js_string!(val2.clone())))
                })
            };
            ObjectInitializer::new(ctx)
                .property(js_string!("_trustedType"), js_string!("TrustedScriptURL"), Attribute::READONLY)
                .property(js_string!("_value"), js_string!(value), Attribute::READONLY)
                .function(to_string, js_string!("toString"), 0)
                .function(to_json, js_string!("toJSON"), 0)
                .build()
        }

        // createPolicy - creates a TrustedTypePolicy
        let create_policy = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let policy_name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let options = args.get_or_undefined(1);

            // Store callback references
            let create_html_cb = if let Some(opts) = options.as_object() {
                opts.get(js_string!("createHTML"), ctx).ok()
            } else {
                None
            };
            let create_script_cb = if let Some(opts) = options.as_object() {
                opts.get(js_string!("createScript"), ctx).ok()
            } else {
                None
            };
            let create_script_url_cb = if let Some(opts) = options.as_object() {
                opts.get(js_string!("createScriptURL"), ctx).ok()
            } else {
                None
            };

            // createHTML method
            let html_fn = if let Some(cb) = create_html_cb.filter(|c| c.is_callable()) {
                unsafe {
                    NativeFunction::from_closure(move |_this, args, ctx| {
                        let input = args.get_or_undefined(0);
                        if let Some(callback) = cb.as_callable() {
                            let result = callback.call(&JsValue::undefined(), &[input.clone()], ctx)?;
                            let html_str = result.to_string(ctx)?.to_std_string_escaped();
                            return Ok(JsValue::from(create_trusted_html_obj(ctx, &html_str)));
                        }
                        Ok(JsValue::undefined())
                    })
                }
            } else {
                NativeFunction::from_copy_closure(|_this, args, ctx| {
                    let input = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    Ok(JsValue::from(create_trusted_html_obj(ctx, &input)))
                })
            };

            // createScript method
            let script_fn = if let Some(cb) = create_script_cb.filter(|c| c.is_callable()) {
                unsafe {
                    NativeFunction::from_closure(move |_this, args, ctx| {
                        let input = args.get_or_undefined(0);
                        if let Some(callback) = cb.as_callable() {
                            let result = callback.call(&JsValue::undefined(), &[input.clone()], ctx)?;
                            let script_str = result.to_string(ctx)?.to_std_string_escaped();
                            return Ok(JsValue::from(create_trusted_script_obj(ctx, &script_str)));
                        }
                        Ok(JsValue::undefined())
                    })
                }
            } else {
                NativeFunction::from_copy_closure(|_this, args, ctx| {
                    let input = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    Ok(JsValue::from(create_trusted_script_obj(ctx, &input)))
                })
            };

            // createScriptURL method
            let url_fn = if let Some(cb) = create_script_url_cb.filter(|c| c.is_callable()) {
                unsafe {
                    NativeFunction::from_closure(move |_this, args, ctx| {
                        let input = args.get_or_undefined(0);
                        if let Some(callback) = cb.as_callable() {
                            let result = callback.call(&JsValue::undefined(), &[input.clone()], ctx)?;
                            let url_str = result.to_string(ctx)?.to_std_string_escaped();
                            return Ok(JsValue::from(create_trusted_script_url_obj(ctx, &url_str)));
                        }
                        Ok(JsValue::undefined())
                    })
                }
            } else {
                NativeFunction::from_copy_closure(|_this, args, ctx| {
                    let input = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    Ok(JsValue::from(create_trusted_script_url_obj(ctx, &input)))
                })
            };

            // Create the policy object
            let policy = ObjectInitializer::new(ctx)
                .property(js_string!("name"), js_string!(policy_name), Attribute::READONLY)
                .function(html_fn, js_string!("createHTML"), 1)
                .function(script_fn, js_string!("createScript"), 1)
                .function(url_fn, js_string!("createScriptURL"), 1)
                .build();

            Ok(JsValue::from(policy))
        });

        // isHTML - checks if value is TrustedHTML
        let is_html = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(obj) = args.get_or_undefined(0).as_object() {
                if let Ok(type_val) = obj.get(js_string!("_trustedType"), ctx) {
                    if let Ok(type_str) = type_val.to_string(ctx) {
                        if type_str.to_std_string_escaped() == "TrustedHTML" {
                            return Ok(JsValue::from(true));
                        }
                    }
                }
            }
            Ok(JsValue::from(false))
        });

        // isScript - checks if value is TrustedScript
        let is_script = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(obj) = args.get_or_undefined(0).as_object() {
                if let Ok(type_val) = obj.get(js_string!("_trustedType"), ctx) {
                    if let Ok(type_str) = type_val.to_string(ctx) {
                        if type_str.to_std_string_escaped() == "TrustedScript" {
                            return Ok(JsValue::from(true));
                        }
                    }
                }
            }
            Ok(JsValue::from(false))
        });

        // isScriptURL - checks if value is TrustedScriptURL
        let is_script_url = NativeFunction::from_copy_closure(|_this, args, ctx| {
            if let Some(obj) = args.get_or_undefined(0).as_object() {
                if let Ok(type_val) = obj.get(js_string!("_trustedType"), ctx) {
                    if let Ok(type_str) = type_val.to_string(ctx) {
                        if type_str.to_std_string_escaped() == "TrustedScriptURL" {
                            return Ok(JsValue::from(true));
                        }
                    }
                }
            }
            Ok(JsValue::from(false))
        });

        // getAttributeType - returns expected TrustedType for attribute
        let get_attribute_type = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let tag = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase();
            let attr = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped().to_lowercase();

            // Per Trusted Types spec - also handle innerHTML/outerHTML for convenience
            let trusted_type = match (tag.as_str(), attr.as_str()) {
                ("script", "src") => Some("TrustedScriptURL"),
                ("iframe", "srcdoc") => Some("TrustedHTML"),
                ("embed", "src") | ("object", "data") | ("object", "codebase") => Some("TrustedScriptURL"),
                (_, "onclick") | (_, "onload") | (_, "onerror") | (_, "onmouseover") |
                (_, "onkeydown") | (_, "onkeyup") | (_, "onsubmit") | (_, "onfocus") |
                (_, "onblur") | (_, "onchange") | (_, "oninput") => Some("TrustedScript"),
                // innerHTML/outerHTML are properties but often used as attributes
                (_, "innerhtml") | (_, "outerhtml") => Some("TrustedHTML"),
                _ => None,
            };

            match trusted_type {
                Some(t) => Ok(JsValue::from(js_string!(t))),
                None => Ok(JsValue::null()),
            }
        });

        // getPropertyType - returns expected TrustedType for property
        let get_property_type = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let tagname = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let prop = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();

            // Per Trusted Types spec - handle both interface names (HTMLScriptElement) and tag names (script)
            let is_script = tagname.eq_ignore_ascii_case("script") ||
                            tagname.eq_ignore_ascii_case("htmlscriptelement");
            let is_iframe = tagname.eq_ignore_ascii_case("iframe") ||
                            tagname.eq_ignore_ascii_case("htmliframeelement");

            let trusted_type = match prop.as_str() {
                "innerHTML" | "outerHTML" => Some("TrustedHTML"),
                "src" if is_script => Some("TrustedScriptURL"),
                "srcdoc" if is_iframe => Some("TrustedHTML"),
                "text" | "textContent" | "innerText" if is_script => Some("TrustedScript"),
                _ => None,
            };

            match trusted_type {
                Some(t) => Ok(JsValue::from(js_string!(t))),
                None => Ok(JsValue::null()),
            }
        });

        // emptyHTML - an empty TrustedHTML value
        let empty_html_obj = create_trusted_html_obj(context, "");

        // emptyScript - an empty TrustedScript value
        let empty_script_obj = create_trusted_script_obj(context, "");

        // Create the trustedTypes factory object (TrustedTypePolicyFactory instance)
        let trusted_types = ObjectInitializer::new(context)
            .function(create_policy, js_string!("createPolicy"), 2)
            .function(is_html, js_string!("isHTML"), 1)
            .function(is_script, js_string!("isScript"), 1)
            .function(is_script_url, js_string!("isScriptURL"), 1)
            .function(get_attribute_type, js_string!("getAttributeType"), 3)
            .function(get_property_type, js_string!("getPropertyType"), 3)
            .property(js_string!("emptyHTML"), JsValue::from(empty_html_obj), Attribute::READONLY)
            .property(js_string!("emptyScript"), JsValue::from(empty_script_obj), Attribute::READONLY)
            .property(js_string!("defaultPolicy"), JsValue::null(), Attribute::all())
            .build();

        context.global_object().set(js_string!("trustedTypes"), JsValue::from(trusted_types), false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register trustedTypes: {}", e)))?;

        // TrustedTypePolicyFactory constructor (throws - trustedTypes is the singleton)
        let trusted_type_policy_factory_constructor = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Err(JsNativeError::typ()
                .with_message("TrustedTypePolicyFactory cannot be constructed directly, use window.trustedTypes")
                .into())
        });
        let factory_ctor = FunctionObjectBuilder::new(context.realm(), trusted_type_policy_factory_constructor)
            .name(js_string!("TrustedTypePolicyFactory"))
            .length(0)
            .constructor(true)
            .build();
        context.global_object().set(js_string!("TrustedTypePolicyFactory"), factory_ctor, false, context)
            .map_err(|e| JsError::InitError(format!("Failed to register TrustedTypePolicyFactory: {}", e)))?;

        Ok(())
    }

    /// Initialize Error polyfills for V8-specific APIs
    /// React and other frameworks expect these to exist
    fn init_error_polyfills(context: &mut Context) -> Result<()> {
        // Get the Error constructor from global
        let global = context.global_object();

        // Add Error.captureStackTrace as a no-op function
        // V8-specific API used by React and other frameworks
        let capture_stack_trace = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            // No-op - just return undefined
            // In V8, this adds a 'stack' property to the target object
            // We'll handle stack traces differently
            Ok(JsValue::undefined())
        });

        if let Ok(error_val) = global.get(js_string!("Error"), context) {
            if let Some(error_obj) = error_val.as_object() {
                let _ = error_obj.set(
                    js_string!("captureStackTrace"),
                    capture_stack_trace.to_js_function(context.realm()),
                    false,
                    context
                );
            }
        }

        // Add a polyfill for Error.prototype.stack by running JS code
        // This creates a getter that returns a basic stack trace string
        let stack_polyfill = r#"
            if (!Error.prototype.hasOwnProperty('stack')) {
                Object.defineProperty(Error.prototype, 'stack', {
                    get: function() {
                        return this.name + ': ' + this.message + '\n    at <anonymous>';
                    },
                    configurable: true
                });
            }
        "#;
        let _ = context.eval(Source::from_bytes(stack_polyfill.as_bytes()));

        Ok(())
    }
}

// Base64 encoding
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let mut i = 0;

    while i < data.len() {
        let b0 = data[i] as usize;
        let b1 = if i + 1 < data.len() { data[i + 1] as usize } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] as usize } else { 0 };

        result.push(ALPHABET[(b0 >> 2) & 0x3F] as char);
        result.push(ALPHABET[((b0 << 4) | (b1 >> 4)) & 0x3F] as char);

        if i + 1 < data.len() {
            result.push(ALPHABET[((b1 << 2) | (b2 >> 6)) & 0x3F] as char);
        } else {
            result.push('=');
        }

        if i + 2 < data.len() {
            result.push(ALPHABET[b2 & 0x3F] as char);
        } else {
            result.push('=');
        }

        i += 3;
    }

    result
}

// Base64 decoding
fn base64_decode(data: &str) -> std::result::Result<Vec<u8>, ()> {
    const DECODE_TABLE: [i8; 256] = {
        let mut table = [-1i8; 256];
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut i = 0;
        while i < 64 {
            table[alphabet[i] as usize] = i as i8;
            i += 1;
        }
        table
    };

    let data_trimmed = data.trim_end_matches('=');
    let bytes: Vec<u8> = data_trimmed.bytes().collect();

    if bytes.is_empty() {
        return Ok(Vec::new());
    }

    let mut result = Vec::with_capacity((bytes.len() * 3) / 4 + 1);

    for chunk in bytes.chunks(4) {
        let b0 = DECODE_TABLE[chunk[0] as usize];
        if b0 < 0 { return Err(()); }

        let b1 = if chunk.len() > 1 {
            let v = DECODE_TABLE[chunk[1] as usize];
            if v < 0 { return Err(()); }
            v
        } else { 0 };

        let b2 = if chunk.len() > 2 {
            let v = DECODE_TABLE[chunk[2] as usize];
            if v < 0 { return Err(()); }
            v
        } else { 0 };

        let b3 = if chunk.len() > 3 {
            let v = DECODE_TABLE[chunk[3] as usize];
            if v < 0 { return Err(()); }
            v
        } else { 0 };

        result.push(((b0 as u8) << 2) | ((b1 as u8) >> 4));
        if chunk.len() > 2 {
            result.push(((b1 as u8) << 4) | ((b2 as u8) >> 2));
        }
        if chunk.len() > 3 {
            result.push(((b2 as u8) << 6) | (b3 as u8));
        }
    }

    Ok(result)
}

/// Register FinalizationRegistry constructor
fn register_finalization_registry(context: &mut Context) -> JsResult<()> {
    use std::cell::RefCell;
    use std::rc::Rc;

    let finalization_registry_constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let callback = args.get_or_undefined(0).clone();

        // FinalizationRegistry requires a callback function
        if !callback.is_callable() {
            return Err(JsNativeError::typ()
                .with_message("FinalizationRegistry: callback must be callable")
                .into());
        }

        // Store registrations in a RefCell wrapped in Rc for sharing between closures
        // Each entry: (target_id, held_value, unregister_token)
        let entries: Rc<RefCell<Vec<(u64, JsValue, Option<JsValue>)>>> = Rc::new(RefCell::new(Vec::new()));
        let next_id = Rc::new(RefCell::new(1u64));

        // register(target, heldValue, unregisterToken?)
        let reg_entries = entries.clone();
        let reg_next_id = next_id.clone();
        let register = unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                let target = args.get_or_undefined(0);
                let held_value = args.get_or_undefined(1).clone();
                let unregister_token = if args.len() > 2 && !args.get_or_undefined(2).is_undefined() {
                    Some(args.get_or_undefined(2).clone())
                } else {
                    None
                };

                // Target must be an object or symbol
                if !target.is_object() && !target.is_symbol() {
                    return Err(JsNativeError::typ()
                        .with_message("FinalizationRegistry.register: target must be an object or symbol")
                        .into());
                }

                // heldValue cannot be the same as target
                if target == &held_value {
                    return Err(JsNativeError::typ()
                        .with_message("FinalizationRegistry.register: heldValue cannot be the same as target")
                        .into());
                }

                // Register the entry
                let target_id = {
                    let mut id = reg_next_id.borrow_mut();
                    let current = *id;
                    *id += 1;
                    current
                };
                reg_entries.borrow_mut().push((target_id, held_value, unregister_token));

                Ok(JsValue::undefined())
            })
        };

        // unregister(unregisterToken) -> boolean
        let unreg_entries = entries.clone();
        let unregister = unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                let token = args.get_or_undefined(0);

                // Token must be an object or symbol
                if !token.is_object() && !token.is_symbol() {
                    return Err(JsNativeError::typ()
                        .with_message("FinalizationRegistry.unregister: unregisterToken must be an object or symbol")
                        .into());
                }

                let mut entries = unreg_entries.borrow_mut();
                let initial_len = entries.len();
                entries.retain(|(_, _, unreg_token)| {
                    if let Some(ref t) = unreg_token {
                        if t == token {
                            return false; // Remove this entry
                        }
                    }
                    true // Keep this entry
                });
                let removed = entries.len() < initial_len;

                Ok(JsValue::from(removed))
            })
        };

        // cleanupSome(callback?) - allows manual cleanup iteration
        let cleanup_entries = entries.clone();
        let cleanup_callback = callback.clone();
        let cleanup_some = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                // Use provided callback or the one from constructor
                let cb = if args.len() > 0 && args.get_or_undefined(0).is_callable() {
                    args.get_or_undefined(0).clone()
                } else {
                    cleanup_callback.clone()
                };

                // Get held values for cleanup
                let held_values: Vec<JsValue> = cleanup_entries.borrow()
                    .iter()
                    .map(|(_, hv, _)| hv.clone())
                    .collect();

                // Call callback for each held value (simulating cleanup)
                if let Some(callable) = cb.as_callable() {
                    for held_value in held_values.iter().take(1) {
                        let _ = callable.call(&JsValue::undefined(), &[held_value.clone()], ctx);
                    }
                }

                Ok(JsValue::undefined())
            })
        };

        let registry = ObjectInitializer::new(ctx)
            .function(register, js_string!("register"), 3)
            .function(unregister, js_string!("unregister"), 1)
            .function(cleanup_some, js_string!("cleanupSome"), 1)
            .build();

        Ok(JsValue::from(registry))
    });

    let finalization_registry_ctor = FunctionObjectBuilder::new(context.realm(), finalization_registry_constructor)
        .name(js_string!("FinalizationRegistry"))
        .length(1)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("FinalizationRegistry"), finalization_registry_ctor, false, context)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_dom() -> Rc<Dom> {
        Rc::new(
            Dom::parse("<html><head><title>Test</title></head><body><div id='main'>Hello</div></body></html>")
                .unwrap(),
        )
    }

    #[test]
    fn test_console_log() {
        let dom = create_test_dom();
        let mut runtime = JsRuntime::new(dom).unwrap();
        runtime.execute("console.log('hello', 'world');").unwrap();
        let output = runtime.console_output();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], "hello world");
    }

    #[test]
    fn test_document_get_element_by_id() {
        let dom = create_test_dom();
        let mut runtime = JsRuntime::new(dom).unwrap();
        runtime
            .execute("var el = document.getElementById('main'); console.log(el.tagName);")
            .unwrap();
        let output = runtime.console_output();
        assert_eq!(output[0], "DIV");
    }

    #[test]
    fn test_document_query_selector() {
        let dom = create_test_dom();
        let mut runtime = JsRuntime::new(dom).unwrap();
        runtime
            .execute("var el = document.querySelector('div'); console.log(el ? el.id : 'null');")
            .unwrap();
        let output = runtime.console_output();
        assert_eq!(output[0], "main");
    }

    #[test]
    fn test_btoa_atob() {
        let dom = create_test_dom();
        let mut runtime = JsRuntime::new(dom).unwrap();
        runtime.execute("console.log(btoa('hello'));").unwrap();
        runtime.execute("console.log(atob('aGVsbG8='));").unwrap();
        let output = runtime.console_output();
        assert_eq!(output[0], "aGVsbG8=");
        assert_eq!(output[1], "hello");
    }

    #[test]
    fn test_create_element() {
        let dom = create_test_dom();
        let mut runtime = JsRuntime::new(dom).unwrap();
        runtime
            .execute(
                r#"
                var div = document.createElement('div');
                div.id = 'new';
                div.className = 'test-class';
                console.log(div.tagName);
                console.log(div.id);
                console.log(div.className);
            "#,
            )
            .unwrap();
        let output = runtime.console_output();
        assert_eq!(output[0], "DIV");
        assert_eq!(output[1], "new");
        assert_eq!(output[2], "test-class");
    }

    #[test]
    fn test_queue_microtask() {
        // Reset job queue to avoid state from previous tests
        timers::reset_job_queue();

        let dom = create_test_dom();
        let mut runtime = JsRuntime::new(dom).unwrap();

        // Test that queueMicrotask exists and the callback eventually runs
        runtime
            .execute(
                r#"
                console.log('1');
                queueMicrotask(function() {
                    console.log('microtask');
                });
                console.log('2');
            "#,
            )
            .unwrap();

        // Run event loop to ensure all tasks complete
        runtime.run_event_loop_tick();

        let output = runtime.console_output();
        // All three should have run (order may vary due to Boa internals)
        assert_eq!(output.len(), 3, "Expected 3 outputs, got: {:?}", output);
        assert!(output.contains(&"1".to_string()));
        assert!(output.contains(&"2".to_string()));
        assert!(output.contains(&"microtask".to_string()));
    }

    #[test]
    fn test_set_immediate() {
        // Reset job queue to avoid state from previous tests
        timers::reset_job_queue();

        let dom = create_test_dom();
        let mut runtime = JsRuntime::new(dom).unwrap();
        runtime
            .execute(
                r#"
                console.log('1');
                setImmediate(function() {
                    console.log('immediate');
                });
                console.log('2');
            "#,
            )
            .unwrap();

        // Before event loop, only sync code ran
        let output_before = runtime.console_output();
        assert_eq!(output_before.len(), 2, "Before event loop: {:?}", output_before);

        // Run event loop to process immediate tasks
        runtime.run_event_loop_tick();

        let output = runtime.console_output();
        assert_eq!(output.len(), 3, "After event loop: {:?}", output);
        assert_eq!(output[0], "1");
        assert_eq!(output[1], "2");
        assert_eq!(output[2], "immediate");
    }

    #[test]
    fn test_event_loop_methods() {
        let dom = create_test_dom();
        let runtime = JsRuntime::new(dom).unwrap();
        // Initially no pending tasks
        assert!(!runtime.has_pending_tasks());
    }

    #[test]
    fn test_weakref() {
        let dom = create_test_dom();
        let mut runtime = JsRuntime::new(dom).unwrap();
        runtime
            .execute(
                r#"
                var obj = { name: 'test' };
                var ref = new WeakRef(obj);
                var deref = ref.deref();
                console.log(deref.name);
                "#,
            )
            .unwrap();
        let output = runtime.console_output();
        assert_eq!(output[0], "test");
    }

    #[test]
    fn test_weakref_validation() {
        let dom = create_test_dom();
        let mut runtime = JsRuntime::new(dom).unwrap();
        let result = runtime.execute("new WeakRef(123);");
        assert!(result.is_err(), "WeakRef should reject primitive values");
    }

    #[test]
    fn test_finalization_registry() {
        let dom = create_test_dom();
        let mut runtime = JsRuntime::new(dom).unwrap();
        runtime
            .execute(
                r#"
                var called = false;
                var registry = new FinalizationRegistry(function(heldValue) {
                    called = true;
                    console.log('cleanup: ' + heldValue);
                });
                var obj = {};
                var token = {};
                registry.register(obj, 'myValue', token);
                var removed = registry.unregister(token);
                console.log('removed: ' + removed);
                "#,
            )
            .unwrap();
        let output = runtime.console_output();
        assert_eq!(output[0], "removed: true");
    }

    #[test]
    fn test_finalization_registry_validation() {
        let dom = create_test_dom();
        let mut runtime = JsRuntime::new(dom).unwrap();
        let result = runtime.execute("new FinalizationRegistry('not a function');");
        assert!(result.is_err(), "FinalizationRegistry should require a callback");
    }
}

#[cfg(test)]
mod es_feature_tests {
    use super::*;
    use boa_engine::Context;

    #[test]
    fn test_proxy_support() {
        let mut ctx = Context::default();
        let result = ctx.eval(Source::from_bytes(b"
            const target = { value: 42 };
            const handler = {
                get: function(obj, prop) {
                    return prop in obj ? obj[prop] : 'not found';
                }
            };
            const proxy = new Proxy(target, handler);
            [proxy.value, proxy.other].join(',');
        "));
        assert!(result.is_ok(), "Proxy should work: {:?}", result);
        let val = result.unwrap().to_string(&mut ctx).unwrap().to_std_string_escaped();
        assert_eq!(val, "42,not found");
    }

    #[test]
    fn test_reflect_support() {
        let mut ctx = Context::default();
        let result = ctx.eval(Source::from_bytes(b"
            const obj = { x: 1 };
            Reflect.set(obj, 'y', 2);
            [Reflect.get(obj, 'x'), obj.y].join(',');
        "));
        assert!(result.is_ok(), "Reflect should work: {:?}", result);
        let val = result.unwrap().to_string(&mut ctx).unwrap().to_std_string_escaped();
        assert_eq!(val, "1,2");
    }

    #[test]
    fn test_symbol_support() {
        let mut ctx = Context::default();
        let result = ctx.eval(Source::from_bytes(b"
            const sym = Symbol('test');
            typeof sym;
        "));
        assert!(result.is_ok(), "Symbol should work");
        let val = result.unwrap().to_string(&mut ctx).unwrap().to_std_string_escaped();
        assert_eq!(val, "symbol");
    }

    #[test]
    fn test_async_await() {
        let mut ctx = Context::default();
        let result = ctx.eval(Source::from_bytes(b"
            async function test() { return 42; }
            typeof test;
        "));
        assert!(result.is_ok(), "async/await should work");
    }
}
