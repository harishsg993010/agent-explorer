//! Markdown crate - Convert DOM snapshots to Markdown representation.
//!
//! Renders semantic HTML content as clean, readable Markdown output.
//!
//! Multiple rendering modes are available:
//! - `render()` - Uses custom rendering logic with link extraction
//! - `render_dom_query()` - Uses dom_query's built-in markdown serialization
//! - `LayoutEngine` - Full CSS-aware layout engine with margin collapsing, flex, tables
//!
//! # Layout Engine
//!
//! The layout engine provides Chrome-like rendering of DOM content to Markdown:
//!
//! ```ignore
//! use markdown::{LayoutEngine, LayoutEngineConfig, Viewport};
//!
//! let config = LayoutEngineConfig::default();
//! let engine = LayoutEngine::new(config);
//! let viewport = Viewport::new(80);
//!
//! // Layout from DOM with computed styles
//! let output = engine.layout(&dom, &viewport, &styles);
//! println!("{}", output.markdown);
//! ```

pub mod ast;
pub mod dom_converter;
pub mod ids;
pub mod layout;
pub mod render;
pub mod style_store;

use dom::{Dom, DomSnapshot};
use std::rc::Rc;

// Re-exports for public API
pub use ast::{Block, BlockKind, Document, InlineContent, Overlay, OverlayPlan, Span, SpanKind};
pub use ids::{AnchorInfo, AnchorMap, LinkId, NodeId, OverlayId, WidgetId, WidgetInfo, WidgetMap, WidgetType};
pub use layout::{
    AlignItems, ComputedStyle, ContainerCondition, ContainerQuery, ContainerType, Display,
    FlexDirection, FlexWrap, FontStyle, FontWeight, JustifyContent, LayoutPlan, LineRecord,
    ListStyleType, Overflow, PipelineOutput, Position, SimpleUrlResolver, TextAlign,
    TextDecoration, UrlResolver, Viewport, Visibility, WhiteSpace,
};
pub use render::{RenderConfig, RenderResult};
pub use style_store::{ElementId, StyleStore};

/// Layout a DOM and return the pipeline output with widget map.
///
/// This is the main entry point for the layout engine from a DOM.
/// Returns a tuple of (PipelineOutput, WidgetMap) for rendering.
pub fn layout_dom(dom: &Dom, viewport: &Viewport) -> (PipelineOutput, WidgetMap) {
    use crate::layout::block::{layout_block, BlockLayoutContext};

    // Convert DOM to LayoutTree
    let layout_tree = dom_converter::dom_to_layout_tree(dom, viewport.clone());

    // Create layout context and layout the tree
    let mut ctx = BlockLayoutContext::new(viewport);
    let blocks = layout_block(&layout_tree.root, &mut ctx);

    // Create layout plan
    let layout_plan = LayoutPlan {
        blocks,
        overlays: OverlayPlan::default(),
    };

    // Render to Markdown
    let render_config = RenderConfig {
        max_width: viewport.width,
        trailing_newline: true,
        reference_links: false,
        indent_string: "  ".to_string(),
        emit_source_map: true,
    };

    let render_result = render::render(&layout_plan, &render_config);

    let output = PipelineOutput {
        markdown: render_result.markdown,
        layout_plan,
        line_map: render_result.line_map,
    };

    // TODO: Build widget map from layout tree
    let widget_map = WidgetMap::new();

    (output, widget_map)
}

/// Render a DOM snapshot as Markdown text.
///
/// Output format:
/// - H1 with page title
/// - Body text as paragraphs
/// - Links section at bottom
pub fn render(snapshot: &Rc<DomSnapshot>) -> String {
    let mut output = String::new();

    // Title as H1
    let title = snapshot.get_title();
    if !title.is_empty() {
        output.push_str("# ");
        output.push_str(&title);
        output.push_str("\n\n");
    }

    // Body text
    let body = &snapshot.body_text;
    if !body.is_empty() {
        // Split into paragraphs and format
        for paragraph in body.split("\n\n") {
            let trimmed = paragraph.trim();
            if !trimmed.is_empty() {
                output.push_str(trimmed);
                output.push_str("\n\n");
            }
        }
    }

    // Links section
    if !snapshot.links.is_empty() {
        output.push_str("---\n\n");
        output.push_str("## Links\n\n");

        for link in &snapshot.links {
            output.push_str("- [");
            output.push_str(&escape_markdown(&link.text));
            output.push_str("](");
            output.push_str(&escape_url(&link.href));
            output.push_str(")\n");
        }
    }

    output.trim_end().to_string()
}

/// Render a DOM using dom_query's built-in markdown serialization.
///
/// This uses dom_query's advanced HTML-to-Markdown conversion which handles:
/// - Proper heading conversion
/// - Bold/italic text
/// - Links and images
/// - Lists (ordered and unordered)
/// - Code blocks and inline code
/// - Blockquotes
/// - Tables
/// - Form elements (input, button, textarea, select)
///
/// Automatically skips script, style, meta, and head tags.
pub fn render_dom_query(dom: &Dom) -> String {
    let mut output = String::new();

    // Add title as H1
    let title = dom.get_title();
    if !title.is_empty() {
        output.push_str("# ");
        output.push_str(&title);
        output.push_str("\n\n");
    }

    // Use dom_query's markdown rendering
    let body_md = dom.to_markdown();
    if !body_md.is_empty() {
        output.push_str(&body_md);
        output.push_str("\n\n");
    }

    // Extract form elements
    let html = dom.query_html("html").unwrap_or_default();
    let doc = dom::dom_query::Document::from(html.as_str());

    let mut element_id: u64 = 1;
    let mut has_forms = false;

    // Process each form element
    for form_node in doc.select("form").iter() {
        // Try multiple sources for form action
        let action = form_node.attr("action")
            .or_else(|| form_node.attr("data-action"))
            .or_else(|| form_node.attr("data-url"))
            .unwrap_or_default();
        let method = form_node.attr("method").map(|m| m.to_lowercase()).unwrap_or_else(|| "get".to_string());

        if !has_forms {
            output.push_str("\n---\n\n## Form\n\n");
            has_forms = true;
        }

        // Output form marker with action: {{FORM:action:method}}
        output.push_str(&format!(
            "{{{{FORM:{}:{}}}}}\n",
            escape_form_field(&action),
            method
        ));

        // Find inputs within this form
        for node in form_node.select("input[type='text'], input[type='search'], input[type='email'], input[type='password'], input[type='url'], input[type='tel'], input:not([type])").iter() {
            let input_type = node.attr("type").unwrap_or("text".into());
            // Try name, then id, then data-name as fallbacks
            let name = node.attr("name")
                .or_else(|| node.attr("id"))
                .or_else(|| node.attr("data-name"))
                .unwrap_or_default();
            let placeholder = node.attr("placeholder").unwrap_or_default();
            let value = node.attr("value").unwrap_or_default();

            output.push_str(&format!(
                "{{{{INPUT:{}:{}:{}:{}:{}}}}}\n",
                element_id,
                input_type,
                escape_form_field(&name),
                escape_form_field(&placeholder),
                escape_form_field(&value)
            ));
            element_id += 1;
        }

        // Textareas within form
        for node in form_node.select("textarea").iter() {
            let name = node.attr("name")
                .or_else(|| node.attr("id"))
                .or_else(|| node.attr("data-name"))
                .unwrap_or_default();
            let placeholder = node.attr("placeholder").unwrap_or_default();
            let value = node.text();

            output.push_str(&format!(
                "{{{{INPUT:{}:textarea:{}:{}:{}}}}}\n",
                element_id,
                escape_form_field(&name),
                escape_form_field(&placeholder),
                escape_form_field(&value.trim())
            ));
            element_id += 1;
        }

        // Buttons within form - check for formaction override
        for node in form_node.select("button, input[type='submit'], input[type='button']").iter() {
            // Check if button has formaction that overrides form action
            if let Some(formaction) = node.attr("formaction") {
                // Re-output FORM marker with button's formaction
                output.push_str(&format!(
                    "{{{{FORM:{}:{}}}}}\n",
                    escape_form_field(&formaction),
                    method
                ));
            }

            let label: String = if node.attr("type").as_deref() == Some("submit") || node.attr("type").as_deref() == Some("button") {
                node.attr("value").map(|s| s.to_string()).unwrap_or_else(|| "Submit".to_string())
            } else {
                let text = node.text();
                if text.trim().is_empty() {
                    node.attr("aria-label").map(|s| s.to_string()).unwrap_or_else(|| "Button".to_string())
                } else {
                    text.trim().to_string()
                }
            };

            output.push_str(&format!(
                "{{{{BUTTON:{}:{}}}}}\n",
                element_id,
                escape_form_field(&label)
            ));
            element_id += 1;
        }
    }

    // Also check for inputs/buttons outside of forms (common in modern SPAs)
    for node in doc.select("body > input, body > textarea, [role='search'] input, [role='search'] textarea").iter() {
        let input_type = node.attr("type").map(|s| s.to_string()).unwrap_or_else(|| "text".to_string());
        let name = node.attr("name")
            .or_else(|| node.attr("id"))
            .or_else(|| node.attr("data-name"))
            .unwrap_or_default();
        let placeholder = node.attr("placeholder").unwrap_or_default();
        let value = node.attr("value").unwrap_or_default();

        // Skip if already processed (has a form ancestor)
        if input_type == "hidden" || input_type == "submit" || input_type == "button" {
            continue;
        }

        if !has_forms {
            output.push_str("\n---\n\n## Form\n\n");
            output.push_str("{{FORM:/search:get}}\n"); // Default search action
            has_forms = true;
        }

        output.push_str(&format!(
            "{{{{INPUT:{}:{}:{}:{}:{}}}}}\n",
            element_id,
            input_type,
            escape_form_field(&name),
            escape_form_field(&placeholder),
            escape_form_field(&value)
        ));
        element_id += 1;
    }

    // Checkboxes
    for node in doc.select("input[type='checkbox']").iter() {
        let name = node.attr("name").unwrap_or_default();
        let label = node.attr("aria-label")
            .or_else(|| node.attr("title"))
            .unwrap_or_default();
        let checked = node.attr("checked").is_some();

        if !has_forms {
            output.push_str("\n---\n\n## Form\n\n");
            has_forms = true;
        }

        output.push_str(&format!(
            "{{{{CHECKBOX:{}:{}:{}:{}}}}}\n",
            element_id,
            escape_form_field(&name),
            escape_form_field(&label),
            if checked { "true" } else { "false" }
        ));
        element_id += 1;
    }

    // Extract links for a links section (using dom_query)
    let links: Vec<(String, String)> = doc
        .select("a[href]")
        .iter()
        .filter_map(|node| {
            let href = node.attr("href")?;
            let text = node.text();
            let text_str = text.trim();
            if !text_str.is_empty() && !href.starts_with('#') && !href.starts_with("javascript:") {
                Some((text_str.to_string(), href.to_string()))
            } else {
                None
            }
        })
        .collect();

    if !links.is_empty() {
        output.push_str("---\n\n");
        output.push_str("## Links\n\n");
        for (text, href) in links {
            output.push_str("- [");
            output.push_str(&escape_markdown(&text));
            output.push_str("](");
            output.push_str(&escape_url(&href));
            output.push_str(")\n");
        }
    }

    output.trim_end().to_string()
}

/// Render a DOM using the full Layout Engine pipeline.
///
/// This uses the CSS-aware layout engine which handles:
/// - Block and inline layout with proper flow
/// - Margin collapsing approximation
/// - Flexbox layout
/// - Table layout with auto-sizing
/// - Overlay detection (fixed, absolute, dialogs)
/// - Source mapping back to DOM nodes
///
/// This is more accurate for complex layouts but slightly slower than `render_dom_query`.
pub fn render_with_layout_engine(dom: &Dom, viewport_width: usize) -> String {
    render_with_style_store(dom, viewport_width, None)
}

/// Render DOM to Markdown using the Layout Engine with optional StyleStore integration.
///
/// When a StyleStore is provided, computed styles are stored during layout
/// for CSSOM↔Layout integration with JavaScript.
pub fn render_with_style_store(
    dom: &Dom,
    viewport_width: usize,
    style_store: Option<&StyleStore>,
) -> String {
    let viewport = Viewport::new(viewport_width);
    let layout_tree = dom_converter::dom_to_layout_tree_with_store(dom, viewport.clone(), style_store);

    let engine = LayoutEngine::default();
    let output = engine.layout_tree(&layout_tree.root, &viewport);

    let mut result = String::new();

    // Add title
    let title = dom.get_title();
    if !title.is_empty() {
        result.push_str("# ");
        result.push_str(&title);
        result.push_str("\n\n");
    }

    // Add rendered content
    if !output.markdown.is_empty() {
        result.push_str(&output.markdown);
        if !result.ends_with('\n') {
            result.push('\n');
        }
    }

    result.trim_end().to_string()
}

/// Escape special characters in form field values
fn escape_form_field(text: &str) -> String {
    text.replace(':', "\\:")
        .replace('}', "\\}")
        .replace('\n', " ")
        .replace('\r', "")
}

/// Escape special Markdown characters in text
fn escape_markdown(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '[' | ']' | '(' | ')' | '*' | '_' | '`' | '#' | '\\' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    result
}

/// Escape special characters in URLs for Markdown links
fn escape_url(url: &str) -> String {
    let mut result = String::with_capacity(url.len());
    for ch in url.chars() {
        match ch {
            ')' => result.push_str("%29"),
            '(' => result.push_str("%28"),
            ' ' => result.push_str("%20"),
            _ => result.push(ch),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use dom::Link;
    use std::cell::RefCell;

    fn create_snapshot(title: &str, body: &str, links: Vec<Link>) -> Rc<DomSnapshot> {
        Rc::new(DomSnapshot {
            title: RefCell::new(title.to_string()),
            body_text: body.to_string(),
            links,
            scripts: vec![],
        })
    }

    #[test]
    fn test_render_title() {
        let snapshot = create_snapshot("Test Page", "", vec![]);
        let md = render(&snapshot);

        assert!(md.starts_with("# Test Page"));
    }

    #[test]
    fn test_render_body() {
        let snapshot = create_snapshot("Title", "Hello world\n\nSecond paragraph", vec![]);
        let md = render(&snapshot);

        assert!(md.contains("Hello world"));
        assert!(md.contains("Second paragraph"));
    }

    #[test]
    fn test_render_links() {
        let links = vec![
            Link {
                text: "Example".to_string(),
                href: "https://example.com".to_string(),
            },
            Link {
                text: "Test Link".to_string(),
                href: "https://test.com/page".to_string(),
            },
        ];

        let snapshot = create_snapshot("Title", "Body", links);
        let md = render(&snapshot);

        assert!(md.contains("## Links"));
        assert!(md.contains("- [Example](https://example.com)"));
        assert!(md.contains("- [Test Link](https://test.com/page)"));
    }

    #[test]
    fn test_escape_markdown() {
        let text = "Hello [world] (test)";
        let escaped = escape_markdown(text);

        assert_eq!(escaped, "Hello \\[world\\] \\(test\\)");
    }

    #[test]
    fn test_escape_url() {
        let url = "https://example.com/path (with spaces)";
        let escaped = escape_url(url);

        assert_eq!(
            escaped,
            "https://example.com/path%20%28with%20spaces%29"
        );
    }

    #[test]
    fn test_empty_snapshot() {
        let snapshot = create_snapshot("", "", vec![]);
        let md = render(&snapshot);

        assert!(md.is_empty());
    }

    #[test]
    fn test_no_links_section_when_empty() {
        let snapshot = create_snapshot("Title", "Body text", vec![]);
        let md = render(&snapshot);

        assert!(!md.contains("## Links"));
        assert!(!md.contains("---"));
    }
}

// =============================================================================
// Layout Engine API
// =============================================================================

use crate::ast::OverlayRenderMode;
use crate::layout::block::BlockLayoutContext;
use crate::layout::overlay::{extract_overlays, filter_overlays_from_tree, has_blocking_modal, layout_overlays};
use crate::layout::tree::LayoutBox;
use std::collections::HashMap;

/// Configuration for the layout engine
#[derive(Debug, Clone)]
pub struct LayoutEngineConfig {
    /// Maximum heading depth to render (1-6)
    pub max_heading_depth: u8,
    /// Whether to enable soft wrapping
    pub soft_wrap: bool,
    /// Overlay rendering mode
    pub overlay_mode: OverlayRenderMode,
    /// Whether to extract and render overlays
    pub extract_overlays: bool,
    /// Whether to emit source mapping
    pub emit_source_map: bool,
    /// Default text alignment
    pub default_align: TextAlign,
    /// Indent string for nested content
    pub indent_string: String,
}

impl Default for LayoutEngineConfig {
    fn default() -> Self {
        LayoutEngineConfig {
            max_heading_depth: 6,
            soft_wrap: true,
            overlay_mode: OverlayRenderMode::InlineFallback,
            extract_overlays: true,
            emit_source_map: false,
            default_align: TextAlign::Left,
            indent_string: "  ".to_string(),
        }
    }
}

/// The main layout engine for converting DOM to Markdown
pub struct LayoutEngine {
    config: LayoutEngineConfig,
}

impl LayoutEngine {
    /// Create a new layout engine with the given configuration
    pub fn new(config: LayoutEngineConfig) -> Self {
        LayoutEngine { config }
    }

    /// Create a layout engine with default configuration
    pub fn default() -> Self {
        Self::new(LayoutEngineConfig::default())
    }

    /// Get the current configuration
    pub fn config(&self) -> &LayoutEngineConfig {
        &self.config
    }

    /// Layout a DOM with computed styles and produce Markdown output
    pub fn layout(
        &self,
        _dom: &Dom,
        viewport: &Viewport,
        styles: &HashMap<NodeId, ComputedStyle>,
    ) -> PipelineOutput {
        // Build layout tree from DOM
        // For now, create an empty tree - actual DOM integration would go here
        let root_id = NodeId::new(0);
        let root_style = styles.get(&root_id).cloned().unwrap_or_default();
        let root = LayoutBox::new(root_id, "body", root_style);

        self.layout_tree(&root, viewport)
    }

    /// Layout a pre-built layout tree
    pub fn layout_tree(&self, root: &LayoutBox, viewport: &Viewport) -> PipelineOutput {
        // Extract overlays if enabled
        let (main_tree, overlays) = if self.config.extract_overlays {
            let overlays = extract_overlays(root);
            let filtered = filter_overlays_from_tree(root);
            (filtered, overlays)
        } else {
            (root.clone(), Vec::new())
        };

        // Check for blocking modal
        let has_modal = has_blocking_modal(&overlays);

        // Layout main content
        let mut main_blocks = if has_modal && self.config.overlay_mode == OverlayRenderMode::ModalFocus {
            // Skip main content when modal is blocking
            Vec::new()
        } else {
            let mut ctx = BlockLayoutContext::new(viewport);
            crate::layout::block::layout_block(&main_tree, &mut ctx)
        };

        // Layout overlays
        let overlay_blocks = layout_overlays(&overlays, viewport, self.config.overlay_mode);

        // Combine blocks based on overlay mode
        let all_blocks = match self.config.overlay_mode {
            OverlayRenderMode::ModalFocus if has_modal => overlay_blocks,
            OverlayRenderMode::PinnedRegions => {
                // Overlays go at top/bottom
                let mut combined = overlay_blocks;
                combined.extend(main_blocks);
                combined
            }
            _ => {
                // Inline fallback - overlays go at the end
                main_blocks.extend(overlay_blocks);
                main_blocks
            }
        };

        // Create overlay plan
        let overlay_plan = OverlayPlan {
            overlays: overlays
                .into_iter()
                .map(|o| Overlay {
                    id: o.id,
                    kind: o.kind,
                    source_node: o.node_id,
                    content: Document::new(),
                    z_index: o.z_index,
                    visible: true,
                })
                .collect(),
            mode: self.config.overlay_mode,
        };

        // Create layout plan
        let layout_plan = LayoutPlan {
            blocks: all_blocks,
            overlays: overlay_plan,
        };

        // Render to Markdown
        let render_config = RenderConfig {
            max_width: viewport.width,
            trailing_newline: true,
            reference_links: false,
            indent_string: self.config.indent_string.clone(),
            emit_source_map: self.config.emit_source_map,
        };

        let render_result = crate::render::render(&layout_plan, &render_config);

        PipelineOutput {
            markdown: render_result.markdown,
            layout_plan,
            line_map: render_result.line_map,
        }
    }

    /// Layout inline content only (useful for fragments)
    pub fn layout_inline(&self, content: &InlineContent, viewport: &Viewport) -> String {
        crate::render::render_inline(content)
    }

    /// Quick layout from a layout box without full pipeline
    pub fn quick_layout(&self, root: &LayoutBox, viewport: &Viewport) -> String {
        let mut ctx = BlockLayoutContext::new(viewport);
        let blocks = crate::layout::block::layout_block(root, &mut ctx);

        let plan = LayoutPlan {
            blocks,
            overlays: OverlayPlan::default(),
        };

        crate::render::render_to_string(&plan, viewport)
    }
}

/// Builder for creating LayoutBox trees programmatically
pub struct LayoutBoxBuilder {
    node_id: NodeId,
    tag: String,
    style: ComputedStyle,
    text: Option<String>,
    children: Vec<LayoutBox>,
    attrs: HashMap<String, String>,
}

impl LayoutBoxBuilder {
    /// Create a new builder for a given tag
    pub fn new(tag: &str) -> Self {
        LayoutBoxBuilder {
            node_id: NodeId::new_unique(),
            tag: tag.to_string(),
            style: ComputedStyle::default(),
            text: None,
            children: Vec::new(),
            attrs: HashMap::new(),
        }
    }

    /// Set the node ID
    pub fn id(mut self, id: NodeId) -> Self {
        self.node_id = id;
        self
    }

    /// Set the computed style
    pub fn style(mut self, style: ComputedStyle) -> Self {
        self.style = style;
        self
    }

    /// Set display property
    pub fn display(mut self, display: Display) -> Self {
        self.style.display = display;
        self
    }

    /// Set text content (creates a child text node for block elements)
    pub fn text(mut self, text: &str) -> Self {
        // For block elements, create a child text node
        // For inline elements, set text directly
        let text_node = LayoutBox::text_node(NodeId::new_unique(), text);
        self.children.push(text_node);
        self
    }

    /// Set text content directly on this box (for text nodes)
    pub fn text_direct(mut self, text: &str) -> Self {
        self.text = Some(text.to_string());
        self
    }

    /// Add a child
    pub fn child(mut self, child: LayoutBox) -> Self {
        self.children.push(child);
        self
    }

    /// Add multiple children
    pub fn children(mut self, children: Vec<LayoutBox>) -> Self {
        self.children.extend(children);
        self
    }

    /// Set an attribute
    pub fn attr(mut self, key: &str, value: &str) -> Self {
        self.attrs.insert(key.to_string(), value.to_string());
        self
    }

    /// Set margin
    pub fn margin(mut self, top: i32, right: i32, bottom: i32, left: i32) -> Self {
        self.style.margin_top = top;
        self.style.margin_right = right;
        self.style.margin_bottom = bottom;
        self.style.margin_left = left;
        self
    }

    /// Set padding
    pub fn padding(mut self, top: i32, right: i32, bottom: i32, left: i32) -> Self {
        self.style.padding_top = top;
        self.style.padding_right = right;
        self.style.padding_bottom = bottom;
        self.style.padding_left = left;
        self
    }

    /// Set explicit width
    pub fn width(mut self, width: usize) -> Self {
        self.style.width = Some(width);
        self
    }

    /// Build the LayoutBox
    pub fn build(self) -> LayoutBox {
        let mut layout_box = LayoutBox::new(self.node_id, &self.tag, self.style);
        layout_box.text = self.text;
        layout_box.children = self.children;
        layout_box.attrs = self.attrs;
        layout_box
    }
}

#[cfg(test)]
mod layout_engine_tests {
    use super::*;

    #[test]
    fn test_layout_engine_creation() {
        let config = LayoutEngineConfig::default();
        let engine = LayoutEngine::new(config);

        assert_eq!(engine.config().max_heading_depth, 6);
        assert!(engine.config().soft_wrap);
    }

    #[test]
    fn test_layout_box_builder() {
        let layout_box = LayoutBoxBuilder::new("div")
            .display(Display::Block)
            .text("Hello World")
            .margin(1, 0, 1, 0)
            .build();

        assert_eq!(layout_box.tag, "div");
        // text() creates a child text node, not direct text
        assert_eq!(layout_box.children.len(), 1);
        assert_eq!(layout_box.children[0].text, Some("Hello World".to_string()));
        assert_eq!(layout_box.style.margin_top, 1);
    }

    #[test]
    fn test_quick_layout() {
        let engine = LayoutEngine::default();
        let viewport = Viewport::new(80);

        let layout_box = LayoutBoxBuilder::new("p")
            .text("Test paragraph")
            .build();

        let markdown = engine.quick_layout(&layout_box, &viewport);
        assert!(markdown.contains("Test paragraph"));
    }

    #[test]
    fn test_layout_tree() {
        let engine = LayoutEngine::default();
        let viewport = Viewport::new(80);

        let h1 = LayoutBoxBuilder::new("h1")
            .text("Title")
            .build();

        let p = LayoutBoxBuilder::new("p")
            .text("Content here")
            .build();

        let root = LayoutBoxBuilder::new("div")
            .child(h1)
            .child(p)
            .build();

        let output = engine.layout_tree(&root, &viewport);

        assert!(output.markdown.contains("# Title"));
        assert!(output.markdown.contains("Content here"));
    }

    #[test]
    fn test_nested_layout() {
        let engine = LayoutEngine::default();
        let viewport = Viewport::new(80);

        let li1 = LayoutBoxBuilder::new("li")
            .text("First item")
            .build();

        let li2 = LayoutBoxBuilder::new("li")
            .text("Second item")
            .build();

        let ul = LayoutBoxBuilder::new("ul")
            .child(li1)
            .child(li2)
            .build();

        let output = engine.layout_tree(&ul, &viewport);

        assert!(output.markdown.contains("- First item"));
        assert!(output.markdown.contains("- Second item"));
    }
}
