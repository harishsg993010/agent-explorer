//! DOM to LayoutTree converter.
//!
//! Converts a dom::Dom into a LayoutTree by:
//! 1. Traversing the DOM tree structure
//! 2. Computing styles from inline styles and tag defaults
//! 3. Building LayoutBox nodes with proper hierarchy

use crate::ids::NodeId;
use crate::layout::{
    AccentColor, AlignItems, AlignSelf, Animation, AnimationDirection, AnimationFillMode,
    AnimationIterationCount, AnimationPlayState, BackdropFilter, BorderCollapse, BoxDecorationBreak,
    BoxSizing, BreakInside, BreakValue, CaptionSide, CaretColor, Clear, ClipRect, ColorScheme,
    ComputedStyle, Contain, ContainerType, ContentSizing, ContentVisibility, Cursor,
    CssValueKeyword, Direction, Display, EmptyCells, Filter, FlexDirection, FlexWrap, Float,
    FontStyle, FontWeight, ForcedColorAdjust, GeneratedContent, GridTrackSize, HangingPunctuation,
    Hyphens, JustifyContent, JustifySelf, LayoutBox, LayoutTree, ListStylePosition, ListStyleType,
    MaskClip, MaskComposite, MaskImage, MaskMode, MaskOrigin, MaskPosition, MaskRepeat, MaskShorthand,
    MaskSize, MixBlendMode, ObjectFit, ObjectPosition, Overflow, OverflowWrap, OutlineStyle,
    PointerEvents, Position, Resize, RubyPosition, ScrollBehavior, ScrollSnapAlign, ScrollSnapStop,
    ScrollSnapType, SubgridValue, TableLayout, TextAlign, TextDecoration, TextEmphasisPosition,
    TextEmphasisStyle, TextOrientation, TextOverflow, TextTransform, TextUnderlinePosition,
    TimingFunction, Transform, Transition, TransitionProperty, UserSelect, VerticalAlign,
    ViewTransitionName, Visibility, Viewport, WhiteSpace, WordBreak, WritingMode,
};
use dom::{Dom, Element};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Counter for generating unique node IDs during conversion
static NODE_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_node_id() -> NodeId {
    NodeId::new(NODE_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Convert a DOM to a LayoutTree
pub fn dom_to_layout_tree(dom: &Dom, viewport: Viewport) -> LayoutTree {
    dom_to_layout_tree_with_store(dom, viewport, None)
}

/// Convert DOM to LayoutTree, optionally populating a StyleStore
pub fn dom_to_layout_tree_with_store(
    dom: &Dom,
    viewport: Viewport,
    style_store: Option<&crate::style_store::StyleStore>,
) -> LayoutTree {
    let mut overlays = Vec::new();

    let root = if let Some(body) = dom.body() {
        convert_element(&body, &mut overlays, style_store)
            .unwrap_or_else(|| LayoutBox::new(next_node_id(), "body", ComputedStyle::default()))
    } else if let Some(html) = dom.document_element() {
        convert_element(&html, &mut overlays, style_store)
            .unwrap_or_else(|| LayoutBox::new(next_node_id(), "html", ComputedStyle::default()))
    } else {
        LayoutBox::new(next_node_id(), "div", ComputedStyle::default())
    };

    LayoutTree {
        root,
        viewport,
        overlays,
    }
}

/// Convert a single DOM element to a LayoutBox
fn convert_element(
    element: &Element,
    overlays: &mut Vec<LayoutBox>,
    style_store: Option<&crate::style_store::StyleStore>,
) -> Option<LayoutBox> {
    let tag = element.tag_name().to_lowercase();

    // Skip certain elements entirely
    if should_skip_element(&tag) {
        return None;
    }

    // Check if element is hidden
    if is_element_hidden(element) {
        return None;
    }

    // Check if element is a tooltip or hidden UI
    if is_tooltip_or_hidden_ui(element) {
        return None;
    }

    // Handle text nodes
    if element.is_text() {
        let text = element.text_content();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(LayoutBox::text_node(NodeId::new(element.unique_id()), trimmed));
    }

    // Not an element node (comment, etc.)
    if !element.is_element() {
        return None;
    }

    // Handle form elements specially - convert to meaningful text
    if let Some(form_box) = convert_form_element(element, &tag) {
        return Some(form_box);
    }

    // Handle images - show alt text or placeholder
    if let Some(img_box) = convert_image_element(element, &tag) {
        return Some(img_box);
    }

    // Compute style for this element
    let style = compute_element_style(element);

    // Skip if display:none
    if style.display == Display::None {
        return None;
    }

    // Create the layout box using element's unique_id for CSSOM↔Layout integration
    // This ensures the same element has the same ID in both JS and layout systems
    let node_id = NodeId::new(element.unique_id());
    let mut layout_box = LayoutBox::new(node_id, &tag, style.clone());

    // Store computed style in StyleStore if provided
    if let Some(store) = style_store {
        store.set_computed_style(element.unique_id(), style.clone());
    }

    // Copy relevant attributes
    layout_box.attrs = extract_layout_attrs(element);

    // Process children
    for child in element.child_nodes() {
        if let Some(child_box) = convert_element(&child, overlays, style_store) {
            // Check if child is an overlay (fixed/absolute positioned)
            if child_box.style.is_overlay() {
                overlays.push(child_box);
            } else {
                layout_box.children.push(child_box);
            }
        }
    }

    // Normalize children if this is a block with mixed inline/block content
    if layout_box.is_block() && !layout_box.children.is_empty() {
        layout_box.children = normalize_children(node_id, layout_box.children);
    }

    Some(layout_box)
}

/// Check if an element should be skipped entirely
fn should_skip_element(tag: &str) -> bool {
    matches!(
        tag.to_lowercase().as_str(),
        "script" | "style" | "noscript" | "template" | "head" | "meta" | "link" | "svg" | "iframe"
    )
}

/// Convert form elements to meaningful text representations
fn convert_form_element(element: &Element, tag: &str) -> Option<LayoutBox> {
    match tag {
        "input" => {
            // Skip hidden inputs
            let input_type = element.get_attribute("type")
                .unwrap_or_else(|| "text".to_string())
                .to_lowercase();

            if input_type == "hidden" {
                return None;
            }

            // Get placeholder, value, or type as label
            let label = element.get_attribute("placeholder")
                .or_else(|| element.get_attribute("value"))
                .or_else(|| element.get_attribute("aria-label"))
                .or_else(|| element.get_attribute("name"))
                .unwrap_or_else(|| input_type.clone());

            // Skip if label is empty
            if label.trim().is_empty() {
                return None;
            }

            // Format based on type
            let display_text = match input_type.as_str() {
                "submit" | "button" => format!("[{}]", label),
                "checkbox" => format!("[ ] {}", label),
                "radio" => format!("( ) {}", label),
                "search" => format!("[search: {}]", label),
                "email" => format!("[email: {}]", label),
                "password" => format!("[password]"),
                "file" => format!("[choose file]"),
                _ => format!("[{}]", label),
            };

            Some(LayoutBox::text_node(NodeId::new(element.unique_id()), &display_text))
        }

        "button" => {
            // Get button text from content, value, or aria-label
            let text = element.text_content();
            let trimmed = text.trim();

            let label = if !trimmed.is_empty() {
                trimmed.to_string()
            } else {
                element.get_attribute("value")
                    .or_else(|| element.get_attribute("aria-label"))
                    .unwrap_or_else(|| "button".to_string())
            };

            if label.trim().is_empty() {
                return None;
            }

            let display_text = format!("[{}]", label);
            Some(LayoutBox::text_node(NodeId::new(element.unique_id()), &display_text))
        }

        "select" => {
            // Get selected option or first option as label
            let mut options = Vec::new();
            collect_options(element, &mut options);

            let label = if let Some(selected) = options.iter().find(|(_, selected)| *selected) {
                selected.0.clone()
            } else if let Some(first) = options.first() {
                first.0.clone()
            } else {
                element.get_attribute("aria-label")
                    .or_else(|| element.get_attribute("name"))
                    .unwrap_or_else(|| "select".to_string())
            };

            if label.trim().is_empty() {
                return None;
            }

            let display_text = format!("[{}▼]", label);
            Some(LayoutBox::text_node(NodeId::new(element.unique_id()), &display_text))
        }

        "textarea" => {
            // Get placeholder or content
            let label = element.get_attribute("placeholder")
                .or_else(|| {
                    let text = element.text_content();
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                })
                .or_else(|| element.get_attribute("aria-label"))
                .unwrap_or_else(|| "text area".to_string());

            if label.trim().is_empty() {
                return None;
            }

            let display_text = format!("[{}]", label);
            Some(LayoutBox::text_node(NodeId::new(element.unique_id()), &display_text))
        }

        "label" => {
            // Labels are processed normally to show their text
            None
        }

        _ => None,
    }
}

/// Collect options from a select element
fn collect_options(element: &Element, options: &mut Vec<(String, bool)>) {
    for child in element.child_nodes() {
        let tag = child.tag_name().to_lowercase();
        if tag == "option" {
            let text = child.text_content();
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                let selected = child.has_attribute("selected");
                options.push((trimmed.to_string(), selected));
            }
        } else if tag == "optgroup" {
            // Recurse into optgroups
            collect_options(&child, options);
        }
    }
}

/// Convert image elements to meaningful text representations
fn convert_image_element(element: &Element, tag: &str) -> Option<LayoutBox> {
    if tag != "img" {
        return None;
    }

    // Get alt text, title, or aria-label
    let alt = element.get_attribute("alt");
    let title = element.get_attribute("title");
    let aria_label = element.get_attribute("aria-label");

    // Check if this is a spacer/tracking pixel (1x1 or very small)
    let width = element.get_attribute("width")
        .and_then(|w| w.trim_end_matches("px").parse::<u32>().ok());
    let height = element.get_attribute("height")
        .and_then(|h| h.trim_end_matches("px").parse::<u32>().ok());

    // Skip tiny images (likely spacers or tracking pixels)
    if let (Some(w), Some(h)) = (width, height) {
        if w <= 2 && h <= 2 {
            return None;
        }
    }

    // Skip images with empty alt (usually decorative)
    if alt.as_deref() == Some("") {
        return None;
    }

    // Try to get meaningful description
    let description = alt
        .filter(|s| !s.trim().is_empty())
        .or(title.filter(|s| !s.trim().is_empty()))
        .or(aria_label.filter(|s| !s.trim().is_empty()));

    let display_text = if let Some(desc) = description {
        // Has alt text - show it
        format!("[img: {}]", desc.trim())
    } else {
        // No alt text - try to extract filename from src
        let src = element.get_attribute("src").unwrap_or_default();
        let filename = extract_image_filename(&src);

        if let Some(name) = filename {
            format!("[img: {}]", name)
        } else {
            // Generic placeholder
            "[image]".to_string()
        }
    };

    Some(LayoutBox::text_node(NodeId::new(element.unique_id()), &display_text))
}

/// Extract a readable filename from an image URL
fn extract_image_filename(src: &str) -> Option<String> {
    // Skip data URLs and empty sources
    if src.is_empty() || src.starts_with("data:") {
        return None;
    }

    // Get the path part (remove query string and fragment)
    let path = src.split('?').next()?.split('#').next()?;

    // Get the filename
    let filename = path.rsplit('/').next()?;

    // Skip if it's just an extension or looks like a hash
    if filename.is_empty() || filename.len() < 3 {
        return None;
    }

    // Remove extension and clean up
    let name = filename
        .rsplit('.')
        .last()
        .unwrap_or(filename);

    // Skip if name looks like a hash (all hex or random chars)
    if name.len() > 20 && name.chars().all(|c| c.is_ascii_hexdigit() || c == '-' || c == '_') {
        return None;
    }

    // Clean up common patterns
    let cleaned = name
        .replace('-', " ")
        .replace('_', " ");

    if cleaned.trim().is_empty() || cleaned.len() < 3 {
        return None;
    }

    Some(cleaned)
}

/// Check if an element is a tooltip or hidden UI element
fn is_tooltip_or_hidden_ui(element: &Element) -> bool {
    // Check role attribute
    if element.get_attribute("role").as_deref() == Some("tooltip") {
        return true;
    }

    // Check class for tooltip patterns
    if let Some(class) = element.get_attribute("class") {
        let class_lower = class.to_lowercase();
        if class_lower.contains("tooltip") || class_lower.contains("popover") {
            return true;
        }
    }

    // Check aria-hidden
    if element.get_attribute("aria-hidden").as_deref() == Some("true") {
        return true;
    }

    false
}

/// Check if an element is hidden via attributes or inline style
fn is_element_hidden(element: &Element) -> bool {
    // Check hidden attribute
    if element.has_attribute("hidden") {
        return true;
    }

    // Check aria-hidden
    if element.get_attribute("aria-hidden").as_deref() == Some("true") {
        return true;
    }

    // Check inline style for display:none or visibility:hidden
    if let Some(style) = element.get_attribute("style") {
        let style_lower = style.to_lowercase();
        if style_lower.contains("display:none")
            || style_lower.contains("display: none")
            || style_lower.contains("visibility:hidden")
            || style_lower.contains("visibility: hidden")
        {
            return true;
        }
    }

    false
}

/// Extract attributes relevant for layout (href, src, alt, etc.)
fn extract_layout_attrs(element: &Element) -> HashMap<String, String> {
    let mut attrs = HashMap::new();

    // Important attributes for rendering
    let important = [
        "href", "src", "alt", "title", "role", "aria-label", "aria-modal", "type", "value",
        "placeholder", "name", "id", "class", "colspan", "rowspan", "open",
    ];

    for attr in important {
        if let Some(value) = element.get_attribute(attr) {
            attrs.insert(attr.to_string(), value);
        }
    }

    attrs
}

/// Compute the style for an element based on tag defaults and inline styles
fn compute_element_style(element: &Element) -> ComputedStyle {
    let tag = element.tag_name().to_lowercase();
    let mut style = tag_default_style(&tag);

    // Parse inline style attribute
    if let Some(inline_style) = element.get_attribute("style") {
        apply_inline_style(&mut style, &inline_style);
    }

    style
}

/// Get default style for a tag
fn tag_default_style(tag: &str) -> ComputedStyle {
    let mut style = ComputedStyle::default();

    match tag {
        // Block elements
        "div" | "section" | "article" | "main" | "aside" | "nav" | "header" | "footer"
        | "figure" | "figcaption" | "address" | "form" | "fieldset" | "details" | "summary" => {
            style.display = Display::Block;
        }

        // Headings
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            style.display = Display::Block;
            style.font_weight = FontWeight::Bold;
            style.margin_top = 1;
            style.margin_bottom = 1;
        }

        // Paragraphs and text blocks
        "p" => {
            style.display = Display::Block;
            style.margin_top = 1;
            style.margin_bottom = 1;
        }

        // Lists
        "ul" | "ol" | "menu" => {
            style.display = Display::Block;
            style.margin_top = 1;
            style.margin_bottom = 1;
            style.padding_left = 2;
        }

        "li" => {
            style.display = Display::ListItem;
        }

        // Tables
        "table" => {
            style.display = Display::Table;
            style.margin_top = 1;
            style.margin_bottom = 1;
        }

        "thead" => {
            style.display = Display::TableHeaderGroup;
        }

        "tbody" => {
            style.display = Display::TableRowGroup;
        }

        "tfoot" => {
            style.display = Display::TableFooterGroup;
        }

        "tr" => {
            style.display = Display::TableRow;
        }

        "td" | "th" => {
            style.display = Display::TableCell;
            if tag == "th" {
                style.font_weight = FontWeight::Bold;
            }
        }

        // Preformatted
        "pre" => {
            style.display = Display::Block;
            style.white_space = WhiteSpace::Pre;
            style.margin_top = 1;
            style.margin_bottom = 1;
        }

        "code" | "kbd" | "samp" | "tt" => {
            style.display = Display::Inline;
            style.white_space = WhiteSpace::Pre;
        }

        // Blockquote
        "blockquote" => {
            style.display = Display::Block;
            style.margin_top = 1;
            style.margin_bottom = 1;
            style.margin_left = 2;
        }

        // Horizontal rule
        "hr" => {
            style.display = Display::Block;
            style.margin_top = 1;
            style.margin_bottom = 1;
        }

        // Inline elements (without 'a' - handled separately below)
        "span" | "abbr" | "acronym" | "cite" | "dfn" | "time" | "var" | "sub" | "sup"
        | "small" | "mark" | "bdi" | "bdo" | "ruby" | "rt" | "rp" | "wbr" | "data" | "q" => {
            style.display = Display::Inline;
        }

        // Emphasis
        "em" | "i" => {
            style.display = Display::Inline;
            style.font_style = FontStyle::Italic;
        }

        // Strong
        "strong" | "b" => {
            style.display = Display::Inline;
            style.font_weight = FontWeight::Bold;
        }

        // Strikethrough
        "del" | "s" | "strike" => {
            style.display = Display::Inline;
            style.text_decoration = TextDecoration::LineThrough;
        }

        // Underline
        "u" | "ins" => {
            style.display = Display::Inline;
            style.text_decoration = TextDecoration::Underline;
        }

        // Links
        "a" => {
            style.display = Display::Inline;
            style.text_decoration = TextDecoration::Underline;
        }

        // Images
        "img" => {
            style.display = Display::InlineBlock;
        }

        // Form elements
        "input" | "button" | "select" | "textarea" => {
            style.display = Display::InlineBlock;
        }

        // Line break
        "br" => {
            style.display = Display::Inline;
        }

        // Dialog
        "dialog" => {
            style.display = Display::Block;
            style.position = Position::Absolute;
        }

        _ => {
            // Default to inline for unknown elements
            style.display = Display::Inline;
        }
    }

    style
}

/// Resolve CSS var() references in a value
fn resolve_css_var(value: &str, variables: &HashMap<String, String>) -> String {
    let mut result = value.to_string();

    // Keep resolving until no more var() references
    let mut iterations = 0;
    while result.contains("var(") && iterations < 10 {
        iterations += 1;

        if let Some(start) = result.find("var(") {
            // Find matching closing paren
            let after_var = &result[start + 4..];
            let mut depth = 1;
            let mut end_offset = 0;
            for (i, c) in after_var.chars().enumerate() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end_offset = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }

            let var_content = &after_var[..end_offset];
            let end = start + 4 + end_offset + 1;

            // Parse var(--name) or var(--name, fallback)
            let (var_name, fallback) = if let Some(comma_pos) = var_content.find(',') {
                let name = var_content[..comma_pos].trim();
                let fb = var_content[comma_pos + 1..].trim();
                (name, Some(fb))
            } else {
                (var_content.trim(), None)
            };

            // Resolve the variable
            let resolved = if let Some(val) = variables.get(var_name) {
                val.clone()
            } else if let Some(fb) = fallback {
                fb.to_string()
            } else {
                String::new()
            };

            result = format!("{}{}{}", &result[..start], resolved, &result[end..]);
        }
    }

    result
}

/// Apply inline style string to computed style
fn apply_inline_style(style: &mut ComputedStyle, inline: &str) {
    // First pass: collect CSS variable declarations
    for declaration in inline.split(';') {
        let declaration = declaration.trim();
        if declaration.is_empty() {
            continue;
        }

        let parts: Vec<&str> = declaration.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }

        let property = parts[0].trim();
        let value = parts[1].trim();

        // Store CSS custom properties
        if property.starts_with("--") {
            style.css_variables.insert(property.to_string(), value.to_string());
        }
    }

    // Second pass: apply styles with var() resolution
    for declaration in inline.split(';') {
        let declaration = declaration.trim();
        if declaration.is_empty() {
            continue;
        }

        let parts: Vec<&str> = declaration.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }

        let property = parts[0].trim().to_lowercase();
        let raw_value = parts[1].trim();

        // Skip CSS variable declarations in this pass
        if property.starts_with("--") {
            continue;
        }

        // Resolve var() references
        let value = if raw_value.contains("var(") {
            resolve_css_var(raw_value, &style.css_variables)
        } else {
            raw_value.to_string()
        };
        let value = value.as_str();

        match property.as_str() {
            "display" => style.display = Display::from_str(value),
            "position" => style.position = Position::from_str(value),
            "visibility" => style.visibility = Visibility::from_str(value),

            // Position offsets (for sticky, fixed, absolute, relative)
            "top" => {
                if value != "auto" {
                    if let Some(val) = parse_length(value) {
                        style.top = Some(val);
                    }
                }
            }
            "right" => {
                if value != "auto" {
                    if let Some(val) = parse_length(value) {
                        style.right = Some(val);
                    }
                }
            }
            "bottom" => {
                if value != "auto" {
                    if let Some(val) = parse_length(value) {
                        style.bottom = Some(val);
                    }
                }
            }
            "left" => {
                if value != "auto" {
                    if let Some(val) = parse_length(value) {
                        style.left = Some(val);
                    }
                }
            }

            "white-space" => style.white_space = WhiteSpace::from_str(value),
            "overflow" | "overflow-x" => style.overflow_x = Overflow::from_str(value),
            "font-weight" => style.font_weight = FontWeight::from_str(value),
            "font-style" => style.font_style = FontStyle::from_str(value),
            "text-decoration" | "text-decoration-line" => {
                style.text_decoration = TextDecoration::from_str(value)
            }
            "text-align" => style.text_align = TextAlign::from_str(value),
            "flex-direction" => style.flex_direction = FlexDirection::from_str(value),
            "flex-wrap" => style.flex_wrap = FlexWrap::from_str(value),
            "justify-content" => style.justify_content = JustifyContent::from_str(value),
            "list-style-type" => style.list_style_type = ListStyleType::from_str(value),

            // Box model properties (parse pixel values and convert to ch approximation)
            "margin" => {
                // Handle 1-4 value shorthand
                let parts: Vec<&str> = value.split_whitespace().collect();
                match parts.len() {
                    1 => {
                        if let Some(val) = parse_length(parts[0]) {
                            style.margin_top = val;
                            style.margin_right = val;
                            style.margin_bottom = val;
                            style.margin_left = val;
                        }
                    }
                    2 => {
                        // top/bottom, left/right
                        if let Some(tb) = parse_length(parts[0]) {
                            style.margin_top = tb;
                            style.margin_bottom = tb;
                        }
                        if let Some(lr) = parse_length(parts[1]) {
                            style.margin_left = lr;
                            style.margin_right = lr;
                        }
                    }
                    3 => {
                        // top, left/right, bottom
                        if let Some(top) = parse_length(parts[0]) {
                            style.margin_top = top;
                        }
                        if let Some(lr) = parse_length(parts[1]) {
                            style.margin_left = lr;
                            style.margin_right = lr;
                        }
                        if let Some(bottom) = parse_length(parts[2]) {
                            style.margin_bottom = bottom;
                        }
                    }
                    4 => {
                        // top, right, bottom, left
                        if let Some(top) = parse_length(parts[0]) {
                            style.margin_top = top;
                        }
                        if let Some(right) = parse_length(parts[1]) {
                            style.margin_right = right;
                        }
                        if let Some(bottom) = parse_length(parts[2]) {
                            style.margin_bottom = bottom;
                        }
                        if let Some(left) = parse_length(parts[3]) {
                            style.margin_left = left;
                        }
                    }
                    _ => {}
                }
            }
            "margin-top" => {
                if let Some(val) = parse_length(value) {
                    style.margin_top = val;
                }
            }
            "margin-right" => {
                if let Some(val) = parse_length(value) {
                    style.margin_right = val;
                }
            }
            "margin-bottom" => {
                if let Some(val) = parse_length(value) {
                    style.margin_bottom = val;
                }
            }
            "margin-left" => {
                if let Some(val) = parse_length(value) {
                    style.margin_left = val;
                }
            }
            "padding" => {
                // Handle 1-4 value shorthand
                let parts: Vec<&str> = value.split_whitespace().collect();
                match parts.len() {
                    1 => {
                        if let Some(val) = parse_length(parts[0]) {
                            style.padding_top = val;
                            style.padding_right = val;
                            style.padding_bottom = val;
                            style.padding_left = val;
                        }
                    }
                    2 => {
                        if let Some(tb) = parse_length(parts[0]) {
                            style.padding_top = tb;
                            style.padding_bottom = tb;
                        }
                        if let Some(lr) = parse_length(parts[1]) {
                            style.padding_left = lr;
                            style.padding_right = lr;
                        }
                    }
                    3 => {
                        if let Some(top) = parse_length(parts[0]) {
                            style.padding_top = top;
                        }
                        if let Some(lr) = parse_length(parts[1]) {
                            style.padding_left = lr;
                            style.padding_right = lr;
                        }
                        if let Some(bottom) = parse_length(parts[2]) {
                            style.padding_bottom = bottom;
                        }
                    }
                    4 => {
                        if let Some(top) = parse_length(parts[0]) {
                            style.padding_top = top;
                        }
                        if let Some(right) = parse_length(parts[1]) {
                            style.padding_right = right;
                        }
                        if let Some(bottom) = parse_length(parts[2]) {
                            style.padding_bottom = bottom;
                        }
                        if let Some(left) = parse_length(parts[3]) {
                            style.padding_left = left;
                        }
                    }
                    _ => {}
                }
            }
            "padding-top" => {
                if let Some(val) = parse_length(value) {
                    style.padding_top = val;
                }
            }
            "padding-right" => {
                if let Some(val) = parse_length(value) {
                    style.padding_right = val;
                }
            }
            "padding-bottom" => {
                if let Some(val) = parse_length(value) {
                    style.padding_bottom = val;
                }
            }
            "padding-left" => {
                if let Some(val) = parse_length(value) {
                    style.padding_left = val;
                }
            }

            // Width constraints
            "width" => {
                // Check for content sizing keywords first
                if let Some(sizing) = parse_content_sizing(value) {
                    style.width_sizing = sizing;
                } else if let Some(val) = parse_length_usize(value) {
                    style.width = Some(val);
                }
            }
            "min-width" => {
                if let Some(val) = parse_length_usize(value) {
                    style.min_width = Some(val);
                }
            }
            "max-width" => {
                if let Some(val) = parse_length_usize(value) {
                    style.max_width = Some(val);
                }
            }
            "height" => {
                // Check for content sizing keywords first
                if let Some(sizing) = parse_content_sizing(value) {
                    style.height_sizing = sizing;
                } else if let Some(val) = parse_length_usize(value) {
                    style.height = Some(val);
                }
            }

            // Flex item properties
            "flex-grow" => {
                if let Ok(val) = value.parse::<f32>() {
                    style.flex_grow = val;
                }
            }
            "flex-shrink" => {
                if let Ok(val) = value.parse::<f32>() {
                    style.flex_shrink = val;
                }
            }
            "flex-basis" => {
                if value == "auto" {
                    style.flex_basis = None;
                } else if let Some(val) = parse_length_usize(value) {
                    style.flex_basis = Some(val);
                }
            }
            "flex" => {
                // Handle shorthand: flex: grow shrink basis
                let parts: Vec<&str> = value.split_whitespace().collect();
                if let Some(first) = parts.first() {
                    if let Ok(grow) = first.parse::<f32>() {
                        style.flex_grow = grow;
                    }
                }
                if let Some(second) = parts.get(1) {
                    if let Ok(shrink) = second.parse::<f32>() {
                        style.flex_shrink = shrink;
                    }
                }
                if let Some(third) = parts.get(2) {
                    if let Some(basis) = parse_length_usize(third) {
                        style.flex_basis = Some(basis);
                    }
                }
            }

            "gap" => {
                if let Some(val) = parse_length_usize(value) {
                    style.gap = Some(val);
                }
            }

            // Alignment properties
            "align-items" => style.align_items = AlignItems::from_str(value),

            // Grid properties
            "grid-template-columns" => {
                if value.contains("subgrid") {
                    style.grid_template_columns_subgrid = SubgridValue::Subgrid;
                } else {
                    style.grid_template_columns = parse_grid_template(value);
                }
            }
            "grid-template-rows" => {
                if value.contains("subgrid") {
                    style.grid_template_rows_subgrid = SubgridValue::Subgrid;
                } else {
                    style.grid_template_rows = parse_grid_template(value);
                }
            }
            "grid-auto-columns" => {
                style.grid_auto_columns = parse_grid_track_size(value);
            }
            "grid-auto-rows" => {
                style.grid_auto_rows = parse_grid_track_size(value);
            }
            "grid-column-start" => {
                if let Ok(val) = value.parse::<i32>() {
                    style.grid_column_start = Some(val);
                }
            }
            "grid-column-end" => {
                if let Ok(val) = value.parse::<i32>() {
                    style.grid_column_end = Some(val);
                }
            }
            "grid-row-start" => {
                if let Ok(val) = value.parse::<i32>() {
                    style.grid_row_start = Some(val);
                }
            }
            "grid-row-end" => {
                if let Ok(val) = value.parse::<i32>() {
                    style.grid_row_end = Some(val);
                }
            }
            "grid-column" => {
                // Handle shorthand: grid-column: start / end
                let parts: Vec<&str> = value.split('/').map(|s| s.trim()).collect();
                if let Some(start) = parts.first() {
                    if let Ok(val) = start.parse::<i32>() {
                        style.grid_column_start = Some(val);
                    }
                }
                if let Some(end) = parts.get(1) {
                    if let Ok(val) = end.parse::<i32>() {
                        style.grid_column_end = Some(val);
                    }
                }
            }
            "grid-row" => {
                // Handle shorthand: grid-row: start / end
                let parts: Vec<&str> = value.split('/').map(|s| s.trim()).collect();
                if let Some(start) = parts.first() {
                    if let Ok(val) = start.parse::<i32>() {
                        style.grid_row_start = Some(val);
                    }
                }
                if let Some(end) = parts.get(1) {
                    if let Ok(val) = end.parse::<i32>() {
                        style.grid_row_end = Some(val);
                    }
                }
            }

            "z-index" => {
                if let Ok(val) = value.parse::<i32>() {
                    style.z_index = Some(val);
                }
            }

            // Container Query properties
            "container-type" => {
                style.container_type = ContainerType::from_str(value);
            }
            "container-name" => {
                let name = value.trim();
                if !name.is_empty() && name != "none" {
                    style.container_name = Some(name.to_string());
                }
            }
            "container" => {
                // Shorthand: container: name / type or just type
                if let Some((name, ctype)) = value.split_once('/') {
                    let name = name.trim();
                    if !name.is_empty() && name != "none" {
                        style.container_name = Some(name.to_string());
                    }
                    style.container_type = ContainerType::from_str(ctype.trim());
                } else {
                    style.container_type = ContainerType::from_str(value);
                }
            }

            // Float properties
            "float" => {
                style.float = Float::from_str(value);
            }
            "clear" => {
                style.clear = Clear::from_str(value);
            }

            // Text properties
            "line-height" => {
                if let Some(lh) = parse_line_height(value) {
                    style.line_height = Some(lh);
                }
            }
            "letter-spacing" => {
                if value == "normal" {
                    style.letter_spacing = 0;
                } else if let Some(val) = parse_length(value) {
                    style.letter_spacing = val;
                }
            }
            "word-spacing" => {
                if value == "normal" {
                    style.word_spacing = 0;
                } else if let Some(val) = parse_length(value) {
                    style.word_spacing = val;
                }
            }
            "text-indent" => {
                if let Some(val) = parse_length(value) {
                    style.text_indent = val;
                }
            }

            // Generated content
            "content" => {
                // Note: In real CSS, content only applies to ::before/::after
                // We store it for potential use
                if let Some(gc) = GeneratedContent::parse(value) {
                    style.content_before = Some(gc);
                }
            }
            // Custom data attributes for before/after (non-standard but useful)
            "--content-before" => {
                if let Some(gc) = GeneratedContent::parse(value) {
                    style.content_before = Some(gc);
                }
            }
            "--content-after" => {
                if let Some(gc) = GeneratedContent::parse(value) {
                    style.content_after = Some(gc);
                }
            }

            // Multi-column layout properties
            "column-count" => {
                if value == "auto" {
                    style.column_count = None;
                } else if let Ok(count) = value.parse::<usize>() {
                    style.column_count = Some(count);
                }
            }
            "column-width" => {
                if value == "auto" {
                    style.column_width = None;
                } else if let Some(val) = parse_length_usize(value) {
                    style.column_width = Some(val);
                }
            }
            "column-gap" => {
                if value == "normal" {
                    style.column_gap = 1; // Default gap
                } else if let Some(val) = parse_length_usize(value) {
                    style.column_gap = val;
                }
            }
            "column-rule-width" => {
                if let Some(val) = parse_length_usize(value) {
                    style.column_rule_width = val;
                }
            }
            "columns" => {
                // Shorthand: columns: width count or count width
                let parts: Vec<&str> = value.split_whitespace().collect();
                for part in parts {
                    if part == "auto" {
                        continue;
                    }
                    // Try as count first (no unit)
                    if let Ok(count) = part.parse::<usize>() {
                        style.column_count = Some(count);
                    } else if let Some(width) = parse_length_usize(part) {
                        style.column_width = Some(width);
                    }
                }
            }

            // CSS Counters
            "counter-reset" => {
                style.counter_reset = parse_counter_list(value);
            }
            "counter-increment" => {
                style.counter_increment = parse_counter_list(value);
            }

            // Text overflow
            "text-overflow" => {
                style.text_overflow = TextOverflow::from_str(value);
            }

            // Aspect ratio
            "aspect-ratio" => {
                style.aspect_ratio = parse_aspect_ratio(value);
            }

            // Writing modes and direction
            "writing-mode" => {
                style.writing_mode = WritingMode::from_str(value);
            }
            "direction" => {
                style.direction = Direction::from_str(value);
            }
            "text-orientation" => {
                style.text_orientation = TextOrientation::from_str(value);
            }

            // Logical properties - margins
            "margin-inline-start" => {
                if let Some(val) = parse_length(value) {
                    style.margin_inline_start = Some(val);
                }
            }
            "margin-inline-end" => {
                if let Some(val) = parse_length(value) {
                    style.margin_inline_end = Some(val);
                }
            }
            "margin-block-start" => {
                if let Some(val) = parse_length(value) {
                    style.margin_block_start = Some(val);
                }
            }
            "margin-block-end" => {
                if let Some(val) = parse_length(value) {
                    style.margin_block_end = Some(val);
                }
            }
            "margin-inline" => {
                if let Some(val) = parse_length(value) {
                    style.margin_inline_start = Some(val);
                    style.margin_inline_end = Some(val);
                }
            }
            "margin-block" => {
                if let Some(val) = parse_length(value) {
                    style.margin_block_start = Some(val);
                    style.margin_block_end = Some(val);
                }
            }

            // Logical properties - padding
            "padding-inline-start" => {
                if let Some(val) = parse_length(value) {
                    style.padding_inline_start = Some(val);
                }
            }
            "padding-inline-end" => {
                if let Some(val) = parse_length(value) {
                    style.padding_inline_end = Some(val);
                }
            }
            "padding-block-start" => {
                if let Some(val) = parse_length(value) {
                    style.padding_block_start = Some(val);
                }
            }
            "padding-block-end" => {
                if let Some(val) = parse_length(value) {
                    style.padding_block_end = Some(val);
                }
            }
            "padding-inline" => {
                if let Some(val) = parse_length(value) {
                    style.padding_inline_start = Some(val);
                    style.padding_inline_end = Some(val);
                }
            }
            "padding-block" => {
                if let Some(val) = parse_length(value) {
                    style.padding_block_start = Some(val);
                    style.padding_block_end = Some(val);
                }
            }

            // Logical properties - inset
            "inset-inline-start" => {
                if let Some(val) = parse_length(value) {
                    style.inset_inline_start = Some(val);
                }
            }
            "inset-inline-end" => {
                if let Some(val) = parse_length(value) {
                    style.inset_inline_end = Some(val);
                }
            }
            "inset-block-start" => {
                if let Some(val) = parse_length(value) {
                    style.inset_block_start = Some(val);
                }
            }
            "inset-block-end" => {
                if let Some(val) = parse_length(value) {
                    style.inset_block_end = Some(val);
                }
            }
            "inset-inline" => {
                if let Some(val) = parse_length(value) {
                    style.inset_inline_start = Some(val);
                    style.inset_inline_end = Some(val);
                }
            }
            "inset-block" => {
                if let Some(val) = parse_length(value) {
                    style.inset_block_start = Some(val);
                    style.inset_block_end = Some(val);
                }
            }
            "inset" => {
                // Shorthand for all inset properties
                if let Some(val) = parse_length(value) {
                    style.inset_inline_start = Some(val);
                    style.inset_inline_end = Some(val);
                    style.inset_block_start = Some(val);
                    style.inset_block_end = Some(val);
                }
            }

            // Object fit/position (for images)
            "object-fit" => {
                style.object_fit = ObjectFit::from_str(value);
            }
            "object-position" => {
                style.object_position = parse_object_position(value);
            }

            // Table cell properties
            "colspan" => {
                if let Ok(val) = value.parse::<usize>() {
                    style.colspan = val.max(1);
                }
            }
            "rowspan" => {
                if let Ok(val) = value.parse::<usize>() {
                    style.rowspan = val.max(1);
                }
            }
            "vertical-align" => {
                style.vertical_align = VerticalAlign::from_str(value);
            }

            // Hyphenation
            "hyphens" => {
                style.hyphens = Hyphens::from_str(value);
            }

            // List marker styling
            "list-style-position" => {
                style.list_style_position = ListStylePosition::from_str(value);
            }

            // Transforms
            "transform" => {
                style.transform = Transform::parse(value);
            }

            // Clipping
            "clip" => {
                style.clip = ClipRect::parse(value);
            }
            "overflow-clip-margin" => {
                if let Some(val) = parse_length(value) {
                    style.overflow_clip_margin = val;
                }
            }

            // Box decoration break
            "box-decoration-break" => {
                style.box_decoration_break = BoxDecorationBreak::from_str(value);
            }
            "-webkit-box-decoration-break" => {
                style.box_decoration_break = BoxDecorationBreak::from_str(value);
            }

            // Break properties
            "break-before" => {
                style.break_before = BreakValue::from_str(value);
            }
            "break-after" => {
                style.break_after = BreakValue::from_str(value);
            }
            "break-inside" => {
                style.break_inside = BreakInside::from_str(value);
            }
            "page-break-before" => {
                // Legacy property
                style.break_before = match value.to_lowercase().as_str() {
                    "always" => BreakValue::Page,
                    "avoid" => BreakValue::AvoidPage,
                    "left" => BreakValue::Left,
                    "right" => BreakValue::Right,
                    _ => BreakValue::Auto,
                };
            }
            "page-break-after" => {
                // Legacy property
                style.break_after = match value.to_lowercase().as_str() {
                    "always" => BreakValue::Page,
                    "avoid" => BreakValue::AvoidPage,
                    "left" => BreakValue::Left,
                    "right" => BreakValue::Right,
                    _ => BreakValue::Auto,
                };
            }
            "page-break-inside" => {
                // Legacy property
                style.break_inside = match value.to_lowercase().as_str() {
                    "avoid" => BreakInside::AvoidPage,
                    _ => BreakInside::Auto,
                };
            }

            // Orphans and widows
            "orphans" => {
                if let Ok(val) = value.parse::<usize>() {
                    style.orphans = val.max(1);
                }
            }
            "widows" => {
                if let Ok(val) = value.parse::<usize>() {
                    style.widows = val.max(1);
                }
            }

            // Word breaking
            "word-break" => {
                style.word_break = WordBreak::from_str(value);
            }
            "overflow-wrap" | "word-wrap" => {
                // word-wrap is legacy alias for overflow-wrap
                style.overflow_wrap = OverflowWrap::from_str(value);
            }

            // Box sizing
            "box-sizing" => {
                style.box_sizing = BoxSizing::from_str(value);
            }

            // Outline properties
            "outline-width" => {
                if let Some(val) = parse_length(value) {
                    style.outline_width = val;
                }
            }
            "outline-style" => {
                style.outline_style = OutlineStyle::from_str(value);
            }
            "outline-offset" => {
                if let Some(val) = parse_length(value) {
                    style.outline_offset = val;
                }
            }
            "outline" => {
                // Shorthand: outline: width style color
                // We parse width and style, ignore color
                for part in value.split_whitespace() {
                    if let Some(width) = parse_length(part) {
                        style.outline_width = width;
                    } else {
                        let outline_style = OutlineStyle::from_str(part);
                        if outline_style != OutlineStyle::None || part == "none" {
                            style.outline_style = outline_style;
                        }
                    }
                }
            }

            // Tab size
            "tab-size" | "-moz-tab-size" => {
                if let Ok(val) = value.parse::<usize>() {
                    style.tab_size = val;
                } else if let Some(val) = parse_length_usize(value) {
                    style.tab_size = val;
                }
            }

            // Text transform
            "text-transform" => {
                style.text_transform = TextTransform::from_str(value);
            }

            // Resize
            "resize" => {
                style.resize = Resize::from_str(value);
            }

            // Pointer events
            "pointer-events" => {
                style.pointer_events = PointerEvents::from_str(value);
            }

            // User select
            "user-select" | "-webkit-user-select" | "-moz-user-select" | "-ms-user-select" => {
                style.user_select = UserSelect::from_str(value);
            }

            // Quotes
            "quotes" => {
                style.quotes = parse_quotes(value);
            }

            // Table border properties
            "border-collapse" => {
                style.border_collapse = BorderCollapse::from_str(value);
            }
            "border-spacing" => {
                style.border_spacing = parse_border_spacing(value);
            }
            "empty-cells" => {
                style.empty_cells = EmptyCells::from_str(value);
            }
            "caption-side" => {
                style.caption_side = CaptionSide::from_str(value);
            }
            "table-layout" => {
                style.table_layout = TableLayout::from_str(value);
            }

            // Text emphasis properties
            "text-emphasis" | "text-emphasis-style" => {
                style.text_emphasis_style = TextEmphasisStyle::from_str(value);
            }
            "text-emphasis-position" => {
                style.text_emphasis_position = TextEmphasisPosition::from_str(value);
            }
            "text-underline-position" => {
                style.text_underline_position = TextUnderlinePosition::from_str(value);
            }
            "text-underline-offset" => {
                if let Some(val) = parse_length(value) {
                    style.text_underline_offset = val;
                }
            }
            "ruby-position" => {
                style.ruby_position = RubyPosition::from_str(value);
            }

            // Typography properties
            "hanging-punctuation" => {
                style.hanging_punctuation = HangingPunctuation::from_str(value);
            }
            "initial-letter" => {
                style.initial_letter = parse_initial_letter(value);
            }

            // Accessibility & theming
            "color-scheme" => {
                style.color_scheme = ColorScheme::from_str(value);
            }
            "forced-color-adjust" => {
                style.forced_color_adjust = ForcedColorAdjust::from_str(value);
            }
            "accent-color" => {
                style.accent_color = AccentColor::from_str(value);
            }

            // Cursor
            "cursor" => {
                style.cursor = Cursor::from_str(value);
            }
            "caret-color" => {
                style.caret_color = CaretColor::from_str(value);
            }

            // Scroll margin properties
            "scroll-margin" => {
                if let Some(val) = parse_length(value) {
                    style.scroll_margin_top = val;
                    style.scroll_margin_right = val;
                    style.scroll_margin_bottom = val;
                    style.scroll_margin_left = val;
                }
            }
            "scroll-margin-top" => {
                if let Some(val) = parse_length(value) {
                    style.scroll_margin_top = val;
                }
            }
            "scroll-margin-right" => {
                if let Some(val) = parse_length(value) {
                    style.scroll_margin_right = val;
                }
            }
            "scroll-margin-bottom" => {
                if let Some(val) = parse_length(value) {
                    style.scroll_margin_bottom = val;
                }
            }
            "scroll-margin-left" => {
                if let Some(val) = parse_length(value) {
                    style.scroll_margin_left = val;
                }
            }
            "scroll-margin-block" => {
                if let Some(val) = parse_length(value) {
                    style.scroll_margin_top = val;
                    style.scroll_margin_bottom = val;
                }
            }
            "scroll-margin-inline" => {
                if let Some(val) = parse_length(value) {
                    style.scroll_margin_left = val;
                    style.scroll_margin_right = val;
                }
            }

            // Scroll padding properties
            "scroll-padding" => {
                if let Some(val) = parse_length(value) {
                    style.scroll_padding_top = val;
                    style.scroll_padding_right = val;
                    style.scroll_padding_bottom = val;
                    style.scroll_padding_left = val;
                }
            }
            "scroll-padding-top" => {
                if let Some(val) = parse_length(value) {
                    style.scroll_padding_top = val;
                }
            }
            "scroll-padding-right" => {
                if let Some(val) = parse_length(value) {
                    style.scroll_padding_right = val;
                }
            }
            "scroll-padding-bottom" => {
                if let Some(val) = parse_length(value) {
                    style.scroll_padding_bottom = val;
                }
            }
            "scroll-padding-left" => {
                if let Some(val) = parse_length(value) {
                    style.scroll_padding_left = val;
                }
            }
            "scroll-padding-block" => {
                if let Some(val) = parse_length(value) {
                    style.scroll_padding_top = val;
                    style.scroll_padding_bottom = val;
                }
            }
            "scroll-padding-inline" => {
                if let Some(val) = parse_length(value) {
                    style.scroll_padding_left = val;
                    style.scroll_padding_right = val;
                }
            }

            // Performance/containment
            "contain" => {
                style.contain = Contain::from_str(value);
            }
            "content-visibility" => {
                style.content_visibility = ContentVisibility::from_str(value);
            }

            // Flex/Grid item ordering and alignment
            "order" => {
                if let Ok(val) = value.parse::<i32>() {
                    style.order = val;
                }
            }
            "align-self" => {
                style.align_self = AlignSelf::from_str(value);
            }
            "justify-self" => {
                style.justify_self = JustifySelf::from_str(value);
            }
            "place-self" => {
                // Shorthand: place-self: align-self justify-self
                let parts: Vec<&str> = value.split_whitespace().collect();
                if let Some(first) = parts.first() {
                    style.align_self = AlignSelf::from_str(first);
                    if let Some(second) = parts.get(1) {
                        style.justify_self = JustifySelf::from_str(second);
                    } else {
                        style.justify_self = JustifySelf::from_str(first);
                    }
                }
            }
            "row-gap" => {
                if let Some(val) = parse_length_usize(value) {
                    style.row_gap = val;
                }
            }
            // column-gap already parsed above
            "place-items" => {
                // Shorthand: place-items: align-items justify-items
                let parts: Vec<&str> = value.split_whitespace().collect();
                if let Some(first) = parts.first() {
                    style.align_items = AlignItems::from_str(first);
                }
            }
            "place-content" => {
                // Shorthand: place-content: align-content justify-content
                let parts: Vec<&str> = value.split_whitespace().collect();
                if let Some(first) = parts.first() {
                    style.justify_content = JustifyContent::from_str(first);
                }
            }

            // Scroll snap properties
            "scroll-snap-type" => {
                style.scroll_snap_type = ScrollSnapType::from_str(value);
            }
            "scroll-snap-align" => {
                style.scroll_snap_align = ScrollSnapAlign::from_str(value);
            }
            "scroll-snap-stop" => {
                style.scroll_snap_stop = ScrollSnapStop::from_str(value);
            }
            "scroll-behavior" => {
                style.scroll_behavior = ScrollBehavior::from_str(value);
            }

            // Filter and opacity
            "opacity" => {
                if let Ok(val) = value.parse::<f32>() {
                    style.opacity = val.clamp(0.0, 1.0);
                } else if let Some(pct) = value.strip_suffix('%') {
                    if let Ok(val) = pct.parse::<f32>() {
                        style.opacity = (val / 100.0).clamp(0.0, 1.0);
                    }
                }
            }
            "filter" => {
                style.filter = Filter::from_str(value);
            }
            "mix-blend-mode" => {
                style.mix_blend_mode = MixBlendMode::from_str(value);
            }
            "backdrop-filter" | "-webkit-backdrop-filter" => {
                style.backdrop_filter = BackdropFilter::from_str(value);
            }

            // CSS Masking
            "mask" | "-webkit-mask" => {
                let mask = MaskShorthand::parse(value);
                mask.apply_to(style);
            }
            "mask-image" | "-webkit-mask-image" => {
                style.mask_image = MaskImage::from_str(value);
            }
            "mask-mode" => {
                style.mask_mode = MaskMode::from_str(value);
            }
            "mask-repeat" | "-webkit-mask-repeat" => {
                style.mask_repeat = MaskRepeat::from_str(value);
            }
            "mask-position" | "-webkit-mask-position" => {
                style.mask_position = MaskPosition::from_str(value);
            }
            "mask-size" | "-webkit-mask-size" => {
                style.mask_size = MaskSize::from_str(value);
            }
            "mask-composite" | "-webkit-mask-composite" => {
                style.mask_composite = MaskComposite::from_str(value);
            }
            "mask-clip" | "-webkit-mask-clip" => {
                style.mask_clip = MaskClip::from_str(value);
            }
            "mask-origin" | "-webkit-mask-origin" => {
                style.mask_origin = MaskOrigin::from_str(value);
            }

            // View Transitions
            "view-transition-name" => {
                style.view_transition_name = ViewTransitionName::from_str(value);
            }

            // CSS "all" property
            "all" => {
                if let Some(keyword) = CssValueKeyword::from_str(value) {
                    match keyword {
                        CssValueKeyword::Initial => {
                            // Reset to initial values (default)
                            *style = ComputedStyle::default();
                        }
                        CssValueKeyword::Unset | CssValueKeyword::Revert => {
                            // For unset/revert, reset to default (simplified)
                            *style = ComputedStyle::default();
                        }
                        CssValueKeyword::Inherit => {
                            // Inherit is handled by the cascade
                        }
                    }
                }
            }

            // Transition properties
            "transition" => {
                if let Some(transitions) = parse_transition(value) {
                    style.transitions = transitions;
                }
            }
            "transition-property" => {
                // Update existing transitions or create new one
                let props: Vec<TransitionProperty> = value
                    .split(',')
                    .map(|p| TransitionProperty::from_str(p.trim()))
                    .collect();
                for (i, prop) in props.into_iter().enumerate() {
                    if i < style.transitions.len() {
                        style.transitions[i].property = prop;
                    } else {
                        style.transitions.push(Transition {
                            property: prop,
                            ..Default::default()
                        });
                    }
                }
            }
            "transition-duration" => {
                let durations: Vec<f32> = value
                    .split(',')
                    .filter_map(|d| parse_time(d.trim()))
                    .collect();
                for (i, dur) in durations.into_iter().enumerate() {
                    if i < style.transitions.len() {
                        style.transitions[i].duration = dur;
                    } else {
                        style.transitions.push(Transition {
                            duration: dur,
                            ..Default::default()
                        });
                    }
                }
            }
            "transition-timing-function" => {
                let timings: Vec<TimingFunction> = value
                    .split(',')
                    .map(|t| TimingFunction::from_str(t.trim()))
                    .collect();
                for (i, timing) in timings.into_iter().enumerate() {
                    if i < style.transitions.len() {
                        style.transitions[i].timing = timing;
                    } else {
                        style.transitions.push(Transition {
                            timing,
                            ..Default::default()
                        });
                    }
                }
            }
            "transition-delay" => {
                let delays: Vec<f32> = value
                    .split(',')
                    .filter_map(|d| parse_time(d.trim()))
                    .collect();
                for (i, delay) in delays.into_iter().enumerate() {
                    if i < style.transitions.len() {
                        style.transitions[i].delay = delay;
                    } else {
                        style.transitions.push(Transition {
                            delay,
                            ..Default::default()
                        });
                    }
                }
            }

            // Animation properties
            "animation" => {
                if let Some(animations) = parse_animation(value) {
                    style.animations = animations;
                }
            }
            "animation-name" => {
                let names: Vec<&str> = value.split(',').map(|n| n.trim()).collect();
                for (i, name) in names.into_iter().enumerate() {
                    if i < style.animations.len() {
                        style.animations[i].name = name.to_string();
                    } else {
                        style.animations.push(Animation {
                            name: name.to_string(),
                            ..Default::default()
                        });
                    }
                }
            }
            "animation-duration" => {
                let durations: Vec<f32> = value
                    .split(',')
                    .filter_map(|d| parse_time(d.trim()))
                    .collect();
                for (i, dur) in durations.into_iter().enumerate() {
                    if i < style.animations.len() {
                        style.animations[i].duration = dur;
                    } else {
                        style.animations.push(Animation {
                            duration: dur,
                            ..Default::default()
                        });
                    }
                }
            }
            "animation-timing-function" => {
                let timings: Vec<TimingFunction> = value
                    .split(',')
                    .map(|t| TimingFunction::from_str(t.trim()))
                    .collect();
                for (i, timing) in timings.into_iter().enumerate() {
                    if i < style.animations.len() {
                        style.animations[i].timing = timing;
                    } else {
                        style.animations.push(Animation {
                            timing,
                            ..Default::default()
                        });
                    }
                }
            }
            "animation-delay" => {
                let delays: Vec<f32> = value
                    .split(',')
                    .filter_map(|d| parse_time(d.trim()))
                    .collect();
                for (i, delay) in delays.into_iter().enumerate() {
                    if i < style.animations.len() {
                        style.animations[i].delay = delay;
                    } else {
                        style.animations.push(Animation {
                            delay,
                            ..Default::default()
                        });
                    }
                }
            }
            "animation-iteration-count" => {
                let counts: Vec<AnimationIterationCount> = value
                    .split(',')
                    .map(|c| parse_iteration_count(c.trim()))
                    .collect();
                for (i, count) in counts.into_iter().enumerate() {
                    if i < style.animations.len() {
                        style.animations[i].iteration_count = count;
                    } else {
                        style.animations.push(Animation {
                            iteration_count: count,
                            ..Default::default()
                        });
                    }
                }
            }
            "animation-direction" => {
                let directions: Vec<AnimationDirection> = value
                    .split(',')
                    .map(|d| parse_animation_direction(d.trim()))
                    .collect();
                for (i, dir) in directions.into_iter().enumerate() {
                    if i < style.animations.len() {
                        style.animations[i].direction = dir;
                    } else {
                        style.animations.push(Animation {
                            direction: dir,
                            ..Default::default()
                        });
                    }
                }
            }
            "animation-fill-mode" => {
                let modes: Vec<AnimationFillMode> = value
                    .split(',')
                    .map(|m| parse_fill_mode(m.trim()))
                    .collect();
                for (i, mode) in modes.into_iter().enumerate() {
                    if i < style.animations.len() {
                        style.animations[i].fill_mode = mode;
                    } else {
                        style.animations.push(Animation {
                            fill_mode: mode,
                            ..Default::default()
                        });
                    }
                }
            }
            "animation-play-state" => {
                let states: Vec<AnimationPlayState> = value
                    .split(',')
                    .map(|s| parse_play_state(s.trim()))
                    .collect();
                for (i, state) in states.into_iter().enumerate() {
                    if i < style.animations.len() {
                        style.animations[i].play_state = state;
                    } else {
                        style.animations.push(Animation {
                            play_state: state,
                            ..Default::default()
                        });
                    }
                }
            }

            // Anchor positioning
            "anchor-name" => {
                if value == "none" {
                    style.anchor_name = None;
                } else {
                    style.anchor_name = Some(value.trim().to_string());
                }
            }
            "position-anchor" => {
                if value == "auto" {
                    style.position_anchor = None;
                } else {
                    style.position_anchor = Some(value.trim().to_string());
                }
            }

            // Border shorthand (simple version: width style color)
            "border" => {
                // Parse border: width style color
                let parts: Vec<&str> = value.split_whitespace().collect();
                for part in parts {
                    if let Some(width) = parse_length(part) {
                        style.border_top_width = width;
                        style.border_right_width = width;
                        style.border_bottom_width = width;
                        style.border_left_width = width;
                        break; // Take first length value as width
                    }
                }
            }
            "border-width" => {
                let parts: Vec<&str> = value.split_whitespace().collect();
                match parts.len() {
                    1 => {
                        if let Some(val) = parse_length(parts[0]) {
                            style.border_top_width = val;
                            style.border_right_width = val;
                            style.border_bottom_width = val;
                            style.border_left_width = val;
                        }
                    }
                    2 => {
                        if let Some(tb) = parse_length(parts[0]) {
                            style.border_top_width = tb;
                            style.border_bottom_width = tb;
                        }
                        if let Some(lr) = parse_length(parts[1]) {
                            style.border_left_width = lr;
                            style.border_right_width = lr;
                        }
                    }
                    3 => {
                        if let Some(top) = parse_length(parts[0]) {
                            style.border_top_width = top;
                        }
                        if let Some(lr) = parse_length(parts[1]) {
                            style.border_left_width = lr;
                            style.border_right_width = lr;
                        }
                        if let Some(bottom) = parse_length(parts[2]) {
                            style.border_bottom_width = bottom;
                        }
                    }
                    4 => {
                        if let Some(top) = parse_length(parts[0]) {
                            style.border_top_width = top;
                        }
                        if let Some(right) = parse_length(parts[1]) {
                            style.border_right_width = right;
                        }
                        if let Some(bottom) = parse_length(parts[2]) {
                            style.border_bottom_width = bottom;
                        }
                        if let Some(left) = parse_length(parts[3]) {
                            style.border_left_width = left;
                        }
                    }
                    _ => {}
                }
            }

            // Font shorthand (simplified: style weight size family)
            "font" => {
                // Parse font: style weight size/line-height family
                let parts: Vec<&str> = value.split_whitespace().collect();
                for part in parts.iter() {
                    // Check for font-style
                    match *part {
                        "italic" => style.font_style = FontStyle::Italic,
                        "oblique" => style.font_style = FontStyle::Italic,
                        "bold" => style.font_weight = FontWeight::Bold,
                        "bolder" => style.font_weight = FontWeight::Bold,
                        "lighter" => style.font_weight = FontWeight::Normal,
                        _ => {
                            // Check for numeric weight
                            if let Ok(weight) = part.parse::<u32>() {
                                style.font_weight = if weight >= 600 {
                                    FontWeight::Bold
                                } else {
                                    FontWeight::Normal
                                };
                            }
                        }
                    }
                }
            }

            _ => {}
        }
    }
}

/// Check if a value has !important and strip it
fn strip_important(value: &str) -> (&str, bool) {
    let value = value.trim();
    if value.ends_with("!important") {
        (value.strip_suffix("!important").unwrap().trim(), true)
    } else if value.ends_with("! important") {
        (value.strip_suffix("! important").unwrap().trim(), true)
    } else {
        (value, false)
    }
}

/// Parse env() function value
fn parse_env(value: &str) -> Option<i32> {
    use crate::layout::EnvValue;

    let inner = value.strip_prefix("env(")?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').collect();

    if let Some(env_name) = parts.first() {
        if let Some(env_val) = EnvValue::from_str(env_name.trim()) {
            // If there's a fallback value, try to parse it
            if let Some(fallback) = parts.get(1) {
                if let Some(val) = parse_length(fallback.trim()) {
                    return Some(val);
                }
            }
            // Otherwise return the default for this env variable
            return Some(env_val.default_value());
        }
    }
    None
}

/// Parse attr() function (simplified - returns 0 for now)
fn parse_attr(_value: &str) -> Option<i32> {
    // attr() requires access to the element, which we don't have here
    // Return None to fall through to other parsing
    None
}

/// Parse a CSS length value and convert to character units (approximate)
fn parse_length(value: &str) -> Option<i32> {
    let value = value.trim();
    if value == "0" {
        return Some(0);
    }

    // Handle CSS math functions
    if value.starts_with("calc(") {
        return parse_calc(value);
    }
    if value.starts_with("min(") {
        return parse_css_min(value);
    }
    if value.starts_with("max(") {
        return parse_css_max(value);
    }
    if value.starts_with("clamp(") {
        return parse_clamp(value);
    }
    if value.starts_with("env(") {
        return parse_env(value);
    }
    if value.starts_with("attr(") {
        return parse_attr(value);
    }

    // Try parsing with unit
    if let Some(num_str) = value.strip_suffix("px") {
        if let Ok(px) = num_str.trim().parse::<f32>() {
            // Approximate: 1ch ≈ 8px
            return Some((px / 8.0).round() as i32);
        }
    } else if let Some(num_str) = value.strip_suffix("em") {
        if let Ok(em) = num_str.trim().parse::<f32>() {
            // 1em ≈ 1ch for monospace
            return Some(em.round() as i32);
        }
    } else if let Some(num_str) = value.strip_suffix("rem") {
        if let Ok(rem) = num_str.trim().parse::<f32>() {
            return Some(rem.round() as i32);
        }
    } else if let Some(num_str) = value.strip_suffix("ch") {
        if let Ok(ch) = num_str.trim().parse::<f32>() {
            return Some(ch.round() as i32);
        }
    } else if let Some(num_str) = value.strip_suffix('%') {
        // Percentages - can't resolve without context, return small value
        if let Ok(_pct) = num_str.trim().parse::<f32>() {
            return Some(0);
        }
    }

    // Try plain number
    if let Ok(num) = value.parse::<f32>() {
        return Some((num / 8.0).round() as i32);
    }

    None
}

/// Parse calc() expression (simplified - handles basic operations)
fn parse_calc(value: &str) -> Option<i32> {
    let inner = value.strip_prefix("calc(")?.strip_suffix(')')?;

    // Simple case: just a length value
    if !inner.contains(['+', '-', '*', '/']) {
        return parse_length(inner.trim());
    }

    // Try to evaluate simple expressions like "100% - 20px"
    // For now, just try to extract and use the first numeric value
    let parts: Vec<&str> = inner.split(['+', '-', '*', '/']).collect();
    if let Some(first) = parts.first() {
        return parse_length(first.trim());
    }

    None
}

/// Parse min() function
fn parse_css_min(value: &str) -> Option<i32> {
    let inner = value.strip_prefix("min(")?.strip_suffix(')')?;
    let values: Vec<i32> = inner
        .split(',')
        .filter_map(|v| parse_length(v.trim()))
        .collect();
    values.into_iter().min()
}

/// Parse max() function
fn parse_css_max(value: &str) -> Option<i32> {
    let inner = value.strip_prefix("max(")?.strip_suffix(')')?;
    let values: Vec<i32> = inner
        .split(',')
        .filter_map(|v| parse_length(v.trim()))
        .collect();
    values.into_iter().max()
}

/// Parse clamp() function: clamp(min, preferred, max)
fn parse_clamp(value: &str) -> Option<i32> {
    let inner = value.strip_prefix("clamp(")?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 3 {
        return None;
    }

    let min_val = parse_length(parts[0].trim())?;
    let preferred = parse_length(parts[1].trim())?;
    let max_val = parse_length(parts[2].trim())?;

    Some(preferred.max(min_val).min(max_val))
}

/// Parse a CSS length value to usize
fn parse_length_usize(value: &str) -> Option<usize> {
    parse_length(value).map(|v| v.max(0) as usize)
}

/// Parse line-height value (can be unitless number, length, or percentage)
fn parse_line_height(value: &str) -> Option<f32> {
    let value = value.trim();

    if value == "normal" {
        return Some(1.2); // Default line-height
    }

    // Try unitless number first (e.g., "1.5")
    if let Ok(num) = value.parse::<f32>() {
        return Some(num);
    }

    // Try with units
    if let Some(num_str) = value.strip_suffix("px") {
        if let Ok(px) = num_str.trim().parse::<f32>() {
            // Convert px to line multiplier (assume 16px base font)
            return Some(px / 16.0);
        }
    } else if let Some(num_str) = value.strip_suffix("em") {
        if let Ok(em) = num_str.trim().parse::<f32>() {
            return Some(em);
        }
    } else if let Some(num_str) = value.strip_suffix("rem") {
        if let Ok(rem) = num_str.trim().parse::<f32>() {
            return Some(rem);
        }
    } else if let Some(num_str) = value.strip_suffix('%') {
        if let Ok(pct) = num_str.trim().parse::<f32>() {
            return Some(pct / 100.0);
        }
    }

    None
}

/// Parse counter-reset or counter-increment values
/// Format: "counter1" or "counter1 5" or "counter1 5 counter2 -1"
fn parse_counter_list(value: &str) -> Vec<(String, i32)> {
    let value = value.trim();
    if value == "none" || value.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let parts: Vec<&str> = value.split_whitespace().collect();
    let mut i = 0;

    while i < parts.len() {
        let name = parts[i].to_string();
        // Check if next token is a number
        let increment = if i + 1 < parts.len() {
            if let Ok(val) = parts[i + 1].parse::<i32>() {
                i += 1;
                val
            } else {
                1 // Default increment
            }
        } else {
            1 // Default increment
        };
        result.push((name, increment));
        i += 1;
    }

    result
}

/// Parse border-spacing value
/// Format: "10px" or "10px 5px" (horizontal vertical)
fn parse_border_spacing(value: &str) -> (i32, i32) {
    let parts: Vec<&str> = value.split_whitespace().collect();
    match parts.len() {
        0 => (0, 0),
        1 => {
            let val = parse_length(parts[0]).unwrap_or(0);
            (val, val)
        }
        _ => {
            let h = parse_length(parts[0]).unwrap_or(0);
            let v = parse_length(parts[1]).unwrap_or(0);
            (h, v)
        }
    }
}

/// Parse initial-letter value
/// Format: "3" or "3 2" (size sink)
fn parse_initial_letter(value: &str) -> Option<(f32, Option<usize>)> {
    let value = value.trim();
    if value == "normal" || value == "none" {
        return None;
    }

    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let size = parts[0].parse::<f32>().ok()?;
    let sink = parts.get(1).and_then(|s| s.parse::<usize>().ok());

    Some((size, sink))
}

/// Parse content sizing keywords (min-content, max-content, fit-content)
fn parse_content_sizing(value: &str) -> Option<ContentSizing> {
    let value = value.trim().to_lowercase();
    match value.as_str() {
        "min-content" => Some(ContentSizing::MinContent),
        "max-content" => Some(ContentSizing::MaxContent),
        "fit-content" => Some(ContentSizing::FitContent),
        _ if value.starts_with("fit-content(") => {
            // fit-content(length) is treated as fit-content for simplicity
            Some(ContentSizing::FitContent)
        }
        "auto" => Some(ContentSizing::Auto),
        _ => None,
    }
}

/// Parse quotes value
/// Format: '"«" "»"' or '"«" "»" "‹" "›"' or "auto" or "none"
fn parse_quotes(value: &str) -> Option<Vec<(String, String)>> {
    let value = value.trim();

    if value == "none" {
        return Some(Vec::new());
    }

    if value == "auto" {
        // Return standard English curly quotes as default
        return Some(vec![
            ("\u{201C}".to_string(), "\u{201D}".to_string()), // "" - double curly quotes
            ("\u{2018}".to_string(), "\u{2019}".to_string()), // '' - single curly quotes
        ]);
    }

    // Parse quoted strings
    let mut result = Vec::new();
    let mut chars = value.chars().peekable();
    let mut current_pair: Vec<String> = Vec::new();

    while let Some(c) = chars.next() {
        if c == '"' || c == '\'' {
            let quote_char = c;
            let mut quoted = String::new();
            while let Some(&next_c) = chars.peek() {
                chars.next();
                if next_c == quote_char {
                    break;
                }
                quoted.push(next_c);
            }
            current_pair.push(quoted);

            if current_pair.len() == 2 {
                result.push((current_pair[0].clone(), current_pair[1].clone()));
                current_pair.clear();
            }
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Parse aspect-ratio value
/// Format: "16 / 9", "16/9", "1.778", or "auto"
fn parse_aspect_ratio(value: &str) -> Option<(f32, f32)> {
    let value = value.trim();

    if value == "auto" {
        return None;
    }

    // Try ratio format: "16 / 9" or "16/9"
    if let Some((width_str, height_str)) = value.split_once('/') {
        let width = width_str.trim().parse::<f32>().ok()?;
        let height = height_str.trim().parse::<f32>().ok()?;
        if width > 0.0 && height > 0.0 {
            return Some((width, height));
        }
    }

    // Try single number (treated as ratio to 1)
    if let Ok(ratio) = value.parse::<f32>() {
        if ratio > 0.0 {
            return Some((ratio, 1.0));
        }
    }

    None
}

/// Parse object-position value
/// Format: "center", "top left", "50% 50%", "10px 20px"
fn parse_object_position(value: &str) -> (ObjectPosition, ObjectPosition) {
    let value = value.trim();
    let parts: Vec<&str> = value.split_whitespace().collect();

    let parse_single = |s: &str| -> ObjectPosition {
        match s.to_lowercase().as_str() {
            "center" => ObjectPosition::Center,
            "top" => ObjectPosition::Top,
            "bottom" => ObjectPosition::Bottom,
            "left" => ObjectPosition::Left,
            "right" => ObjectPosition::Right,
            _ => {
                // Try percentage
                if let Some(num_str) = s.strip_suffix('%') {
                    if let Ok(pct) = num_str.trim().parse::<f32>() {
                        return ObjectPosition::Percent(pct / 100.0);
                    }
                }
                // Try length
                if let Some(len) = parse_length(s) {
                    return ObjectPosition::Length(len);
                }
                ObjectPosition::Center
            }
        }
    };

    match parts.len() {
        0 => (ObjectPosition::Center, ObjectPosition::Center),
        1 => {
            let pos = parse_single(parts[0]);
            (pos.clone(), pos)
        }
        _ => {
            let x = parse_single(parts[0]);
            let y = parse_single(parts[1]);
            (x, y)
        }
    }
}

/// Parse a single grid track size value
fn parse_grid_track_size(value: &str) -> GridTrackSize {
    let value = value.trim();

    if value == "auto" {
        return GridTrackSize::Auto;
    }

    if value == "min-content" {
        return GridTrackSize::MinContent;
    }

    if value == "max-content" {
        return GridTrackSize::MaxContent;
    }

    // Handle fr units
    if let Some(num_str) = value.strip_suffix("fr") {
        if let Ok(fr) = num_str.trim().parse::<f32>() {
            return GridTrackSize::Fr(fr);
        }
    }

    // Handle fixed sizes
    if let Some(px) = parse_length_usize(value) {
        return GridTrackSize::Fixed(px);
    }

    GridTrackSize::Auto
}

/// Parse a grid-template-columns or grid-template-rows value
fn parse_grid_template(value: &str) -> Vec<GridTrackSize> {
    let value = value.trim();

    // Handle "none"
    if value == "none" {
        return Vec::new();
    }

    // Handle repeat() function - simplified parsing
    // e.g., repeat(3, 1fr) or repeat(auto-fill, minmax(100px, 1fr))
    if value.starts_with("repeat(") {
        if let Some(inner) = value.strip_prefix("repeat(").and_then(|s| s.strip_suffix(')')) {
            if let Some((count_str, track)) = inner.split_once(',') {
                let track = track.trim();
                if let Ok(count) = count_str.trim().parse::<usize>() {
                    let size = parse_grid_track_size(track);
                    return vec![size; count];
                }
            }
        }
        // If we can't parse repeat(), fall back to auto
        return vec![GridTrackSize::Auto];
    }

    // Split by whitespace and parse each track
    value
        .split_whitespace()
        .map(parse_grid_track_size)
        .collect()
}

/// Normalize mixed inline/block children by wrapping inline runs in anonymous blocks
fn normalize_children(parent_id: NodeId, children: Vec<LayoutBox>) -> Vec<LayoutBox> {
    let has_blocks = children.iter().any(|c| c.is_block());
    if !has_blocks {
        return children;
    }

    let mut result = Vec::new();
    let mut inline_run: Vec<LayoutBox> = Vec::new();

    for child in children {
        if child.is_block() {
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

    if !inline_run.is_empty() {
        let mut anon = LayoutBox::anonymous_block(parent_id);
        anon.children = inline_run;
        result.push(anon);
    }

    result
}

/// Parse a CSS time value (s or ms) to seconds
fn parse_time(value: &str) -> Option<f32> {
    let value = value.trim();
    if let Some(ms) = value.strip_suffix("ms") {
        ms.trim().parse::<f32>().ok().map(|v| v / 1000.0)
    } else if let Some(s) = value.strip_suffix('s') {
        s.trim().parse::<f32>().ok()
    } else {
        // Try plain number as seconds
        value.parse::<f32>().ok()
    }
}

/// Parse transition shorthand: property duration timing-function delay
fn parse_transition(value: &str) -> Option<Vec<Transition>> {
    let mut transitions = Vec::new();

    // Split by comma for multiple transitions
    for part in value.split(',') {
        let tokens: Vec<&str> = part.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        let mut transition = Transition::default();
        let mut time_count = 0;

        for token in tokens {
            // Check if it's a timing function
            if token.starts_with("cubic-bezier(") || token.starts_with("steps(")
               || matches!(token, "ease" | "ease-in" | "ease-out" | "ease-in-out" | "linear" | "step-start" | "step-end") {
                transition.timing = TimingFunction::from_str(token);
            }
            // Check if it's a time value
            else if token.ends_with('s') || token.ends_with("ms") {
                if let Some(time) = parse_time(token) {
                    if time_count == 0 {
                        transition.duration = time;
                    } else {
                        transition.delay = time;
                    }
                    time_count += 1;
                }
            }
            // Otherwise it's a property name
            else if token != "none" && token != "all" {
                transition.property = TransitionProperty::Property(token.to_string());
            } else if token == "all" {
                transition.property = TransitionProperty::All;
            } else if token == "none" {
                transition.property = TransitionProperty::None;
            }
        }

        transitions.push(transition);
    }

    if transitions.is_empty() {
        None
    } else {
        Some(transitions)
    }
}

/// Parse animation shorthand
fn parse_animation(value: &str) -> Option<Vec<Animation>> {
    let mut animations = Vec::new();

    for part in value.split(',') {
        let tokens: Vec<&str> = part.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        let mut animation = Animation::default();
        let mut time_count = 0;
        let mut found_name = false;

        for token in tokens {
            // Check for timing function
            if token.starts_with("cubic-bezier(") || token.starts_with("steps(")
               || matches!(token, "ease" | "ease-in" | "ease-out" | "ease-in-out" | "linear" | "step-start" | "step-end") {
                animation.timing = TimingFunction::from_str(token);
            }
            // Check for time values
            else if token.ends_with('s') || token.ends_with("ms") {
                if let Some(time) = parse_time(token) {
                    if time_count == 0 {
                        animation.duration = time;
                    } else {
                        animation.delay = time;
                    }
                    time_count += 1;
                }
            }
            // Check for iteration count
            else if token == "infinite" {
                animation.iteration_count = AnimationIterationCount::Infinite;
            } else if let Ok(count) = token.parse::<f32>() {
                animation.iteration_count = AnimationIterationCount::Count(count);
            }
            // Check for direction
            else if matches!(token, "normal" | "reverse" | "alternate" | "alternate-reverse") {
                animation.direction = parse_animation_direction(token);
            }
            // Check for fill mode
            else if matches!(token, "none" | "forwards" | "backwards" | "both") && !found_name {
                animation.fill_mode = parse_fill_mode(token);
            }
            // Check for play state
            else if matches!(token, "running" | "paused") {
                animation.play_state = parse_play_state(token);
            }
            // Otherwise it's the animation name
            else if !found_name {
                animation.name = token.to_string();
                found_name = true;
            }
        }

        animations.push(animation);
    }

    if animations.is_empty() {
        None
    } else {
        Some(animations)
    }
}

fn parse_iteration_count(value: &str) -> AnimationIterationCount {
    if value == "infinite" {
        AnimationIterationCount::Infinite
    } else if let Ok(count) = value.parse::<f32>() {
        AnimationIterationCount::Count(count)
    } else {
        AnimationIterationCount::One
    }
}

fn parse_animation_direction(value: &str) -> AnimationDirection {
    match value {
        "reverse" => AnimationDirection::Reverse,
        "alternate" => AnimationDirection::Alternate,
        "alternate-reverse" => AnimationDirection::AlternateReverse,
        _ => AnimationDirection::Normal,
    }
}

fn parse_fill_mode(value: &str) -> AnimationFillMode {
    match value {
        "forwards" => AnimationFillMode::Forwards,
        "backwards" => AnimationFillMode::Backwards,
        "both" => AnimationFillMode::Both,
        _ => AnimationFillMode::None,
    }
}

fn parse_play_state(value: &str) -> AnimationPlayState {
    match value {
        "paused" => AnimationPlayState::Paused,
        _ => AnimationPlayState::Running,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_default_style() {
        let div_style = tag_default_style("div");
        assert_eq!(div_style.display, Display::Block);

        let span_style = tag_default_style("span");
        assert_eq!(span_style.display, Display::Inline);

        let strong_style = tag_default_style("strong");
        assert!(strong_style.font_weight.is_bold());
    }

    #[test]
    fn test_parse_length() {
        assert_eq!(parse_length("0"), Some(0));
        assert_eq!(parse_length("16px"), Some(2)); // 16/8 = 2
        assert_eq!(parse_length("1em"), Some(1));
        assert_eq!(parse_length("2ch"), Some(2));
    }

    #[test]
    fn test_apply_inline_style() {
        let mut style = ComputedStyle::default();
        apply_inline_style(&mut style, "display: flex; flex-direction: column;");

        assert_eq!(style.display, Display::Flex);
        assert_eq!(style.flex_direction, FlexDirection::Column);
    }

    #[test]
    fn test_should_skip_element() {
        assert!(should_skip_element("script"));
        assert!(should_skip_element("style"));
        assert!(should_skip_element("SCRIPT")); // case insensitive
        assert!(!should_skip_element("div"));
        assert!(!should_skip_element("p"));
    }
}
