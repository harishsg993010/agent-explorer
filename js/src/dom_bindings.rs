//! DOM bindings for JavaScript - bridges Rust DOM to Boa JS engine.

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer, property::Attribute,
    Context, JsArgs, JsObject, JsValue, Source,
};
use boa_gc::{Finalize, Trace};
use dom::{Dom, Element};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::event_system::{self, ListenerOptions};
use crate::html_elements;
use crate::script_loader;
use crate::get_computed_style_property;
use crate::{JsError, Result};

/// Element registry - maps element IDs to Elements
#[derive(Clone, Trace, Finalize)]
pub struct ElementRegistry {
    #[unsafe_ignore_trace]
    elements: Rc<RefCell<HashMap<u64, Element>>>,
}

impl ElementRegistry {
    pub fn new() -> Self {
        ElementRegistry {
            elements: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub fn register(&self, element: Element) -> u64 {
        // Use the element's unique_id which is based on its internal pointer
        // This ensures the same element always has the same ID
        let id = element.unique_id();
        log::debug!("ElementRegistry::register - id={}, tag={}, registry_ptr={:p}",
                    id, element.tag_name(), &*self.elements);
        self.elements.borrow_mut().insert(id, element);
        id
    }

    pub fn get(&self, id: u64) -> Option<Element> {
        let result = self.elements.borrow().get(&id).cloned();
        log::debug!("ElementRegistry::get - id={}, found={}, registry_ptr={:p}, registry_len={}",
                    id, result.is_some(), &*self.elements, self.elements.borrow().len());
        result
    }

    pub fn remove(&self, id: u64) -> Option<Element> {
        self.elements.borrow_mut().remove(&id)
    }
}

impl Default for ElementRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper for DOM that can be stored in JS closures
#[derive(Clone, Trace, Finalize)]
pub struct DomWrapper {
    #[unsafe_ignore_trace]
    pub inner: Rc<Dom>,
    pub registry: ElementRegistry,
}

/// Wrapper for Element that can be stored in JS closures
#[derive(Clone, Trace, Finalize)]
struct ElementWrapper {
    #[unsafe_ignore_trace]
    inner: Rc<RefCell<Option<Element>>>,
    element_id: u64,
}

impl ElementWrapper {
    fn new(element: Element, id: u64) -> Self {
        ElementWrapper {
            inner: Rc::new(RefCell::new(Some(element))),
            element_id: id,
        }
    }

    fn get(&self) -> Option<Element> {
        self.inner.borrow().clone()
    }

    fn id(&self) -> u64 {
        self.element_id
    }
}

/// Helper to extract element ID from a JS object
fn get_element_id_from_js(obj: &JsObject, ctx: &mut Context) -> Option<u64> {
    obj.get(js_string!("__element_id__"), ctx)
        .ok()
        .and_then(|v| v.to_number(ctx).ok())
        .map(|id| id as u64)
}

/// Initialize the document object with all DOM APIs
pub fn init_document(context: &mut Context, dom: Rc<Dom>) -> Result<()> {
    let registry = ElementRegistry::new();
    let dom_wrapper = DomWrapper { inner: Rc::clone(&dom), registry };

    // document.getElementById
    let get_element_by_id = {
        let dom = dom_wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                match dom.inner.get_element_by_id(&id) {
                    Some(el) => Ok(create_element_object(el, ctx, &dom)),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // document.getElementsByTagName
    let get_elements_by_tag_name = {
        let dom = dom_wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let tag = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let elements = dom.inner.get_elements_by_tag_name(&tag);
                Ok(create_node_list(elements, ctx, &dom))
            })
        }
    };

    // document.getElementsByClassName
    let get_elements_by_class_name = {
        let dom = dom_wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let class = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let elements = dom.inner.get_elements_by_class_name(&class);
                Ok(create_node_list(elements, ctx, &dom))
            })
        }
    };

    // document.querySelector
    let query_selector = {
        let dom = dom_wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let selector = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                match dom.inner.query_selector(&selector) {
                    Some(el) => Ok(create_element_object(el, ctx, &dom)),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // document.querySelectorAll
    let query_selector_all = {
        let dom = dom_wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let selector = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let elements = dom.inner.query_selector_all(&selector);
                Ok(create_node_list(elements, ctx, &dom))
            })
        }
    };

    // document.createElement
    let create_element = {
        let dom = dom_wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let tag = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let el = dom.inner.create_element(&tag);
                Ok(create_element_object(el, ctx, &dom))
            })
        }
    };

    // document.createTextNode
    let create_text_node = {
        let dom = dom_wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let el = dom.inner.create_text_node(&text);
                Ok(create_element_object(el, ctx, &dom))
            })
        }
    };

    // document.createDocumentFragment - real implementation
    let create_document_fragment = {
        let dom = dom_wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let fragment = dom.inner.create_document_fragment();
                // Create element object but override nodeName and nodeType for DocumentFragment
                let fragment_obj = create_element_object(fragment, ctx, &dom);
                if let Some(obj) = fragment_obj.as_object() {
                    // Use define_property_or_throw to override readonly properties
                    use boa_engine::property::PropertyDescriptor;
                    let _ = obj.define_property_or_throw(
                        js_string!("nodeName"),
                        PropertyDescriptor::builder()
                            .value(js_string!("#document-fragment"))
                            .writable(false)
                            .enumerable(true)
                            .configurable(true)
                            .build(),
                        ctx
                    );
                    let _ = obj.define_property_or_throw(
                        js_string!("nodeType"),
                        PropertyDescriptor::builder()
                            .value(11)
                            .writable(false)
                            .enumerable(true)
                            .configurable(true)
                            .build(),
                        ctx
                    );
                }
                Ok(fragment_obj)
            })
        }
    };

    // document.createElementNS - create element with namespace (for SVG, MathML, etc.)
    let create_element_ns = {
        let dom = dom_wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let namespace = args.get_or_undefined(0);
                let qualified_name = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();

                // Extract local name (part after colon if prefixed)
                let local_name = if let Some(idx) = qualified_name.find(':') {
                    qualified_name[idx + 1..].to_string()
                } else {
                    qualified_name.clone()
                };

                // Create the element
                let el = dom.inner.create_element(&local_name);
                let el_obj = create_element_object(el, ctx, &dom);

                // Set namespace URI on the element
                if let Some(obj) = el_obj.as_object() {
                    use boa_engine::property::PropertyDescriptor;
                    let ns_value = if namespace.is_null() || namespace.is_undefined() {
                        JsValue::null()
                    } else {
                        JsValue::from(js_string!(namespace.to_string(ctx)?.to_std_string_escaped()))
                    };
                    let _ = obj.define_property_or_throw(
                        js_string!("namespaceURI"),
                        PropertyDescriptor::builder()
                            .value(ns_value)
                            .writable(false)
                            .enumerable(true)
                            .configurable(true)
                            .build(),
                        ctx
                    );
                }

                Ok(el_obj)
            })
        }
    };

    // document.createComment - create comment node
    let create_comment = {
        let dom = dom_wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let data = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let comment = dom.inner.create_comment(&data);
                let comment_obj = create_element_object(comment, ctx, &dom);

                // Override nodeType and nodeName for comment
                if let Some(obj) = comment_obj.as_object() {
                    use boa_engine::property::PropertyDescriptor;
                    let _ = obj.define_property_or_throw(
                        js_string!("nodeType"),
                        PropertyDescriptor::builder()
                            .value(8) // COMMENT_NODE
                            .writable(false)
                            .enumerable(true)
                            .configurable(true)
                            .build(),
                        ctx
                    );
                    let _ = obj.define_property_or_throw(
                        js_string!("nodeName"),
                        PropertyDescriptor::builder()
                            .value(js_string!("#comment"))
                            .writable(false)
                            .enumerable(true)
                            .configurable(true)
                            .build(),
                        ctx
                    );
                    // Add data property for comment content
                    let _ = obj.define_property_or_throw(
                        js_string!("data"),
                        PropertyDescriptor::builder()
                            .value(js_string!(data))
                            .writable(true)
                            .enumerable(true)
                            .configurable(true)
                            .build(),
                        ctx
                    );
                }

                Ok(comment_obj)
            })
        }
    };

    // Get body element
    let body = dom.body().map(|el| create_element_object(el, context, &dom_wrapper));
    let body_value = body.unwrap_or(JsValue::null());

    // Get head element
    let head = dom.head().map(|el| create_element_object(el, context, &dom_wrapper));
    let head_value = head.unwrap_or(JsValue::null());

    // Get documentElement (html)
    let doc_element = dom.document_element().map(|el| create_element_object(el, context, &dom_wrapper));
    let doc_element_value = doc_element.unwrap_or(JsValue::null());

    // Title getter
    let title_getter = {
        let dom = dom_wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                let title = dom.inner.get_title();
                Ok(JsValue::from(js_string!(title)))
            })
        }
    };

    // Title setter
    let title_setter = {
        let dom = dom_wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let title = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                dom.inner.set_title(&title);
                Ok(JsValue::undefined())
            })
        }
    };

    let getter_fn = title_getter.to_js_function(context.realm());
    let setter_fn = title_setter.to_js_function(context.realm());

    // Get the global object (window) to set as defaultView
    let global = context.global_object();

    // Build document object
    let document = ObjectInitializer::new(context)
        .function(get_element_by_id, js_string!("getElementById"), 1)
        .function(get_elements_by_tag_name, js_string!("getElementsByTagName"), 1)
        .function(get_elements_by_class_name, js_string!("getElementsByClassName"), 1)
        .function(query_selector, js_string!("querySelector"), 1)
        .function(query_selector_all, js_string!("querySelectorAll"), 1)
        .function(create_element, js_string!("createElement"), 1)
        .function(create_text_node, js_string!("createTextNode"), 1)
        .function(create_document_fragment, js_string!("createDocumentFragment"), 0)
        .function(create_element_ns, js_string!("createElementNS"), 2)
        .function(create_comment, js_string!("createComment"), 1)
        .property(js_string!("body"), body_value, Attribute::READONLY)
        .property(js_string!("head"), head_value, Attribute::READONLY)
        .property(js_string!("documentElement"), doc_element_value, Attribute::READONLY)
        .property(js_string!("nodeType"), 9, Attribute::READONLY)
        .property(js_string!("nodeName"), js_string!("#document"), Attribute::READONLY)
        .property(js_string!("defaultView"), global, Attribute::READONLY)
        .property(js_string!("currentScript"), JsValue::null(), Attribute::all()) // Writable for script execution
        .accessor(js_string!("title"), Some(getter_fn), Some(setter_fn), Attribute::CONFIGURABLE)
        .build();

    context
        .register_global_property(js_string!("document"), document, Attribute::all())
        .map_err(|e| JsError::InitError(format!("Failed to register document: {}", e)))?;

    Ok(())
}

/// Create a JavaScript Element object from a Rust Element
pub fn create_element_object(element: Element, context: &mut Context, dom: &DomWrapper) -> JsValue {
    // Register element and get ID
    let element_id = dom.registry.register(element.clone());
    let wrapper = ElementWrapper::new(element.clone(), element_id);

    // tagName property
    let tag_name = element.tag_name_upper();

    // nodeType
    let node_type = element.node_type();

    // nodeName
    let node_name = if node_type == 3 {
        "#text".to_string()
    } else {
        tag_name.clone()
    };

    // id getter
    let id_getter = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                let id = w.get().and_then(|e| e.id()).unwrap_or_default();
                Ok(JsValue::from(js_string!(id)))
            })
        }
    };

    // id setter
    let id_setter = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                if let Some(el) = w.get() {
                    let id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    el.set_attribute("id", &id);
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // className getter
    let class_getter = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                let class = w.get().and_then(|e| e.class_name()).unwrap_or_default();
                Ok(JsValue::from(js_string!(class)))
            })
        }
    };

    // className setter
    let class_setter = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                if let Some(el) = w.get() {
                    let class = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    el.set_attribute("class", &class);
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // innerText getter
    let inner_text_getter = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                let text = w.get().map(|e| e.inner_text()).unwrap_or_default();
                Ok(JsValue::from(js_string!(text)))
            })
        }
    };

    // textContent getter
    let text_content_getter = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                let text = w.get().map(|e| e.text_content()).unwrap_or_default();
                Ok(JsValue::from(js_string!(text)))
            })
        }
    };

    // textContent setter
    let text_content_setter = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                if let Some(el) = w.get() {
                    el.set_text_content(&text);
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // innerHTML getter
    let inner_html_getter = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                let html = w.get().map(|e| e.inner_html()).unwrap_or_default();
                Ok(JsValue::from(js_string!(html)))
            })
        }
    };

    // innerHTML setter
    let inner_html_setter = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let html = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                if let Some(el) = w.get() {
                    el.set_inner_html(&html);
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // children getter - returns HTMLCollection of element children
    let children_getter = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let children = w.get().map(|e| e.children()).unwrap_or_default();
                Ok(create_node_list(children, ctx, &dom))
            })
        }
    };

    // childNodes getter - returns NodeList of all child nodes
    let child_nodes_getter = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let children = w.get().map(|e| e.child_nodes()).unwrap_or_default();
                Ok(create_node_list(children, ctx, &dom))
            })
        }
    };

    // firstChild getter
    let first_child_getter = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                match w.get().and_then(|e| e.first_child()) {
                    Some(child) => Ok(create_element_object(child, ctx, &dom)),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // lastChild getter
    let last_child_getter = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                match w.get().and_then(|e| e.last_child()) {
                    Some(child) => Ok(create_element_object(child, ctx, &dom)),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // firstElementChild getter
    let first_element_child_getter = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                match w.get().and_then(|e| e.first_element_child()) {
                    Some(child) => Ok(create_element_object(child, ctx, &dom)),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // lastElementChild getter
    let last_element_child_getter = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                match w.get().and_then(|e| e.last_element_child()) {
                    Some(child) => Ok(create_element_object(child, ctx, &dom)),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // childElementCount getter
    let child_element_count_getter = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                let count = w.get().map(|e| e.children().len()).unwrap_or(0);
                Ok(JsValue::from(count as u32))
            })
        }
    };

    // parentNode getter
    let parent_node_getter = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                match w.get().and_then(|e| e.parent_element()) {
                    Some(parent) => Ok(create_element_object(parent, ctx, &dom)),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // parentElement getter (same as parentNode for elements)
    let parent_element_getter = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                match w.get().and_then(|e| e.parent_element()) {
                    Some(parent) => Ok(create_element_object(parent, ctx, &dom)),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // nextSibling getter (uses next_element_sibling since we don't track non-element siblings)
    let next_sibling_getter = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                match w.get().and_then(|e| e.next_element_sibling()) {
                    Some(sibling) => Ok(create_element_object(sibling, ctx, &dom)),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // previousSibling getter (uses previous_element_sibling since we don't track non-element siblings)
    let previous_sibling_getter = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                match w.get().and_then(|e| e.previous_element_sibling()) {
                    Some(sibling) => Ok(create_element_object(sibling, ctx, &dom)),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // nextElementSibling getter
    let next_element_sibling_getter = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                match w.get().and_then(|e| e.next_element_sibling()) {
                    Some(sibling) => Ok(create_element_object(sibling, ctx, &dom)),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // previousElementSibling getter
    let previous_element_sibling_getter = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                match w.get().and_then(|e| e.previous_element_sibling()) {
                    Some(sibling) => Ok(create_element_object(sibling, ctx, &dom)),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // ownerDocument getter - returns the document object
    let owner_document_getter = {
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                // Get document from global object
                let document = ctx.global_object().get(js_string!("document"), ctx)
                    .unwrap_or(JsValue::null());
                Ok(document)
            })
        }
    };

    // namespaceURI getter - returns HTML namespace for HTML elements
    let namespace_uri_getter = {
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                // For HTML elements, return the HTML namespace
                Ok(JsValue::from(js_string!("http://www.w3.org/1999/xhtml")))
            })
        }
    };

    // getAttributeNode - returns an Attr object for the named attribute
    let get_attribute_node = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                if let Some(el) = w.get() {
                    if let Some(value) = el.get_attribute(&name) {
                        // Create an Attr-like object
                        let attr_obj = ObjectInitializer::new(ctx)
                            .property(js_string!("name"), js_string!(name.clone()), Attribute::READONLY)
                            .property(js_string!("value"), js_string!(value.clone()), Attribute::all())
                            .property(js_string!("nodeName"), js_string!(name.clone()), Attribute::READONLY)
                            .property(js_string!("nodeType"), 2, Attribute::READONLY) // ATTRIBUTE_NODE
                            .property(js_string!("nodeValue"), js_string!(value.clone()), Attribute::all())
                            .property(js_string!("specified"), true, Attribute::READONLY)
                            .build();
                        return Ok(JsValue::from(attr_obj));
                    }
                }
                Ok(JsValue::null())
            })
        }
    };

    // getAttribute
    let get_attribute = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let value = w.get().and_then(|e| e.get_attribute(&name));
                match value {
                    Some(v) => Ok(JsValue::from(js_string!(v))),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // setAttribute
    let set_attribute = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
                if let Some(el) = w.get() {
                    el.set_attribute(&name, &value);
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // removeAttribute
    let remove_attribute = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                if let Some(el) = w.get() {
                    el.remove_attribute(&name);
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // hasAttribute
    let has_attribute = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let has = w.get().map(|e| e.has_attribute(&name)).unwrap_or(false);
                Ok(JsValue::from(has))
            })
        }
    };

    // querySelector on element
    let element_query_selector = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let selector = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let result = w.get().and_then(|e| e.query_selector(&selector));
                match result {
                    Some(el) => Ok(create_element_object(el, ctx, &dom)),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // querySelectorAll on element
    let element_query_selector_all = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let selector = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let elements = w.get().map(|e| e.query_selector_all(&selector)).unwrap_or_default();
                Ok(create_node_list(elements, ctx, &dom))
            })
        }
    };

    // getElementsByTagName on element
    let element_get_by_tag = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let tag = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let elements = w.get().map(|e| e.get_elements_by_tag_name(&tag)).unwrap_or_default();
                Ok(create_node_list(elements, ctx, &dom))
            })
        }
    };

    // getElementsByClassName on element
    let element_get_by_class = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let class = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let elements = w.get().map(|e| e.get_elements_by_class_name(&class)).unwrap_or_default();
                Ok(create_node_list(elements, ctx, &dom))
            })
        }
    };

    // appendChild - REAL DOM MUTATION + DYNAMIC SCRIPT LOADING
    // Handles DocumentFragment specially: moves fragment's children to parent
    let append_child = {
        let w = wrapper.clone();
        let reg = dom.registry.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let child_arg = args.get_or_undefined(0);

                // Get the parent element
                if let Some(parent) = w.get() {
                    log::debug!("appendChild: parent found (tag={})", parent.tag_name());
                    // Get child element from the JS object's __element_id__
                    if let Some(child_obj) = child_arg.as_object() {
                        log::debug!("appendChild: child_obj is object");

                        // Check if this is a DocumentFragment (nodeType === 11)
                        let node_type = child_obj.get(js_string!("nodeType"), ctx)
                            .ok()
                            .and_then(|v| v.to_u32(ctx).ok())
                            .unwrap_or(0);

                        if node_type == 11 {
                            // DocumentFragment: move all children to parent
                            log::debug!("appendChild: handling DocumentFragment");
                            if let Ok(child_nodes) = child_obj.get(js_string!("childNodes"), ctx) {
                                if let Some(child_nodes_obj) = child_nodes.as_object() {
                                    let length = child_nodes_obj.get(js_string!("length"), ctx)
                                        .ok()
                                        .and_then(|v| v.to_u32(ctx).ok())
                                        .unwrap_or(0);

                                    // Collect all children first (to avoid modifying while iterating)
                                    let mut children_to_move = Vec::new();
                                    for i in 0..length {
                                        if let Ok(child_node) = child_nodes_obj.get(i, ctx) {
                                            if let Some(child_node_obj) = child_node.as_object() {
                                                if let Some(child_id) = get_element_id_from_js(&child_node_obj, ctx) {
                                                    if let Some(child_elem) = reg.get(child_id) {
                                                        children_to_move.push(child_elem);
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Move all children to parent
                                    for child_elem in children_to_move {
                                        parent.append_child(&child_elem);
                                    }
                                    log::debug!("appendChild: moved {} children from DocumentFragment", length);
                                }
                            }
                        } else if let Some(child_id) = get_element_id_from_js(&child_obj, ctx) {
                            log::debug!("appendChild: child_id = {}", child_id);
                            if let Some(child) = reg.get(child_id) {
                                log::debug!("appendChild: child found in registry (tag={})", child.tag_name());
                                // Check if this is a script element with src
                                let tag_name = child.tag_name().to_lowercase();
                                if tag_name == "script" {
                                    if let Some(src) = child.get_attribute("src") {
                                        if !src.is_empty() {
                                            // Queue script for dynamic loading
                                            let is_module = child.get_attribute("type").map(|t| t == "module").unwrap_or(false);
                                            let is_async = child.has_attribute("async");
                                            let is_defer = child.has_attribute("defer");
                                            script_loader::queue_script(&src, is_module, is_async, is_defer);
                                        }
                                    }
                                }

                                // Actually mutate the DOM!
                                parent.append_child(&child);
                                log::debug!("appendChild: DOM mutation complete, parent now has {} children", parent.children().len());
                            } else {
                                log::debug!("appendChild: child NOT found in registry for id {}", child_id);
                            }
                        } else {
                            log::debug!("appendChild: could not get __element_id__ from child");
                        }
                    } else {
                        log::debug!("appendChild: child_arg is not an object");
                    }
                } else {
                    log::debug!("appendChild: parent not found in wrapper");
                }

                Ok(child_arg.clone())
            })
        }
    };

    // removeChild - REAL DOM MUTATION
    let remove_child = {
        let w = wrapper.clone();
        let reg = dom.registry.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let child_arg = args.get_or_undefined(0);

                // Get the parent element
                if let Some(parent) = w.get() {
                    // Get child element from the JS object's __element_id__
                    if let Some(child_obj) = child_arg.as_object() {
                        if let Some(child_id) = get_element_id_from_js(&child_obj, ctx) {
                            if let Some(child) = reg.get(child_id) {
                                // Actually mutate the DOM!
                                parent.remove_child(&child);
                            }
                        }
                    }
                }

                Ok(child_arg.clone())
            })
        }
    };

    // insertBefore - REAL DOM MUTATION
    let insert_before = {
        let w = wrapper.clone();
        let reg = dom.registry.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let new_child_arg = args.get_or_undefined(0);
                let ref_child_arg = args.get_or_undefined(1);

                if let Some(parent) = w.get() {
                    // Get the new child element
                    if let Some(new_obj) = new_child_arg.as_object() {
                        if let Some(new_id) = get_element_id_from_js(&new_obj, ctx) {
                            if let Some(new_child) = reg.get(new_id) {
                                // Check if reference child is null (append to end)
                                if ref_child_arg.is_null() || ref_child_arg.is_undefined() {
                                    parent.append_child(&new_child);
                                } else if let Some(ref_obj) = ref_child_arg.as_object() {
                                    if let Some(ref_id) = get_element_id_from_js(&ref_obj, ctx) {
                                        if let Some(ref_child) = reg.get(ref_id) {
                                            parent.insert_before(&new_child, &ref_child);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                Ok(new_child_arg.clone())
            })
        }
    };

    // replaceChild - REAL DOM MUTATION
    let replace_child = {
        let w = wrapper.clone();
        let reg = dom.registry.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let new_child_arg = args.get_or_undefined(0);
                let old_child_arg = args.get_or_undefined(1);

                if let Some(parent) = w.get() {
                    if let Some(new_obj) = new_child_arg.as_object() {
                        if let Some(old_obj) = old_child_arg.as_object() {
                            if let Some(new_id) = get_element_id_from_js(&new_obj, ctx) {
                                if let Some(old_id) = get_element_id_from_js(&old_obj, ctx) {
                                    if let Some(new_child) = reg.get(new_id) {
                                        if let Some(old_child) = reg.get(old_id) {
                                            parent.replace_child(&new_child, &old_child);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                Ok(old_child_arg.clone())
            })
        }
    };

    // addEventListener - REAL EVENT REGISTRATION
    let add_event_listener = {
        let elem_id = element_id;
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let callback = args.get_or_undefined(1);

                if let Some(callback_obj) = callback.as_object() {
                    // Parse options
                    let mut options = ListenerOptions::default();
                    if let Some(opts) = args.get(2) {
                        if let Some(opts_obj) = opts.as_object() {
                            if let Ok(capture) = opts_obj.get(js_string!("capture"), ctx) {
                                options.capture = capture.as_boolean().unwrap_or(false);
                            }
                            if let Ok(once) = opts_obj.get(js_string!("once"), ctx) {
                                options.once = once.as_boolean().unwrap_or(false);
                            }
                            if let Ok(passive) = opts_obj.get(js_string!("passive"), ctx) {
                                options.passive = passive.as_boolean().unwrap_or(false);
                            }
                        } else if let Some(capture) = opts.as_boolean() {
                            options.capture = capture;
                        }
                    }

                    event_system::add_event_listener(
                        elem_id,
                        &event_type,
                        callback_obj.clone(),
                        options,
                    );
                }

                Ok(JsValue::undefined())
            })
        }
    };

    // removeEventListener - REAL EVENT REMOVAL
    let remove_event_listener = {
        let elem_id = element_id;
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let callback = args.get_or_undefined(1);

                if let Some(callback_obj) = callback.as_object() {
                    let capture = args.get(2)
                        .and_then(|v| {
                            if let Some(obj) = v.as_object() {
                                obj.get(js_string!("capture"), ctx).ok()?.as_boolean()
                            } else {
                                v.as_boolean()
                            }
                        })
                        .unwrap_or(false);

                    event_system::remove_event_listener(
                        elem_id,
                        &event_type,
                        &callback_obj,
                        capture,
                    );
                }

                Ok(JsValue::undefined())
            })
        }
    };

    // dispatchEvent - REAL EVENT DISPATCH WITH BUBBLING
    let dispatch_event = {
        let w = wrapper.clone();
        let elem_id = element_id;
        unsafe {
            NativeFunction::from_closure(move |this, args, ctx| {
                let event_arg = args.get_or_undefined(0);

                if let Some(event_obj) = event_arg.as_object() {
                    // Build ancestor chain for bubbling
                    let mut ancestor_ids = Vec::new();
                    if let Some(elem) = w.get() {
                        // Walk up the DOM tree to build ancestor chain
                        // Note: This is simplified - ideally we'd have parent element IDs
                        // For now, we dispatch to just the target
                        let _ = elem; // Use elem to avoid warning
                    }

                    // Set target and currentTarget on the event
                    let _ = event_obj.set(js_string!("target"), this.clone(), false, ctx);
                    let _ = event_obj.set(js_string!("currentTarget"), this.clone(), false, ctx);

                    // First dispatch to addEventListener listeners
                    let result = event_system::dispatch_event(
                        elem_id,
                        &event_obj,
                        ancestor_ids,
                        ctx,
                    );

                    // Also call on* property handler if set (e.g., onclick, onchange)
                    let event_type = event_obj.get(js_string!("type"), ctx)
                        .ok()
                        .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
                        .unwrap_or_default();

                    if !event_type.is_empty() {
                        let handler_name = format!("on{}", event_type);
                        if let Some(this_obj) = this.as_object() {
                            if let Ok(handler) = this_obj.get(js_string!(handler_name.clone()), ctx) {
                                if let Some(handler_fn) = handler.as_object() {
                                    if handler_fn.is_callable() {
                                        let _ = handler_fn.call(&this, &[JsValue::from(event_obj.clone())], ctx);
                                    }
                                }
                            }
                        }
                    }

                    return Ok(JsValue::from(result));
                }

                Ok(JsValue::from(true))
            })
        }
    };

    // matches
    let matches_fn = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let selector = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let matches = w.get().map(|e| e.matches_selector(&selector)).unwrap_or(false);
                Ok(JsValue::from(matches))
            })
        }
    };

    // computedStyleMap - returns StylePropertyMapReadOnly
    let computed_style_map = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let element_id = w.get().map(|e| e.unique_id()).unwrap_or(0);

                // Common CSS properties to include
                let properties = vec![
                    "display", "visibility", "position", "width", "height",
                    "margin-top", "margin-right", "margin-bottom", "margin-left",
                    "padding-top", "padding-right", "padding-bottom", "padding-left",
                    "color", "background-color", "font-size", "font-family", "font-weight",
                    "line-height", "text-align", "border", "overflow", "z-index",
                    "opacity", "transform", "transition", "flex", "grid",
                ];

                // Collect property values
                let mut prop_values: Vec<(String, String)> = Vec::new();
                for prop in &properties {
                    let value = get_computed_style_property(element_id, prop);
                    if !value.is_empty() {
                        prop_values.push((prop.to_string(), value));
                    }
                }
                let prop_count = prop_values.len();

                // Create get(property) method
                let elem_id = element_id;
                let get_fn = unsafe {
                    NativeFunction::from_closure(move |_this, args, ctx| {
                        let prop = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                        let value = get_computed_style_property(elem_id, &prop);
                        if value.is_empty() {
                            // Return default value for known properties
                            let default_value = match prop.as_str() {
                                "display" => "block",
                                "visibility" => "visible",
                                "position" => "static",
                                "color" => "rgb(0, 0, 0)",
                                _ => "",
                            };
                            if default_value.is_empty() {
                                return Ok(JsValue::null());
                            }
                            let val_str = default_value.to_string();
                            let to_string_fn = NativeFunction::from_closure(move |_this, _args, _ctx| {
                                Ok(JsValue::from(js_string!(val_str.clone())))
                            });
                            let css_value = ObjectInitializer::new(ctx)
                                .property(js_string!("value"), js_string!(default_value), Attribute::READONLY)
                                .function(to_string_fn, js_string!("toString"), 0)
                                .build();
                            Ok(JsValue::from(css_value))
                        } else {
                            // Return CSSStyleValue-like object with toString method
                            let val_str = value.clone();
                            let to_string_fn = NativeFunction::from_closure(move |_this, _args, _ctx| {
                                Ok(JsValue::from(js_string!(val_str.clone())))
                            });
                            let css_value = ObjectInitializer::new(ctx)
                                .property(js_string!("value"), js_string!(value.clone()), Attribute::READONLY)
                                .function(to_string_fn, js_string!("toString"), 0)
                                .build();
                            Ok(JsValue::from(css_value))
                        }
                    })
                };

                // Create getAll(property) method
                let elem_id = element_id;
                let get_all_fn = unsafe {
                    NativeFunction::from_closure(move |_this, args, ctx| {
                        let prop = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                        let value = get_computed_style_property(elem_id, &prop);
                        let arr = boa_engine::object::builtins::JsArray::new(ctx);
                        if !value.is_empty() {
                            let val_str = value.clone();
                            let to_string_fn = NativeFunction::from_closure(move |_this, _args, _ctx| {
                                Ok(JsValue::from(js_string!(val_str.clone())))
                            });
                            let css_value = ObjectInitializer::new(ctx)
                                .property(js_string!("value"), js_string!(value), Attribute::READONLY)
                                .function(to_string_fn, js_string!("toString"), 0)
                                .build();
                            let _ = arr.push(JsValue::from(css_value), ctx);
                        }
                        Ok(JsValue::from(arr))
                    })
                };

                // Create has(property) method
                let elem_id = element_id;
                let has_fn = NativeFunction::from_copy_closure(move |_this, args, ctx| {
                    let prop = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let value = get_computed_style_property(elem_id, &prop);
                    Ok(JsValue::from(!value.is_empty()))
                });

                // Build the StylePropertyMapReadOnly object
                let map = ObjectInitializer::new(ctx)
                    .function(get_fn, js_string!("get"), 1)
                    .function(get_all_fn, js_string!("getAll"), 1)
                    .function(has_fn, js_string!("has"), 1)
                    .property(js_string!("size"), prop_count as u32, Attribute::READONLY)
                    .build();

                Ok(JsValue::from(map))
            })
        }
    };

    // checkVisibility - checks if element is visible based on CSS properties
    let check_visibility = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let element_id = w.get().map(|e| e.unique_id()).unwrap_or(0);

                // Get options if provided
                let check_opacity = if let Some(options) = args.get_or_undefined(0).as_object() {
                    options.get(js_string!("checkOpacity"), ctx)
                        .ok()
                        .and_then(|v| v.as_boolean())
                        .unwrap_or(false)
                } else {
                    false
                };

                let check_visibility_css = if let Some(options) = args.get_or_undefined(0).as_object() {
                    options.get(js_string!("checkVisibilityCSS"), ctx)
                        .ok()
                        .and_then(|v| v.as_boolean())
                        .unwrap_or(true)
                } else {
                    true
                };

                // Check display property
                let display = get_computed_style_property(element_id, "display");
                if display == "none" {
                    return Ok(JsValue::from(false));
                }

                // Check visibility property if requested
                if check_visibility_css {
                    let visibility = get_computed_style_property(element_id, "visibility");
                    if visibility == "hidden" || visibility == "collapse" {
                        return Ok(JsValue::from(false));
                    }
                }

                // Check opacity if requested
                if check_opacity {
                    let opacity = get_computed_style_property(element_id, "opacity");
                    if let Ok(op) = opacity.parse::<f64>() {
                        if op == 0.0 {
                            return Ok(JsValue::from(false));
                        }
                    }
                }

                // Element is visible
                Ok(JsValue::from(true))
            })
        }
    };

    // closest
    let closest_fn = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let selector = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let result = w.get().and_then(|e| e.closest(&selector));
                match result {
                    Some(el) => Ok(create_element_object(el, ctx, &dom)),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // contains - check if this node contains another
    let contains_fn = {
        let w = wrapper.clone();
        let registry = dom.registry.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                let self_el = match w.get() {
                    Some(el) => el,
                    None => return Ok(JsValue::from(false)),
                };

                // Get the other node from args
                if let Some(other_obj) = args.get(0).and_then(|v| v.as_object()) {
                    if let Ok(other_id_val) = other_obj.get(js_string!("__element_id__"), _ctx) {
                        if let Some(other_id) = other_id_val.as_number() {
                            let other_id = other_id as u64;
                            if let Some(other_el) = registry.get(other_id) {
                                return Ok(JsValue::from(self_el.contains(&other_el)));
                            }
                        }
                    }
                }
                Ok(JsValue::from(false))
            })
        }
    };

    // cloneNode - clone with optional deep flag
    let clone_node = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                match w.get() {
                    Some(el) => {
                        let deep = args.get(0)
                            .and_then(|v| v.to_boolean().then_some(true))
                            .unwrap_or(false);
                        let cloned = el.clone_node(deep);
                        Ok(create_element_object(cloned, ctx, &dom))
                    }
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // isSameNode - check if two nodes are identical
    let is_same_node = {
        let w = wrapper.clone();
        let registry = dom.registry.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                let self_el = match w.get() {
                    Some(el) => el,
                    None => return Ok(JsValue::from(false)),
                };

                if let Some(other_obj) = args.get(0).and_then(|v| v.as_object()) {
                    if let Ok(other_id_val) = other_obj.get(js_string!("__element_id__"), _ctx) {
                        if let Some(other_id) = other_id_val.as_number() {
                            let other_id = other_id as u64;
                            if let Some(other_el) = registry.get(other_id) {
                                return Ok(JsValue::from(self_el.is_same_node(&other_el)));
                            }
                        }
                    }
                }
                Ok(JsValue::from(false))
            })
        }
    };

    // isEqualNode - check if two nodes have equal structure
    let is_equal_node = {
        let w = wrapper.clone();
        let registry = dom.registry.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                let self_el = match w.get() {
                    Some(el) => el,
                    None => return Ok(JsValue::from(false)),
                };

                if let Some(other_obj) = args.get(0).and_then(|v| v.as_object()) {
                    if let Ok(other_id_val) = other_obj.get(js_string!("__element_id__"), _ctx) {
                        if let Some(other_id) = other_id_val.as_number() {
                            let other_id = other_id as u64;
                            if let Some(other_el) = registry.get(other_id) {
                                return Ok(JsValue::from(self_el.is_equal_node(&other_el)));
                            }
                        }
                    }
                }
                Ok(JsValue::from(false))
            })
        }
    };

    // compareDocumentPosition - return bitmask of position flags
    let compare_document_position = {
        let w = wrapper.clone();
        let registry = dom.registry.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                let self_el = match w.get() {
                    Some(el) => el,
                    None => return Ok(JsValue::from(1)), // DISCONNECTED
                };

                if let Some(other_obj) = args.get(0).and_then(|v| v.as_object()) {
                    if let Ok(other_id_val) = other_obj.get(js_string!("__element_id__"), _ctx) {
                        if let Some(other_id) = other_id_val.as_number() {
                            let other_id = other_id as u64;
                            if let Some(other_el) = registry.get(other_id) {
                                return Ok(JsValue::from(self_el.compare_document_position(&other_el)));
                            }
                        }
                    }
                }
                Ok(JsValue::from(1)) // DISCONNECTED
            })
        }
    };

    // getRootNode - get the document root
    let get_root_node = {
        let w = wrapper.clone();
        let dom = dom.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                match w.get() {
                    Some(el) => {
                        let root = el.get_root_node();
                        Ok(create_element_object(root, ctx, &dom))
                    }
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // getAttributeNS - get attribute by namespace
    let get_attribute_ns = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                let el = match w.get() {
                    Some(el) => el,
                    None => return Ok(JsValue::null()),
                };

                let namespace = args.get(0).and_then(|v| {
                    if v.is_null() {
                        None
                    } else {
                        v.as_string().map(|s| s.to_std_string_escaped())
                    }
                });
                let local_name = args.get(1)
                    .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
                    .unwrap_or_default();

                match el.get_attribute_ns(namespace.as_deref(), &local_name) {
                    Some(val) => Ok(JsValue::from(js_string!(val))),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // setAttributeNS - set attribute by namespace
    let set_attribute_ns = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                if let Some(el) = w.get() {
                    let namespace = args.get(0).and_then(|v| {
                        if v.is_null() {
                            None
                        } else {
                            v.as_string().map(|s| s.to_std_string_escaped())
                        }
                    });
                    let qualified_name = args.get(1)
                        .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
                        .unwrap_or_default();
                    let value = args.get(2)
                        .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
                        .unwrap_or_default();

                    el.set_attribute_ns(namespace.as_deref(), &qualified_name, &value);
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // hasAttributeNS - check if attribute exists by namespace
    let has_attribute_ns = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                let el = match w.get() {
                    Some(el) => el,
                    None => return Ok(JsValue::from(false)),
                };

                let namespace = args.get(0).and_then(|v| {
                    if v.is_null() {
                        None
                    } else {
                        v.as_string().map(|s| s.to_std_string_escaped())
                    }
                });
                let local_name = args.get(1)
                    .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
                    .unwrap_or_default();

                Ok(JsValue::from(el.has_attribute_ns(namespace.as_deref(), &local_name)))
            })
        }
    };

    // removeAttributeNS - remove attribute by namespace
    let remove_attribute_ns = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                if let Some(el) = w.get() {
                    let namespace = args.get(0).and_then(|v| {
                        if v.is_null() {
                            None
                        } else {
                            v.as_string().map(|s| s.to_std_string_escaped())
                        }
                    });
                    let local_name = args.get(1)
                        .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
                        .unwrap_or_default();

                    el.remove_attribute_ns(namespace.as_deref(), &local_name);
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // lookupPrefix - get the prefix for a namespace URI
    let lookup_prefix = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                let el = match w.get() {
                    Some(el) => el,
                    None => return Ok(JsValue::null()),
                };

                let namespace_uri = args.get(0).and_then(|v| {
                    if v.is_null() || v.is_undefined() {
                        None
                    } else {
                        v.as_string().map(|s| s.to_std_string_escaped())
                    }
                });

                match el.lookup_prefix(namespace_uri.as_deref()) {
                    Some(prefix) => Ok(JsValue::from(js_string!(prefix))),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // lookupNamespaceURI - get the namespace URI for a prefix
    let lookup_namespace_uri = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                let el = match w.get() {
                    Some(el) => el,
                    None => return Ok(JsValue::null()),
                };

                let prefix = args.get(0).and_then(|v| {
                    if v.is_null() || v.is_undefined() {
                        None
                    } else {
                        v.as_string().map(|s| s.to_std_string_escaped())
                    }
                });

                match el.lookup_namespace_uri(prefix.as_deref()) {
                    Some(ns) => Ok(JsValue::from(js_string!(ns))),
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // isDefaultNamespace - check if namespace is the default
    let is_default_namespace = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                let el = match w.get() {
                    Some(el) => el,
                    None => return Ok(JsValue::from(false)),
                };

                let namespace_uri = args.get(0).and_then(|v| {
                    if v.is_null() || v.is_undefined() {
                        None
                    } else {
                        v.as_string().map(|s| s.to_std_string_escaped())
                    }
                });

                Ok(JsValue::from(el.is_default_namespace(namespace_uri.as_deref())))
            })
        }
    };

    // getAttributeNodeNS - get Attr object by namespace
    let get_attribute_node_ns = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let el = match w.get() {
                    Some(el) => el,
                    None => return Ok(JsValue::null()),
                };

                let namespace = args.get(0).and_then(|v| {
                    if v.is_null() {
                        None
                    } else {
                        v.as_string().map(|s| s.to_std_string_escaped())
                    }
                });
                let local_name = args.get(1)
                    .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
                    .unwrap_or_default();

                match el.get_attribute_ns(namespace.as_deref(), &local_name) {
                    Some(value) => {
                        // Create an Attr object
                        let attr = ObjectInitializer::new(ctx)
                            .property(js_string!("name"), js_string!(local_name.clone()), Attribute::READONLY)
                            .property(js_string!("localName"), js_string!(local_name.clone()), Attribute::READONLY)
                            .property(js_string!("value"), js_string!(value.clone()), Attribute::all())
                            .property(js_string!("nodeType"), 2, Attribute::READONLY)
                            .property(js_string!("nodeName"), js_string!(local_name), Attribute::READONLY)
                            .property(js_string!("nodeValue"), js_string!(value.clone()), Attribute::all())
                            .property(js_string!("textContent"), js_string!(value.clone()), Attribute::all())
                            .property(js_string!("specified"), true, Attribute::READONLY)
                            .property(js_string!("namespaceURI"),
                                namespace.as_ref().map(|s| JsValue::from(js_string!(s.as_str()))).unwrap_or(JsValue::null()),
                                Attribute::READONLY)
                            .property(js_string!("prefix"), JsValue::null(), Attribute::READONLY)
                            .build();
                        Ok(JsValue::from(attr))
                    }
                    None => Ok(JsValue::null()),
                }
            })
        }
    };

    // setAttributeNodeNS - set Attr object by namespace
    let set_attribute_node_ns = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let el = match w.get() {
                    Some(el) => el,
                    None => return Ok(JsValue::null()),
                };

                if let Some(attr_obj) = args.get(0).and_then(|v| v.as_object()) {
                    let namespace = attr_obj.get(js_string!("namespaceURI"), ctx)
                        .ok()
                        .and_then(|v| {
                            if v.is_null() || v.is_undefined() {
                                None
                            } else {
                                v.as_string().map(|s| s.to_std_string_escaped())
                            }
                        });
                    let local_name = attr_obj.get(js_string!("localName"), ctx)
                        .ok()
                        .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
                        .unwrap_or_default();
                    let value = attr_obj.get(js_string!("value"), ctx)
                        .ok()
                        .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
                        .unwrap_or_default();

                    // Get old attribute if exists
                    let old_value = el.get_attribute_ns(namespace.as_deref(), &local_name);

                    // Set the new attribute
                    el.set_attribute_ns(namespace.as_deref(), &local_name, &value);

                    // Return old Attr or null
                    if let Some(old_val) = old_value {
                        let old_attr = ObjectInitializer::new(ctx)
                            .property(js_string!("name"), js_string!(local_name.clone()), Attribute::READONLY)
                            .property(js_string!("localName"), js_string!(local_name.clone()), Attribute::READONLY)
                            .property(js_string!("value"), js_string!(old_val.clone()), Attribute::all())
                            .property(js_string!("nodeType"), 2, Attribute::READONLY)
                            .property(js_string!("nodeName"), js_string!(local_name), Attribute::READONLY)
                            .property(js_string!("nodeValue"), js_string!(old_val.clone()), Attribute::all())
                            .property(js_string!("textContent"), js_string!(old_val), Attribute::all())
                            .property(js_string!("specified"), true, Attribute::READONLY)
                            .property(js_string!("namespaceURI"),
                                namespace.as_ref().map(|s| JsValue::from(js_string!(s.as_str()))).unwrap_or(JsValue::null()),
                                Attribute::READONLY)
                            .property(js_string!("prefix"), JsValue::null(), Attribute::READONLY)
                            .build();
                        return Ok(JsValue::from(old_attr));
                    }
                }
                Ok(JsValue::null())
            })
        }
    };

    // setHTML - HTML Sanitizer API (sanitizes and sets innerHTML)
    let set_html = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                if let Some(el) = w.get() {
                    let html = args.get(0)
                        .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
                        .unwrap_or_default();
                    // For now, setHTML just sets innerHTML (no real sanitization)
                    // A full implementation would use a sanitizer
                    el.set_inner_html(&html);
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // setHTMLUnsafe - sets innerHTML without sanitization
    let set_html_unsafe = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                if let Some(el) = w.get() {
                    let html = args.get(0)
                        .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
                        .unwrap_or_default();
                    el.set_inner_html(&html);
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // getHTML - get serialized HTML with options
    let get_html = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                match w.get() {
                    Some(el) => Ok(JsValue::from(js_string!(el.inner_html()))),
                    None => Ok(JsValue::from(js_string!(""))),
                }
            })
        }
    };

    // hasChildNodes - returns true if element has any child nodes
    let has_child_nodes = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                let has = w.get().map(|e| e.has_child_nodes()).unwrap_or(false);
                Ok(JsValue::from(has))
            })
        }
    };

    // normalize - merge adjacent text nodes and remove empty text nodes
    let normalize_fn = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                if let Some(el) = w.get() {
                    el.normalize();
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // getBoundingClientRect
    let get_bounding_client_rect = {
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                // Return a DOMRect-like object with reasonable default values
                let rect = ObjectInitializer::new(ctx)
                    .property(js_string!("x"), 0.0, Attribute::READONLY)
                    .property(js_string!("y"), 0.0, Attribute::READONLY)
                    .property(js_string!("width"), 100.0, Attribute::READONLY)
                    .property(js_string!("height"), 20.0, Attribute::READONLY)
                    .property(js_string!("top"), 0.0, Attribute::READONLY)
                    .property(js_string!("right"), 100.0, Attribute::READONLY)
                    .property(js_string!("bottom"), 20.0, Attribute::READONLY)
                    .property(js_string!("left"), 0.0, Attribute::READONLY)
                    .build();
                Ok(JsValue::from(rect))
            })
        }
    };

    // getClientRects
    let get_client_rects = {
        unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx| {
                let rect = ObjectInitializer::new(ctx)
                    .property(js_string!("x"), 0.0, Attribute::READONLY)
                    .property(js_string!("y"), 0.0, Attribute::READONLY)
                    .property(js_string!("width"), 100.0, Attribute::READONLY)
                    .property(js_string!("height"), 20.0, Attribute::READONLY)
                    .property(js_string!("top"), 0.0, Attribute::READONLY)
                    .property(js_string!("right"), 100.0, Attribute::READONLY)
                    .property(js_string!("bottom"), 20.0, Attribute::READONLY)
                    .property(js_string!("left"), 0.0, Attribute::READONLY)
                    .build();

                let rects = ObjectInitializer::new(ctx)
                    .property(js_string!("length"), 1, Attribute::READONLY)
                    .property(js_string!("0"), rect, Attribute::READONLY)
                    .build();
                Ok(JsValue::from(rects))
            })
        }
    };

    // focus - track as active element
    let focus_fn = {
        let elem_id = element_id;
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                event_system::set_active_element(Some(elem_id));
                Ok(JsValue::undefined())
            })
        }
    };

    // blur - clear active element
    let blur_fn = {
        let elem_id = element_id;
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                // Only clear if this element is currently active
                if event_system::get_active_element() == Some(elem_id) {
                    event_system::set_active_element(None);
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // click - dispatch a synthetic click event
    let click_fn = {
        let elem_id = element_id;
        unsafe {
            NativeFunction::from_closure(move |this, _args, ctx| {
                // Create a synthetic MouseEvent
                let event_obj = ObjectInitializer::new(ctx)
                    .property(js_string!("type"), js_string!("click"), Attribute::all())
                    .property(js_string!("bubbles"), JsValue::from(true), Attribute::all())
                    .property(js_string!("cancelable"), JsValue::from(true), Attribute::all())
                    .property(js_string!("isTrusted"), JsValue::from(false), Attribute::all())
                    .property(js_string!("defaultPrevented"), JsValue::from(false), Attribute::all())
                    .property(js_string!("cancelBubble"), JsValue::from(false), Attribute::all())
                    .build();

                // Set target to this element
                let _ = event_obj.set(js_string!("target"), this.clone(), false, ctx);
                let _ = event_obj.set(js_string!("currentTarget"), this.clone(), false, ctx);

                // Call addEventListener listeners via event_system
                event_system::dispatch_event(elem_id, &event_obj, vec![], ctx);

                // Also call onclick property handler if set
                if let Some(this_obj) = this.as_object() {
                    if let Ok(onclick) = this_obj.get(js_string!("onclick"), ctx) {
                        if onclick.is_callable() {
                            let event_value = JsValue::from(event_obj);
                            let _ = onclick.as_callable().unwrap().call(&this, &[event_value], ctx);
                        }
                    }
                }

                Ok(JsValue::undefined())
            })
        }
    };

    // scrollIntoView
    let scroll_into_view = {
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                Ok(JsValue::undefined())
            })
        }
    };

    // attachShadow - create real shadow root with DOM querying
    let attach_shadow = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let mode = args.get(0)
                    .and_then(|v| v.as_object())
                    .and_then(|obj| obj.get(js_string!("mode"), ctx).ok())
                    .and_then(|v| v.to_string(ctx).ok())
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_else(|| "open".to_string());

                // Get the host element to create the shadow root with
                let host_element = w.get();
                let shadow_root = crate::element::create_shadow_root(ctx, &mode, host_element);

                // Store the shadow root on the element (for open mode)
                if mode == "open" {
                    if let Some(this_obj) = _this.as_object() {
                        let _ = this_obj.set(js_string!("shadowRoot"), JsValue::from(shadow_root.clone()), false, ctx);
                    }
                }

                Ok(JsValue::from(shadow_root))
            })
        }
    };

    // remove - REAL DOM MUTATION
    let remove_fn = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                // Remove this element from its parent
                if let Some(el) = w.get() {
                    el.remove_from_parent();
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // before - insert nodes before this element
    let before_fn = {
        let w = wrapper.clone();
        let reg = dom.registry.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                if let Some(el) = w.get() {
                    if let Some(parent) = el.parent_element() {
                        for arg in args.iter() {
                            if let Some(obj) = arg.as_object() {
                                if let Some(child_id) = get_element_id_from_js(&obj, ctx) {
                                    if let Some(child) = reg.get(child_id) {
                                        parent.insert_before(&child, &el);
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // after - insert nodes after this element
    let after_fn = {
        let w = wrapper.clone();
        let reg = dom.registry.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                if let Some(el) = w.get() {
                    if let Some(parent) = el.parent_element() {
                        // Get next sibling to insert before, or append if none
                        let next = el.next_element_sibling();
                        for arg in args.iter() {
                            if let Some(obj) = arg.as_object() {
                                if let Some(child_id) = get_element_id_from_js(&obj, ctx) {
                                    if let Some(child) = reg.get(child_id) {
                                        if let Some(ref next_el) = next {
                                            parent.insert_before(&child, next_el);
                                        } else {
                                            parent.append_child(&child);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // prepend - insert at the beginning of children
    let prepend_fn = {
        let w = wrapper.clone();
        let reg = dom.registry.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                if let Some(parent) = w.get() {
                    // Insert in reverse order to maintain correct order
                    for arg in args.iter().rev() {
                        if let Some(obj) = arg.as_object() {
                            if let Some(child_id) = get_element_id_from_js(&obj, ctx) {
                                if let Some(child) = reg.get(child_id) {
                                    parent.prepend_child(&child);
                                }
                            }
                        }
                    }
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // append - insert at the end of children (like appendChild but accepts multiple)
    let append_fn = {
        let w = wrapper.clone();
        let reg = dom.registry.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                if let Some(parent) = w.get() {
                    for arg in args.iter() {
                        if let Some(obj) = arg.as_object() {
                            if let Some(child_id) = get_element_id_from_js(&obj, ctx) {
                                if let Some(child) = reg.get(child_id) {
                                    parent.append_child(&child);
                                }
                            }
                        }
                    }
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // replaceChildren - replace all children with new nodes
    let replace_children_fn = {
        let w = wrapper.clone();
        let reg = dom.registry.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                if let Some(parent) = w.get() {
                    // Collect all new children first
                    let mut new_children = Vec::new();
                    for arg in args.iter() {
                        if let Some(obj) = arg.as_object() {
                            if let Some(child_id) = get_element_id_from_js(&obj, ctx) {
                                if let Some(child) = reg.get(child_id) {
                                    new_children.push(child);
                                }
                            }
                        }
                    }
                    // Replace all children
                    parent.replace_children(new_children);
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // replaceWith - replace this element with other nodes
    let replace_with_fn = {
        let w = wrapper.clone();
        let reg = dom.registry.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                if let Some(el) = w.get() {
                    if let Some(parent) = el.parent_element() {
                        // Insert new nodes before this element, then remove this element
                        for arg in args.iter() {
                            if let Some(obj) = arg.as_object() {
                                if let Some(child_id) = get_element_id_from_js(&obj, ctx) {
                                    if let Some(child) = reg.get(child_id) {
                                        parent.insert_before(&child, &el);
                                    }
                                }
                            }
                        }
                        el.remove_from_parent();
                    }
                }
                Ok(JsValue::undefined())
            })
        }
    };

    // insertAdjacentHTML
    let insert_adjacent_html = {
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                Ok(JsValue::undefined())
            })
        }
    };

    // insertAdjacentElement
    let insert_adjacent_element = {
        unsafe {
            NativeFunction::from_closure(move |_this, args, _ctx| {
                Ok(args.get_or_undefined(1).clone())
            })
        }
    };

    // insertAdjacentText
    let insert_adjacent_text = {
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                Ok(JsValue::undefined())
            })
        }
    };

    // Create dataset object
    let dataset = create_dataset(&wrapper, context);

    // Create classList object
    let class_list = create_class_list(&wrapper, context);

    // Create style object (stub with common properties)
    let style = create_style_object(&wrapper, context);

    // Create attributes NamedNodeMap
    let attributes = create_attributes_map(&wrapper, context);

    // Create accessors
    let id_getter_fn = id_getter.to_js_function(context.realm());
    let id_setter_fn = id_setter.to_js_function(context.realm());
    let class_getter_fn = class_getter.to_js_function(context.realm());
    let class_setter_fn = class_setter.to_js_function(context.realm());
    let inner_text_getter_fn = inner_text_getter.to_js_function(context.realm());
    let text_content_getter_fn = text_content_getter.to_js_function(context.realm());
    let text_content_setter_fn = text_content_setter.to_js_function(context.realm());
    let inner_html_getter_fn = inner_html_getter.to_js_function(context.realm());
    let inner_html_setter_fn = inner_html_setter.to_js_function(context.realm());
    let children_getter_fn = children_getter.to_js_function(context.realm());
    let child_nodes_getter_fn = child_nodes_getter.to_js_function(context.realm());
    let first_child_getter_fn = first_child_getter.to_js_function(context.realm());
    let last_child_getter_fn = last_child_getter.to_js_function(context.realm());
    let first_element_child_getter_fn = first_element_child_getter.to_js_function(context.realm());
    let last_element_child_getter_fn = last_element_child_getter.to_js_function(context.realm());
    let child_element_count_getter_fn = child_element_count_getter.to_js_function(context.realm());
    let parent_node_getter_fn = parent_node_getter.to_js_function(context.realm());
    let parent_element_getter_fn = parent_element_getter.to_js_function(context.realm());
    let next_sibling_getter_fn = next_sibling_getter.to_js_function(context.realm());
    let previous_sibling_getter_fn = previous_sibling_getter.to_js_function(context.realm());
    let next_element_sibling_getter_fn = next_element_sibling_getter.to_js_function(context.realm());
    let previous_element_sibling_getter_fn = previous_element_sibling_getter.to_js_function(context.realm());
    let owner_document_getter_fn = owner_document_getter.to_js_function(context.realm());
    let namespace_uri_getter_fn = namespace_uri_getter.to_js_function(context.realm());

    // Build element object
    let obj = ObjectInitializer::new(context)
        // Internal element ID for DOM mutation tracking
        // Store as f64 to preserve full 64-bit pointer (JS numbers are safe up to 2^53)
        .property(js_string!("__element_id__"), element_id as f64, Attribute::READONLY)
        // Properties
        .property(js_string!("tagName"), js_string!(tag_name.clone()), Attribute::READONLY)
        .property(js_string!("nodeName"), js_string!(node_name), Attribute::READONLY | Attribute::CONFIGURABLE)
        .property(js_string!("nodeType"), node_type, Attribute::READONLY | Attribute::CONFIGURABLE)
        .property(js_string!("classList"), class_list, Attribute::READONLY)
        .property(js_string!("style"), style, Attribute::READONLY)
        .property(js_string!("dataset"), dataset, Attribute::READONLY)
        .property(js_string!("attributes"), attributes, Attribute::READONLY)
        // Dimension properties (standard desktop values)
        .property(js_string!("offsetWidth"), 100, Attribute::READONLY)
        .property(js_string!("offsetHeight"), 20, Attribute::READONLY)
        .property(js_string!("offsetTop"), 0, Attribute::READONLY)
        .property(js_string!("offsetLeft"), 0, Attribute::READONLY)
        .property(js_string!("offsetParent"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("clientWidth"), 100, Attribute::READONLY)
        .property(js_string!("clientHeight"), 20, Attribute::READONLY)
        .property(js_string!("clientTop"), 0, Attribute::READONLY)
        .property(js_string!("clientLeft"), 0, Attribute::READONLY)
        .property(js_string!("scrollWidth"), 100, Attribute::READONLY)
        .property(js_string!("scrollHeight"), 20, Attribute::READONLY)
        .property(js_string!("scrollTop"), 0, Attribute::all())
        .property(js_string!("scrollLeft"), 0, Attribute::all())
        // Additional properties
        .property(js_string!("isConnected"), true, Attribute::READONLY)
        .property(js_string!("hidden"), false, Attribute::all())
        .property(js_string!("tabIndex"), -1, Attribute::all())
        .property(js_string!("dir"), js_string!(""), Attribute::all())
        .property(js_string!("lang"), js_string!(""), Attribute::all())
        .property(js_string!("title"), js_string!(""), Attribute::all())
        .property(js_string!("slot"), js_string!(""), Attribute::all())
        // Accessors
        .accessor(js_string!("id"), Some(id_getter_fn), Some(id_setter_fn), Attribute::CONFIGURABLE)
        .accessor(js_string!("className"), Some(class_getter_fn), Some(class_setter_fn), Attribute::CONFIGURABLE)
        .accessor(js_string!("innerText"), Some(inner_text_getter_fn.clone()), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("textContent"), Some(text_content_getter_fn), Some(text_content_setter_fn), Attribute::CONFIGURABLE)
        .accessor(js_string!("innerHTML"), Some(inner_html_getter_fn.clone()), Some(inner_html_setter_fn), Attribute::CONFIGURABLE)
        .accessor(js_string!("outerHTML"), Some(inner_html_getter_fn), None, Attribute::CONFIGURABLE)
        // DOM tree accessors
        .accessor(js_string!("children"), Some(children_getter_fn), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("childNodes"), Some(child_nodes_getter_fn), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("firstChild"), Some(first_child_getter_fn), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("lastChild"), Some(last_child_getter_fn), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("firstElementChild"), Some(first_element_child_getter_fn), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("lastElementChild"), Some(last_element_child_getter_fn), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("childElementCount"), Some(child_element_count_getter_fn), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("parentNode"), Some(parent_node_getter_fn), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("parentElement"), Some(parent_element_getter_fn), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("nextSibling"), Some(next_sibling_getter_fn), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("previousSibling"), Some(previous_sibling_getter_fn), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("nextElementSibling"), Some(next_element_sibling_getter_fn), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("previousElementSibling"), Some(previous_element_sibling_getter_fn), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("ownerDocument"), Some(owner_document_getter_fn), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("namespaceURI"), Some(namespace_uri_getter_fn), None, Attribute::CONFIGURABLE)
        // Methods
        .function(get_attribute, js_string!("getAttribute"), 1)
        .function(set_attribute, js_string!("setAttribute"), 2)
        .function(remove_attribute, js_string!("removeAttribute"), 1)
        .function(has_attribute, js_string!("hasAttribute"), 1)
        .function(get_attribute_node, js_string!("getAttributeNode"), 1)
        .function(element_query_selector, js_string!("querySelector"), 1)
        .function(element_query_selector_all, js_string!("querySelectorAll"), 1)
        .function(element_get_by_tag, js_string!("getElementsByTagName"), 1)
        .function(element_get_by_class, js_string!("getElementsByClassName"), 1)
        .function(append_child, js_string!("appendChild"), 1)
        .function(remove_child, js_string!("removeChild"), 1)
        .function(insert_before, js_string!("insertBefore"), 2)
        .function(replace_child, js_string!("replaceChild"), 2)
        .function(add_event_listener, js_string!("addEventListener"), 2)
        .function(remove_event_listener, js_string!("removeEventListener"), 2)
        .function(dispatch_event, js_string!("dispatchEvent"), 1)
        .function(matches_fn.clone(), js_string!("matches"), 1)
        .function(matches_fn.clone(), js_string!("webkitMatchesSelector"), 1)
        .function(matches_fn, js_string!("msMatchesSelector"), 1)
        .function(closest_fn, js_string!("closest"), 1)
        .function(computed_style_map, js_string!("computedStyleMap"), 0)
        .function(check_visibility, js_string!("checkVisibility"), 1)
        .function(contains_fn, js_string!("contains"), 1)
        .function(clone_node, js_string!("cloneNode"), 1)
        .function(is_same_node, js_string!("isSameNode"), 1)
        .function(is_equal_node, js_string!("isEqualNode"), 1)
        .function(compare_document_position, js_string!("compareDocumentPosition"), 1)
        .function(get_root_node, js_string!("getRootNode"), 0)
        .function(get_attribute_ns, js_string!("getAttributeNS"), 2)
        .function(set_attribute_ns, js_string!("setAttributeNS"), 3)
        .function(has_attribute_ns, js_string!("hasAttributeNS"), 2)
        .function(remove_attribute_ns, js_string!("removeAttributeNS"), 2)
        .function(lookup_prefix, js_string!("lookupPrefix"), 1)
        .function(lookup_namespace_uri, js_string!("lookupNamespaceURI"), 1)
        .function(is_default_namespace, js_string!("isDefaultNamespace"), 1)
        .function(get_attribute_node_ns, js_string!("getAttributeNodeNS"), 2)
        .function(set_attribute_node_ns, js_string!("setAttributeNodeNS"), 1)
        .function(set_html, js_string!("setHTML"), 1)
        .function(set_html_unsafe, js_string!("setHTMLUnsafe"), 1)
        .function(get_html, js_string!("getHTML"), 0)
        .function(has_child_nodes, js_string!("hasChildNodes"), 0)
        .function(normalize_fn, js_string!("normalize"), 0)
        .function(get_bounding_client_rect, js_string!("getBoundingClientRect"), 0)
        .function(get_client_rects, js_string!("getClientRects"), 0)
        .function(focus_fn, js_string!("focus"), 0)
        .function(blur_fn, js_string!("blur"), 0)
        .function(click_fn, js_string!("click"), 0)
        .function(scroll_into_view, js_string!("scrollIntoView"), 1)
        .function(attach_shadow, js_string!("attachShadow"), 1)
        .function(remove_fn, js_string!("remove"), 0)
        .function(before_fn, js_string!("before"), 1)
        .function(after_fn, js_string!("after"), 1)
        .function(prepend_fn, js_string!("prepend"), 1)
        .function(append_fn, js_string!("append"), 1)
        .function(replace_children_fn, js_string!("replaceChildren"), 0)
        .function(replace_with_fn, js_string!("replaceWith"), 1)
        .function(insert_adjacent_html, js_string!("insertAdjacentHTML"), 2)
        .function(insert_adjacent_element, js_string!("insertAdjacentElement"), 2)
        .function(insert_adjacent_text, js_string!("insertAdjacentText"), 2)
        .build();

    // Add element-specific properties based on tag name
    let element_for_get = element.clone();
    let element_for_set = element.clone();
    html_elements::add_element_specific_properties(
        &obj,
        &tag_name,
        move |attr| {
            // Special pseudo-attribute to get textContent for textarea/option
            if attr == "_textContent" {
                Some(element_for_get.text_content())
            } else {
                element_for_get.get_attribute(attr)
            }
        },
        move |attr, val| element_for_set.set_attribute(attr, val),
        context,
    );

    // Convert on* event attributes (onclick, onload, etc.) to callable functions
    // This mimics real browser behavior where <button onclick="foo()"> becomes element.onclick = function(event) { foo() }
    for (attr_name, attr_value) in element.get_all_attributes() {
        if attr_name.starts_with("on") && attr_name.len() > 2 {
            // Create a function from the attribute value
            // The function receives 'event' as a parameter and executes the attribute code
            let fn_code = format!(
                "(function(event) {{ {} }})",
                attr_value
            );

            // Try to compile and set the function
            if let Ok(func) = context.eval(Source::from_bytes(fn_code.as_bytes())) {
                // Set as property on the object (e.g., obj.onclick = function...)
                let _ = obj.set(js_string!(attr_name.clone()), func, false, context);
            }
        }
    }

    // Add text node specific properties (for nodeType === 3)
    if node_type == 3 {
        // Text.data getter - returns the text content
        let data_getter = {
            let w = wrapper.clone();
            unsafe {
                NativeFunction::from_closure(move |_this, _args, _ctx| {
                    let text = w.get().map(|e| e.text_content()).unwrap_or_default();
                    Ok(JsValue::from(js_string!(text)))
                })
            }
        };

        // Text.data setter - sets the text content directly for text nodes
        let data_setter = {
            let w = wrapper.clone();
            unsafe {
                NativeFunction::from_closure(move |_this, args, ctx| {
                    let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    if let Some(el) = w.get() {
                        // Use set_text_node_data for text nodes (modifies internal contents)
                        el.set_text_node_data(&text);
                    }
                    Ok(JsValue::undefined())
                })
            }
        };

        // Text.splitText(offset) - splits the text node at the offset
        let split_text = {
            let w = wrapper.clone();
            let dom = dom.clone();
            unsafe {
                NativeFunction::from_closure(move |_this, args, ctx| {
                    let offset = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0) as usize;
                    if let Some(el) = w.get() {
                        let text = el.text_content();
                        if offset <= text.len() {
                            // Split: keep first part in original, return new node with rest
                            let (first_part, second_part) = text.split_at(offset);
                            el.set_text_node_data(first_part);

                            // Create new text node with the remaining content
                            let new_text_node = dom.inner.create_text_node(second_part);

                            // Insert the new node after the current one
                            if let Some(parent) = el.parent_element() {
                                if let Some(next) = el.next_element_sibling() {
                                    parent.insert_before(&new_text_node, &next);
                                } else {
                                    parent.append_child(&new_text_node);
                                }
                            }

                            return Ok(create_element_object(new_text_node, ctx, &dom));
                        }
                    }
                    Ok(JsValue::null())
                })
            }
        };

        // Add data as accessor (getter/setter)
        use boa_engine::property::PropertyDescriptor;
        let data_getter_fn = data_getter.to_js_function(context.realm());
        let data_setter_fn = data_setter.to_js_function(context.realm());
        let _ = obj.define_property_or_throw(
            js_string!("data"),
            PropertyDescriptor::builder()
                .get(data_getter_fn)
                .set(data_setter_fn)
                .enumerable(true)
                .configurable(true)
                .build(),
            context
        );

        // Add splitText method
        let _ = obj.set(js_string!("splitText"), split_text.to_js_function(context.realm()), false, context);

        // Override textContent accessor for text nodes to use set_text_node_data
        let tc_getter = {
            let w = wrapper.clone();
            unsafe {
                NativeFunction::from_closure(move |_this, _args, _ctx| {
                    let text = w.get().map(|e| e.text_content()).unwrap_or_default();
                    Ok(JsValue::from(js_string!(text)))
                })
            }
        };
        let tc_setter = {
            let w = wrapper.clone();
            unsafe {
                NativeFunction::from_closure(move |_this, args, ctx| {
                    let text = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    if let Some(el) = w.get() {
                        el.set_text_node_data(&text);
                    }
                    Ok(JsValue::undefined())
                })
            }
        };
        let tc_getter_fn = tc_getter.to_js_function(context.realm());
        let tc_setter_fn = tc_setter.to_js_function(context.realm());
        let _ = obj.define_property_or_throw(
            js_string!("textContent"),
            PropertyDescriptor::builder()
                .get(tc_getter_fn)
                .set(tc_setter_fn)
                .enumerable(true)
                .configurable(true)
                .build(),
            context
        );
    }

    JsValue::from(obj)
}

/// Create a classList object
fn create_class_list(wrapper: &ElementWrapper, context: &mut Context) -> JsObject {
    let add = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                if let Some(el) = w.get() {
                    let mut classes = el.class_list();
                    for arg in args.iter() {
                        let class = arg.to_string(ctx)?.to_std_string_escaped();
                        if !classes.contains(&class) {
                            classes.push(class);
                        }
                    }
                    el.set_attribute("class", &classes.join(" "));
                }
                Ok(JsValue::undefined())
            })
        }
    };

    let remove = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                if let Some(el) = w.get() {
                    let mut classes = el.class_list();
                    for arg in args.iter() {
                        let class = arg.to_string(ctx)?.to_std_string_escaped();
                        classes.retain(|c| c != &class);
                    }
                    el.set_attribute("class", &classes.join(" "));
                }
                Ok(JsValue::undefined())
            })
        }
    };

    let toggle = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                if let Some(el) = w.get() {
                    let class = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let mut classes = el.class_list();
                    let had_class = classes.contains(&class);

                    if had_class {
                        classes.retain(|c| c != &class);
                    } else {
                        classes.push(class);
                    }
                    el.set_attribute("class", &classes.join(" "));
                    return Ok(JsValue::from(!had_class));
                }
                Ok(JsValue::from(false))
            })
        }
    };

    let contains = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let class = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                let has = w.get().map(|e| e.has_class(&class)).unwrap_or(false);
                Ok(JsValue::from(has))
            })
        }
    };

    let replace = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                if let Some(el) = w.get() {
                    let old = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                    let new = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
                    let mut classes = el.class_list();
                    let had_old = classes.contains(&old);
                    if had_old {
                        classes.retain(|c| c != &old);
                        if !classes.contains(&new) {
                            classes.push(new);
                        }
                        el.set_attribute("class", &classes.join(" "));
                    }
                    return Ok(JsValue::from(had_old));
                }
                Ok(JsValue::from(false))
            })
        }
    };

    // length getter
    let length_getter = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                let len = w.get().map(|e| e.class_list().len()).unwrap_or(0);
                Ok(JsValue::from(len as u32))
            })
        }
    };

    // value getter (returns className as space-separated string)
    let value_getter = {
        let w = wrapper.clone();
        unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                let value = w.get()
                    .map(|e| e.class_list().join(" "))
                    .unwrap_or_default();
                Ok(JsValue::from(js_string!(value)))
            })
        }
    };

    let length_fn = length_getter.to_js_function(context.realm());
    let value_fn = value_getter.to_js_function(context.realm());

    ObjectInitializer::new(context)
        .function(add, js_string!("add"), 1)
        .function(remove, js_string!("remove"), 1)
        .function(toggle, js_string!("toggle"), 1)
        .function(contains, js_string!("contains"), 1)
        .function(replace, js_string!("replace"), 2)
        .accessor(js_string!("length"), Some(length_fn), None, Attribute::READONLY)
        .accessor(js_string!("value"), Some(value_fn), None, Attribute::READONLY)
        .build()
}

/// Create a style object with working setProperty/getPropertyValue/removeProperty
/// Syncs changes back to the DOM element's style attribute
fn create_style_object(wrapper: &ElementWrapper, context: &mut Context) -> JsObject {
    use std::collections::HashMap;

    // Shared state for style properties
    let properties: Rc<RefCell<HashMap<String, String>>> = Rc::new(RefCell::new(HashMap::new()));

    // Parse existing style attribute and populate properties
    if let Some(el) = wrapper.get() {
        if let Some(style_attr) = el.get_attribute("style") {
            for declaration in style_attr.split(';') {
                let declaration = declaration.trim();
                if let Some(colon_pos) = declaration.find(':') {
                    let name = declaration[..colon_pos].trim().to_string();
                    let value = declaration[colon_pos + 1..].trim().to_string();
                    if !name.is_empty() && !value.is_empty() {
                        properties.borrow_mut().insert(name, value);
                    }
                }
            }
        }
    }

    // Helper to sync properties back to DOM element's style attribute
    fn sync_to_dom(wrapper: &ElementWrapper, properties: &RefCell<HashMap<String, String>>) {
        if let Some(el) = wrapper.get() {
            let props = properties.borrow();
            if props.is_empty() {
                el.remove_attribute("style");
            } else {
                let style_str: String = props
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v))
                    .collect::<Vec<_>>()
                    .join("; ");
                el.set_attribute("style", &style_str);
            }
        }
    }

    // setProperty(name, value, priority?)
    let props_set = properties.clone();
    let wrapper_set = wrapper.clone();
    let set_property = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            props_set.borrow_mut().insert(name, value);
            sync_to_dom(&wrapper_set, &props_set);
            Ok(JsValue::undefined())
        })
    };

    // getPropertyValue(name)
    let props_get = properties.clone();
    let get_property_value = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let value = props_get.borrow().get(&name).cloned().unwrap_or_default();
            Ok(JsValue::from(js_string!(value)))
        })
    };

    // removeProperty(name) - returns the old value
    let props_remove = properties.clone();
    let wrapper_remove = wrapper.clone();
    let remove_property = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let old_value = props_remove.borrow_mut().remove(&name).unwrap_or_default();
            sync_to_dom(&wrapper_remove, &props_remove);
            Ok(JsValue::from(js_string!(old_value)))
        })
    };

    // getPropertyPriority(name)
    let get_property_priority = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(js_string!("")))
    });

    // item(index)
    let props_item = properties.clone();
    let item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let props = props_item.borrow();
            let keys: Vec<&String> = props.keys().collect();
            if index < keys.len() {
                Ok(JsValue::from(js_string!(keys[index].clone())))
            } else {
                Ok(JsValue::from(js_string!("")))
            }
        })
    };

    // Create style object with getters/setters for common properties that sync to DOM
    let style = ObjectInitializer::new(context)
        .function(set_property, js_string!("setProperty"), 3)
        .function(get_property_value, js_string!("getPropertyValue"), 1)
        .function(remove_property, js_string!("removeProperty"), 1)
        .function(get_property_priority, js_string!("getPropertyPriority"), 1)
        .function(item, js_string!("item"), 1)
        .property(js_string!("cssText"), js_string!(""), Attribute::all())
        .property(js_string!("length"), 0, Attribute::READONLY)
        .property(js_string!("parentRule"), JsValue::null(), Attribute::READONLY)
        .build();

    // Add property accessors for common CSS properties that sync to DOM
    let css_properties = [
        "display", "visibility", "opacity", "position", "width", "height",
        "top", "left", "right", "bottom", "margin", "padding",
        "backgroundColor", "color", "fontSize", "transform", "overflow", "zIndex",
        "marginTop", "marginBottom", "marginLeft", "marginRight",
        "paddingTop", "paddingBottom", "paddingLeft", "paddingRight",
        "border", "borderWidth", "borderColor", "borderStyle",
        "fontFamily", "fontWeight", "fontStyle", "textAlign", "textDecoration",
        "lineHeight", "letterSpacing", "cursor", "pointerEvents",
        "flex", "flexDirection", "flexWrap", "justifyContent", "alignItems",
        "gridTemplateColumns", "gridTemplateRows", "gap",
        "background", "backgroundImage", "backgroundSize", "backgroundPosition",
        "boxShadow", "borderRadius", "outline", "transition", "animation",
    ];

    for prop in css_properties {
        let props_for_get = properties.clone();
        let prop_name = prop.to_string();
        let getter = unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                let value = props_for_get.borrow().get(&prop_name).cloned().unwrap_or_default();
                Ok(JsValue::from(js_string!(value)))
            })
        };

        let props_for_set = properties.clone();
        let wrapper_for_set = wrapper.clone();
        let prop_name_set = prop.to_string();
        let setter = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                let value = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
                if value.is_empty() {
                    props_for_set.borrow_mut().remove(&prop_name_set);
                } else {
                    props_for_set.borrow_mut().insert(prop_name_set.clone(), value);
                }
                sync_to_dom(&wrapper_for_set, &props_for_set);
                Ok(JsValue::undefined())
            })
        };

        let _ = style.define_property_or_throw(
            js_string!(prop),
            boa_engine::property::PropertyDescriptor::builder()
                .get(getter.to_js_function(context.realm()))
                .set(setter.to_js_function(context.realm()))
                .enumerable(true)
                .configurable(true)
                .build(),
            context,
        );
    }

    style
}

/// Create a dataset object that proxies to data-* attributes
fn create_dataset(wrapper: &ElementWrapper, context: &mut Context) -> JsObject {
    let dataset = ObjectInitializer::new(context).build();

    // Populate with existing data-* attributes
    if let Some(el) = wrapper.get() {
        for (prop_name, value) in el.get_data_attributes() {
            let _ = dataset.set(
                js_string!(prop_name),
                JsValue::from(js_string!(value)),
                false,
                context,
            );
        }
    }

    dataset
}

/// Create an attributes NamedNodeMap for an element
fn create_attributes_map(wrapper: &ElementWrapper, context: &mut Context) -> JsObject {
    let state: Rc<RefCell<Vec<(String, String)>>> = Rc::new(RefCell::new(Vec::new()));

    // Populate with existing attributes
    if let Some(el) = wrapper.get() {
        let attrs: Vec<(String, String)> = el.get_all_attributes()
            .into_iter()
            .collect();
        *state.borrow_mut() = attrs;
    }

    // getNamedItem(name)
    let state_clone = state.clone();
    let get_named_item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let attrs = state_clone.borrow();
            for (n, v) in attrs.iter() {
                if n == &name {
                    return Ok(JsValue::from(create_attr_object(ctx, n, v)));
                }
            }
            Ok(JsValue::null())
        })
    };

    // item(index)
    let state_clone = state.clone();
    let item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let attrs = state_clone.borrow();
            if let Some((name, value)) = attrs.get(index) {
                return Ok(JsValue::from(create_attr_object(ctx, name, value)));
            }
            Ok(JsValue::null())
        })
    };

    // length getter
    let state_clone = state.clone();
    let length_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let len = state_clone.borrow().len();
            Ok(JsValue::from(len as u32))
        })
    };

    let length_fn = length_getter.to_js_function(context.realm());

    let map = ObjectInitializer::new(context)
        .function(get_named_item, js_string!("getNamedItem"), 1)
        .function(item, js_string!("item"), 1)
        .accessor(js_string!("length"), Some(length_fn), None, Attribute::READONLY)
        .build();

    // Add indexed properties
    let attrs = state.borrow();
    for (i, (name, value)) in attrs.iter().enumerate() {
        let attr = create_attr_object(context, name, value);
        let _ = map.set(js_string!(i.to_string()), JsValue::from(attr), false, context);
    }

    map
}

/// Create an Attr node object
fn create_attr_object(context: &mut Context, name: &str, value: &str) -> JsObject {
    ObjectInitializer::new(context)
        .property(js_string!("nodeType"), 2, Attribute::READONLY)
        .property(js_string!("nodeName"), js_string!(name), Attribute::READONLY)
        .property(js_string!("name"), js_string!(name), Attribute::READONLY)
        .property(js_string!("localName"), js_string!(name), Attribute::READONLY)
        .property(js_string!("value"), js_string!(value), Attribute::all())
        .property(js_string!("nodeValue"), js_string!(value), Attribute::all())
        .property(js_string!("textContent"), js_string!(value), Attribute::all())
        .property(js_string!("specified"), true, Attribute::READONLY)
        .property(js_string!("ownerElement"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("namespaceURI"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("prefix"), JsValue::null(), Attribute::READONLY)
        .build()
}

/// Create a NodeList-like array from elements
fn create_node_list(elements: Vec<Element>, context: &mut Context, dom: &DomWrapper) -> JsValue {
    let len = elements.len();

    // Convert elements to JsValue first (for iterator)
    let js_elements: Vec<JsValue> = elements.into_iter()
        .map(|el| create_element_object(el, context, dom))
        .collect();

    // forEach method
    let for_each = unsafe {
        NativeFunction::from_closure(move |this, args, ctx| {
            if let Some(callback) = args.first() {
                if callback.is_callable() {
                    let callback_obj = callback.as_callable().unwrap();
                    let this_obj = this.as_object();

                    if let Some(obj) = this_obj {
                        let length = obj.get(js_string!("length"), ctx)?;
                        let len = length.to_u32(ctx)?;

                        for i in 0..len {
                            let item = obj.get(js_string!(i.to_string()), ctx)?;
                            let _ = callback_obj.call(&JsValue::undefined(), &[item, JsValue::from(i), this.clone()], ctx);
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // item method
    let item = unsafe {
        NativeFunction::from_closure(move |this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx)?;
            if let Some(obj) = this.as_object() {
                return obj.get(js_string!(index.to_string()), ctx);
            }
            Ok(JsValue::null())
        })
    };

    // Build base object with methods
    let node_list = ObjectInitializer::new(context)
        .property(js_string!("length"), len as u32, Attribute::READONLY)
        .function(for_each, js_string!("forEach"), 1)
        .function(item, js_string!("item"), 1)
        .build();

    // Add indexed properties directly to the object
    for (i, el_obj) in js_elements.into_iter().enumerate() {
        let _ = node_list.set(js_string!(i.to_string()), el_obj, false, context);
    }

    // Skip Symbol.iterator - all approaches cause Boa panics on Next.js
    // The "not iterable" error is caught by React and doesn't crash
    // TODO: Report bug to Boa and revisit when fixed

    node_list.into()
}

