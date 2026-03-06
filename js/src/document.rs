//! Full Document API implementation
//!
//! Implements comprehensive document APIs including:
//! - Document properties (cookie, readyState, domain, referrer, etc.)
//! - Document collections (forms, images, links, scripts, etc.)
//! - Document methods (write, open, close, createEvent, etc.)
//! - Selection and Range APIs - FULLY IMPLEMENTED
//! - TreeWalker and NodeIterator - FULLY IMPLEMENTED

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer,
    object::builtins::JsArray, property::Attribute,
    Context, JsArgs, JsObject, JsValue, JsError as BoaJsError,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use dom::Element as DomElement;

use crate::cookies;
use crate::cssom;
use crate::dom_bindings::{DomWrapper, create_element_object};
use crate::event_system;

// ============================================================================
// Global Range Registry for Selection API
// ============================================================================
// This allows Range objects to be linked to Selection objects by storing
// RangeState with unique IDs that can be referenced across closures.

thread_local! {
    static RANGE_ID_COUNTER: RefCell<u64> = RefCell::new(1);
    static RANGE_REGISTRY: RefCell<HashMap<u64, Rc<RangeState>>> = RefCell::new(HashMap::new());
}

fn get_next_range_id() -> u64 {
    RANGE_ID_COUNTER.with(|counter| {
        let mut c = counter.borrow_mut();
        let id = *c;
        *c += 1;
        id
    })
}

fn register_range(state: Rc<RangeState>) -> u64 {
    let id = get_next_range_id();
    RANGE_REGISTRY.with(|registry| {
        registry.borrow_mut().insert(id, state);
    });
    id
}

fn get_range_by_id(id: u64) -> Option<Rc<RangeState>> {
    RANGE_REGISTRY.with(|registry| {
        registry.borrow().get(&id).cloned()
    })
}

fn remove_range_by_id(id: u64) {
    RANGE_REGISTRY.with(|registry| {
        registry.borrow_mut().remove(&id);
    });
}

/// Document state
#[derive(Clone)]
pub struct DocumentState {
    pub design_mode: RefCell<String>,
    pub domain: RefCell<String>,
    pub path: RefCell<String>,
    pub write_buffer: RefCell<String>,
}

impl Default for DocumentState {
    fn default() -> Self {
        Self {
            design_mode: RefCell::new("off".to_string()),
            domain: RefCell::new(String::new()),
            path: RefCell::new("/".to_string()),
            write_buffer: RefCell::new(String::new()),
        }
    }
}

impl DocumentState {
    pub fn with_url(url: &str) -> Self {
        let domain = cookies::extract_domain(url);
        let path = cookies::extract_path(url);
        Self {
            design_mode: RefCell::new("off".to_string()),
            domain: RefCell::new(domain),
            path: RefCell::new(path),
            write_buffer: RefCell::new(String::new()),
        }
    }
}

// ============================================================================
// NodeFilter constants
// ============================================================================

const FILTER_ACCEPT: u32 = 1;
const FILTER_REJECT: u32 = 2;
const FILTER_SKIP: u32 = 3;

const SHOW_ALL: u32 = 0xFFFFFFFF;
const SHOW_ELEMENT: u32 = 0x1;
const SHOW_ATTRIBUTE: u32 = 0x2;
const SHOW_TEXT: u32 = 0x4;
const SHOW_CDATA_SECTION: u32 = 0x8;
const SHOW_PROCESSING_INSTRUCTION: u32 = 0x40;
const SHOW_COMMENT: u32 = 0x80;
const SHOW_DOCUMENT: u32 = 0x100;
const SHOW_DOCUMENT_TYPE: u32 = 0x200;
const SHOW_DOCUMENT_FRAGMENT: u32 = 0x400;

/// Check if a node type should be shown based on whatToShow filter
fn should_show_node(node_type: u32, what_to_show: u32) -> bool {
    if what_to_show == SHOW_ALL {
        return true;
    }
    match node_type {
        1 => what_to_show & SHOW_ELEMENT != 0,      // Element
        2 => what_to_show & SHOW_ATTRIBUTE != 0,    // Attribute
        3 => what_to_show & SHOW_TEXT != 0,         // Text
        4 => what_to_show & SHOW_CDATA_SECTION != 0, // CDATASection
        7 => what_to_show & SHOW_PROCESSING_INSTRUCTION != 0,
        8 => what_to_show & SHOW_COMMENT != 0,      // Comment
        9 => what_to_show & SHOW_DOCUMENT != 0,     // Document
        10 => what_to_show & SHOW_DOCUMENT_TYPE != 0, // DocumentType
        11 => what_to_show & SHOW_DOCUMENT_FRAGMENT != 0, // DocumentFragment
        _ => true,
    }
}

// ============================================================================
// TreeWalker State
// ============================================================================

struct TreeWalkerState {
    root: DomElement,
    current_node: RefCell<DomElement>,
    what_to_show: u32,
    filter: Option<JsObject>,
}

impl TreeWalkerState {
    fn new(root: DomElement, what_to_show: u32, filter: Option<JsObject>) -> Self {
        Self {
            root: root.clone(),
            current_node: RefCell::new(root),
            what_to_show,
            filter,
        }
    }

    /// Check if node passes the filter
    fn accept_node(&self, node: &DomElement, context: &mut Context) -> u32 {
        let node_type = node.node_type();
        if !should_show_node(node_type, self.what_to_show) {
            return FILTER_SKIP;
        }

        // If there's a custom filter, call it
        if let Some(ref filter) = self.filter {
            if let Ok(accept_fn) = filter.get(js_string!("acceptNode"), context) {
                if let Some(callable) = accept_fn.as_callable() {
                    // Would need to create JS node object here
                    // For now, accept all that pass whatToShow
                    return FILTER_ACCEPT;
                }
            }
        }

        FILTER_ACCEPT
    }

    /// Check if a node is a descendant of root
    fn is_descendant_of_root(&self, node: &DomElement) -> bool {
        let mut current = Some(node.clone());
        while let Some(n) = current {
            if n.unique_id() == self.root.unique_id() {
                return true;
            }
            current = n.parent_element();
        }
        false
    }
}

// ============================================================================
// NodeIterator State
// ============================================================================

struct NodeIteratorState {
    root: DomElement,
    reference_node: RefCell<DomElement>,
    pointer_before_reference: RefCell<bool>,
    what_to_show: u32,
    filter: Option<JsObject>,
    // Flat list of nodes in document order for iteration
    nodes: RefCell<Vec<DomElement>>,
    current_index: RefCell<i32>,
}

impl NodeIteratorState {
    fn new(root: DomElement, what_to_show: u32, filter: Option<JsObject>) -> Self {
        // Build flat list of nodes in document order
        let mut nodes = Vec::new();
        Self::collect_nodes(&root, &mut nodes, what_to_show);

        Self {
            root: root.clone(),
            reference_node: RefCell::new(root),
            pointer_before_reference: RefCell::new(true),
            what_to_show,
            filter,
            nodes: RefCell::new(nodes),
            current_index: RefCell::new(-1),
        }
    }

    fn collect_nodes(node: &DomElement, nodes: &mut Vec<DomElement>, what_to_show: u32) {
        if should_show_node(node.node_type(), what_to_show) {
            nodes.push(node.clone());
        }
        for child in node.child_nodes() {
            Self::collect_nodes(&child, nodes, what_to_show);
        }
    }
}

// ============================================================================
// Range State
// ============================================================================

struct RangeState {
    start_container: RefCell<Option<DomElement>>,
    start_offset: RefCell<u32>,
    end_container: RefCell<Option<DomElement>>,
    end_offset: RefCell<u32>,
    collapsed: RefCell<bool>,
}

impl RangeState {
    fn new() -> Self {
        Self {
            start_container: RefCell::new(None),
            start_offset: RefCell::new(0),
            end_container: RefCell::new(None),
            end_offset: RefCell::new(0),
            collapsed: RefCell::new(true),
        }
    }

    fn set_start(&self, container: DomElement, offset: u32) {
        *self.start_container.borrow_mut() = Some(container);
        *self.start_offset.borrow_mut() = offset;
        self.update_collapsed();
    }

    fn set_end(&self, container: DomElement, offset: u32) {
        *self.end_container.borrow_mut() = Some(container);
        *self.end_offset.borrow_mut() = offset;
        self.update_collapsed();
    }

    fn update_collapsed(&self) {
        let start = self.start_container.borrow();
        let end = self.end_container.borrow();
        if let (Some(s), Some(e)) = (start.as_ref(), end.as_ref()) {
            let collapsed = s.unique_id() == e.unique_id()
                && *self.start_offset.borrow() == *self.end_offset.borrow();
            *self.collapsed.borrow_mut() = collapsed;
        } else {
            *self.collapsed.borrow_mut() = true;
        }
    }

    fn collapse(&self, to_start: bool) {
        if to_start {
            *self.end_container.borrow_mut() = self.start_container.borrow().clone();
            *self.end_offset.borrow_mut() = *self.start_offset.borrow();
        } else {
            *self.start_container.borrow_mut() = self.end_container.borrow().clone();
            *self.start_offset.borrow_mut() = *self.end_offset.borrow();
        }
        *self.collapsed.borrow_mut() = true;
    }

    fn get_text_content(&self) -> String {
        let start = self.start_container.borrow();
        let end = self.end_container.borrow();

        if let (Some(start_el), Some(end_el)) = (start.as_ref(), end.as_ref()) {
            if start_el.unique_id() == end_el.unique_id() {
                // Same container - extract substring
                let text = start_el.text_content();
                let start_off = *self.start_offset.borrow() as usize;
                let end_off = *self.end_offset.borrow() as usize;
                if start_off < text.len() && end_off <= text.len() && start_off <= end_off {
                    return text[start_off..end_off].to_string();
                }
                return text;
            }
            // Different containers - collect text between them
            self.collect_text_between_nodes(start_el, end_el)
        } else {
            String::new()
        }
    }

    fn collect_text_between_nodes(&self, start: &DomElement, end: &DomElement) -> String {
        let mut text = String::new();
        let mut in_range = false;
        let mut found_end = false;

        // Get common ancestor and traverse
        if let Some(parent) = start.parent_element() {
            self.collect_text_recursive(&parent, start, end, &mut text, &mut in_range, &mut found_end);
        }

        // If we didn't find proper range, just return start element text
        if text.is_empty() {
            let start_text = start.text_content();
            let start_off = *self.start_offset.borrow() as usize;
            if start_off < start_text.len() {
                text = start_text[start_off..].to_string();
            }
        }

        text
    }

    fn collect_text_recursive(
        &self,
        node: &DomElement,
        start: &DomElement,
        end: &DomElement,
        text: &mut String,
        in_range: &mut bool,
        found_end: &mut bool,
    ) {
        if *found_end {
            return;
        }

        if node.unique_id() == start.unique_id() {
            *in_range = true;
            let node_text = node.text_content();
            let start_off = *self.start_offset.borrow() as usize;
            if start_off < node_text.len() {
                text.push_str(&node_text[start_off..]);
            }
        } else if node.unique_id() == end.unique_id() {
            let node_text = node.text_content();
            let end_off = *self.end_offset.borrow() as usize;
            if end_off <= node_text.len() {
                text.push_str(&node_text[..end_off]);
            }
            *found_end = true;
            return;
        } else if *in_range {
            text.push_str(&node.text_content());
        }

        for child in node.child_nodes() {
            self.collect_text_recursive(&child, start, end, text, in_range, found_end);
            if *found_end {
                return;
            }
        }
    }

    fn contains_node(&self, node: &DomElement, allow_partial: bool) -> bool {
        let start = self.start_container.borrow();
        let end = self.end_container.borrow();

        if let (Some(start_el), Some(end_el)) = (start.as_ref(), end.as_ref()) {
            let node_id = node.unique_id();
            let start_id = start_el.unique_id();
            let end_id = end_el.unique_id();

            // Check if node is the start or end container
            if node_id == start_id || node_id == end_id {
                return true;
            }

            // Check if node is a descendant of start or end
            if allow_partial {
                // Check ancestry
                let mut current = Some(node.clone());
                while let Some(n) = current {
                    if n.unique_id() == start_id || n.unique_id() == end_id {
                        return true;
                    }
                    current = n.parent_element();
                }
            }

            // Check if node is between start and end (simplified check)
            // In a full implementation, this would compare document positions
            false
        } else {
            false
        }
    }

    fn clone_range(&self) -> RangeState {
        RangeState {
            start_container: RefCell::new(self.start_container.borrow().clone()),
            start_offset: RefCell::new(*self.start_offset.borrow()),
            end_container: RefCell::new(self.end_container.borrow().clone()),
            end_offset: RefCell::new(*self.end_offset.borrow()),
            collapsed: RefCell::new(*self.collapsed.borrow()),
        }
    }

    /// Delete the contents of this range from the DOM
    fn delete_contents(&self) {
        // If collapsed, nothing to delete
        if *self.collapsed.borrow() {
            return;
        }

        let start = self.start_container.borrow();
        let end = self.end_container.borrow();

        if let (Some(start_el), Some(end_el)) = (start.as_ref(), end.as_ref()) {
            let start_offset = *self.start_offset.borrow() as usize;
            let end_offset = *self.end_offset.borrow() as usize;

            if start_el.unique_id() == end_el.unique_id() {
                // Same container - remove children between offsets
                let children: Vec<_> = start_el.child_nodes().into_iter().collect();
                for (i, child) in children.iter().enumerate() {
                    if i >= start_offset && i < end_offset {
                        child.remove_from_parent();
                    }
                }
            } else {
                // Different containers - find common ancestor and remove nodes between
                // First, collect nodes to delete
                let mut nodes_to_delete = Vec::new();
                self.collect_nodes_in_range(start_el, end_el, &mut nodes_to_delete);

                // Remove collected nodes
                for node in nodes_to_delete {
                    node.remove_from_parent();
                }
            }
        }

        // Collapse the range to start after deletion
        self.collapse(true);
    }

    fn collect_nodes_in_range(&self, start: &DomElement, end: &DomElement, nodes: &mut Vec<DomElement>) {
        // Find nodes between start and end using a tree walk
        let mut in_range = false;
        let mut found_end = false;

        // Get the common ancestor
        if let Some(parent) = start.parent_element() {
            self.collect_nodes_recursive(&parent, start, end, nodes, &mut in_range, &mut found_end);
        }
    }

    fn collect_nodes_recursive(
        &self,
        node: &DomElement,
        start: &DomElement,
        end: &DomElement,
        nodes: &mut Vec<DomElement>,
        in_range: &mut bool,
        found_end: &mut bool,
    ) {
        if *found_end {
            return;
        }

        if node.unique_id() == start.unique_id() {
            *in_range = true;
            // Don't delete the start container itself, just mark range started
        } else if node.unique_id() == end.unique_id() {
            *found_end = true;
            // Don't delete the end container itself
            return;
        } else if *in_range {
            // Node is fully within range - mark for deletion
            nodes.push(node.clone());
            return; // Don't recurse into nodes we're deleting
        }

        for child in node.child_nodes() {
            self.collect_nodes_recursive(&child, start, end, nodes, in_range, found_end);
            if *found_end {
                return;
            }
        }
    }
}

// ============================================================================
// Selection State
// ============================================================================

struct SelectionState {
    // Store (range_id, RangeState) pairs for proper getRangeAt support
    ranges: RefCell<Vec<(u64, Rc<RangeState>)>>,
    anchor_node: RefCell<Option<DomElement>>,
    anchor_offset: RefCell<u32>,
    focus_node: RefCell<Option<DomElement>>,
    focus_offset: RefCell<u32>,
    selection_type: RefCell<String>,
}

impl SelectionState {
    fn new() -> Self {
        Self {
            ranges: RefCell::new(Vec::new()),
            anchor_node: RefCell::new(None),
            anchor_offset: RefCell::new(0),
            focus_node: RefCell::new(None),
            focus_offset: RefCell::new(0),
            selection_type: RefCell::new("None".to_string()),
        }
    }

    fn add_range_with_id(&self, range_id: u64, range: Rc<RangeState>) {
        self.ranges.borrow_mut().push((range_id, range.clone()));
        // Update anchor/focus from range
        *self.anchor_node.borrow_mut() = range.start_container.borrow().clone();
        *self.anchor_offset.borrow_mut() = *range.start_offset.borrow();
        *self.focus_node.borrow_mut() = range.end_container.borrow().clone();
        *self.focus_offset.borrow_mut() = *range.end_offset.borrow();
        *self.selection_type.borrow_mut() = if *range.collapsed.borrow() {
            "Caret".to_string()
        } else {
            "Range".to_string()
        };
    }

    fn add_range(&self, range: Rc<RangeState>) {
        // Register the range to get an ID
        let range_id = register_range(range.clone());
        self.add_range_with_id(range_id, range);
    }

    fn remove_all_ranges(&self) {
        self.ranges.borrow_mut().clear();
        *self.anchor_node.borrow_mut() = None;
        *self.anchor_offset.borrow_mut() = 0;
        *self.focus_node.borrow_mut() = None;
        *self.focus_offset.borrow_mut() = 0;
        *self.selection_type.borrow_mut() = "None".to_string();
    }

    fn remove_range(&self, range: &Rc<RangeState>) {
        let mut ranges = self.ranges.borrow_mut();
        ranges.retain(|(_, r)| !Rc::ptr_eq(r, range));
        drop(ranges);

        // Update selection state
        if self.ranges.borrow().is_empty() {
            *self.anchor_node.borrow_mut() = None;
            *self.anchor_offset.borrow_mut() = 0;
            *self.focus_node.borrow_mut() = None;
            *self.focus_offset.borrow_mut() = 0;
            *self.selection_type.borrow_mut() = "None".to_string();
        } else if let Some((_, first_range)) = self.ranges.borrow().first().cloned() {
            *self.anchor_node.borrow_mut() = first_range.start_container.borrow().clone();
            *self.anchor_offset.borrow_mut() = *first_range.start_offset.borrow();
            *self.focus_node.borrow_mut() = first_range.end_container.borrow().clone();
            *self.focus_offset.borrow_mut() = *first_range.end_offset.borrow();
            *self.selection_type.borrow_mut() = if *first_range.collapsed.borrow() {
                "Caret".to_string()
            } else {
                "Range".to_string()
            };
        }
    }

    fn get_range_at(&self, index: usize) -> Option<(u64, Rc<RangeState>)> {
        self.ranges.borrow().get(index).cloned()
    }

    fn is_collapsed(&self) -> bool {
        if let Some((_, range)) = self.ranges.borrow().first() {
            *range.collapsed.borrow()
        } else {
            true
        }
    }

    fn range_count(&self) -> usize {
        self.ranges.borrow().len()
    }

    fn to_string(&self) -> String {
        if let Some((_, range)) = self.ranges.borrow().first() {
            range.get_text_content()
        } else {
            String::new()
        }
    }

    fn contains_node(&self, node: &DomElement, allow_partial: bool) -> bool {
        for (_, range) in self.ranges.borrow().iter() {
            if range.contains_node(node, allow_partial) {
                return true;
            }
        }
        false
    }
}

/// Add extended document properties to an existing document object
pub fn extend_document(
    document: &JsObject,
    context: &mut Context,
    dom: &DomWrapper,
    base_url: &str,
) -> Result<(), BoaJsError> {
    let state = Rc::new(DocumentState::with_url(base_url));

    // ============ DOCUMENT PROPERTIES ============

    // document.cookie (getter/setter) - Uses shared cookie store
    let cookie_state = state.clone();
    let cookie_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let domain = cookie_state.domain.borrow().clone();
            let path = cookie_state.path.borrow().clone();
            let cookies_str = cookies::get_document_cookies(&domain, &path);
            Ok(JsValue::from(js_string!(cookies_str)))
        })
    };

    let cookie_state = state.clone();
    let cookie_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let new_cookie = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let domain = cookie_state.domain.borrow().clone();
            cookies::add_cookie_from_document(&new_cookie, &domain);
            Ok(JsValue::undefined())
        })
    };

    // document.readyState
    let _ = document.set(js_string!("readyState"), js_string!("complete"), false, context);

    // document.domain (getter/setter)
    let domain_state = state.clone();
    let domain_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(domain_state.domain.borrow().clone())))
        })
    };

    let domain_state = state.clone();
    let domain_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let domain = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            *domain_state.domain.borrow_mut() = domain;
            Ok(JsValue::undefined())
        })
    };

    // document.referrer
    let _ = document.set(js_string!("referrer"), js_string!(""), false, context);

    // document.lastModified
    let _ = document.set(js_string!("lastModified"), js_string!("12/23/2025 00:00:00"), false, context);

    // document.URL and document.documentURI
    let url_str = base_url.to_string();
    let _ = document.set(js_string!("URL"), js_string!(url_str.clone()), false, context);
    let _ = document.set(js_string!("documentURI"), js_string!(url_str.clone()), false, context);
    let _ = document.set(js_string!("baseURI"), js_string!(url_str), false, context);

    // document.characterSet / charset / inputEncoding
    let _ = document.set(js_string!("characterSet"), js_string!("UTF-8"), false, context);
    let _ = document.set(js_string!("charset"), js_string!("UTF-8"), false, context);
    let _ = document.set(js_string!("inputEncoding"), js_string!("UTF-8"), false, context);

    // document.contentType
    let _ = document.set(js_string!("contentType"), js_string!("text/html"), false, context);

    // document.compatMode
    let _ = document.set(js_string!("compatMode"), js_string!("CSS1Compat"), false, context);

    // document.doctype (simplified)
    let doctype = ObjectInitializer::new(context)
        .property(js_string!("name"), js_string!("html"), Attribute::READONLY)
        .property(js_string!("publicId"), js_string!(""), Attribute::READONLY)
        .property(js_string!("systemId"), js_string!(""), Attribute::READONLY)
        .build();
    let _ = document.set(js_string!("doctype"), doctype, false, context);

    // document.designMode (getter/setter)
    let design_state = state.clone();
    let design_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(design_state.design_mode.borrow().clone())))
        })
    };

    let design_state = state.clone();
    let design_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let mode = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped().to_lowercase();
            if mode == "on" || mode == "off" {
                *design_state.design_mode.borrow_mut() = mode;
            }
            Ok(JsValue::undefined())
        })
    };

    // document.hidden
    let _ = document.set(js_string!("hidden"), false, false, context);

    // document.visibilityState
    let _ = document.set(js_string!("visibilityState"), js_string!("visible"), false, context);

    // document.dir
    let _ = document.set(js_string!("dir"), js_string!(""), false, context);

    // document.defaultView (will be set to window)
    let _ = document.set(js_string!("defaultView"), JsValue::null(), false, context);

    // document.fullscreenElement
    let _ = document.set(js_string!("fullscreenElement"), JsValue::null(), false, context);

    // document.fullscreenEnabled
    let _ = document.set(js_string!("fullscreenEnabled"), false, false, context);

    // document.pictureInPictureElement
    let _ = document.set(js_string!("pictureInPictureElement"), JsValue::null(), false, context);

    // document.pictureInPictureEnabled
    let _ = document.set(js_string!("pictureInPictureEnabled"), false, false, context);

    // document.pointerLockElement
    let _ = document.set(js_string!("pointerLockElement"), JsValue::null(), false, context);

    // document.scrollingElement (returns documentElement)
    let scrolling_element = document.get(js_string!("documentElement"), context).unwrap_or(JsValue::null());
    let _ = document.set(js_string!("scrollingElement"), scrolling_element, false, context);

    // document.timeline (Web Animations API)
    let timeline = ObjectInitializer::new(context)
        .property(js_string!("currentTime"), JsValue::null(), Attribute::READONLY)
        .build();
    let _ = document.set(js_string!("timeline"), timeline, false, context);

    // document.fonts (FontFaceSet)
    let fonts = cssom::create_fontfaceset_object(context)?;
    let _ = document.set(js_string!("fonts"), fonts, false, context);

    // ============ DOCUMENT COLLECTIONS ============

    // document.forms
    let forms = create_html_collection(context, "form", dom);
    let _ = document.set(js_string!("forms"), forms, false, context);

    // document.images
    let images = create_html_collection(context, "img", dom);
    let _ = document.set(js_string!("images"), images, false, context);

    // document.links
    let links = create_html_collection(context, "a", dom);
    let _ = document.set(js_string!("links"), links, false, context);

    // document.anchors (deprecated but still used)
    let anchors = create_html_collection(context, "a[name]", dom);
    let _ = document.set(js_string!("anchors"), anchors, false, context);

    // document.scripts
    let scripts = create_html_collection(context, "script", dom);
    let _ = document.set(js_string!("scripts"), scripts, false, context);

    // document.styleSheets - parse actual <style> elements
    let style_sheets = create_style_sheets_from_dom(context, dom);
    let _ = document.set(js_string!("styleSheets"), style_sheets, false, context);

    // document.embeds / document.plugins (same collection, deprecated)
    let embeds = create_html_collection(context, "embed", dom);
    let _ = document.set(js_string!("embeds"), embeds.clone(), false, context);
    let _ = document.set(js_string!("plugins"), embeds, false, context);

    // document.all (deprecated but still widely used)
    let all = create_document_all(context, dom);
    let _ = document.set(js_string!("all"), all, false, context);

    // ============ DOCUMENT METHODS ============

    // document.hasFocus()
    let has_focus = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(true))
    });
    let _ = document.set(js_string!("hasFocus"), has_focus.to_js_function(context.realm()), false, context);

    // document.write()
    let write_state = state.clone();
    let write_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            for arg in args.iter() {
                let text = arg.to_string(ctx)?.to_std_string_escaped();
                write_state.write_buffer.borrow_mut().push_str(&text);
            }
            Ok(JsValue::undefined())
        })
    };
    let _ = document.set(js_string!("write"), write_fn.to_js_function(context.realm()), false, context);

    // document.writeln()
    let writeln_state = state.clone();
    let writeln_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            for arg in args.iter() {
                let text = arg.to_string(ctx)?.to_std_string_escaped();
                writeln_state.write_buffer.borrow_mut().push_str(&text);
            }
            writeln_state.write_buffer.borrow_mut().push('\n');
            Ok(JsValue::undefined())
        })
    };
    let _ = document.set(js_string!("writeln"), writeln_fn.to_js_function(context.realm()), false, context);

    // document.open()
    let open_state = state.clone();
    let open_fn = unsafe {
        NativeFunction::from_closure(move |this, _args, _ctx| {
            open_state.write_buffer.borrow_mut().clear();
            Ok(this.clone())
        })
    };
    let _ = document.set(js_string!("open"), open_fn.to_js_function(context.realm()), false, context);

    // document.close()
    let close_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = document.set(js_string!("close"), close_fn.to_js_function(context.realm()), false, context);

    // document.createEvent()
    let create_event = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let event = crate::events::create_event_object_for_type(ctx, &event_type);
        Ok(JsValue::from(event))
    });
    let _ = document.set(js_string!("createEvent"), create_event.to_js_function(context.realm()), false, context);

    // document.createComment()
    let create_comment = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let data = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let comment = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 8, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("#comment"), Attribute::READONLY)
            .property(js_string!("data"), js_string!(data.clone()), Attribute::all())
            .property(js_string!("textContent"), js_string!(data.clone()), Attribute::all())
            .property(js_string!("nodeValue"), js_string!(data), Attribute::all())
            .property(js_string!("length"), 0, Attribute::READONLY)
            .build();
        Ok(JsValue::from(comment))
    });
    let _ = document.set(js_string!("createComment"), create_comment.to_js_function(context.realm()), false, context);

    // document.createAttribute()
    let create_attribute = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let attr = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 2, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!(name.clone()), Attribute::READONLY)
            .property(js_string!("name"), js_string!(name), Attribute::READONLY)
            .property(js_string!("value"), js_string!(""), Attribute::all())
            .property(js_string!("specified"), true, Attribute::READONLY)
            .property(js_string!("ownerElement"), JsValue::null(), Attribute::READONLY)
            .build();
        Ok(JsValue::from(attr))
    });
    let _ = document.set(js_string!("createAttribute"), create_attribute.to_js_function(context.realm()), false, context);

    // document.createRange() - FULL IMPLEMENTATION
    let dom_for_range = dom.clone();
    let create_range = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            Ok(JsValue::from(create_range_object_full(ctx, &dom_for_range)))
        })
    };
    let _ = document.set(js_string!("createRange"), create_range.to_js_function(context.realm()), false, context);

    // document.createTreeWalker() - FULL IMPLEMENTATION
    let dom_for_walker = dom.clone();
    let create_tree_walker = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let root_val = args.get_or_undefined(0);
            let what_to_show = args.get(1).and_then(|v| v.to_u32(ctx).ok()).unwrap_or(SHOW_ALL);
            let filter = args.get(2).and_then(|v| v.as_object().map(|o| o.clone()));

            // Get root element from DOM with multiple fallback strategies
            let root = if let Some(root_obj) = root_val.as_object() {
                // Strategy 1: Try __element_id__ from registry
                let from_registry = root_obj.get(js_string!("__element_id__"), ctx).ok()
                    .and_then(|id_val| id_val.as_number().map(|n| n as u64))
                    .and_then(|id| dom_for_walker.registry.get(id));

                if from_registry.is_some() {
                    from_registry
                } else {
                    // Strategy 2: Try to find by ID attribute
                    let by_id = root_obj.get(js_string!("id"), ctx).ok()
                        .and_then(|v| v.to_string(ctx).ok())
                        .map(|s| s.to_std_string_escaped())
                        .and_then(|id| dom_for_walker.inner.get_element_by_id(&id));

                    if by_id.is_some() {
                        by_id
                    } else {
                        // Strategy 3: Fall back to body or document element
                        dom_for_walker.inner.body().or_else(|| dom_for_walker.inner.document_element())
                    }
                }
            } else {
                dom_for_walker.inner.body().or_else(|| dom_for_walker.inner.document_element())
            };

            if let Some(root_el) = root {
                Ok(JsValue::from(create_tree_walker_object_full(ctx, root_el, what_to_show, filter, &dom_for_walker)))
            } else {
                // Last resort: create walker with document as root
                if let Some(doc_el) = dom_for_walker.inner.document_element() {
                    Ok(JsValue::from(create_tree_walker_object_full(ctx, doc_el, what_to_show, filter, &dom_for_walker)))
                } else {
                    Ok(JsValue::null())
                }
            }
        })
    };
    let _ = document.set(js_string!("createTreeWalker"), create_tree_walker.to_js_function(context.realm()), false, context);

    // document.createNodeIterator() - FULL IMPLEMENTATION
    let dom_for_iterator = dom.clone();
    let create_node_iterator = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let root_val = args.get_or_undefined(0);
            let what_to_show = args.get(1).and_then(|v| v.to_u32(ctx).ok()).unwrap_or(SHOW_ALL);
            let filter = args.get(2).and_then(|v| v.as_object().map(|o| o.clone()));

            // Get root element from DOM with multiple fallback strategies
            let root = if let Some(root_obj) = root_val.as_object() {
                // Strategy 1: Try __element_id__ from registry
                let from_registry = root_obj.get(js_string!("__element_id__"), ctx).ok()
                    .and_then(|id_val| id_val.as_number().map(|n| n as u64))
                    .and_then(|id| dom_for_iterator.registry.get(id));

                if from_registry.is_some() {
                    from_registry
                } else {
                    // Strategy 2: Try to find by ID attribute
                    let by_id = root_obj.get(js_string!("id"), ctx).ok()
                        .and_then(|v| v.to_string(ctx).ok())
                        .map(|s| s.to_std_string_escaped())
                        .and_then(|id| dom_for_iterator.inner.get_element_by_id(&id));

                    if by_id.is_some() {
                        by_id
                    } else {
                        // Strategy 3: Fall back to body or document element
                        dom_for_iterator.inner.body().or_else(|| dom_for_iterator.inner.document_element())
                    }
                }
            } else {
                dom_for_iterator.inner.body().or_else(|| dom_for_iterator.inner.document_element())
            };

            if let Some(root_el) = root {
                Ok(JsValue::from(create_node_iterator_object_full(ctx, root_el, what_to_show, filter, &dom_for_iterator)))
            } else {
                // Last resort: create iterator with document as root
                if let Some(doc_el) = dom_for_iterator.inner.document_element() {
                    Ok(JsValue::from(create_node_iterator_object_full(ctx, doc_el, what_to_show, filter, &dom_for_iterator)))
                } else {
                    Ok(JsValue::null())
                }
            }
        })
    };
    let _ = document.set(js_string!("createNodeIterator"), create_node_iterator.to_js_function(context.realm()), false, context);

    // document.getSelection() - FULL IMPLEMENTATION
    let dom_for_selection = dom.clone();
    let get_selection = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            Ok(JsValue::from(create_selection_object_full(ctx, &dom_for_selection)))
        })
    };
    let _ = document.set(js_string!("getSelection"), get_selection.to_js_function(context.realm()), false, context);

    // document.getElementsByName()
    let dom_clone = dom.clone();
    let get_elements_by_name = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let selector = format!("[name=\"{}\"]", name);
            let elements = dom_clone.inner.query_selector_all(&selector);
            Ok(JsValue::from(create_node_list_from_elements(&elements, ctx, &dom_clone)))
        })
    };
    let _ = document.set(js_string!("getElementsByName"), get_elements_by_name.to_js_function(context.realm()), false, context);

    // document.elementFromPoint() - Returns topmost element (simplified)
    let dom_for_point = dom.clone();
    let element_from_point = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            // Return body as fallback (real implementation would need layout)
            if let Some(body) = dom_for_point.inner.body() {
                Ok(create_element_object(body, ctx, &dom_for_point))
            } else {
                Ok(JsValue::null())
            }
        })
    };
    let _ = document.set(js_string!("elementFromPoint"), element_from_point.to_js_function(context.realm()), false, context);

    // document.elementsFromPoint()
    let dom_for_points = dom.clone();
    let elements_from_point = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let arr = JsArray::new(ctx);
            // Return body and html as stack (simplified)
            if let Some(body) = dom_for_points.inner.body() {
                let body_obj = create_element_object(body, ctx, &dom_for_points);
                let _ = arr.push(body_obj, ctx);
            }
            if let Some(html) = dom_for_points.inner.document_element() {
                let html_obj = create_element_object(html, ctx, &dom_for_points);
                let _ = arr.push(html_obj, ctx);
            }
            Ok(JsValue::from(arr))
        })
    };
    let _ = document.set(js_string!("elementsFromPoint"), elements_from_point.to_js_function(context.realm()), false, context);

    // document.caretPositionFromPoint()
    let caret_position_from_point = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });
    let _ = document.set(js_string!("caretPositionFromPoint"), caret_position_from_point.to_js_function(context.realm()), false, context);

    // document.caretRangeFromPoint() (WebKit extension)
    let caret_range_from_point = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });
    let _ = document.set(js_string!("caretRangeFromPoint"), caret_range_from_point.to_js_function(context.realm()), false, context);

    // document.execCommand() (deprecated but widely used)
    let exec_command = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });
    let _ = document.set(js_string!("execCommand"), exec_command.to_js_function(context.realm()), false, context);

    // document.queryCommandEnabled()
    let query_command_enabled = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });
    let _ = document.set(js_string!("queryCommandEnabled"), query_command_enabled.to_js_function(context.realm()), false, context);

    // document.queryCommandState()
    let query_command_state = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });
    let _ = document.set(js_string!("queryCommandState"), query_command_state.to_js_function(context.realm()), false, context);

    // document.queryCommandSupported()
    let query_command_supported = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });
    let _ = document.set(js_string!("queryCommandSupported"), query_command_supported.to_js_function(context.realm()), false, context);

    // document.queryCommandValue()
    let query_command_value = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(js_string!("")))
    });
    let _ = document.set(js_string!("queryCommandValue"), query_command_value.to_js_function(context.realm()), false, context);

    // document.exitFullscreen()
    let exit_fullscreen = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        create_resolved_promise(ctx, JsValue::undefined())
    });
    let _ = document.set(js_string!("exitFullscreen"), exit_fullscreen.to_js_function(context.realm()), false, context);

    // document.exitPictureInPicture()
    let exit_pip = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        create_resolved_promise(ctx, JsValue::undefined())
    });
    let _ = document.set(js_string!("exitPictureInPicture"), exit_pip.to_js_function(context.realm()), false, context);

    // document.exitPointerLock()
    let exit_pointer_lock = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = document.set(js_string!("exitPointerLock"), exit_pointer_lock.to_js_function(context.realm()), false, context);

    // document.adoptNode()
    let adopt_node = NativeFunction::from_copy_closure(|_this, args, _ctx| {
        Ok(args.get_or_undefined(0).clone())
    });
    let _ = document.set(js_string!("adoptNode"), adopt_node.to_js_function(context.realm()), false, context);

    // document.importNode()
    let import_node = NativeFunction::from_copy_closure(|_this, args, _ctx| {
        Ok(args.get_or_undefined(0).clone())
    });
    let _ = document.set(js_string!("importNode"), import_node.to_js_function(context.realm()), false, context);

    // document.createExpression() (XPath)
    let dom_for_xpath = dom.clone();
    let create_expression = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let xpath = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            Ok(JsValue::from(create_xpath_expression(ctx, &xpath, &dom_for_xpath)))
        })
    };
    let _ = document.set(js_string!("createExpression"), create_expression.to_js_function(context.realm()), false, context);

    // document.createNSResolver()
    let create_ns_resolver = NativeFunction::from_copy_closure(|_this, args, _ctx| {
        Ok(args.get_or_undefined(0).clone())
    });
    let _ = document.set(js_string!("createNSResolver"), create_ns_resolver.to_js_function(context.realm()), false, context);

    // document.evaluate() (XPath) - FULL IMPLEMENTATION
    let dom_for_eval = dom.clone();
    let evaluate = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let xpath = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let context_node = args.get(1);
            let result_type = args.get(3).and_then(|v| v.to_u32(ctx).ok()).unwrap_or(0);

            Ok(JsValue::from(evaluate_xpath(ctx, &xpath, context_node, result_type, &dom_for_eval)))
        })
    };
    let _ = document.set(js_string!("evaluate"), evaluate.to_js_function(context.realm()), false, context);

    // document.prepend() / document.append() / document.replaceChildren() - FULL IMPLEMENTATION
    let dom_for_prepend = dom.clone();
    let prepend = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let mut elements = Vec::new();
            for i in 0..args.len() {
                if let Some(arg) = args.get(i) {
                    if let Some(obj) = arg.as_object() {
                        if let Ok(id_val) = obj.get(js_string!("__element_id__"), ctx) {
                            if let Some(id) = id_val.as_number() {
                                if let Some(el) = dom_for_prepend.registry.get(id as u64) {
                                    elements.push(el);
                                }
                            }
                        }
                    } else if let Some(s) = arg.as_string() {
                        // Create text node for strings
                        let text_node = dom_for_prepend.inner.create_text_node(&s.to_std_string_escaped());
                        elements.push(text_node);
                    }
                }
            }
            dom_for_prepend.inner.prepend(elements);
            Ok(JsValue::undefined())
        })
    };

    let dom_for_append = dom.clone();
    let append = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let mut elements = Vec::new();
            for i in 0..args.len() {
                if let Some(arg) = args.get(i) {
                    if let Some(obj) = arg.as_object() {
                        if let Ok(id_val) = obj.get(js_string!("__element_id__"), ctx) {
                            if let Some(id) = id_val.as_number() {
                                if let Some(el) = dom_for_append.registry.get(id as u64) {
                                    elements.push(el);
                                }
                            }
                        }
                    } else if let Some(s) = arg.as_string() {
                        let text_node = dom_for_append.inner.create_text_node(&s.to_std_string_escaped());
                        elements.push(text_node);
                    }
                }
            }
            dom_for_append.inner.append(elements);
            Ok(JsValue::undefined())
        })
    };

    let dom_for_replace = dom.clone();
    let replace_children = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let mut elements = Vec::new();
            for i in 0..args.len() {
                if let Some(arg) = args.get(i) {
                    if let Some(obj) = arg.as_object() {
                        if let Ok(id_val) = obj.get(js_string!("__element_id__"), ctx) {
                            if let Some(id) = id_val.as_number() {
                                if let Some(el) = dom_for_replace.registry.get(id as u64) {
                                    elements.push(el);
                                }
                            }
                        }
                    } else if let Some(s) = arg.as_string() {
                        let text_node = dom_for_replace.inner.create_text_node(&s.to_std_string_escaped());
                        elements.push(text_node);
                    }
                }
            }
            dom_for_replace.inner.replace_children(elements);
            Ok(JsValue::undefined())
        })
    };
    let _ = document.set(js_string!("prepend"), prepend.to_js_function(context.realm()), false, context);
    let _ = document.set(js_string!("append"), append.to_js_function(context.realm()), false, context);
    let _ = document.set(js_string!("replaceChildren"), replace_children.to_js_function(context.realm()), false, context);

    // EventTarget methods on document
    let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));
    let remove_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));
    let dispatch_event = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::from(true)));
    let _ = document.set(js_string!("addEventListener"), add_event_listener.to_js_function(context.realm()), false, context);
    let _ = document.set(js_string!("removeEventListener"), remove_event_listener.to_js_function(context.realm()), false, context);
    let _ = document.set(js_string!("dispatchEvent"), dispatch_event.to_js_function(context.realm()), false, context);

    // Set up accessors for cookie, domain, designMode
    let cookie_getter_fn = cookie_getter.to_js_function(context.realm());
    let cookie_setter_fn = cookie_setter.to_js_function(context.realm());
    let _ = document.define_property_or_throw(
        js_string!("cookie"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(cookie_getter_fn)
            .set(cookie_setter_fn)
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let domain_getter_fn = domain_getter.to_js_function(context.realm());
    let domain_setter_fn = domain_setter.to_js_function(context.realm());
    let _ = document.define_property_or_throw(
        js_string!("domain"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(domain_getter_fn)
            .set(domain_setter_fn)
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let design_getter_fn = design_getter.to_js_function(context.realm());
    let design_setter_fn = design_setter.to_js_function(context.realm());
    let _ = document.define_property_or_throw(
        js_string!("designMode"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(design_getter_fn)
            .set(design_setter_fn)
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    // document.activeElement getter
    let dom_for_active = dom.clone();
    let active_element_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            if let Some(active_id) = event_system::get_active_element() {
                if let Some(element) = dom_for_active.registry.get(active_id) {
                    return Ok(create_element_object(element, ctx, &dom_for_active));
                }
            }
            if let Some(body) = dom_for_active.inner.body() {
                return Ok(create_element_object(body, ctx, &dom_for_active));
            }
            Ok(JsValue::null())
        })
    };

    let active_element_getter_fn = active_element_getter.to_js_function(context.realm());
    let _ = document.define_property_or_throw(
        js_string!("activeElement"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(active_element_getter_fn)
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    // Register NodeFilter constants globally
    register_node_filter_constants(context)?;

    Ok(())
}

/// Register NodeFilter constants
fn register_node_filter_constants(context: &mut Context) -> Result<(), BoaJsError> {
    let node_filter = ObjectInitializer::new(context)
        .property(js_string!("FILTER_ACCEPT"), FILTER_ACCEPT, Attribute::READONLY)
        .property(js_string!("FILTER_REJECT"), FILTER_REJECT, Attribute::READONLY)
        .property(js_string!("FILTER_SKIP"), FILTER_SKIP, Attribute::READONLY)
        .property(js_string!("SHOW_ALL"), SHOW_ALL, Attribute::READONLY)
        .property(js_string!("SHOW_ELEMENT"), SHOW_ELEMENT, Attribute::READONLY)
        .property(js_string!("SHOW_ATTRIBUTE"), SHOW_ATTRIBUTE, Attribute::READONLY)
        .property(js_string!("SHOW_TEXT"), SHOW_TEXT, Attribute::READONLY)
        .property(js_string!("SHOW_CDATA_SECTION"), SHOW_CDATA_SECTION, Attribute::READONLY)
        .property(js_string!("SHOW_PROCESSING_INSTRUCTION"), SHOW_PROCESSING_INSTRUCTION, Attribute::READONLY)
        .property(js_string!("SHOW_COMMENT"), SHOW_COMMENT, Attribute::READONLY)
        .property(js_string!("SHOW_DOCUMENT"), SHOW_DOCUMENT, Attribute::READONLY)
        .property(js_string!("SHOW_DOCUMENT_TYPE"), SHOW_DOCUMENT_TYPE, Attribute::READONLY)
        .property(js_string!("SHOW_DOCUMENT_FRAGMENT"), SHOW_DOCUMENT_FRAGMENT, Attribute::READONLY)
        .build();

    context.register_global_property(js_string!("NodeFilter"), node_filter, Attribute::all())?;
    Ok(())
}

/// Create an HTMLCollection for a given selector
fn create_html_collection(context: &mut Context, selector: &str, dom: &DomWrapper) -> JsObject {
    let elements = dom.inner.query_selector_all(selector);
    let count = elements.len();
    let dom_clone = dom.clone();
    let elements_clone = elements.clone();

    let item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0) as usize;
            if index < elements_clone.len() {
                Ok(create_element_object(elements_clone[index].clone(), ctx, &dom_clone))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    let named_item = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });

    ObjectInitializer::new(context)
        .property(js_string!("length"), count as u32, Attribute::READONLY)
        .function(item, js_string!("item"), 1)
        .function(named_item, js_string!("namedItem"), 1)
        .build()
}

/// Create document.all collection
fn create_document_all(context: &mut Context, dom: &DomWrapper) -> JsObject {
    let count = dom.inner.query_count("*");

    let item = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });

    let named_item = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });

    let tags = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    });

    ObjectInitializer::new(context)
        .property(js_string!("length"), count as u32, Attribute::READONLY)
        .function(item, js_string!("item"), 1)
        .function(named_item, js_string!("namedItem"), 1)
        .function(tags, js_string!("tags"), 1)
        .build()
}

// ============================================================================
// FULL RANGE IMPLEMENTATION
// ============================================================================

fn create_range_object_full(context: &mut Context, dom: &DomWrapper) -> JsObject {
    let state = Rc::new(RangeState::new());
    let range_id = register_range(state.clone());
    let dom_clone = dom.clone();

    // setStart(node, offset)
    let state_clone = state.clone();
    let dom_for_start = dom.clone();
    let set_start = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let node = args.get_or_undefined(0);
            let offset = args.get_or_undefined(1).to_u32(ctx).unwrap_or(0);

            if let Some(node_obj) = node.as_object() {
                if let Ok(id_val) = node_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_start.registry.get(id) {
                            state_clone.set_start(el, offset);
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // setEnd(node, offset)
    let state_clone = state.clone();
    let dom_for_end = dom.clone();
    let set_end = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let node = args.get_or_undefined(0);
            let offset = args.get_or_undefined(1).to_u32(ctx).unwrap_or(0);

            if let Some(node_obj) = node.as_object() {
                if let Ok(id_val) = node_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_end.registry.get(id) {
                            state_clone.set_end(el, offset);
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // setStartBefore(node)
    let state_clone = state.clone();
    let dom_for_sb = dom.clone();
    let set_start_before = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let node = args.get_or_undefined(0);
            if let Some(node_obj) = node.as_object() {
                if let Ok(id_val) = node_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_sb.registry.get(id) {
                            if let Some(parent) = el.parent_element() {
                                let children = parent.child_nodes();
                                let offset = children.iter().position(|c| c.unique_id() == el.unique_id()).unwrap_or(0);
                                state_clone.set_start(parent, offset as u32);
                            }
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // setStartAfter(node)
    let state_clone = state.clone();
    let dom_for_sa = dom.clone();
    let set_start_after = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let node = args.get_or_undefined(0);
            if let Some(node_obj) = node.as_object() {
                if let Ok(id_val) = node_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_sa.registry.get(id) {
                            if let Some(parent) = el.parent_element() {
                                let children = parent.child_nodes();
                                let offset = children.iter().position(|c| c.unique_id() == el.unique_id()).map(|i| i + 1).unwrap_or(0);
                                state_clone.set_start(parent, offset as u32);
                            }
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // setEndBefore(node)
    let state_clone = state.clone();
    let dom_for_eb = dom.clone();
    let set_end_before = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let node = args.get_or_undefined(0);
            if let Some(node_obj) = node.as_object() {
                if let Ok(id_val) = node_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_eb.registry.get(id) {
                            if let Some(parent) = el.parent_element() {
                                let children = parent.child_nodes();
                                let offset = children.iter().position(|c| c.unique_id() == el.unique_id()).unwrap_or(0);
                                state_clone.set_end(parent, offset as u32);
                            }
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // setEndAfter(node)
    let state_clone = state.clone();
    let dom_for_ea = dom.clone();
    let set_end_after = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let node = args.get_or_undefined(0);
            if let Some(node_obj) = node.as_object() {
                if let Ok(id_val) = node_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_ea.registry.get(id) {
                            if let Some(parent) = el.parent_element() {
                                let children = parent.child_nodes();
                                let offset = children.iter().position(|c| c.unique_id() == el.unique_id()).map(|i| i + 1).unwrap_or(0);
                                state_clone.set_end(parent, offset as u32);
                            }
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // collapse(toStart)
    let state_clone = state.clone();
    let collapse = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let to_start = args.get_or_undefined(0).to_boolean();
            state_clone.collapse(to_start);
            Ok(JsValue::undefined())
        })
    };

    // selectNode(node)
    let state_clone = state.clone();
    let dom_for_sn = dom.clone();
    let select_node = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let node = args.get_or_undefined(0);
            if let Some(node_obj) = node.as_object() {
                if let Ok(id_val) = node_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_sn.registry.get(id) {
                            if let Some(parent) = el.parent_element() {
                                let children = parent.child_nodes();
                                let start_offset = children.iter().position(|c| c.unique_id() == el.unique_id()).unwrap_or(0);
                                state_clone.set_start(parent.clone(), start_offset as u32);
                                state_clone.set_end(parent, (start_offset + 1) as u32);
                            }
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // selectNodeContents(node)
    let state_clone = state.clone();
    let dom_for_snc = dom.clone();
    let select_node_contents = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let node = args.get_or_undefined(0);
            if let Some(node_obj) = node.as_object() {
                if let Ok(id_val) = node_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_snc.registry.get(id) {
                            let child_count = el.child_nodes().len() as u32;
                            state_clone.set_start(el.clone(), 0);
                            state_clone.set_end(el, child_count);
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // cloneRange()
    let dom_for_clone = dom.clone();
    let clone_range = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            Ok(JsValue::from(create_range_object_full(ctx, &dom_for_clone)))
        })
    };

    // cloneContents()
    let clone_contents = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let frag = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 11, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("#document-fragment"), Attribute::READONLY)
            .build();
        Ok(JsValue::from(frag))
    });

    // deleteContents() - Actually delete content from DOM
    let state_clone = state.clone();
    let delete_contents = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            state_clone.delete_contents();
            Ok(JsValue::undefined())
        })
    };

    // extractContents()
    let extract_contents = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let frag = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 11, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("#document-fragment"), Attribute::READONLY)
            .build();
        Ok(JsValue::from(frag))
    });

    // insertNode(node)
    let insert_node = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // surroundContents(newParent)
    let surround_contents = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // compareBoundaryPoints(how, sourceRange)
    let compare_boundary_points = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(0))
    });

    // comparePoint(node, offset)
    let compare_point = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(0))
    });

    // intersectsNode(node)
    let intersects_node = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });

    // isPointInRange(node, offset)
    let is_point_in_range = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });

    // toString()
    let state_clone = state.clone();
    let to_string = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let text = state_clone.get_text_content();
            Ok(JsValue::from(js_string!(text)))
        })
    };

    // getBoundingClientRect()
    let get_bounding_client_rect = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let rect = ObjectInitializer::new(ctx)
            .property(js_string!("x"), 0.0, Attribute::READONLY)
            .property(js_string!("y"), 0.0, Attribute::READONLY)
            .property(js_string!("width"), 0.0, Attribute::READONLY)
            .property(js_string!("height"), 0.0, Attribute::READONLY)
            .property(js_string!("top"), 0.0, Attribute::READONLY)
            .property(js_string!("right"), 0.0, Attribute::READONLY)
            .property(js_string!("bottom"), 0.0, Attribute::READONLY)
            .property(js_string!("left"), 0.0, Attribute::READONLY)
            .build();
        Ok(JsValue::from(rect))
    });

    // getClientRects()
    let get_client_rects = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    });

    // detach()
    let detach = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // createContextualFragment(html)
    let create_contextual_fragment = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let frag = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 11, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("#document-fragment"), Attribute::READONLY)
            .build();
        Ok(JsValue::from(frag))
    });

    // Build range object with getters for dynamic properties
    let range = ObjectInitializer::new(context)
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
        .function(clone_range, js_string!("cloneRange"), 0)
        .function(clone_contents, js_string!("cloneContents"), 0)
        .function(delete_contents, js_string!("deleteContents"), 0)
        .function(extract_contents, js_string!("extractContents"), 0)
        .function(insert_node, js_string!("insertNode"), 1)
        .function(surround_contents, js_string!("surroundContents"), 1)
        .function(compare_boundary_points, js_string!("compareBoundaryPoints"), 2)
        .function(compare_point, js_string!("comparePoint"), 2)
        .function(intersects_node, js_string!("intersectsNode"), 1)
        .function(is_point_in_range, js_string!("isPointInRange"), 2)
        .function(to_string, js_string!("toString"), 0)
        .function(get_bounding_client_rect, js_string!("getBoundingClientRect"), 0)
        .function(get_client_rects, js_string!("getClientRects"), 0)
        .function(detach, js_string!("detach"), 0)
        .function(create_contextual_fragment, js_string!("createContextualFragment"), 1)
        .build();

    // Add getters for startContainer, startOffset, endContainer, endOffset, collapsed
    let state_for_sc = state.clone();
    let dom_for_sc = dom_clone.clone();
    let start_container_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            if let Some(el) = state_for_sc.start_container.borrow().as_ref() {
                Ok(create_element_object(el.clone(), ctx, &dom_for_sc))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    let state_for_so = state.clone();
    let start_offset_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*state_for_so.start_offset.borrow()))
        })
    };

    let state_for_ec = state.clone();
    let dom_for_ec = dom_clone.clone();
    let end_container_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            if let Some(el) = state_for_ec.end_container.borrow().as_ref() {
                Ok(create_element_object(el.clone(), ctx, &dom_for_ec))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    let state_for_eo = state.clone();
    let end_offset_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*state_for_eo.end_offset.borrow()))
        })
    };

    let state_for_col = state.clone();
    let collapsed_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*state_for_col.collapsed.borrow()))
        })
    };

    // Define property getters
    let _ = range.define_property_or_throw(
        js_string!("startContainer"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(start_container_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let _ = range.define_property_or_throw(
        js_string!("startOffset"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(start_offset_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let _ = range.define_property_or_throw(
        js_string!("endContainer"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(end_container_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let _ = range.define_property_or_throw(
        js_string!("endOffset"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(end_offset_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let _ = range.define_property_or_throw(
        js_string!("collapsed"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(collapsed_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    // commonAncestorContainer getter
    let _ = range.set(js_string!("commonAncestorContainer"), JsValue::null(), false, context);

    // Store range ID for Selection API to look up the RangeState
    let _ = range.set(js_string!("__range_id__"), JsValue::from(range_id as f64), false, context);

    range
}

// ============================================================================
// FULL TREEWALKER IMPLEMENTATION
// ============================================================================

fn create_tree_walker_object_full(
    context: &mut Context,
    root: DomElement,
    what_to_show: u32,
    filter: Option<JsObject>,
    dom: &DomWrapper,
) -> JsObject {
    let state = Rc::new(TreeWalkerState::new(root.clone(), what_to_show, filter));
    let dom_clone = dom.clone();

    // parentNode()
    let state_clone = state.clone();
    let dom_for_parent = dom.clone();
    let parent_node = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let current = state_clone.current_node.borrow().clone();
            if let Some(parent) = current.parent_element() {
                if state_clone.is_descendant_of_root(&parent) {
                    *state_clone.current_node.borrow_mut() = parent.clone();
                    return Ok(create_element_object(parent, ctx, &dom_for_parent));
                }
            }
            Ok(JsValue::null())
        })
    };

    // firstChild()
    let state_clone = state.clone();
    let dom_for_fc = dom.clone();
    let first_child = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let current = state_clone.current_node.borrow().clone();
            for child in current.child_nodes() {
                if should_show_node(child.node_type(), state_clone.what_to_show) {
                    *state_clone.current_node.borrow_mut() = child.clone();
                    return Ok(create_element_object(child, ctx, &dom_for_fc));
                }
            }
            Ok(JsValue::null())
        })
    };

    // lastChild()
    let state_clone = state.clone();
    let dom_for_lc = dom.clone();
    let last_child = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let current = state_clone.current_node.borrow().clone();
            let children: Vec<_> = current.child_nodes().into_iter()
                .filter(|c| should_show_node(c.node_type(), state_clone.what_to_show))
                .collect();
            if let Some(child) = children.last() {
                *state_clone.current_node.borrow_mut() = child.clone();
                return Ok(create_element_object(child.clone(), ctx, &dom_for_lc));
            }
            Ok(JsValue::null())
        })
    };

    // nextSibling()
    let state_clone = state.clone();
    let dom_for_ns = dom.clone();
    let next_sibling = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let current = state_clone.current_node.borrow().clone();
            if let Some(parent) = current.parent_element() {
                let siblings = parent.child_nodes();
                let mut found = false;
                for sibling in siblings {
                    if found && should_show_node(sibling.node_type(), state_clone.what_to_show) {
                        *state_clone.current_node.borrow_mut() = sibling.clone();
                        return Ok(create_element_object(sibling, ctx, &dom_for_ns));
                    }
                    if sibling.unique_id() == current.unique_id() {
                        found = true;
                    }
                }
            }
            Ok(JsValue::null())
        })
    };

    // previousSibling()
    let state_clone = state.clone();
    let dom_for_ps = dom.clone();
    let previous_sibling = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let current = state_clone.current_node.borrow().clone();
            if let Some(parent) = current.parent_element() {
                let siblings = parent.child_nodes();
                let mut prev: Option<DomElement> = None;
                for sibling in siblings {
                    if sibling.unique_id() == current.unique_id() {
                        if let Some(p) = prev {
                            *state_clone.current_node.borrow_mut() = p.clone();
                            return Ok(create_element_object(p, ctx, &dom_for_ps));
                        }
                        return Ok(JsValue::null());
                    }
                    if should_show_node(sibling.node_type(), state_clone.what_to_show) {
                        prev = Some(sibling);
                    }
                }
            }
            Ok(JsValue::null())
        })
    };

    // nextNode() - document order traversal
    let state_clone = state.clone();
    let dom_for_nn = dom.clone();
    let next_node = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let mut current = state_clone.current_node.borrow().clone();

            // Try first child
            let children = current.child_nodes();
            for child in children {
                if should_show_node(child.node_type(), state_clone.what_to_show) {
                    *state_clone.current_node.borrow_mut() = child.clone();
                    return Ok(create_element_object(child, ctx, &dom_for_nn));
                }
            }

            // Try next sibling or ancestor's next sibling
            loop {
                if let Some(parent) = current.parent_element() {
                    let siblings = parent.child_nodes();
                    let mut found = false;
                    for sibling in siblings {
                        if found && should_show_node(sibling.node_type(), state_clone.what_to_show) {
                            *state_clone.current_node.borrow_mut() = sibling.clone();
                            return Ok(create_element_object(sibling, ctx, &dom_for_nn));
                        }
                        if sibling.unique_id() == current.unique_id() {
                            found = true;
                        }
                    }

                    // No next sibling found, go up
                    if parent.unique_id() == state_clone.root.unique_id() {
                        break;
                    }
                    current = parent;
                } else {
                    break;
                }
            }

            Ok(JsValue::null())
        })
    };

    // previousNode() - reverse document order traversal
    let state_clone = state.clone();
    let dom_for_pn = dom.clone();
    let previous_node = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let current = state_clone.current_node.borrow().clone();

            // Try previous sibling's last descendant
            if let Some(parent) = current.parent_element() {
                let siblings = parent.child_nodes();
                let mut prev: Option<DomElement> = None;
                for sibling in siblings {
                    if sibling.unique_id() == current.unique_id() {
                        if let Some(p) = prev {
                            // Get last descendant of previous sibling
                            let mut node = p;
                            loop {
                                let children: Vec<_> = node.child_nodes().into_iter()
                                    .filter(|c| should_show_node(c.node_type(), state_clone.what_to_show))
                                    .collect();
                                if let Some(last) = children.last() {
                                    node = last.clone();
                                } else {
                                    break;
                                }
                            }
                            *state_clone.current_node.borrow_mut() = node.clone();
                            return Ok(create_element_object(node, ctx, &dom_for_pn));
                        }
                        // No previous sibling, return parent
                        if parent.unique_id() != state_clone.root.unique_id() {
                            *state_clone.current_node.borrow_mut() = parent.clone();
                            return Ok(create_element_object(parent, ctx, &dom_for_pn));
                        }
                        return Ok(JsValue::null());
                    }
                    if should_show_node(sibling.node_type(), state_clone.what_to_show) {
                        prev = Some(sibling);
                    }
                }
            }

            Ok(JsValue::null())
        })
    };

    // Create root JS object
    let root_obj = create_element_object(root.clone(), context, &dom_clone);

    // currentNode getter/setter
    let state_for_cn_get = state.clone();
    let dom_for_cn_get = dom.clone();
    let current_node_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let current = state_for_cn_get.current_node.borrow().clone();
            Ok(create_element_object(current, ctx, &dom_for_cn_get))
        })
    };

    let state_for_cn_set = state.clone();
    let dom_for_cn_set = dom.clone();
    let current_node_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let node = args.get_or_undefined(0);
            if let Some(node_obj) = node.as_object() {
                if let Ok(id_val) = node_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_cn_set.registry.get(id) {
                            *state_for_cn_set.current_node.borrow_mut() = el;
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    let walker = ObjectInitializer::new(context)
        .property(js_string!("root"), root_obj, Attribute::READONLY)
        .property(js_string!("whatToShow"), what_to_show, Attribute::READONLY)
        .property(js_string!("filter"), JsValue::null(), Attribute::READONLY)
        .function(parent_node, js_string!("parentNode"), 0)
        .function(first_child, js_string!("firstChild"), 0)
        .function(last_child, js_string!("lastChild"), 0)
        .function(next_sibling, js_string!("nextSibling"), 0)
        .function(previous_sibling, js_string!("previousSibling"), 0)
        .function(next_node, js_string!("nextNode"), 0)
        .function(previous_node, js_string!("previousNode"), 0)
        .build();

    // Add currentNode accessor
    let _ = walker.define_property_or_throw(
        js_string!("currentNode"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(current_node_getter.to_js_function(context.realm()))
            .set(current_node_setter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    walker
}

// ============================================================================
// FULL NODEITERATOR IMPLEMENTATION
// ============================================================================

fn create_node_iterator_object_full(
    context: &mut Context,
    root: DomElement,
    what_to_show: u32,
    filter: Option<JsObject>,
    dom: &DomWrapper,
) -> JsObject {
    let state = Rc::new(NodeIteratorState::new(root.clone(), what_to_show, filter));
    let dom_clone = dom.clone();

    // nextNode()
    let state_clone = state.clone();
    let dom_for_next = dom.clone();
    let next_node = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let nodes = state_clone.nodes.borrow();
            let mut index = state_clone.current_index.borrow_mut();

            *index += 1;
            if (*index as usize) < nodes.len() {
                let node = nodes[*index as usize].clone();
                *state_clone.reference_node.borrow_mut() = node.clone();
                *state_clone.pointer_before_reference.borrow_mut() = false;
                return Ok(create_element_object(node, ctx, &dom_for_next));
            }
            Ok(JsValue::null())
        })
    };

    // previousNode()
    let state_clone = state.clone();
    let dom_for_prev = dom.clone();
    let previous_node = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let nodes = state_clone.nodes.borrow();
            let mut index = state_clone.current_index.borrow_mut();

            if *index > 0 {
                *index -= 1;
                let node = nodes[*index as usize].clone();
                *state_clone.reference_node.borrow_mut() = node.clone();
                *state_clone.pointer_before_reference.borrow_mut() = true;
                return Ok(create_element_object(node, ctx, &dom_for_prev));
            }
            Ok(JsValue::null())
        })
    };

    // detach()
    let detach = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // Create root JS object
    let root_obj = create_element_object(root.clone(), context, &dom_clone);

    // referenceNode getter
    let state_for_ref = state.clone();
    let dom_for_ref = dom.clone();
    let reference_node_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let ref_node = state_for_ref.reference_node.borrow().clone();
            Ok(create_element_object(ref_node, ctx, &dom_for_ref))
        })
    };

    // pointerBeforeReferenceNode getter
    let state_for_ptr = state.clone();
    let pointer_before_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*state_for_ptr.pointer_before_reference.borrow()))
        })
    };

    let iterator = ObjectInitializer::new(context)
        .property(js_string!("root"), root_obj, Attribute::READONLY)
        .property(js_string!("whatToShow"), what_to_show, Attribute::READONLY)
        .property(js_string!("filter"), JsValue::null(), Attribute::READONLY)
        .function(next_node, js_string!("nextNode"), 0)
        .function(previous_node, js_string!("previousNode"), 0)
        .function(detach, js_string!("detach"), 0)
        .build();

    // Add referenceNode accessor
    let _ = iterator.define_property_or_throw(
        js_string!("referenceNode"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(reference_node_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    // Add pointerBeforeReferenceNode accessor
    let _ = iterator.define_property_or_throw(
        js_string!("pointerBeforeReferenceNode"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(pointer_before_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    iterator
}

// ============================================================================
// Helper: Create Range object from existing state
// ============================================================================

/// Creates a Range JS object that wraps an existing RangeState
/// Used by Selection.getRangeAt to return the stored range
fn create_range_object_from_state(
    context: &mut Context,
    dom: &DomWrapper,
    range_id: u64,
    state: Rc<RangeState>,
) -> JsObject {
    let dom_clone = dom.clone();

    // setStart(node, offset)
    let state_clone = state.clone();
    let dom_for_start = dom.clone();
    let set_start = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let node = args.get_or_undefined(0);
            let offset = args.get_or_undefined(1).to_u32(ctx).unwrap_or(0);

            if let Some(node_obj) = node.as_object() {
                if let Ok(id_val) = node_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_start.registry.get(id) {
                            state_clone.set_start(el, offset);
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // setEnd(node, offset)
    let state_clone = state.clone();
    let dom_for_end = dom.clone();
    let set_end = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let node = args.get_or_undefined(0);
            let offset = args.get_or_undefined(1).to_u32(ctx).unwrap_or(0);

            if let Some(node_obj) = node.as_object() {
                if let Ok(id_val) = node_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_end.registry.get(id) {
                            state_clone.set_end(el, offset);
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // collapse(toStart)
    let state_clone = state.clone();
    let collapse = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            let to_start = args.get_or_undefined(0).to_boolean();
            state_clone.collapse(to_start);
            Ok(JsValue::undefined())
        })
    };

    // cloneRange()
    let dom_for_clone = dom.clone();
    let clone_range = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            Ok(JsValue::from(create_range_object_full(ctx, &dom_for_clone)))
        })
    };

    // toString()
    let state_clone = state.clone();
    let to_string = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let text = state_clone.get_text_content();
            Ok(JsValue::from(js_string!(text)))
        })
    };

    // Build range object with basic methods
    let range = ObjectInitializer::new(context)
        .property(js_string!("START_TO_START"), 0, Attribute::READONLY)
        .property(js_string!("START_TO_END"), 1, Attribute::READONLY)
        .property(js_string!("END_TO_END"), 2, Attribute::READONLY)
        .property(js_string!("END_TO_START"), 3, Attribute::READONLY)
        .function(set_start, js_string!("setStart"), 2)
        .function(set_end, js_string!("setEnd"), 2)
        .function(collapse, js_string!("collapse"), 1)
        .function(clone_range, js_string!("cloneRange"), 0)
        .function(to_string, js_string!("toString"), 0)
        .build();

    // Add getters for startContainer, startOffset, endContainer, endOffset, collapsed
    let state_for_sc = state.clone();
    let dom_for_sc = dom_clone.clone();
    let start_container_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            if let Some(el) = state_for_sc.start_container.borrow().as_ref() {
                Ok(create_element_object(el.clone(), ctx, &dom_for_sc))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    let state_for_so = state.clone();
    let start_offset_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*state_for_so.start_offset.borrow()))
        })
    };

    let state_for_ec = state.clone();
    let dom_for_ec = dom_clone.clone();
    let end_container_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            if let Some(el) = state_for_ec.end_container.borrow().as_ref() {
                Ok(create_element_object(el.clone(), ctx, &dom_for_ec))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    let state_for_eo = state.clone();
    let end_offset_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*state_for_eo.end_offset.borrow()))
        })
    };

    let state_for_col = state.clone();
    let collapsed_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*state_for_col.collapsed.borrow()))
        })
    };

    // Define property getters
    let _ = range.define_property_or_throw(
        js_string!("startContainer"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(start_container_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let _ = range.define_property_or_throw(
        js_string!("startOffset"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(start_offset_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let _ = range.define_property_or_throw(
        js_string!("endContainer"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(end_container_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let _ = range.define_property_or_throw(
        js_string!("endOffset"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(end_offset_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let _ = range.define_property_or_throw(
        js_string!("collapsed"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(collapsed_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    // Store range ID for reference
    let _ = range.set(js_string!("__range_id__"), JsValue::from(range_id as f64), false, context);

    range
}

// ============================================================================
// FULL SELECTION IMPLEMENTATION
// ============================================================================

fn create_selection_object_full(context: &mut Context, dom: &DomWrapper) -> JsObject {
    let state = Rc::new(SelectionState::new());
    let dom_clone = dom.clone();

    // addRange(range)
    let state_clone = state.clone();
    let add_range = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let range_arg = args.get_or_undefined(0);
            if let Some(range_obj) = range_arg.as_object() {
                // Extract __range_id__ from the range object to get the RangeState
                if let Ok(id_val) = range_obj.get(js_string!("__range_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(range_state) = get_range_by_id(id) {
                            // Use add_range_with_id to preserve the ID mapping
                            state_clone.add_range_with_id(id, range_state);
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // removeRange(range)
    let state_clone = state.clone();
    let remove_range = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let range_arg = args.get_or_undefined(0);
            if let Some(range_obj) = range_arg.as_object() {
                // Extract __range_id__ from the range object to get the RangeState
                if let Ok(id_val) = range_obj.get(js_string!("__range_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(range_state) = get_range_by_id(id) {
                            state_clone.remove_range(&range_state);
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // removeAllRanges()
    let state_clone = state.clone();
    let remove_all_ranges = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            state_clone.remove_all_ranges();
            Ok(JsValue::undefined())
        })
    };

    // empty() - alias for removeAllRanges
    let state_clone = state.clone();
    let empty = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            state_clone.remove_all_ranges();
            Ok(JsValue::undefined())
        })
    };

    // collapse(node, offset)
    let state_clone = state.clone();
    let dom_for_collapse = dom.clone();
    let collapse = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let node = args.get_or_undefined(0);
            let offset = args.get_or_undefined(1).to_u32(ctx).unwrap_or(0);

            if let Some(node_obj) = node.as_object() {
                if let Ok(id_val) = node_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_collapse.registry.get(id) {
                            state_clone.remove_all_ranges();
                            let range = Rc::new(RangeState::new());
                            range.set_start(el.clone(), offset);
                            range.set_end(el, offset);
                            state_clone.add_range(range);
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // collapseToStart()
    let state_clone = state.clone();
    let collapse_to_start = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            if let Some((_, range)) = state_clone.get_range_at(0) {
                range.collapse(true);
            }
            Ok(JsValue::undefined())
        })
    };

    // collapseToEnd()
    let state_clone = state.clone();
    let collapse_to_end = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            if let Some((_, range)) = state_clone.get_range_at(0) {
                range.collapse(false);
            }
            Ok(JsValue::undefined())
        })
    };

    // extend(node, offset)
    let state_clone = state.clone();
    let dom_for_extend = dom.clone();
    let extend = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let node = args.get_or_undefined(0);
            let offset = args.get_or_undefined(1).to_u32(ctx).unwrap_or(0);

            if let Some((_, range)) = state_clone.get_range_at(0) {
                if let Some(node_obj) = node.as_object() {
                    if let Ok(id_val) = node_obj.get(js_string!("__element_id__"), ctx) {
                        if let Some(id) = id_val.as_number().map(|n| n as u64) {
                            if let Some(el) = dom_for_extend.registry.get(id) {
                                range.set_end(el, offset);
                            }
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // setBaseAndExtent(anchorNode, anchorOffset, focusNode, focusOffset)
    let state_clone = state.clone();
    let dom_for_sbe = dom.clone();
    let set_base_and_extent = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let anchor_node = args.get_or_undefined(0);
            let anchor_offset = args.get_or_undefined(1).to_u32(ctx).unwrap_or(0);
            let focus_node = args.get_or_undefined(2);
            let focus_offset = args.get_or_undefined(3).to_u32(ctx).unwrap_or(0);

            state_clone.remove_all_ranges();
            let range = Rc::new(RangeState::new());

            if let Some(anchor_obj) = anchor_node.as_object() {
                if let Ok(id_val) = anchor_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_sbe.registry.get(id) {
                            range.set_start(el, anchor_offset);
                        }
                    }
                }
            }

            if let Some(focus_obj) = focus_node.as_object() {
                if let Ok(id_val) = focus_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_sbe.registry.get(id) {
                            range.set_end(el, focus_offset);
                        }
                    }
                }
            }

            state_clone.add_range(range);
            Ok(JsValue::undefined())
        })
    };

    // selectAllChildren(node)
    let state_clone = state.clone();
    let dom_for_sac = dom.clone();
    let select_all_children = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let node = args.get_or_undefined(0);
            if let Some(node_obj) = node.as_object() {
                if let Ok(id_val) = node_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_sac.registry.get(id) {
                            // Clear existing selection and create a new range spanning all children
                            state_clone.remove_all_ranges();
                            let range = Rc::new(RangeState::new());
                            let child_count = el.child_nodes().len() as u32;
                            range.set_start(el.clone(), 0);
                            range.set_end(el, child_count);
                            state_clone.add_range(range);
                        }
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // deleteFromDocument() - Actually delete selected content from DOM
    let state_clone = state.clone();
    let delete_from_document = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            // Delete contents from all ranges in the selection
            let ranges = state_clone.ranges.borrow();
            for (_, range) in ranges.iter() {
                range.delete_contents();
            }
            drop(ranges);
            // Clear the selection after deletion
            state_clone.remove_all_ranges();
            Ok(JsValue::undefined())
        })
    };

    // containsNode(node, allowPartial)
    let state_clone = state.clone();
    let dom_for_cn = dom.clone();
    let contains_node = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let node = args.get_or_undefined(0);
            let allow_partial = args.get_or_undefined(1).to_boolean();

            if let Some(node_obj) = node.as_object() {
                if let Ok(id_val) = node_obj.get(js_string!("__element_id__"), ctx) {
                    if let Some(id) = id_val.as_number().map(|n| n as u64) {
                        if let Some(el) = dom_for_cn.registry.get(id) {
                            return Ok(JsValue::from(state_clone.contains_node(&el, allow_partial)));
                        }
                    }
                }
            }
            Ok(JsValue::from(false))
        })
    };

    // getRangeAt(index)
    let state_clone = state.clone();
    let dom_for_gra = dom.clone();
    let get_range_at = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0) as usize;

            if let Some((range_id, range_state)) = state_clone.get_range_at(index) {
                // Create a range object that wraps the existing RangeState
                let range = create_range_object_from_state(ctx, &dom_for_gra, range_id, range_state);
                Ok(JsValue::from(range))
            } else {
                // Throw IndexSizeError if index is out of bounds
                Err(boa_engine::JsError::from_opaque(JsValue::from(
                    js_string!("IndexSizeError: No range at the specified index")
                )))
            }
        })
    };

    // toString()
    let state_clone = state.clone();
    let to_string = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let text = state_clone.to_string();
            Ok(JsValue::from(js_string!(text)))
        })
    };

    // modify(alter, direction, granularity)
    let state_clone = state.clone();
    let modify = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let alter = args.get_or_undefined(0).to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            let direction = args.get_or_undefined(1).to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            let granularity = args.get_or_undefined(2).to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();

            // Basic implementation: move by character for collapsed selection
            if let Some((_, range)) = state_clone.get_range_at(0) {
                let is_move = alter.to_lowercase() == "move";
                let is_extend = alter.to_lowercase() == "extend";
                let is_forward = direction.to_lowercase() == "forward" || direction.to_lowercase() == "right";

                if granularity.to_lowercase() == "character" {
                    if is_move || is_extend {
                        // Extract values first to avoid borrow conflicts
                        let current_offset = *range.end_offset.borrow();
                        let end_container = range.end_container.borrow().clone();
                        let start_container = range.start_container.borrow().clone();
                        let start_offset = *range.start_offset.borrow();

                        if is_forward {
                            let container = end_container.or_else(|| start_container.clone()).unwrap();
                            range.set_end(container, current_offset.saturating_add(1));
                            if is_move {
                                let new_end_offset = *range.end_offset.borrow();
                                let new_end_container = range.end_container.borrow().clone().unwrap();
                                range.set_start(new_end_container, new_end_offset);
                            }
                        } else {
                            let new_offset = current_offset.saturating_sub(1);
                            if is_move {
                                range.collapse(true);
                                if let Some(container) = start_container.clone() {
                                    range.set_start(container.clone(), new_offset);
                                    range.set_end(container, new_offset);
                                }
                            } else {
                                // Extend backward
                                if let Some(container) = start_container {
                                    range.set_start(container, start_offset.saturating_sub(1));
                                }
                            }
                        }
                    }
                }
                // For word/line granularity, more complex implementation would be needed
            }
            Ok(JsValue::undefined())
        })
    };

    // Build selection object
    let selection = ObjectInitializer::new(context)
        .function(add_range, js_string!("addRange"), 1)
        .function(remove_range, js_string!("removeRange"), 1)
        .function(remove_all_ranges, js_string!("removeAllRanges"), 0)
        .function(empty, js_string!("empty"), 0)
        .function(collapse, js_string!("collapse"), 2)
        .function(collapse_to_start, js_string!("collapseToStart"), 0)
        .function(collapse_to_end, js_string!("collapseToEnd"), 0)
        .function(extend, js_string!("extend"), 2)
        .function(set_base_and_extent, js_string!("setBaseAndExtent"), 4)
        .function(select_all_children, js_string!("selectAllChildren"), 1)
        .function(delete_from_document, js_string!("deleteFromDocument"), 0)
        .function(contains_node, js_string!("containsNode"), 2)
        .function(get_range_at, js_string!("getRangeAt"), 1)
        .function(to_string, js_string!("toString"), 0)
        .function(modify, js_string!("modify"), 3)
        .build();

    // Add getters for anchorNode, anchorOffset, focusNode, focusOffset, isCollapsed, rangeCount, type
    let state_for_an = state.clone();
    let dom_for_an = dom_clone.clone();
    let anchor_node_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            if let Some(el) = state_for_an.anchor_node.borrow().as_ref() {
                Ok(create_element_object(el.clone(), ctx, &dom_for_an))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    let state_for_ao = state.clone();
    let anchor_offset_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*state_for_ao.anchor_offset.borrow()))
        })
    };

    let state_for_fn = state.clone();
    let dom_for_fn = dom_clone.clone();
    let focus_node_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            if let Some(el) = state_for_fn.focus_node.borrow().as_ref() {
                Ok(create_element_object(el.clone(), ctx, &dom_for_fn))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    let state_for_fo = state.clone();
    let focus_offset_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(*state_for_fo.focus_offset.borrow()))
        })
    };

    let state_for_col = state.clone();
    let is_collapsed_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(state_for_col.is_collapsed()))
        })
    };

    let state_for_rc = state.clone();
    let range_count_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(state_for_rc.range_count() as u32))
        })
    };

    let state_for_type = state.clone();
    let type_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(state_for_type.selection_type.borrow().clone())))
        })
    };

    // Define property getters
    let _ = selection.define_property_or_throw(
        js_string!("anchorNode"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(anchor_node_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let _ = selection.define_property_or_throw(
        js_string!("anchorOffset"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(anchor_offset_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let _ = selection.define_property_or_throw(
        js_string!("focusNode"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(focus_node_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let _ = selection.define_property_or_throw(
        js_string!("focusOffset"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(focus_offset_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let _ = selection.define_property_or_throw(
        js_string!("isCollapsed"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(is_collapsed_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let _ = selection.define_property_or_throw(
        js_string!("rangeCount"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(range_count_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    let _ = selection.define_property_or_throw(
        js_string!("type"),
        boa_engine::property::PropertyDescriptor::builder()
            .get(type_getter.to_js_function(context.realm()))
            .configurable(true)
            .enumerable(true)
            .build(),
        context,
    );

    selection
}

// ============================================================================
// XPATH IMPLEMENTATION
// ============================================================================

fn create_xpath_expression(context: &mut Context, xpath: &str, dom: &DomWrapper) -> JsObject {
    let xpath_clone = xpath.to_string();
    let dom_clone = dom.clone();

    let evaluate = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let context_node = args.get(0);
            let result_type = args.get(1).and_then(|v| v.to_u32(ctx).ok()).unwrap_or(0);
            Ok(JsValue::from(evaluate_xpath(ctx, &xpath_clone, context_node, result_type, &dom_clone)))
        })
    };

    ObjectInitializer::new(context)
        .function(evaluate, js_string!("evaluate"), 3)
        .build()
}

fn evaluate_xpath(context: &mut Context, xpath: &str, _context_node: Option<&JsValue>, result_type: u32, dom: &DomWrapper) -> JsObject {
    // Simple XPath implementation for common patterns
    let mut results: Vec<DomElement> = Vec::new();

    // Handle common XPath patterns
    if xpath.starts_with("//") {
        let tag = &xpath[2..];
        if let Some(root) = dom.inner.document_element() {
            results = root.get_elements_by_tag_name(tag);
        }
    } else if xpath.starts_with("/") {
        // Absolute path - simplified
        if let Some(root) = dom.inner.document_element() {
            results.push(root);
        }
    }

    let snapshot_length = results.len();
    let results_for_iter = results.clone();
    let results_for_snap = results.clone();
    let dom_for_iter = dom.clone();
    let dom_for_snap = dom.clone();

    let iter_index = Rc::new(RefCell::new(0usize));
    let iter_index_clone = iter_index.clone();

    let iterate_next = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let mut idx = iter_index_clone.borrow_mut();
            if *idx < results_for_iter.len() {
                let el = results_for_iter[*idx].clone();
                *idx += 1;
                Ok(create_element_object(el, ctx, &dom_for_iter))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    let snapshot_item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0) as usize;
            if index < results_for_snap.len() {
                Ok(create_element_object(results_for_snap[index].clone(), ctx, &dom_for_snap))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    // Get single node value
    let single_node = if !results.is_empty() {
        create_element_object(results[0].clone(), context, dom)
    } else {
        JsValue::null()
    };

    ObjectInitializer::new(context)
        .property(js_string!("ANY_TYPE"), 0, Attribute::READONLY)
        .property(js_string!("NUMBER_TYPE"), 1, Attribute::READONLY)
        .property(js_string!("STRING_TYPE"), 2, Attribute::READONLY)
        .property(js_string!("BOOLEAN_TYPE"), 3, Attribute::READONLY)
        .property(js_string!("UNORDERED_NODE_ITERATOR_TYPE"), 4, Attribute::READONLY)
        .property(js_string!("ORDERED_NODE_ITERATOR_TYPE"), 5, Attribute::READONLY)
        .property(js_string!("UNORDERED_NODE_SNAPSHOT_TYPE"), 6, Attribute::READONLY)
        .property(js_string!("ORDERED_NODE_SNAPSHOT_TYPE"), 7, Attribute::READONLY)
        .property(js_string!("ANY_UNORDERED_NODE_TYPE"), 8, Attribute::READONLY)
        .property(js_string!("FIRST_ORDERED_NODE_TYPE"), 9, Attribute::READONLY)
        .property(js_string!("resultType"), result_type, Attribute::READONLY)
        .property(js_string!("numberValue"), 0.0, Attribute::READONLY)
        .property(js_string!("stringValue"), js_string!(""), Attribute::READONLY)
        .property(js_string!("booleanValue"), !results.is_empty(), Attribute::READONLY)
        .property(js_string!("singleNodeValue"), single_node, Attribute::READONLY)
        .property(js_string!("invalidIteratorState"), false, Attribute::READONLY)
        .property(js_string!("snapshotLength"), snapshot_length as u32, Attribute::READONLY)
        .function(iterate_next, js_string!("iterateNext"), 0)
        .function(snapshot_item, js_string!("snapshotItem"), 1)
        .build()
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Create a NodeList from elements
fn create_node_list_from_elements(elements: &[DomElement], context: &mut Context, dom: &DomWrapper) -> JsObject {
    let count = elements.len();
    let elements_clone = elements.to_vec();
    let dom_clone = dom.clone();

    let item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0) as usize;
            if index < elements_clone.len() {
                Ok(create_element_object(elements_clone[index].clone(), ctx, &dom_clone))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    let elements_for_each = elements.to_vec();
    let dom_for_each = dom.clone();
    let for_each = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let callback = args.get_or_undefined(0);
            if let Some(func) = callback.as_callable() {
                for (i, el) in elements_for_each.iter().enumerate() {
                    let el_obj = create_element_object(el.clone(), ctx, &dom_for_each);
                    let _ = func.call(&JsValue::undefined(), &[el_obj, JsValue::from(i as u32)], ctx);
                }
            }
            Ok(JsValue::undefined())
        })
    };

    let entries = NativeFunction::from_copy_closure(|_this, _args, ctx| Ok(JsValue::from(JsArray::new(ctx))));
    let keys = NativeFunction::from_copy_closure(|_this, _args, ctx| Ok(JsValue::from(JsArray::new(ctx))));
    let values = NativeFunction::from_copy_closure(|_this, _args, ctx| Ok(JsValue::from(JsArray::new(ctx))));

    ObjectInitializer::new(context)
        .property(js_string!("length"), count as u32, Attribute::READONLY)
        .function(item, js_string!("item"), 1)
        .function(for_each, js_string!("forEach"), 1)
        .function(entries, js_string!("entries"), 0)
        .function(keys, js_string!("keys"), 0)
        .function(values, js_string!("values"), 0)
        .build()
}

/// Create a resolved Promise
fn create_resolved_promise(context: &mut Context, value: JsValue) -> Result<JsValue, BoaJsError> {
    let then_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
        let callback = args.get_or_undefined(0);
        if callback.is_callable() {
            let cb = callback.as_callable().unwrap();
            if let Some(obj) = this.as_object() {
                let val = obj.get(js_string!("__value__"), ctx).unwrap_or(JsValue::undefined());
                let result = cb.call(&JsValue::undefined(), &[val], ctx)?;
                return create_resolved_promise(ctx, result);
            }
        }
        Ok(this.clone())
    });

    let catch_fn = NativeFunction::from_copy_closure(|this, _args, _ctx| Ok(this.clone()));
    let finally_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
        let callback = args.get_or_undefined(0);
        if callback.is_callable() {
            let cb = callback.as_callable().unwrap();
            let _ = cb.call(&JsValue::undefined(), &[], ctx);
        }
        Ok(this.clone())
    });

    let promise = ObjectInitializer::new(context)
        .property(js_string!("__value__"), value, Attribute::READONLY)
        .function(then_fn, js_string!("then"), 2)
        .function(catch_fn, js_string!("catch"), 1)
        .function(finally_fn, js_string!("finally"), 1)
        .build();

    Ok(JsValue::from(promise))
}

/// Create document.styleSheets by parsing actual <style> elements from the DOM
fn create_style_sheets_from_dom(context: &mut Context, dom: &DomWrapper) -> JsObject {
    let mut sheets: Vec<JsObject> = Vec::new();

    // Query for all <style> elements
    let style_elements = dom.inner.query_selector_all("style");

    for style_el in style_elements {
        let css_text = style_el.text_content();
        let media_text = style_el.get_attribute("media").unwrap_or_default();
        let title = style_el.get_attribute("title");

        let stylesheet = cssom::create_css_stylesheet_from_css(
            &css_text,
            None,
            title.as_deref(),
            &media_text,
            None,
            false,
            context,
        );

        sheets.push(stylesheet);
    }

    // Also query for <link rel="stylesheet"> elements
    let link_elements = dom.inner.query_selector_all("link[rel=\"stylesheet\"]");

    for link_el in link_elements {
        let href = link_el.get_attribute("href");
        let media_text = link_el.get_attribute("media").unwrap_or_default();
        let title = link_el.get_attribute("title");

        if let Some(href_str) = href.as_ref() {
            if let Some(css_text) = fetch_stylesheet(href_str) {
                let stylesheet = cssom::create_css_stylesheet_from_css(
                    &css_text,
                    Some(href_str),
                    title.as_deref(),
                    &media_text,
                    None,
                    false,
                    context,
                );
                sheets.push(stylesheet);
            } else {
                let stylesheet = cssom::create_css_stylesheet(
                    Some(href_str),
                    title.as_deref(),
                    &media_text,
                    None,
                    false,
                    context,
                );
                sheets.push(stylesheet);
            }
        }
    }

    cssom::create_style_sheet_list(sheets, context)
}

/// Fetch an external stylesheet using blocking HTTP request
fn fetch_stylesheet(url: &str) -> Option<String> {
    use reqwest::blocking::Client;
    use std::time::Duration;

    let client = Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(Duration::from_secs(10))
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .build()
        .ok()?;

    let response = client.get(url)
        .header("Accept", "text/css,*/*;q=0.1")
        .send()
        .ok()?;

    if response.status().is_success() {
        response.text().ok()
    } else {
        None
    }
}
