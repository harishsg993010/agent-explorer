//! Layout tree and formatting contexts.
//!
//! The layout tree is built from the DOM tree with computed styles,
//! organizing elements into formatting contexts for layout.

use super::{ComputedStyle, Display, Viewport};
use crate::ids::NodeId;
use std::collections::HashMap;

/// A box in the layout tree
#[derive(Debug, Clone)]
pub struct LayoutBox {
    /// Source DOM node
    pub node_id: NodeId,
    /// Tag name (for semantic handling)
    pub tag: String,
    /// Computed style
    pub style: ComputedStyle,
    /// Text content (for text nodes/leaves)
    pub text: Option<String>,
    /// Child boxes
    pub children: Vec<LayoutBox>,
    /// Formatting context this box establishes
    pub formatting_context: Option<FormattingContext>,
    /// Attributes relevant for layout (href, src, alt, etc.)
    pub attrs: HashMap<String, String>,
    /// Is this box an anonymous box created for layout?
    pub anonymous: bool,
}

impl LayoutBox {
    pub fn new(node_id: NodeId, tag: impl Into<String>, style: ComputedStyle) -> Self {
        LayoutBox {
            node_id,
            tag: tag.into(),
            style,
            text: None,
            children: Vec::new(),
            formatting_context: None,
            attrs: HashMap::new(),
            anonymous: false,
        }
    }

    pub fn text_node(node_id: NodeId, text: impl Into<String>) -> Self {
        let mut style = ComputedStyle::default();
        style.display = Display::Inline;
        LayoutBox {
            node_id,
            tag: "#text".to_string(),
            style,
            text: Some(text.into()),
            children: Vec::new(),
            formatting_context: None,
            attrs: HashMap::new(),
            anonymous: false,
        }
    }

    pub fn anonymous_block(node_id: NodeId) -> Self {
        LayoutBox {
            node_id,
            tag: "#anon-block".to_string(),
            style: ComputedStyle::default(),
            text: None,
            children: Vec::new(),
            formatting_context: None,
            attrs: HashMap::new(),
            anonymous: true,
        }
    }

    pub fn anonymous_inline(node_id: NodeId) -> Self {
        let mut style = ComputedStyle::default();
        style.display = Display::Inline;
        LayoutBox {
            node_id,
            tag: "#anon-inline".to_string(),
            style,
            text: None,
            children: Vec::new(),
            formatting_context: None,
            attrs: HashMap::new(),
            anonymous: true,
        }
    }

    pub fn is_block(&self) -> bool {
        self.style.is_block_level()
    }

    pub fn is_inline(&self) -> bool {
        self.style.is_inline_level() || self.is_text()
    }

    pub fn is_text(&self) -> bool {
        self.text.is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.children.is_empty() && self.text.as_ref().map(|t| t.is_empty()).unwrap_or(true)
    }

    /// Check if this element is a semantic heading
    pub fn is_heading(&self) -> Option<u8> {
        match self.tag.as_str() {
            "h1" => Some(1),
            "h2" => Some(2),
            "h3" => Some(3),
            "h4" => Some(4),
            "h5" => Some(5),
            "h6" => Some(6),
            _ => None,
        }
    }

    /// Check if this is a list element
    pub fn is_list(&self) -> bool {
        matches!(self.tag.as_str(), "ul" | "ol" | "menu")
    }

    /// Check if this is a list item
    pub fn is_list_item(&self) -> bool {
        self.tag == "li" || self.style.display == Display::ListItem
    }

    /// Check if this is a table element
    pub fn is_table(&self) -> bool {
        self.tag == "table" || self.style.display == Display::Table
    }

    /// Check if this is a link
    pub fn is_link(&self) -> bool {
        self.tag == "a" && self.attrs.contains_key("href")
    }

    /// Check if this is an image
    pub fn is_image(&self) -> bool {
        self.tag == "img"
    }

    /// Check if this is a form input
    pub fn is_input(&self) -> bool {
        matches!(self.tag.as_str(), "input" | "textarea" | "select" | "button")
    }

    /// Check if this is a code element
    pub fn is_code(&self) -> bool {
        matches!(self.tag.as_str(), "code" | "pre" | "kbd" | "samp" | "tt")
    }

    /// Check if this is a blockquote
    pub fn is_blockquote(&self) -> bool {
        self.tag == "blockquote"
    }

    /// Get href attribute
    pub fn href(&self) -> Option<&str> {
        self.attrs.get("href").map(|s| s.as_str())
    }

    /// Get src attribute
    pub fn src(&self) -> Option<&str> {
        self.attrs.get("src").map(|s| s.as_str())
    }

    /// Get alt text
    pub fn alt(&self) -> Option<&str> {
        self.attrs.get("alt").map(|s| s.as_str())
    }

    /// Push a child box
    pub fn push(&mut self, child: LayoutBox) {
        self.children.push(child);
    }

    /// Check if all children are inline
    pub fn all_children_inline(&self) -> bool {
        self.children.iter().all(|c| c.is_inline())
    }

    /// Check if any child is a block
    pub fn any_child_block(&self) -> bool {
        self.children.iter().any(|c| c.is_block())
    }
}

/// The type of formatting context established by a box
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormattingContext {
    /// Block formatting context
    Block,
    /// Inline formatting context
    Inline,
    /// Flex formatting context
    Flex,
    /// Table formatting context
    Table,
    /// Grid formatting context (degrades to block)
    Grid,
}

/// The complete layout tree
#[derive(Debug)]
pub struct LayoutTree {
    /// Root box
    pub root: LayoutBox,
    /// Viewport constraints
    pub viewport: Viewport,
    /// Overlay boxes (fixed/absolute positioned)
    pub overlays: Vec<LayoutBox>,
}

impl LayoutTree {
    pub fn new(root: LayoutBox, viewport: Viewport) -> Self {
        LayoutTree {
            root,
            viewport,
            overlays: Vec::new(),
        }
    }

    /// Build a layout tree from a DOM tree with style resolver
    pub fn from_dom<F>(
        root_node_id: NodeId,
        viewport: Viewport,
        get_children: impl Fn(NodeId) -> Vec<NodeId>,
        get_tag: impl Fn(NodeId) -> String,
        get_text: impl Fn(NodeId) -> Option<String>,
        get_style: F,
        get_attrs: impl Fn(NodeId) -> HashMap<String, String>,
    ) -> Self
    where
        F: Fn(NodeId) -> ComputedStyle,
    {
        fn build_box<F>(
            node_id: NodeId,
            get_children: &impl Fn(NodeId) -> Vec<NodeId>,
            get_tag: &impl Fn(NodeId) -> String,
            get_text: &impl Fn(NodeId) -> Option<String>,
            get_style: &F,
            get_attrs: &impl Fn(NodeId) -> HashMap<String, String>,
            overlays: &mut Vec<LayoutBox>,
        ) -> Option<LayoutBox>
        where
            F: Fn(NodeId) -> ComputedStyle,
        {
            let tag = get_tag(node_id);
            let style = get_style(node_id);

            // Skip invisible elements
            if !style.is_visible() {
                return None;
            }

            // Check for text node
            if let Some(text) = get_text(node_id) {
                if text.is_empty() {
                    return None;
                }
                return Some(LayoutBox::text_node(node_id, text));
            }

            let mut layout_box = LayoutBox::new(node_id, &tag, style.clone());
            layout_box.attrs = get_attrs(node_id);

            // Determine formatting context
            layout_box.formatting_context = match style.display {
                Display::Flex | Display::InlineFlex => Some(FormattingContext::Flex),
                Display::Table => Some(FormattingContext::Table),
                Display::Grid => Some(FormattingContext::Grid),
                Display::Block if style.overflow_x != super::Overflow::Visible => {
                    Some(FormattingContext::Block)
                }
                _ => None,
            };

            // Build children
            for child_id in get_children(node_id) {
                if let Some(child_box) = build_box(
                    child_id,
                    get_children,
                    get_tag,
                    get_text,
                    get_style,
                    get_attrs,
                    overlays,
                ) {
                    // Check if child is an overlay
                    if child_box.style.is_overlay() {
                        overlays.push(child_box);
                    } else {
                        layout_box.push(child_box);
                    }
                }
            }

            // Wrap inline children if mixed with block
            if layout_box.is_block() && !layout_box.children.is_empty() {
                layout_box.children = normalize_children(node_id, layout_box.children);
            }

            Some(layout_box)
        }

        let mut overlays = Vec::new();
        let root = build_box(
            root_node_id,
            &get_children,
            &get_tag,
            &get_text,
            &get_style,
            &get_attrs,
            &mut overlays,
        )
        .unwrap_or_else(|| LayoutBox::new(root_node_id, "div", ComputedStyle::default()));

        LayoutTree {
            root,
            viewport,
            overlays,
        }
    }
}

/// Normalize mixed inline/block children by wrapping inline runs in anonymous blocks
fn normalize_children(parent_id: NodeId, children: Vec<LayoutBox>) -> Vec<LayoutBox> {
    let has_blocks = children.iter().any(|c| c.is_block());
    if !has_blocks {
        // All inline, no normalization needed
        return children;
    }

    let mut result = Vec::new();
    let mut inline_run: Vec<LayoutBox> = Vec::new();

    for child in children {
        if child.is_block() {
            // Flush inline run
            if !inline_run.is_empty() {
                let mut anon = LayoutBox::anonymous_block(parent_id);
                anon.children = std::mem::take(&mut inline_run);
                result.push(anon);
            }
            result.push(child);
        } else {
            inline_run.push(child);
        }
    }

    // Flush remaining inline run
    if !inline_run.is_empty() {
        let mut anon = LayoutBox::anonymous_block(parent_id);
        anon.children = inline_run;
        result.push(anon);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_box_creation() {
        let node_id = NodeId::new(1);
        let style = ComputedStyle::default();
        let layout_box = LayoutBox::new(node_id, "div", style);

        assert_eq!(layout_box.tag, "div");
        assert!(layout_box.is_block());
        assert!(!layout_box.is_inline());
    }

    #[test]
    fn test_text_node() {
        let node_id = NodeId::new(1);
        let text_box = LayoutBox::text_node(node_id, "Hello");

        assert!(text_box.is_text());
        assert!(text_box.is_inline());
        assert_eq!(text_box.text, Some("Hello".to_string()));
    }

    #[test]
    fn test_heading_detection() {
        let node_id = NodeId::new(1);
        let h1 = LayoutBox::new(node_id, "h1", ComputedStyle::default());
        let div = LayoutBox::new(node_id, "div", ComputedStyle::default());

        assert_eq!(h1.is_heading(), Some(1));
        assert_eq!(div.is_heading(), None);
    }
}
