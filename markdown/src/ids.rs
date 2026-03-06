//! Stable identifiers for layout elements.
//!
//! These IDs remain stable across layout passes and can be used
//! for interaction targets and scroll anchors.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique identifier for a DOM node (provided by upstream)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u64);

static NEXT_NODE_ID: AtomicU64 = AtomicU64::new(1);

impl NodeId {
    pub fn new(id: u64) -> Self {
        NodeId(id)
    }

    /// Create a new unique NodeId (for testing/builders)
    pub fn new_unique() -> Self {
        NodeId(NEXT_NODE_ID.fetch_add(1, Ordering::SeqCst))
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Unique identifier for a link in the output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkId(pub u64);

static NEXT_LINK_ID: AtomicU64 = AtomicU64::new(1);

impl LinkId {
    pub fn new() -> Self {
        LinkId(NEXT_LINK_ID.fetch_add(1, Ordering::SeqCst))
    }

    pub fn from_raw(id: u64) -> Self {
        LinkId(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl Default for LinkId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for an overlay/popup
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OverlayId(pub u64);

static NEXT_OVERLAY_ID: AtomicU64 = AtomicU64::new(1);

impl OverlayId {
    pub fn new() -> Self {
        OverlayId(NEXT_OVERLAY_ID.fetch_add(1, Ordering::SeqCst))
    }

    pub fn from_raw(id: u64) -> Self {
        OverlayId(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl Default for OverlayId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for interactive widgets (inputs, buttons)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidgetId(pub u64);

static NEXT_WIDGET_ID: AtomicU64 = AtomicU64::new(1);

impl WidgetId {
    pub fn new() -> Self {
        WidgetId(NEXT_WIDGET_ID.fetch_add(1, Ordering::SeqCst))
    }

    pub fn from_raw(id: u64) -> Self {
        WidgetId(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl Default for WidgetId {
    fn default() -> Self {
        Self::new()
    }
}

/// Widget metadata for interactive elements
#[derive(Debug, Clone)]
pub struct WidgetInfo {
    pub id: WidgetId,
    pub node_id: NodeId,
    pub widget_type: WidgetType,
    pub name: Option<String>,
    pub value: String,
    pub placeholder: Option<String>,
    pub label: Option<String>,
    pub checked: bool,
    pub disabled: bool,
    /// Action URL for forms/buttons
    pub action: Option<String>,
    /// HTTP method for form submission
    pub method: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidgetType {
    TextInput,
    PasswordInput,
    EmailInput,
    SearchInput,
    UrlInput,
    TelInput,
    NumberInput,
    TextArea,
    Button,
    SubmitButton,
    Checkbox,
    Radio,
    Select,
    Hidden,
}

impl WidgetType {
    pub fn from_input_type(input_type: &str) -> Self {
        match input_type.to_lowercase().as_str() {
            "password" => WidgetType::PasswordInput,
            "email" => WidgetType::EmailInput,
            "search" => WidgetType::SearchInput,
            "url" => WidgetType::UrlInput,
            "tel" => WidgetType::TelInput,
            "number" => WidgetType::NumberInput,
            "checkbox" => WidgetType::Checkbox,
            "radio" => WidgetType::Radio,
            "submit" => WidgetType::SubmitButton,
            "button" => WidgetType::Button,
            "hidden" => WidgetType::Hidden,
            _ => WidgetType::TextInput,
        }
    }
}

/// Link anchor information
#[derive(Debug, Clone)]
pub struct AnchorInfo {
    pub id: LinkId,
    pub url: String,
    pub source_node: NodeId,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
}

/// Map of all anchors in the output
pub type AnchorMap = HashMap<LinkId, AnchorInfo>;

/// Map of all widgets in the output
pub type WidgetMap = HashMap<WidgetId, WidgetInfo>;

/// Map of scroll targets (NodeId -> line number)
pub type ScrollTargets = HashMap<NodeId, usize>;
