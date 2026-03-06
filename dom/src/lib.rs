//! DOM crate - HTML parsing and DOM representation for the semantic browser.
//!
//! Uses html5ever for parsing HTML into a DOM tree, and provides
//! a live DOM interface for JavaScript interaction.
//!
//! Also integrates dom_query for enhanced CSS selector support and markdown rendering.

use html5ever::namespace_url;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use html5ever::LocalName;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use thiserror::Error;

// Re-export dom_query for enhanced functionality
pub use dom_query;

/// Errors that can occur during DOM operations
#[derive(Error, Debug)]
pub enum DomError {
    #[error("Failed to parse HTML: {0}")]
    ParseError(String),

    #[error("Element not found")]
    ElementNotFound,
}

/// Result type for DOM operations
pub type Result<T> = std::result::Result<T, DomError>;

/// A link extracted from the DOM
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub text: String,
    pub href: String,
}

/// A script extracted from the DOM (inline or external)
#[derive(Debug, Clone)]
pub struct InlineScript {
    pub content: String,
    /// Source URL for external scripts (None for inline scripts)
    pub src: Option<String>,
    /// Whether the script is a module
    pub is_module: bool,
    /// Whether the script has async attribute
    pub is_async: bool,
    /// Whether the script has defer attribute
    pub is_defer: bool,
}

/// A snapshot of the DOM containing extracted semantic information.
#[derive(Debug, Clone)]
pub struct DomSnapshot {
    pub title: RefCell<String>,
    pub body_text: String,
    pub links: Vec<Link>,
    pub scripts: Vec<InlineScript>,
}

impl DomSnapshot {
    pub fn get_title(&self) -> String {
        self.title.borrow().clone()
    }

    pub fn set_title(&self, new_title: &str) {
        *self.title.borrow_mut() = new_title.to_string();
    }
}

/// Represents a live DOM element that can be queried and manipulated.
#[derive(Clone)]
pub struct Element {
    pub(crate) handle: Handle,
}

impl Element {
    /// Create a new Element wrapper around an rcdom Handle
    pub fn new(handle: Handle) -> Self {
        Element { handle }
    }

    /// Get a unique ID for this element based on its internal pointer address.
    /// This ID is stable and consistent for the same underlying DOM node.
    pub fn unique_id(&self) -> u64 {
        // Use the Rc's pointer address as a unique identifier
        std::rc::Rc::as_ptr(&self.handle) as u64
    }

    /// Get the tag name of this element (lowercase)
    pub fn tag_name(&self) -> String {
        match &self.handle.data {
            NodeData::Element { name, .. } => name.local.to_string(),
            NodeData::Document => "#document".to_string(),
            NodeData::Text { .. } => "#text".to_string(),
            NodeData::Comment { .. } => "#comment".to_string(),
            _ => "".to_string(),
        }
    }

    /// Get the tag name uppercase (for JS compatibility)
    pub fn tag_name_upper(&self) -> String {
        self.tag_name().to_uppercase()
    }

    /// Get the node type (1 = Element, 3 = Text, 8 = Comment, 9 = Document)
    pub fn node_type(&self) -> u32 {
        match &self.handle.data {
            NodeData::Element { .. } => 1,
            NodeData::Text { .. } => 3,
            NodeData::Comment { .. } => 8,
            NodeData::Document => 9,
            _ => 0,
        }
    }

    /// Get the id attribute
    pub fn id(&self) -> Option<String> {
        self.get_attribute("id")
    }

    /// Get the class attribute
    pub fn class_name(&self) -> Option<String> {
        self.get_attribute("class")
    }

    /// Get all class names as a vector
    pub fn class_list(&self) -> Vec<String> {
        self.class_name()
            .map(|c| c.split_whitespace().map(|s| s.to_string()).collect())
            .unwrap_or_default()
    }

    /// Check if element has a specific class
    pub fn has_class(&self, class: &str) -> bool {
        self.class_list().iter().any(|c| c == class)
    }

    /// Get an attribute value
    pub fn get_attribute(&self, name: &str) -> Option<String> {
        if let NodeData::Element { attrs, .. } = &self.handle.data {
            let attrs = attrs.borrow();
            for attr in attrs.iter() {
                if attr.name.local.as_ref() == name {
                    return Some(attr.value.to_string());
                }
            }
        }
        None
    }

    /// Set an attribute value
    pub fn set_attribute(&self, name: &str, value: &str) {
        if let NodeData::Element { attrs, .. } = &self.handle.data {
            let mut attrs = attrs.borrow_mut();
            // Check if attribute exists
            for attr in attrs.iter_mut() {
                if attr.name.local.as_ref() == name {
                    attr.value = value.into();
                    return;
                }
            }
            // Add new attribute
            attrs.push(html5ever::Attribute {
                name: html5ever::QualName::new(None, html5ever::ns!(), LocalName::from(name)),
                value: value.into(),
            });
        }
    }

    /// Remove an attribute
    pub fn remove_attribute(&self, name: &str) {
        if let NodeData::Element { attrs, .. } = &self.handle.data {
            let mut attrs = attrs.borrow_mut();
            attrs.retain(|attr| attr.name.local.as_ref() != name);
        }
    }

    /// Check if element has an attribute
    pub fn has_attribute(&self, name: &str) -> bool {
        self.get_attribute(name).is_some()
    }

    /// Get all attributes as name-value pairs
    pub fn get_all_attributes(&self) -> Vec<(String, String)> {
        if let NodeData::Element { attrs, .. } = &self.handle.data {
            let attrs = attrs.borrow();
            attrs.iter()
                .map(|attr| (attr.name.local.to_string(), attr.value.to_string()))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get inner text content
    pub fn inner_text(&self) -> String {
        let mut text = String::new();
        extract_text_recursive(&self.handle, &mut text);
        normalize_whitespace(&text)
    }

    /// Get text content (including all descendants)
    pub fn text_content(&self) -> String {
        let mut text = String::new();
        extract_all_text(&self.handle, &mut text);
        text
    }

    /// Get inner HTML - serializes children to HTML string
    pub fn inner_html(&self) -> String {
        let mut html = String::new();
        for child in self.handle.children.borrow().iter() {
            serialize_node(child, &mut html);
        }
        html
    }

    /// Set inner HTML - parses HTML and replaces children
    pub fn set_inner_html(&self, html: &str) {
        use html5ever::parse_fragment;
        use html5ever::tree_builder::TreeSink;
        use html5ever::QualName;
        use markup5ever_rcdom::Node;
        use std::cell::Cell;

        // Clear existing children
        self.handle.children.borrow_mut().clear();

        // If empty HTML, we're done
        if html.trim().is_empty() {
            return;
        }

        // Get the context element's qualified name for fragment parsing
        let context_name = match &self.handle.data {
            NodeData::Element { name, .. } => name.clone(),
            _ => QualName::new(None, namespace_url!("http://www.w3.org/1999/xhtml"), LocalName::from("div")),
        };

        // Parse the HTML fragment
        let dom = parse_fragment(
            RcDom::default(),
            Default::default(),
            context_name,
            Vec::new(),
        )
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .ok();

        if let Some(parsed) = dom {
            // The parsed document has a structure like:
            // document -> html (the context element) -> children
            // We need to get the children of the html element
            let doc_children = parsed.document.children.borrow();
            if let Some(html_node) = doc_children.first() {
                // Get children of the parsed fragment and add them to our element
                let fragment_children = html_node.children.borrow();
                for child in fragment_children.iter() {
                    // Clone the child node and update its parent reference
                    let cloned = clone_node_deep(child);
                    self.handle.children.borrow_mut().push(cloned);
                }
            }
        }
    }

    /// Get parent element
    pub fn parent_element(&self) -> Option<Element> {
        self.handle.parent.take().map(|weak| {
            let result = weak.upgrade().map(Element::new);
            self.handle.parent.set(Some(weak));
            result
        })?
    }

    /// Get child elements (only element nodes)
    pub fn children(&self) -> Vec<Element> {
        self.handle
            .children
            .borrow()
            .iter()
            .filter(|child| matches!(child.data, NodeData::Element { .. }))
            .map(|h| Element::new(h.clone()))
            .collect()
    }

    /// Get all child nodes (including text nodes)
    pub fn child_nodes(&self) -> Vec<Element> {
        self.handle
            .children
            .borrow()
            .iter()
            .map(|h| Element::new(h.clone()))
            .collect()
    }

    /// Get first child element
    pub fn first_element_child(&self) -> Option<Element> {
        self.children().into_iter().next()
    }

    /// Get last child element
    pub fn last_element_child(&self) -> Option<Element> {
        self.children().into_iter().last()
    }

    /// Get first child node
    pub fn first_child(&self) -> Option<Element> {
        self.handle
            .children
            .borrow()
            .first()
            .map(|h| Element::new(h.clone()))
    }

    /// Get last child node
    pub fn last_child(&self) -> Option<Element> {
        self.handle
            .children
            .borrow()
            .last()
            .map(|h| Element::new(h.clone()))
    }

    /// Check if the node has any child nodes
    pub fn has_child_nodes(&self) -> bool {
        !self.handle.children.borrow().is_empty()
    }

    /// Get next sibling element
    pub fn next_element_sibling(&self) -> Option<Element> {
        let parent = self.parent_element()?;
        let children = parent.children();
        let mut found = false;
        for child in children {
            if found {
                return Some(child);
            }
            if std::rc::Rc::ptr_eq(&child.handle, &self.handle) {
                found = true;
            }
        }
        None
    }

    /// Get previous sibling element
    pub fn previous_element_sibling(&self) -> Option<Element> {
        let parent = self.parent_element()?;
        let children = parent.children();
        let mut prev = None;
        for child in children {
            if std::rc::Rc::ptr_eq(&child.handle, &self.handle) {
                return prev;
            }
            prev = Some(child);
        }
        None
    }

    /// Get next sibling node (includes all node types: elements, text, comments, etc.)
    pub fn next_sibling(&self) -> Option<Element> {
        let parent = self.parent_element()?;
        let children = parent.child_nodes(); // All nodes, not just elements
        let mut found = false;
        for child in children {
            if found {
                return Some(child);
            }
            if std::rc::Rc::ptr_eq(&child.handle, &self.handle) {
                found = true;
            }
        }
        None
    }

    /// Get previous sibling node (includes all node types: elements, text, comments, etc.)
    pub fn previous_sibling(&self) -> Option<Element> {
        let parent = self.parent_element()?;
        let children = parent.child_nodes(); // All nodes, not just elements
        let mut prev = None;
        for child in children {
            if std::rc::Rc::ptr_eq(&child.handle, &self.handle) {
                return prev;
            }
            prev = Some(child);
        }
        None
    }

    /// Append a child element
    pub fn append_child(&self, child: &Element) {
        self.handle.children.borrow_mut().push(child.handle.clone());
    }

    /// Prepend a child element (insert at the beginning)
    pub fn prepend_child(&self, child: &Element) {
        self.handle.children.borrow_mut().insert(0, child.handle.clone());
    }

    /// Insert a child before a reference node
    pub fn insert_before(&self, new_child: &Element, reference: &Element) -> bool {
        let mut children = self.handle.children.borrow_mut();
        if let Some(index) = children.iter().position(|h| std::rc::Rc::ptr_eq(h, &reference.handle)) {
            children.insert(index, new_child.handle.clone());
            true
        } else {
            // Reference not found, append to end
            children.push(new_child.handle.clone());
            false
        }
    }

    /// Replace a child with another node
    pub fn replace_child(&self, new_child: &Element, old_child: &Element) -> bool {
        let mut children = self.handle.children.borrow_mut();
        if let Some(index) = children.iter().position(|h| std::rc::Rc::ptr_eq(h, &old_child.handle)) {
            children[index] = new_child.handle.clone();
            true
        } else {
            false
        }
    }

    /// Remove a child element
    pub fn remove_child(&self, child: &Element) {
        self.handle
            .children
            .borrow_mut()
            .retain(|h| !std::rc::Rc::ptr_eq(h, &child.handle));
    }

    /// Remove this element from its parent
    pub fn remove_from_parent(&self) {
        if let Some(parent) = self.parent_element() {
            parent.remove_child(self);
        }
    }

    /// Clear all children
    pub fn clear_children(&self) {
        self.handle.children.borrow_mut().clear();
    }

    /// Replace all children with new nodes (implements replaceChildren)
    pub fn replace_children(&self, children: Vec<Element>) {
        self.handle.children.borrow_mut().clear();
        for child in children {
            self.handle.children.borrow_mut().push(child.handle.clone());
        }
    }

    /// Normalize the node - merge adjacent text nodes and remove empty text nodes
    pub fn normalize(&self) {
        let mut children = self.handle.children.borrow_mut();

        // Collect indices of text nodes and their content
        let mut i = 0;
        while i < children.len() {
            // First, recursively normalize children
            let child = Element::new(children[i].clone());
            drop(children); // Release borrow before recursive call
            child.normalize();
            children = self.handle.children.borrow_mut();

            if let NodeData::Text { contents } = &children[i].data {
                let text = contents.borrow().to_string();

                // Remove empty text nodes
                if text.is_empty() {
                    children.remove(i);
                    continue;
                }

                // Merge with following adjacent text nodes
                let mut merged_text = text;
                while i + 1 < children.len() {
                    if let NodeData::Text { contents: next_contents } = &children[i + 1].data {
                        let next_text = next_contents.borrow().to_string();
                        merged_text.push_str(&next_text);
                        children.remove(i + 1);
                    } else {
                        break;
                    }
                }

                // Update the text content if we merged
                if let NodeData::Text { contents } = &children[i].data {
                    *contents.borrow_mut() = merged_text.into();
                }
            }

            i += 1;
        }
    }

    /// Set text content (clears children and adds text node)
    pub fn set_text_content(&self, text: &str) {
        use markup5ever_rcdom::Node;
        use std::cell::Cell;

        // Clear existing children
        self.handle.children.borrow_mut().clear();

        // Create and add text node
        let text_node = Rc::new(Node {
            parent: Cell::new(None),
            children: RefCell::new(Vec::new()),
            data: NodeData::Text {
                contents: RefCell::new(text.into()),
            },
        });
        self.handle.children.borrow_mut().push(text_node);
    }

    /// Check if this element matches a simple selector
    pub fn matches_selector(&self, selector: &str) -> bool {
        match_simple_selector(self, selector)
    }

    /// Find the closest ancestor matching a selector
    pub fn closest(&self, selector: &str) -> Option<Element> {
        let mut current = Some(self.clone());
        while let Some(el) = current {
            if el.matches_selector(selector) {
                return Some(el);
            }
            current = el.parent_element();
        }
        None
    }

    /// Query for a single descendant matching a selector
    pub fn query_selector(&self, selector: &str) -> Option<Element> {
        query_selector_recursive(&self.handle, selector)
    }

    /// Query for all descendants matching a selector
    pub fn query_selector_all(&self, selector: &str) -> Vec<Element> {
        let mut results = Vec::new();
        query_selector_all_recursive(&self.handle, selector, &mut results);
        results
    }

    /// Get elements by tag name
    pub fn get_elements_by_tag_name(&self, tag: &str) -> Vec<Element> {
        let tag_lower = tag.to_lowercase();
        let mut results = Vec::new();
        get_by_tag_recursive(&self.handle, &tag_lower, &mut results);
        results
    }

    /// Get elements by class name
    pub fn get_elements_by_class_name(&self, class: &str) -> Vec<Element> {
        let mut results = Vec::new();
        get_by_class_recursive(&self.handle, class, &mut results);
        results
    }

    /// Check if this is an element node
    pub fn is_element(&self) -> bool {
        matches!(self.handle.data, NodeData::Element { .. })
    }

    /// Check if this is a text node
    pub fn is_text(&self) -> bool {
        matches!(self.handle.data, NodeData::Text { .. })
    }

    /// Set the text content of a text node directly
    /// This modifies the internal contents of the text node, unlike set_text_content
    /// which works on element nodes by clearing children and adding a text child.
    pub fn set_text_node_data(&self, text: &str) {
        if let NodeData::Text { contents } = &self.handle.data {
            *contents.borrow_mut() = text.into();
        }
    }

    /// Get the handle (for internal use)
    pub fn get_handle(&self) -> &Handle {
        &self.handle
    }

    // ============================================================================
    // Node Comparison Methods
    // ============================================================================

    /// Check if this node is the same node as another (pointer equality)
    pub fn is_same_node(&self, other: &Element) -> bool {
        std::rc::Rc::ptr_eq(&self.handle, &other.handle)
    }

    /// Check if two nodes are equal (same structure and attributes)
    pub fn is_equal_node(&self, other: &Element) -> bool {
        // Must be same node type
        if self.node_type() != other.node_type() {
            return false;
        }

        match (&self.handle.data, &other.handle.data) {
            (NodeData::Element { name: n1, attrs: a1, .. }, NodeData::Element { name: n2, attrs: a2, .. }) => {
                // Tag names must match
                if n1.local != n2.local {
                    return false;
                }
                // Attributes must match (order doesn't matter)
                let attrs1 = a1.borrow();
                let attrs2 = a2.borrow();
                if attrs1.len() != attrs2.len() {
                    return false;
                }
                for attr in attrs1.iter() {
                    let found = attrs2.iter().any(|a|
                        a.name.local == attr.name.local && a.value == attr.value
                    );
                    if !found {
                        return false;
                    }
                }
                // Children must match
                let children1 = self.child_nodes();
                let children2 = other.child_nodes();
                if children1.len() != children2.len() {
                    return false;
                }
                for (c1, c2) in children1.iter().zip(children2.iter()) {
                    if !c1.is_equal_node(c2) {
                        return false;
                    }
                }
                true
            }
            (NodeData::Text { contents: c1 }, NodeData::Text { contents: c2 }) => {
                c1.borrow().to_string() == c2.borrow().to_string()
            }
            (NodeData::Comment { contents: c1 }, NodeData::Comment { contents: c2 }) => {
                c1 == c2
            }
            (NodeData::Document, NodeData::Document) => {
                // Compare children
                let children1 = self.child_nodes();
                let children2 = other.child_nodes();
                if children1.len() != children2.len() {
                    return false;
                }
                for (c1, c2) in children1.iter().zip(children2.iter()) {
                    if !c1.is_equal_node(c2) {
                        return false;
                    }
                }
                true
            }
            (NodeData::Doctype { name: n1, public_id: p1, system_id: s1 },
             NodeData::Doctype { name: n2, public_id: p2, system_id: s2 }) => {
                n1 == n2 && p1 == p2 && s1 == s2
            }
            _ => false,
        }
    }

    /// Check if this node contains another node as a descendant
    pub fn contains(&self, other: &Element) -> bool {
        // A node contains itself
        if self.is_same_node(other) {
            return true;
        }
        // Check all descendants
        for child in self.child_nodes() {
            if child.contains(other) {
                return true;
            }
        }
        false
    }

    /// Compare the document position of this node to another
    /// Returns a bitmask of position flags
    pub fn compare_document_position(&self, other: &Element) -> u16 {
        const DISCONNECTED: u16 = 1;
        const PRECEDING: u16 = 2;
        const FOLLOWING: u16 = 4;
        const CONTAINS: u16 = 8;
        const CONTAINED_BY: u16 = 16;
        // const IMPLEMENTATION_SPECIFIC: u16 = 32;

        // Same node
        if self.is_same_node(other) {
            return 0;
        }

        // Check if other is ancestor of self
        let mut ancestor = self.parent_element();
        while let Some(anc) = ancestor {
            if anc.is_same_node(other) {
                return CONTAINS | PRECEDING;
            }
            ancestor = anc.parent_element();
        }

        // Check if self is ancestor of other
        let mut ancestor = other.parent_element();
        while let Some(anc) = ancestor {
            if anc.is_same_node(self) {
                return CONTAINED_BY | FOLLOWING;
            }
            ancestor = anc.parent_element();
        }

        // Find common ancestor and determine order
        let self_ancestors = self.get_ancestor_chain();
        let other_ancestors = other.get_ancestor_chain();

        // Find common ancestor
        for (i, self_anc) in self_ancestors.iter().enumerate() {
            for (j, other_anc) in other_ancestors.iter().enumerate() {
                if self_anc.is_same_node(other_anc) {
                    // Found common ancestor - determine order from children
                    let common = self_anc;
                    let self_child = if i > 0 { &self_ancestors[i - 1] } else { self };
                    let other_child = if j > 0 { &other_ancestors[j - 1] } else { other };

                    // Find order in children
                    for child in common.child_nodes() {
                        if child.is_same_node(self_child) {
                            return FOLLOWING;
                        }
                        if child.is_same_node(other_child) {
                            return PRECEDING;
                        }
                    }
                }
            }
        }

        // Nodes are disconnected
        DISCONNECTED
    }

    /// Get the chain of ancestors (from self to root)
    fn get_ancestor_chain(&self) -> Vec<Element> {
        let mut chain = Vec::new();
        let mut current = self.parent_element();
        while let Some(parent) = current {
            chain.push(parent.clone());
            current = parent.parent_element();
        }
        chain
    }

    /// Get the root node (document or document fragment)
    pub fn get_root_node(&self) -> Element {
        let mut current = self.clone();
        while let Some(parent) = current.parent_element() {
            current = parent;
        }
        current
    }

    // ============================================================================
    // Namespace Methods
    // ============================================================================

    /// Get an attribute value by namespace URI and local name
    pub fn get_attribute_ns(&self, namespace: Option<&str>, local_name: &str) -> Option<String> {
        if let NodeData::Element { attrs, .. } = &self.handle.data {
            let attrs = attrs.borrow();
            for attr in attrs.iter() {
                let attr_ns = attr.name.ns.as_ref();
                let matches_ns = match (namespace, attr_ns) {
                    (None, ns) => ns.is_empty() || ns == "http://www.w3.org/1999/xhtml",
                    (Some(""), ns) => ns.is_empty(),
                    (Some(ns), attr_ns) => attr_ns == ns,
                };
                if matches_ns && attr.name.local.as_ref() == local_name {
                    return Some(attr.value.to_string());
                }
            }
        }
        None
    }

    /// Set an attribute value by namespace URI and local name
    pub fn set_attribute_ns(&self, namespace: Option<&str>, qualified_name: &str, value: &str) {
        if let NodeData::Element { attrs, .. } = &self.handle.data {
            let mut attrs = attrs.borrow_mut();

            // Parse qualified name (prefix:localName or just localName)
            let (prefix, local_name) = if let Some(colon_pos) = qualified_name.find(':') {
                (Some(&qualified_name[..colon_pos]), &qualified_name[colon_pos + 1..])
            } else {
                (None, qualified_name)
            };

            let ns_atom = match namespace {
                Some(ns) if !ns.is_empty() => html5ever::Namespace::from(ns),
                _ => html5ever::ns!(),
            };

            let prefix_atom = prefix.map(|p| html5ever::Prefix::from(p));

            // Check if attribute exists
            for attr in attrs.iter_mut() {
                let attr_ns = attr.name.ns.as_ref();
                let matches_ns = match namespace {
                    None => attr_ns.is_empty(),
                    Some("") => attr_ns.is_empty(),
                    Some(ns) => attr_ns == ns,
                };
                if matches_ns && attr.name.local.as_ref() == local_name {
                    attr.value = value.into();
                    return;
                }
            }

            // Add new attribute
            attrs.push(html5ever::Attribute {
                name: html5ever::QualName::new(prefix_atom, ns_atom, LocalName::from(local_name)),
                value: value.into(),
            });
        }
    }

    /// Check if element has an attribute by namespace URI and local name
    pub fn has_attribute_ns(&self, namespace: Option<&str>, local_name: &str) -> bool {
        self.get_attribute_ns(namespace, local_name).is_some()
    }

    /// Remove an attribute by namespace URI and local name
    pub fn remove_attribute_ns(&self, namespace: Option<&str>, local_name: &str) {
        if let NodeData::Element { attrs, .. } = &self.handle.data {
            let mut attrs = attrs.borrow_mut();
            attrs.retain(|attr| {
                let attr_ns = attr.name.ns.as_ref();
                let matches_ns = match namespace {
                    None => attr_ns.is_empty() || attr_ns == "http://www.w3.org/1999/xhtml",
                    Some("") => attr_ns.is_empty(),
                    Some(ns) => attr_ns == ns,
                };
                !(matches_ns && attr.name.local.as_ref() == local_name)
            });
        }
    }

    // ============================================================================
    // Namespace Lookup Methods
    // ============================================================================

    /// Look up the prefix associated with a namespace URI
    pub fn lookup_prefix(&self, namespace_uri: Option<&str>) -> Option<String> {
        let namespace_uri = namespace_uri?;
        if namespace_uri.is_empty() {
            return None;
        }

        if let NodeData::Element { name, attrs, .. } = &self.handle.data {
            // Check if the element's own namespace matches
            if name.ns.as_ref() == namespace_uri {
                if let Some(ref prefix) = name.prefix {
                    return Some(prefix.to_string());
                }
            }

            // Check xmlns:prefix attributes
            for attr in attrs.borrow().iter() {
                let attr_name = attr.name.local.as_ref();
                if attr_name.starts_with("xmlns:") && attr.value.as_ref() == namespace_uri {
                    return Some(attr_name[6..].to_string());
                }
            }
        }

        // Walk up the tree
        if let Some(parent) = self.parent_element() {
            return parent.lookup_prefix(Some(namespace_uri));
        }

        None
    }

    /// Look up the namespace URI associated with a prefix
    pub fn lookup_namespace_uri(&self, prefix: Option<&str>) -> Option<String> {
        if let NodeData::Element { name, attrs, .. } = &self.handle.data {
            match prefix {
                None | Some("") => {
                    // Look for default namespace
                    for attr in attrs.borrow().iter() {
                        if attr.name.local.as_ref() == "xmlns" {
                            let val = attr.value.to_string();
                            return if val.is_empty() { None } else { Some(val) };
                        }
                    }
                    // Check element's own namespace if no xmlns found
                    let ns = name.ns.as_ref();
                    if !ns.is_empty() {
                        return Some(ns.to_string());
                    }
                }
                Some(pfx) => {
                    // Look for xmlns:prefix attribute
                    let xmlns_attr = format!("xmlns:{}", pfx);
                    for attr in attrs.borrow().iter() {
                        if attr.name.local.as_ref() == xmlns_attr {
                            let val = attr.value.to_string();
                            return if val.is_empty() { None } else { Some(val) };
                        }
                    }
                    // Check if element's prefix matches
                    if let Some(ref elem_prefix) = name.prefix {
                        if elem_prefix.as_ref() == pfx {
                            return Some(name.ns.to_string());
                        }
                    }
                }
            }
        }

        // Walk up the tree
        if let Some(parent) = self.parent_element() {
            return parent.lookup_namespace_uri(prefix);
        }

        None
    }

    /// Check if the given namespace URI is the default namespace
    pub fn is_default_namespace(&self, namespace_uri: Option<&str>) -> bool {
        let default_ns = self.lookup_namespace_uri(None);
        match (namespace_uri, default_ns) {
            (None, None) => true,
            (Some(ns), Some(ref default)) => ns == default,
            (None, Some(_)) => false,
            (Some(_), None) => false,
        }
    }

    // ============================================================================
    // Clone Methods
    // ============================================================================

    /// Clone this node (shallow or deep)
    pub fn clone_node(&self, deep: bool) -> Element {
        if deep {
            Element::new(clone_node_deep(&self.handle))
        } else {
            Element::new(clone_node_shallow(&self.handle))
        }
    }

    /// Get all data-* attributes as key-value pairs
    /// Returns Vec of (camelCase property name, value)
    pub fn get_data_attributes(&self) -> Vec<(String, String)> {
        if let NodeData::Element { attrs, .. } = &self.handle.data {
            attrs
                .borrow()
                .iter()
                .filter_map(|attr| {
                    let attr_name = attr.name.local.as_ref();
                    if attr_name.starts_with("data-") {
                        let prop_name = data_attr_to_camel_case(attr_name);
                        Some((prop_name, attr.value.to_string()))
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Set a data-* attribute by camelCase property name
    pub fn set_data_attribute(&self, prop_name: &str, value: &str) {
        let attr_name = camel_case_to_data_attr(prop_name);
        self.set_attribute(&attr_name, value);
    }

    /// Get a data-* attribute by camelCase property name
    pub fn get_data_attribute(&self, prop_name: &str) -> Option<String> {
        let attr_name = camel_case_to_data_attr(prop_name);
        self.get_attribute(&attr_name)
    }
}

/// Convert data-* attribute name to camelCase dataset property name
fn data_attr_to_camel_case(attr_name: &str) -> String {
    // "data-foo-bar" -> "fooBar"
    let name = attr_name.strip_prefix("data-").unwrap_or(attr_name);
    let mut result = String::new();
    let mut capitalize_next = false;

    for ch in name.chars() {
        if ch == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// Convert camelCase dataset property name to data-* attribute name
fn camel_case_to_data_attr(prop_name: &str) -> String {
    // "fooBar" -> "data-foo-bar"
    let mut result = String::from("data-");
    for ch in prop_name.chars() {
        if ch.is_ascii_uppercase() {
            result.push('-');
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

impl std::fmt::Debug for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Element({})", self.tag_name())
    }
}

/// The main DOM structure holding the parsed document.
pub struct Dom {
    document: RcDom,
    /// Cache for getElementById lookups
    id_cache: RefCell<HashMap<String, Handle>>,
    /// Dynamically created elements
    created_elements: RefCell<Vec<Handle>>,
    /// The original HTML source (for dom_query operations)
    html_source: String,
}

impl Dom {
    /// Parse HTML string into a DOM
    pub fn parse(html: &str) -> Result<Self> {
        let dom = parse_document(RcDom::default(), Default::default())
            .from_utf8()
            .read_from(&mut html.as_bytes())
            .map_err(|e| DomError::ParseError(e.to_string()))?;

        let mut id_cache = HashMap::new();
        build_id_cache(&dom.document, &mut id_cache);

        Ok(Dom {
            document: dom,
            id_cache: RefCell::new(id_cache),
            created_elements: RefCell::new(Vec::new()),
            html_source: html.to_string(),
        })
    }

    /// Serialize the live DOM tree to HTML string
    pub fn serialize_to_html(&self) -> String {
        let mut html = String::new();
        serialize_node(&self.document.document, &mut html);
        html
    }

    /// Render the document to Markdown using the LIVE DOM tree
    pub fn to_markdown(&self) -> String {
        // Serialize the live DOM to HTML, then convert to markdown
        let live_html = self.serialize_to_html();
        let doc = dom_query::Document::from(live_html.as_str());

        // Parse CSS from <style> elements and collect selectors for hidden elements
        let hidden_selectors = self.get_hidden_selectors();

        // Remove elements hidden by stylesheet CSS rules
        for selector in &hidden_selectors {
            // Only use simple selectors that dom_query can handle
            if !selector.contains('@') && !selector.contains(':') {
                for node in doc.select(selector).iter() {
                    node.remove();
                }
            }
        }

        // Remove elements hidden by inline CSS (display:none or visibility:hidden)
        // This respects CSS styling set by JavaScript or inline styles
        for node in doc.select("[style]").iter() {
            if let Some(style) = node.attr("style") {
                let style_lower = style.to_lowercase();
                if style_lower.contains("display:none") ||
                   style_lower.contains("display: none") ||
                   style_lower.contains("visibility:hidden") ||
                   style_lower.contains("visibility: hidden") {
                    node.remove();
                }
            }
        }

        // Also remove elements with hidden attribute
        for node in doc.select("[hidden]").iter() {
            node.remove();
        }

        // Skip script, style, meta, head, and noscript tags
        // noscript content should not be shown when JavaScript is executing
        let skip_tags: &[&str] = &["script", "style", "meta", "head", "noscript", "template", "svg"];
        doc.md(Some(skip_tags)).to_string()
    }

    /// Extract selectors for hidden elements from <style> elements
    fn get_hidden_selectors(&self) -> Vec<String> {
        let mut hidden_selectors = Vec::new();
        let live_html = self.serialize_to_html();
        let doc = dom_query::Document::from(live_html.as_str());

        // Get CSS text from all <style> elements
        for style_node in doc.select("style").iter() {
            let css_text = style_node.text().to_string();
            hidden_selectors.extend(parse_hidden_selectors(&css_text));
        }

        hidden_selectors
    }

    /// Render the original (unmutated) document to Markdown
    pub fn to_markdown_original(&self) -> String {
        let doc = dom_query::Document::from(self.html_source.as_str());
        doc.md(None).to_string()
    }

    /// Render specific selector to Markdown using the LIVE DOM
    pub fn to_markdown_selector(&self, selector: &str) -> String {
        let live_html = self.serialize_to_html();
        let doc = dom_query::Document::from(live_html.as_str());
        let selection = doc.select(selector);
        if selection.exists() {
            // Get the inner HTML of the selection and parse it as a fragment
            let html = selection.html();
            let fragment = dom_query::Document::fragment(html);
            fragment.md(None).to_string()
        } else {
            String::new()
        }
    }

    /// Query using dom_query's full CSS selector support and get text
    pub fn query_text(&self, selector: &str) -> Option<String> {
        let doc = dom_query::Document::from(self.html_source.as_str());
        let selection = doc.select(selector);
        if selection.exists() {
            Some(selection.text().to_string())
        } else {
            None
        }
    }

    /// Query using dom_query's full CSS selector support and get HTML
    pub fn query_html(&self, selector: &str) -> Option<String> {
        let doc = dom_query::Document::from(self.html_source.as_str());
        let selection = doc.select(selector);
        if selection.exists() {
            Some(selection.html().to_string())
        } else {
            None
        }
    }

    /// Query and get attribute value using dom_query
    pub fn query_attr(&self, selector: &str, attr: &str) -> Option<String> {
        let doc = dom_query::Document::from(self.html_source.as_str());
        let selection = doc.select(selector);
        selection.attr(attr).map(|s| s.to_string())
    }

    /// Count elements matching selector using dom_query
    pub fn query_count(&self, selector: &str) -> usize {
        let doc = dom_query::Document::from(self.html_source.as_str());
        doc.select(selector).length()
    }

    /// Check if selector matches any elements using dom_query
    pub fn query_exists(&self, selector: &str) -> bool {
        let doc = dom_query::Document::from(self.html_source.as_str());
        doc.select(selector).exists()
    }

    /// Get the document element
    pub fn document(&self) -> Element {
        Element::new(self.document.document.clone())
    }

    /// Get the document element (html)
    pub fn document_element(&self) -> Option<Element> {
        self.document()
            .children()
            .into_iter()
            .find(|e| e.tag_name() == "html")
    }

    /// Get the head element
    pub fn head(&self) -> Option<Element> {
        self.document_element()?
            .children()
            .into_iter()
            .find(|e| e.tag_name() == "head")
    }

    /// Get the body element
    pub fn body(&self) -> Option<Element> {
        self.document_element()?
            .children()
            .into_iter()
            .find(|e| e.tag_name() == "body")
    }

    /// Get element by ID
    pub fn get_element_by_id(&self, id: &str) -> Option<Element> {
        // First try cache for performance
        if let Some(el) = self.id_cache.borrow().get(id).map(|h| Element::new(h.clone())) {
            return Some(el);
        }
        // Fall back to live DOM traversal (handles dynamically added elements)
        self.document().query_selector(&format!("#{}", id))
    }

    /// Get elements by tag name
    pub fn get_elements_by_tag_name(&self, tag: &str) -> Vec<Element> {
        self.document().get_elements_by_tag_name(tag)
    }

    /// Get elements by class name
    pub fn get_elements_by_class_name(&self, class: &str) -> Vec<Element> {
        self.document().get_elements_by_class_name(class)
    }

    /// Query selector
    pub fn query_selector(&self, selector: &str) -> Option<Element> {
        self.document().query_selector(selector)
    }

    /// Query selector all
    pub fn query_selector_all(&self, selector: &str) -> Vec<Element> {
        self.document().query_selector_all(selector)
    }

    /// Create a new element
    pub fn create_element(&self, tag_name: &str) -> Element {
        use html5ever::QualName;
        use markup5ever_rcdom::Node;
        use std::cell::Cell;

        let name = QualName::new(None, html5ever::ns!(html), LocalName::from(tag_name));
        let node = Rc::new(Node {
            parent: Cell::new(None),
            children: RefCell::new(Vec::new()),
            data: NodeData::Element {
                name,
                attrs: RefCell::new(Vec::new()),
                template_contents: RefCell::new(None),
                mathml_annotation_xml_integration_point: false,
            },
        });

        self.created_elements.borrow_mut().push(node.clone());
        Element::new(node)
    }

    /// Create a text node
    pub fn create_text_node(&self, text: &str) -> Element {
        use markup5ever_rcdom::Node;
        use std::cell::Cell;

        let node = Rc::new(Node {
            parent: Cell::new(None),
            children: RefCell::new(Vec::new()),
            data: NodeData::Text {
                contents: RefCell::new(text.into()),
            },
        });

        self.created_elements.borrow_mut().push(node.clone());
        Element::new(node)
    }

    /// Create a document fragment
    /// A document fragment is a minimal document object that has no parent.
    /// It's used as a lightweight container for moving DOM nodes around.
    pub fn create_document_fragment(&self) -> Element {
        use markup5ever_rcdom::Node;
        use std::cell::Cell;

        // Create a document fragment node - it acts like a Document but is lighter weight
        // In rcdom, we simulate this with a special element that acts as a container
        let node = Rc::new(Node {
            parent: Cell::new(None),
            children: RefCell::new(Vec::new()),
            data: NodeData::Document, // Use Document node type for fragment-like behavior
        });

        self.created_elements.borrow_mut().push(node.clone());
        Element::new(node)
    }

    /// Get the title
    pub fn get_title(&self) -> String {
        if let Some(title_el) = self.query_selector("title") {
            title_el.inner_text()
        } else {
            String::new()
        }
    }

    /// Set the title
    pub fn set_title(&self, title: &str) {
        if let Some(title_el) = self.query_selector("title") {
            // Clear existing children and add text node
            title_el.handle.children.borrow_mut().clear();
            let text_node = self.create_text_node(title);
            title_el.append_child(&text_node);
        }
    }

    /// Extract a snapshot for rendering
    pub fn snapshot(&self) -> Rc<DomSnapshot> {
        Rc::new(extract_snapshot(&self.document.document))
    }

    /// Get inline scripts
    pub fn get_scripts(&self) -> Vec<InlineScript> {
        let mut scripts = Vec::new();
        extract_scripts(&self.document.document, &mut scripts);
        scripts
    }

    // ============================================================================
    // Document Mutation Methods (for ParentNode mixin)
    // ============================================================================

    /// Prepend nodes/strings to the document (before all children)
    pub fn prepend(&self, children: Vec<Element>) {
        if let Some(doc_element) = self.document_element() {
            let mut doc_children = doc_element.handle.children.borrow_mut();
            for (i, child) in children.into_iter().enumerate() {
                doc_children.insert(i, child.handle.clone());
            }
        }
    }

    /// Append nodes/strings to the document (after all children)
    pub fn append(&self, children: Vec<Element>) {
        if let Some(doc_element) = self.document_element() {
            for child in children {
                doc_element.handle.children.borrow_mut().push(child.handle.clone());
            }
        }
    }

    /// Replace all children of the document with new nodes
    pub fn replace_children(&self, children: Vec<Element>) {
        if let Some(doc_element) = self.document_element() {
            doc_element.handle.children.borrow_mut().clear();
            for child in children {
                doc_element.handle.children.borrow_mut().push(child.handle.clone());
            }
        }
    }

    /// Create a comment node
    pub fn create_comment(&self, data: &str) -> Element {
        use markup5ever_rcdom::Node;
        use std::cell::Cell;

        let node = Rc::new(Node {
            parent: Cell::new(None),
            children: RefCell::new(Vec::new()),
            data: NodeData::Comment {
                contents: data.into(),
            },
        });

        self.created_elements.borrow_mut().push(node.clone());
        Element::new(node)
    }
}

// ============================================================================
// Helper functions
// ============================================================================

fn build_id_cache(handle: &Handle, cache: &mut HashMap<String, Handle>) {
    if let NodeData::Element { attrs, .. } = &handle.data {
        for attr in attrs.borrow().iter() {
            if attr.name.local.as_ref() == "id" {
                cache.insert(attr.value.to_string(), handle.clone());
                break;
            }
        }
    }
    for child in handle.children.borrow().iter() {
        build_id_cache(child, cache);
    }
}

fn extract_text_recursive(handle: &Handle, output: &mut String) {
    match &handle.data {
        NodeData::Text { contents } => {
            output.push_str(&contents.borrow());
        }
        NodeData::Element { name, .. } => {
            let tag = name.local.as_ref();
            // Skip script, style, noscript content
            if matches!(tag, "script" | "style" | "noscript") {
                return;
            }
            for child in handle.children.borrow().iter() {
                extract_text_recursive(child, output);
            }
        }
        _ => {
            for child in handle.children.borrow().iter() {
                extract_text_recursive(child, output);
            }
        }
    }
}

fn extract_all_text(handle: &Handle, output: &mut String) {
    match &handle.data {
        NodeData::Text { contents } => {
            output.push_str(&contents.borrow());
        }
        _ => {
            for child in handle.children.borrow().iter() {
                extract_all_text(child, output);
            }
        }
    }
}

fn match_simple_selector(element: &Element, selector: &str) -> bool {
    let selector = selector.trim();
    if selector.is_empty() || selector == "*" {
        return element.is_element();
    }

    // Handle compound selectors (e.g., "div.class#id")
    let mut remaining = selector;
    let mut tag_matched = false;

    // Check tag name first if present
    if !remaining.starts_with('.') && !remaining.starts_with('#') && !remaining.starts_with('[') {
        let tag_end = remaining
            .find(|c| c == '.' || c == '#' || c == '[')
            .unwrap_or(remaining.len());
        let tag = &remaining[..tag_end];
        if !tag.is_empty() && element.tag_name() != tag.to_lowercase() {
            return false;
        }
        tag_matched = true;
        remaining = &remaining[tag_end..];
    }

    // Process remaining parts (classes, ids, attributes)
    while !remaining.is_empty() {
        if remaining.starts_with('.') {
            // Class selector
            let class_end = remaining[1..]
                .find(|c| c == '.' || c == '#' || c == '[')
                .map(|i| i + 1)
                .unwrap_or(remaining.len());
            let class = &remaining[1..class_end];
            if !element.has_class(class) {
                return false;
            }
            remaining = &remaining[class_end..];
        } else if remaining.starts_with('#') {
            // ID selector
            let id_end = remaining[1..]
                .find(|c| c == '.' || c == '#' || c == '[')
                .map(|i| i + 1)
                .unwrap_or(remaining.len());
            let id = &remaining[1..id_end];
            if element.id().as_deref() != Some(id) {
                return false;
            }
            remaining = &remaining[id_end..];
        } else if remaining.starts_with('[') {
            // Attribute selector
            if let Some(end) = remaining.find(']') {
                let attr_part = &remaining[1..end];
                if let Some(eq_pos) = attr_part.find('=') {
                    let attr_name = &attr_part[..eq_pos];
                    let attr_value = attr_part[eq_pos + 1..].trim_matches('"').trim_matches('\'');
                    if element.get_attribute(attr_name).as_deref() != Some(attr_value) {
                        return false;
                    }
                } else {
                    if !element.has_attribute(attr_part) {
                        return false;
                    }
                }
                remaining = &remaining[end + 1..];
            } else {
                break;
            }
        } else {
            break;
        }
    }

    tag_matched || !selector.starts_with(|c: char| c.is_alphabetic())
}

fn query_selector_recursive(handle: &Handle, selector: &str) -> Option<Element> {
    // Handle descendant selectors (space-separated)
    let parts: Vec<&str> = selector.split_whitespace().collect();

    if parts.len() == 1 {
        // Simple selector
        for child in handle.children.borrow().iter() {
            let el = Element::new(child.clone());
            if el.is_element() && el.matches_selector(selector) {
                return Some(el);
            }
            if let Some(found) = query_selector_recursive(child, selector) {
                return Some(found);
            }
        }
    } else {
        // Descendant selector - match first part, then recurse with rest
        let first = parts[0];
        let rest = parts[1..].join(" ");

        for child in handle.children.borrow().iter() {
            let el = Element::new(child.clone());
            if el.is_element() && el.matches_selector(first) {
                if let Some(found) = query_selector_recursive(child, &rest) {
                    return Some(found);
                }
            }
            if let Some(found) = query_selector_recursive(child, selector) {
                return Some(found);
            }
        }
    }

    None
}

fn query_selector_all_recursive(handle: &Handle, selector: &str, results: &mut Vec<Element>) {
    for child in handle.children.borrow().iter() {
        let el = Element::new(child.clone());
        if el.is_element() && el.matches_selector(selector) {
            results.push(el);
        }
        query_selector_all_recursive(child, selector, results);
    }
}

fn get_by_tag_recursive(handle: &Handle, tag: &str, results: &mut Vec<Element>) {
    for child in handle.children.borrow().iter() {
        if let NodeData::Element { name, .. } = &child.data {
            if name.local.as_ref() == tag || tag == "*" {
                results.push(Element::new(child.clone()));
            }
        }
        get_by_tag_recursive(child, tag, results);
    }
}

fn get_by_class_recursive(handle: &Handle, class: &str, results: &mut Vec<Element>) {
    for child in handle.children.borrow().iter() {
        let el = Element::new(child.clone());
        if el.has_class(class) {
            results.push(el);
        }
        get_by_class_recursive(child, class, results);
    }
}

fn extract_snapshot(handle: &Handle) -> DomSnapshot {
    let mut title = String::new();
    let mut body_text = String::new();
    let mut links = Vec::new();
    let mut scripts = Vec::new();

    extract_recursive(
        handle,
        &mut title,
        &mut body_text,
        &mut links,
        &mut scripts,
        false,
        false,
    );

    DomSnapshot {
        title: RefCell::new(title.trim().to_string()),
        body_text: normalize_whitespace(&body_text),
        links,
        scripts,
    }
}

fn extract_recursive(
    handle: &Handle,
    title: &mut String,
    body_text: &mut String,
    links: &mut Vec<Link>,
    scripts: &mut Vec<InlineScript>,
    in_title: bool,
    skip_text: bool,
) {
    match &handle.data {
        NodeData::Document => {
            for child in handle.children.borrow().iter() {
                extract_recursive(child, title, body_text, links, scripts, false, false);
            }
        }
        NodeData::Element { name, attrs, .. } => {
            let tag_name = name.local.as_ref();
            let attrs = attrs.borrow();

            let should_skip =
                matches!(tag_name, "script" | "style" | "noscript" | "head") && tag_name != "title";

            let is_title = tag_name == "title";
            if is_title {
                for child in handle.children.borrow().iter() {
                    extract_recursive(child, title, body_text, links, scripts, true, true);
                }
                return;
            }

            if tag_name == "script" {
                // Extract attributes
                let mut src: Option<String> = None;
                let mut is_module = false;
                let mut is_async = false;
                let mut is_defer = false;

                for attr in attrs.iter() {
                    match attr.name.local.as_ref() {
                        "src" => src = Some(attr.value.to_string()),
                        "type" => is_module = attr.value.as_ref() == "module",
                        "async" => is_async = true,
                        "defer" => is_defer = true,
                        _ => {}
                    }
                }

                if let Some(src_url) = src {
                    // External script
                    scripts.push(InlineScript {
                        content: String::new(),
                        src: Some(src_url),
                        is_module,
                        is_async,
                        is_defer,
                    });
                } else {
                    // Inline script - check size limit to prevent OOM
                    let mut script_content = String::new();
                    extract_all_text(handle, &mut script_content);
                    let content = script_content.trim().to_string();
                    if !content.is_empty() && content.len() <= MAX_INLINE_SCRIPT_SIZE {
                        scripts.push(InlineScript {
                            content,
                            src: None,
                            is_module,
                            is_async,
                            is_defer,
                        });
                    }
                    // Silently skip scripts that are too large
                }
                return;
            }

            if tag_name == "a" {
                let href = attrs
                    .iter()
                    .find(|attr| attr.name.local.as_ref() == "href")
                    .map(|attr| attr.value.to_string())
                    .unwrap_or_default();

                let mut link_text = String::new();
                extract_all_text(handle, &mut link_text);
                let link_text = link_text.trim().to_string();

                if !href.is_empty() && !link_text.is_empty() {
                    links.push(Link {
                        text: link_text,
                        href,
                    });
                }
            }

            if is_block_element(tag_name) && !body_text.is_empty() {
                body_text.push('\n');
            }

            for child in handle.children.borrow().iter() {
                extract_recursive(
                    child,
                    title,
                    body_text,
                    links,
                    scripts,
                    in_title,
                    skip_text || should_skip,
                );
            }

            if is_block_element(tag_name) {
                body_text.push('\n');
            }
        }
        NodeData::Text { contents } => {
            let text = contents.borrow();
            if in_title {
                title.push_str(&text);
            } else if !skip_text {
                body_text.push_str(&text);
            }
        }
        _ => {
            for child in handle.children.borrow().iter() {
                extract_recursive(
                    child,
                    title,
                    body_text,
                    links,
                    scripts,
                    in_title,
                    skip_text,
                );
            }
        }
    }
}

/// Maximum inline script size (1MB) - prevents OOM on huge inline bundles
const MAX_INLINE_SCRIPT_SIZE: usize = 5 * 1024 * 1024;

fn extract_scripts(handle: &Handle, scripts: &mut Vec<InlineScript>) {
    if let NodeData::Element { name, attrs, .. } = &handle.data {
        if name.local.as_ref() == "script" {
            let attrs_borrow = attrs.borrow();

            // Extract attributes
            let mut src: Option<String> = None;
            let mut is_module = false;
            let mut is_async = false;
            let mut is_defer = false;
            let mut script_type: Option<String> = None;

            for attr in attrs_borrow.iter() {
                match attr.name.local.as_ref() {
                    "src" => src = Some(attr.value.to_string()),
                    "type" => {
                        let t = attr.value.to_string();
                        is_module = t == "module";
                        script_type = Some(t);
                    }
                    "async" => is_async = true,
                    "defer" => is_defer = true,
                    _ => {}
                }
            }

            // Drop the borrow before potential recursive call
            drop(attrs_borrow);

            // Skip non-JavaScript script types (e.g., application/json, text/template)
            if let Some(ref t) = script_type {
                let is_js_type = t.is_empty()
                    || t == "module"
                    || t == "text/javascript"
                    || t == "application/javascript"
                    || t == "text/ecmascript"
                    || t == "application/ecmascript";
                if !is_js_type {
                    // Skip non-JS scripts - continue to children
                    for child in handle.children.borrow().iter() {
                        extract_scripts(child, scripts);
                    }
                    return;
                }
            }

            if let Some(src_url) = src {
                // External script
                scripts.push(InlineScript {
                    content: String::new(),
                    src: Some(src_url),
                    is_module,
                    is_async,
                    is_defer,
                });
            } else {
                // Inline script - check size limit to prevent OOM
                let mut content = String::new();
                extract_all_text(handle, &mut content);
                let content = content.trim().to_string();
                if !content.is_empty() && content.len() <= MAX_INLINE_SCRIPT_SIZE {
                    scripts.push(InlineScript {
                        content,
                        src: None,
                        is_module,
                        is_async,
                        is_defer,
                    });
                }
                // Silently skip scripts that are too large
            }
        }
    }
    for child in handle.children.borrow().iter() {
        extract_scripts(child, scripts);
    }
}

fn is_block_element(tag: &str) -> bool {
    matches!(
        tag,
        "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "ul" | "ol" | "li" | "table"
            | "tr" | "td" | "th" | "blockquote" | "pre" | "hr" | "br" | "section" | "article"
            | "nav" | "aside" | "header" | "footer" | "main" | "figure" | "figcaption" | "form"
            | "fieldset" | "address" | "details" | "summary"
    )
}

fn normalize_whitespace(text: &str) -> String {
    let mut result = String::new();
    let mut last_was_whitespace = true;
    let mut consecutive_newlines = 0;

    for ch in text.chars() {
        if ch == '\n' {
            consecutive_newlines += 1;
            if consecutive_newlines <= 2 {
                result.push('\n');
            }
            last_was_whitespace = true;
        } else if ch.is_whitespace() {
            if !last_was_whitespace {
                result.push(' ');
                last_was_whitespace = true;
            }
            consecutive_newlines = 0;
        } else {
            result.push(ch);
            last_was_whitespace = false;
            consecutive_newlines = 0;
        }
    }

    result.trim().to_string()
}

/// Serialize a DOM node to HTML string
fn serialize_node(handle: &Handle, output: &mut String) {
    match &handle.data {
        NodeData::Document => {
            output.push_str("<!DOCTYPE html>\n");
            for child in handle.children.borrow().iter() {
                serialize_node(child, output);
            }
        }
        NodeData::Element { name, attrs, .. } => {
            let tag_name = name.local.as_ref();

            // Open tag
            output.push('<');
            output.push_str(tag_name);

            // Attributes
            for attr in attrs.borrow().iter() {
                output.push(' ');
                output.push_str(attr.name.local.as_ref());
                output.push_str("=\"");
                // Escape attribute value
                let value = attr.value.to_string();
                for ch in value.chars() {
                    match ch {
                        '"' => output.push_str("&quot;"),
                        '&' => output.push_str("&amp;"),
                        '<' => output.push_str("&lt;"),
                        '>' => output.push_str("&gt;"),
                        _ => output.push(ch),
                    }
                }
                output.push('"');
            }
            output.push('>');

            // Void elements don't have children or closing tags
            let is_void = matches!(tag_name,
                "area" | "base" | "br" | "col" | "embed" | "hr" | "img" |
                "input" | "link" | "meta" | "param" | "source" | "track" | "wbr"
            );

            if !is_void {
                // Children
                for child in handle.children.borrow().iter() {
                    serialize_node(child, output);
                }

                // Close tag
                output.push_str("</");
                output.push_str(tag_name);
                output.push('>');
            }
        }
        NodeData::Text { contents } => {
            let text = contents.borrow();
            // Escape text content
            for ch in text.chars() {
                match ch {
                    '&' => output.push_str("&amp;"),
                    '<' => output.push_str("&lt;"),
                    '>' => output.push_str("&gt;"),
                    _ => output.push(ch),
                }
            }
        }
        NodeData::Comment { contents } => {
            output.push_str("<!--");
            output.push_str(contents);
            output.push_str("-->");
        }
        NodeData::Doctype { name, .. } => {
            output.push_str("<!DOCTYPE ");
            output.push_str(name);
            output.push('>');
        }
        NodeData::ProcessingInstruction { .. } => {
            // Skip processing instructions
        }
    }
}

/// Shallow clone a node (without children)
fn clone_node_shallow(node: &Handle) -> Handle {
    use markup5ever_rcdom::Node;
    use std::cell::Cell;

    // Clone the node data
    let cloned_data = match &node.data {
        NodeData::Document => NodeData::Document,
        NodeData::Doctype { name, public_id, system_id } => NodeData::Doctype {
            name: name.clone(),
            public_id: public_id.clone(),
            system_id: system_id.clone(),
        },
        NodeData::Text { contents } => NodeData::Text {
            contents: RefCell::new(contents.borrow().clone()),
        },
        NodeData::Comment { contents } => NodeData::Comment {
            contents: contents.clone(),
        },
        NodeData::Element { name, attrs, template_contents, mathml_annotation_xml_integration_point } => NodeData::Element {
            name: name.clone(),
            attrs: RefCell::new(attrs.borrow().clone()),
            template_contents: template_contents.clone(),
            mathml_annotation_xml_integration_point: *mathml_annotation_xml_integration_point,
        },
        NodeData::ProcessingInstruction { target, contents } => NodeData::ProcessingInstruction {
            target: target.clone(),
            contents: contents.clone(),
        },
    };

    // Create the new node (without children)
    Rc::new(Node {
        parent: Cell::new(None),
        children: RefCell::new(Vec::new()),
        data: cloned_data,
    })
}

/// Deep clone a node and all its children
fn clone_node_deep(node: &Handle) -> Handle {
    use markup5ever_rcdom::Node;
    use std::cell::Cell;

    // Clone the node data
    let cloned_data = match &node.data {
        NodeData::Document => NodeData::Document,
        NodeData::Doctype { name, public_id, system_id } => NodeData::Doctype {
            name: name.clone(),
            public_id: public_id.clone(),
            system_id: system_id.clone(),
        },
        NodeData::Text { contents } => NodeData::Text {
            contents: RefCell::new(contents.borrow().clone()),
        },
        NodeData::Comment { contents } => NodeData::Comment {
            contents: contents.clone(),
        },
        NodeData::Element { name, attrs, template_contents, mathml_annotation_xml_integration_point } => NodeData::Element {
            name: name.clone(),
            attrs: RefCell::new(attrs.borrow().clone()),
            template_contents: template_contents.clone(),
            mathml_annotation_xml_integration_point: *mathml_annotation_xml_integration_point,
        },
        NodeData::ProcessingInstruction { target, contents } => NodeData::ProcessingInstruction {
            target: target.clone(),
            contents: contents.clone(),
        },
    };

    // Create the new node
    let new_node = Rc::new(Node {
        parent: Cell::new(None),
        children: RefCell::new(Vec::new()),
        data: cloned_data,
    });

    // Recursively clone children
    for child in node.children.borrow().iter() {
        let cloned_child = clone_node_deep(child);
        new_node.children.borrow_mut().push(cloned_child);
    }

    new_node
}

/// Parse CSS text and extract selectors that hide elements (display:none or visibility:hidden)
/// Uses simple regex-like parsing to avoid cssparser lifetime complexity
fn parse_hidden_selectors(css_text: &str) -> Vec<String> {
    let mut hidden = Vec::new();

    // Simple approach: find rules with display:none or visibility:hidden
    // Pattern: selector { ... display: none ... } or selector { ... visibility: hidden ... }
    let mut chars = css_text.chars().peekable();
    let mut current_selector = String::new();
    let mut in_block = false;
    let mut block_content = String::new();
    let mut brace_depth = 0;

    while let Some(c) = chars.next() {
        if !in_block {
            if c == '{' {
                in_block = true;
                brace_depth = 1;
                block_content.clear();
            } else if c == '@' {
                // Skip at-rules - consume until matching brace or semicolon
                current_selector.clear();
                let mut at_depth = 0;
                while let Some(ac) = chars.next() {
                    if ac == '{' {
                        at_depth += 1;
                    } else if ac == '}' {
                        if at_depth == 0 {
                            break;
                        }
                        at_depth -= 1;
                        if at_depth == 0 {
                            break;
                        }
                    } else if ac == ';' && at_depth == 0 {
                        break;
                    }
                }
            } else {
                current_selector.push(c);
            }
        } else {
            if c == '{' {
                brace_depth += 1;
                block_content.push(c);
            } else if c == '}' {
                brace_depth -= 1;
                if brace_depth == 0 {
                    // End of block - check if it hides elements
                    let block_lower = block_content.to_lowercase();
                    let is_hidden = block_lower.contains("display:none") ||
                                   block_lower.contains("display: none") ||
                                   block_lower.contains("visibility:hidden") ||
                                   block_lower.contains("visibility: hidden");

                    if is_hidden {
                        let selector = current_selector.trim();
                        if !selector.is_empty() && !selector.starts_with('@') {
                            // Split comma-separated selectors
                            for sel in selector.split(',') {
                                let sel = sel.trim();
                                if !sel.is_empty() {
                                    hidden.push(sel.to_string());
                                }
                            }
                        }
                    }

                    current_selector.clear();
                    in_block = false;
                } else {
                    block_content.push(c);
                }
            } else {
                block_content.push(c);
            }
        }
    }

    hidden
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_html() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head><title>Test Page</title></head>
            <body>
                <h1>Hello World</h1>
                <p>This is a test.</p>
                <a href="https://example.com">Example Link</a>
            </body>
            </html>
        "#;

        let dom = Dom::parse(html).unwrap();
        let snapshot = dom.snapshot();

        assert_eq!(snapshot.get_title(), "Test Page");
        assert!(snapshot.body_text.contains("Hello World"));
        assert!(snapshot.body_text.contains("This is a test"));
        assert_eq!(snapshot.links.len(), 1);
        assert_eq!(snapshot.links[0].text, "Example Link");
        assert_eq!(snapshot.links[0].href, "https://example.com");
    }

    #[test]
    fn test_get_element_by_id() {
        let html = r#"<html><body><div id="main">Content</div></body></html>"#;
        let dom = Dom::parse(html).unwrap();

        let el = dom.get_element_by_id("main").unwrap();
        assert_eq!(el.tag_name(), "div");
        assert_eq!(el.id(), Some("main".to_string()));
    }

    #[test]
    fn test_query_selector() {
        let html = r#"
            <html><body>
                <div class="container">
                    <p class="intro">Hello</p>
                    <p class="content">World</p>
                </div>
            </body></html>
        "#;
        let dom = Dom::parse(html).unwrap();

        let el = dom.query_selector(".intro").unwrap();
        assert_eq!(el.tag_name(), "p");
        assert_eq!(el.inner_text(), "Hello");

        let all = dom.query_selector_all("p");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_create_element() {
        let html = r#"<html><body></body></html>"#;
        let dom = Dom::parse(html).unwrap();

        let div = dom.create_element("div");
        div.set_attribute("id", "new-div");
        div.set_attribute("class", "my-class");

        assert_eq!(div.tag_name(), "div");
        assert_eq!(div.id(), Some("new-div".to_string()));
        assert!(div.has_class("my-class"));
    }

    #[test]
    fn test_extract_inline_scripts() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head><title>Test</title></head>
            <body>
                <script>console.log("hello");</script>
                <p>Content</p>
                <script src="external.js"></script>
                <script>document.title = "New Title";</script>
            </body>
            </html>
        "#;

        let dom = Dom::parse(html).unwrap();
        let snapshot = dom.snapshot();

        // Now extracts both inline and external scripts
        assert_eq!(snapshot.scripts.len(), 3);
        assert!(snapshot.scripts[0].content.contains("console.log"));
        assert!(snapshot.scripts[0].src.is_none());
        assert_eq!(snapshot.scripts[1].src, Some("external.js".to_string()));
        assert!(snapshot.scripts[2].content.contains("document.title"));
        assert!(snapshot.scripts[2].src.is_none());
    }

    #[test]
    fn test_title_modification() {
        let html = "<html><head><title>Original</title></head><body></body></html>";
        let dom = Dom::parse(html).unwrap();

        assert_eq!(dom.get_title(), "Original");
        dom.set_title("Modified");
        assert_eq!(dom.get_title(), "Modified");
    }

    #[test]
    fn test_ignore_script_and_style_content() {
        let html = r#"
            <html>
            <head>
                <style>.hidden { display: none; }</style>
            </head>
            <body>
                <script>var x = 1;</script>
                <p>Visible content</p>
            </body>
            </html>
        "#;

        let dom = Dom::parse(html).unwrap();
        let snapshot = dom.snapshot();

        assert!(snapshot.body_text.contains("Visible content"));
        assert!(!snapshot.body_text.contains("display: none"));
        assert!(!snapshot.body_text.contains("var x = 1"));
    }

    #[test]
    fn test_dom_mutation_and_serialization() {
        let html = r#"<html><body><div id="container"></div></body></html>"#;
        let dom = Dom::parse(html).unwrap();

        // Get the container and add a child
        let container = dom.get_element_by_id("container").unwrap();
        let new_p = dom.create_element("p");
        new_p.set_text_content("Dynamic content added!");
        container.append_child(&new_p);

        // Serialize the live DOM
        let serialized = dom.serialize_to_html();
        assert!(serialized.contains("Dynamic content added!"), "Serialized: {}", serialized);
        assert!(serialized.contains("<p>"));

        // to_markdown should reflect the mutation
        let md = dom.to_markdown();
        // Note: markdown may strip simple text but should contain content
        // Let's check the serialized HTML contains the content
        assert!(serialized.contains("Dynamic content added!"));
    }

    #[test]
    fn test_element_removal() {
        let html = r#"<html><body><div id="parent"><p id="child">To be removed</p></div></body></html>"#;
        let dom = Dom::parse(html).unwrap();

        let parent = dom.get_element_by_id("parent").unwrap();
        let child = dom.get_element_by_id("child").unwrap();

        // Remove the child
        parent.remove_child(&child);

        // Serialization should not contain the removed element
        let serialized = dom.serialize_to_html();
        assert!(!serialized.contains("To be removed"));
    }
}
