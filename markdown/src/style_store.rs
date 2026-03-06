//! StyleStore - Shared computed style storage for CSSOM↔Layout integration.
//!
//! This module provides a shared store for computed styles that can be accessed
//! by both the layout engine and the JavaScript runtime's CSSOM implementation.

use crate::layout::ComputedStyle;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Unique identifier for DOM elements in the style store
pub type ElementId = u64;

/// Shared style store that bridges layout engine and JS CSSOM
#[derive(Debug, Clone)]
pub struct StyleStore {
    inner: Rc<RefCell<StyleStoreInner>>,
}

#[derive(Debug, Default)]
struct StyleStoreInner {
    /// Computed styles indexed by element ID
    styles: HashMap<ElementId, ComputedStyle>,
    /// Elements whose styles have been modified and need relayout
    dirty_elements: Vec<ElementId>,
    /// Flag indicating if any styles have changed since last layout
    needs_relayout: bool,
    /// Generation counter for cache invalidation
    generation: u64,
}

impl Default for StyleStore {
    fn default() -> Self {
        Self::new()
    }
}

impl StyleStore {
    /// Create a new empty style store
    pub fn new() -> Self {
        StyleStore {
            inner: Rc::new(RefCell::new(StyleStoreInner::default())),
        }
    }

    /// Store computed style for an element
    pub fn set_computed_style(&self, element_id: ElementId, style: ComputedStyle) {
        let mut inner = self.inner.borrow_mut();
        inner.styles.insert(element_id, style);
    }

    /// Get computed style for an element
    pub fn get_computed_style(&self, element_id: ElementId) -> Option<ComputedStyle> {
        self.inner.borrow().styles.get(&element_id).cloned()
    }

    /// Get a specific CSS property value as a string (for JS getComputedStyle)
    pub fn get_property_value(&self, element_id: ElementId, property: &str) -> String {
        let inner = self.inner.borrow();
        if let Some(style) = inner.styles.get(&element_id) {
            computed_style_to_css_value(style, property)
        } else {
            String::new()
        }
    }

    /// Set a CSS property value (for JS element.style mutations)
    pub fn set_property_value(&self, element_id: ElementId, property: &str, value: &str) {
        let mut inner = self.inner.borrow_mut();

        // Get or create style for this element
        let style = inner.styles.entry(element_id).or_insert_with(ComputedStyle::default);

        // Apply the property change
        apply_css_property(style, property, value);

        // Mark as dirty
        if !inner.dirty_elements.contains(&element_id) {
            inner.dirty_elements.push(element_id);
        }
        inner.needs_relayout = true;
        inner.generation += 1;
    }

    /// Remove a CSS property (for JS element.style.removeProperty)
    pub fn remove_property(&self, element_id: ElementId, property: &str) -> String {
        let mut inner = self.inner.borrow_mut();

        if let Some(style) = inner.styles.get_mut(&element_id) {
            let old_value = computed_style_to_css_value(style, property);
            reset_css_property(style, property);

            if !inner.dirty_elements.contains(&element_id) {
                inner.dirty_elements.push(element_id);
            }
            inner.needs_relayout = true;
            inner.generation += 1;

            old_value
        } else {
            String::new()
        }
    }

    /// Check if relayout is needed
    pub fn needs_relayout(&self) -> bool {
        self.inner.borrow().needs_relayout
    }

    /// Get list of dirty elements and clear the dirty flag
    pub fn take_dirty_elements(&self) -> Vec<ElementId> {
        let mut inner = self.inner.borrow_mut();
        inner.needs_relayout = false;
        std::mem::take(&mut inner.dirty_elements)
    }

    /// Get current generation (for cache invalidation)
    pub fn generation(&self) -> u64 {
        self.inner.borrow().generation
    }

    /// Clear all styles (for page navigation)
    pub fn clear(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.styles.clear();
        inner.dirty_elements.clear();
        inner.needs_relayout = false;
        inner.generation += 1;
    }

    /// Get all property names for an element (for JS iteration)
    pub fn get_property_names(&self, element_id: ElementId) -> Vec<String> {
        let inner = self.inner.borrow();
        if inner.styles.contains_key(&element_id) {
            CSS_PROPERTY_NAMES.iter().map(|s| s.to_string()).collect()
        } else {
            Vec::new()
        }
    }

    /// Get the number of properties (for CSSStyleDeclaration.length)
    pub fn get_property_count(&self) -> usize {
        CSS_PROPERTY_NAMES.len()
    }

    /// Get property by index (for CSSStyleDeclaration[index])
    pub fn get_property_by_index(&self, index: usize) -> Option<&'static str> {
        CSS_PROPERTY_NAMES.get(index).copied()
    }
}

/// All CSS property names supported by getComputedStyle
const CSS_PROPERTY_NAMES: &[&str] = &[
    "display", "position", "visibility", "top", "right", "bottom", "left",
    "width", "height", "min-width", "max-width", "min-height", "max-height",
    "margin", "margin-top", "margin-right", "margin-bottom", "margin-left",
    "padding", "padding-top", "padding-right", "padding-bottom", "padding-left",
    "border-width", "border-top-width", "border-right-width", "border-bottom-width", "border-left-width",
    "font-family", "font-size", "font-weight", "font-style", "line-height",
    "text-align", "text-decoration", "text-transform", "white-space",
    "color", "background-color", "background",
    "flex", "flex-direction", "flex-wrap", "justify-content", "align-items", "align-self",
    "flex-grow", "flex-shrink", "flex-basis", "order", "gap",
    "grid-template-columns", "grid-template-rows", "grid-column", "grid-row",
    "overflow", "overflow-x", "overflow-y", "opacity", "z-index",
    "transform", "transition", "animation",
    "box-sizing", "float", "clear", "cursor", "pointer-events",
    "filter", "backdrop-filter", "mix-blend-mode",
    "mask", "mask-image", "mask-mode", "mask-repeat", "mask-position", "mask-size",
    "view-transition-name",
];

/// Convert a ComputedStyle field to CSS string value
fn computed_style_to_css_value(style: &ComputedStyle, property: &str) -> String {
    use crate::layout::*;

    match property {
        "display" => match style.display {
            Display::Block => "block",
            Display::Inline => "inline",
            Display::InlineBlock => "inline-block",
            Display::Flex => "flex",
            Display::InlineFlex => "inline-flex",
            Display::Grid => "grid",
            Display::Table => "table",
            Display::TableRow => "table-row",
            Display::TableCell => "table-cell",
            Display::TableRowGroup => "table-row-group",
            Display::TableHeaderGroup => "table-header-group",
            Display::TableFooterGroup => "table-footer-group",
            Display::ListItem => "list-item",
            Display::None => "none",
            Display::Contents => "contents",
        }.to_string(),

        "position" => match style.position {
            Position::Static => "static",
            Position::Relative => "relative",
            Position::Absolute => "absolute",
            Position::Fixed => "fixed",
            Position::Sticky => "sticky",
        }.to_string(),

        "visibility" => match style.visibility {
            Visibility::Visible => "visible",
            Visibility::Hidden => "hidden",
            Visibility::Collapse => "collapse",
        }.to_string(),

        "top" => style.top.map(|v| format!("{}px", v)).unwrap_or_else(|| "auto".to_string()),
        "right" => style.right.map(|v| format!("{}px", v)).unwrap_or_else(|| "auto".to_string()),
        "bottom" => style.bottom.map(|v| format!("{}px", v)).unwrap_or_else(|| "auto".to_string()),
        "left" => style.left.map(|v| format!("{}px", v)).unwrap_or_else(|| "auto".to_string()),

        "width" => style.width.map(|v| format!("{}px", v)).unwrap_or_else(|| "auto".to_string()),
        "height" => style.height.map(|v| format!("{}px", v)).unwrap_or_else(|| "auto".to_string()),
        "min-width" => style.min_width.map(|v| format!("{}px", v)).unwrap_or_else(|| "0px".to_string()),
        "max-width" => style.max_width.map(|v| format!("{}px", v)).unwrap_or_else(|| "none".to_string()),
        "min-height" => "0px".to_string(),
        "max-height" => "none".to_string(),

        "margin-top" => format!("{}px", style.margin_top),
        "margin-right" => format!("{}px", style.margin_right),
        "margin-bottom" => format!("{}px", style.margin_bottom),
        "margin-left" => format!("{}px", style.margin_left),
        "margin" => format!("{}px {}px {}px {}px",
            style.margin_top, style.margin_right, style.margin_bottom, style.margin_left),

        "padding-top" => format!("{}px", style.padding_top),
        "padding-right" => format!("{}px", style.padding_right),
        "padding-bottom" => format!("{}px", style.padding_bottom),
        "padding-left" => format!("{}px", style.padding_left),
        "padding" => format!("{}px {}px {}px {}px",
            style.padding_top, style.padding_right, style.padding_bottom, style.padding_left),

        "border-top-width" => format!("{}px", style.border_top_width),
        "border-right-width" => format!("{}px", style.border_right_width),
        "border-bottom-width" => format!("{}px", style.border_bottom_width),
        "border-left-width" => format!("{}px", style.border_left_width),

        "font-size" => "16px".to_string(), // Default font size
        "font-weight" => match style.font_weight {
            FontWeight::Normal => "400".to_string(),
            FontWeight::Bold => "700".to_string(),
            FontWeight::Lighter => "lighter".to_string(),
            FontWeight::Bolder => "bolder".to_string(),
            FontWeight::Numeric(n) => n.to_string(),
        },
        "font-style" => match style.font_style {
            FontStyle::Normal => "normal",
            FontStyle::Italic => "italic",
            FontStyle::Oblique => "oblique",
        }.to_string(),
        "line-height" => style.line_height.map(|v| format!("{}", v)).unwrap_or_else(|| "normal".to_string()),

        "text-align" => match style.text_align {
            TextAlign::Left => "left",
            TextAlign::Right => "right",
            TextAlign::Center => "center",
            TextAlign::Justify => "justify",
            TextAlign::Start => "start",
            TextAlign::End => "end",
        }.to_string(),

        "white-space" => match style.white_space {
            WhiteSpace::Normal => "normal",
            WhiteSpace::NoWrap => "nowrap",
            WhiteSpace::Pre => "pre",
            WhiteSpace::PreWrap => "pre-wrap",
            WhiteSpace::PreLine => "pre-line",
            WhiteSpace::BreakSpaces => "break-spaces",
        }.to_string(),

        "flex-direction" => match style.flex_direction {
            FlexDirection::Row => "row",
            FlexDirection::RowReverse => "row-reverse",
            FlexDirection::Column => "column",
            FlexDirection::ColumnReverse => "column-reverse",
        }.to_string(),

        "flex-wrap" => match style.flex_wrap {
            FlexWrap::NoWrap => "nowrap",
            FlexWrap::Wrap => "wrap",
            FlexWrap::WrapReverse => "wrap-reverse",
        }.to_string(),

        "justify-content" => match style.justify_content {
            JustifyContent::FlexStart => "flex-start",
            JustifyContent::FlexEnd => "flex-end",
            JustifyContent::Center => "center",
            JustifyContent::SpaceBetween => "space-between",
            JustifyContent::SpaceAround => "space-around",
            JustifyContent::SpaceEvenly => "space-evenly",
        }.to_string(),

        "align-items" => match style.align_items {
            AlignItems::Stretch => "stretch",
            AlignItems::FlexStart => "flex-start",
            AlignItems::FlexEnd => "flex-end",
            AlignItems::Center => "center",
            AlignItems::Baseline => "baseline",
        }.to_string(),

        "flex-grow" => format!("{}", style.flex_grow),
        "flex-shrink" => format!("{}", style.flex_shrink),
        "flex-basis" => style.flex_basis.map(|v| format!("{}px", v)).unwrap_or_else(|| "auto".to_string()),
        "order" => "0".to_string(),
        "gap" => style.gap.map(|v| format!("{}px", v)).unwrap_or_else(|| "0px".to_string()),

        "overflow" | "overflow-x" | "overflow-y" => match style.overflow_x {
            Overflow::Visible => "visible",
            Overflow::Hidden => "hidden",
            Overflow::Scroll => "scroll",
            Overflow::Auto => "auto",
        }.to_string(),

        "opacity" => format!("{}", style.opacity),
        "z-index" => style.z_index.map(|v| v.to_string()).unwrap_or_else(|| "auto".to_string()),

        "float" => match style.float {
            Float::None => "none",
            Float::Left => "left",
            Float::Right => "right",
            Float::InlineStart => "inline-start",
            Float::InlineEnd => "inline-end",
        }.to_string(),

        "clear" => match style.clear {
            Clear::None => "none",
            Clear::Left => "left",
            Clear::Right => "right",
            Clear::Both => "both",
            Clear::InlineStart => "inline-start",
            Clear::InlineEnd => "inline-end",
        }.to_string(),

        "box-sizing" => match style.box_sizing {
            BoxSizing::ContentBox => "content-box",
            BoxSizing::BorderBox => "border-box",
        }.to_string(),

        "view-transition-name" => match &style.view_transition_name {
            ViewTransitionName::None => "none".to_string(),
            ViewTransitionName::Auto => "auto".to_string(),
            ViewTransitionName::Custom(name) => name.clone(),
        },

        // Default for unknown properties
        _ => String::new(),
    }
}

/// Apply a CSS property value to a ComputedStyle
fn apply_css_property(style: &mut ComputedStyle, property: &str, value: &str) {
    use crate::layout::*;

    let value = value.trim();

    match property {
        "display" => style.display = Display::from_str(value),
        "position" => style.position = Position::from_str(value),
        "visibility" => style.visibility = Visibility::from_str(value),

        "top" => style.top = parse_length_option(value),
        "right" => style.right = parse_length_option(value),
        "bottom" => style.bottom = parse_length_option(value),
        "left" => style.left = parse_length_option(value),

        "width" => style.width = parse_size_option(value),
        "height" => style.height = parse_size_option(value),
        "min-width" => style.min_width = parse_size_option(value),
        "max-width" => style.max_width = parse_size_option(value),
        // min-height and max-height not supported in ComputedStyle
        "min-height" | "max-height" => {}

        "margin-top" => style.margin_top = parse_length(value),
        "margin-right" => style.margin_right = parse_length(value),
        "margin-bottom" => style.margin_bottom = parse_length(value),
        "margin-left" => style.margin_left = parse_length(value),

        "padding-top" => style.padding_top = parse_length(value),
        "padding-right" => style.padding_right = parse_length(value),
        "padding-bottom" => style.padding_bottom = parse_length(value),
        "padding-left" => style.padding_left = parse_length(value),

        // font-size not in ComputedStyle
        "font-size" => {}
        "font-weight" => style.font_weight = FontWeight::from_str(value),
        "font-style" => style.font_style = FontStyle::from_str(value),

        "text-align" => style.text_align = TextAlign::from_str(value),
        "white-space" => style.white_space = WhiteSpace::from_str(value),

        "flex-direction" => style.flex_direction = FlexDirection::from_str(value),
        "flex-wrap" => style.flex_wrap = FlexWrap::from_str(value),
        "justify-content" => style.justify_content = JustifyContent::from_str(value),
        "align-items" => style.align_items = AlignItems::from_str(value),

        "flex-grow" => if let Ok(v) = value.parse() { style.flex_grow = v; },
        "flex-shrink" => if let Ok(v) = value.parse() { style.flex_shrink = v; },
        // order not in ComputedStyle
        "order" => {}
        "gap" => {
            let len = parse_length(value);
            if len >= 0 {
                style.gap = Some(len as usize);
            }
        }

        "overflow" | "overflow-x" | "overflow-y" => {
            style.overflow_x = Overflow::from_str(value);
        }

        "opacity" => if let Ok(v) = value.parse::<f32>() { style.opacity = v.clamp(0.0, 1.0); },
        "z-index" => style.z_index = value.parse().ok(),

        "float" => style.float = Float::from_str(value),
        "clear" => style.clear = Clear::from_str(value),
        "box-sizing" => style.box_sizing = BoxSizing::from_str(value),

        "backdrop-filter" | "-webkit-backdrop-filter" => {
            style.backdrop_filter = BackdropFilter::from_str(value);
        }

        "view-transition-name" => {
            style.view_transition_name = ViewTransitionName::from_str(value);
        }

        _ => {} // Ignore unknown properties
    }
}

/// Reset a CSS property to its default value
fn reset_css_property(style: &mut ComputedStyle, property: &str) {
    use crate::layout::*;

    match property {
        "display" => style.display = Display::Block,
        "position" => style.position = Position::Static,
        "visibility" => style.visibility = Visibility::Visible,
        "top" | "right" | "bottom" | "left" => {
            match property {
                "top" => style.top = None,
                "right" => style.right = None,
                "bottom" => style.bottom = None,
                "left" => style.left = None,
                _ => {}
            }
        }
        "width" => style.width = None,
        "height" => style.height = None,
        "margin-top" => style.margin_top = 0,
        "margin-right" => style.margin_right = 0,
        "margin-bottom" => style.margin_bottom = 0,
        "margin-left" => style.margin_left = 0,
        "padding-top" => style.padding_top = 0,
        "padding-right" => style.padding_right = 0,
        "padding-bottom" => style.padding_bottom = 0,
        "padding-left" => style.padding_left = 0,
        "opacity" => style.opacity = 1.0,
        "z-index" => style.z_index = None,
        _ => {}
    }
}

fn parse_length(value: &str) -> i32 {
    let value = value.trim();
    if value == "auto" || value == "0" {
        return 0;
    }
    if let Some(px) = value.strip_suffix("px") {
        return px.trim().parse().unwrap_or(0);
    }
    if let Some(em) = value.strip_suffix("em") {
        return (em.trim().parse::<f32>().unwrap_or(0.0) * 16.0) as i32;
    }
    value.parse().unwrap_or(0)
}

fn parse_length_option(value: &str) -> Option<i32> {
    let value = value.trim();
    if value == "auto" {
        return None;
    }
    Some(parse_length(value))
}

fn parse_size_option(value: &str) -> Option<usize> {
    let value = value.trim();
    if value == "auto" || value == "none" {
        return None;
    }
    let len = parse_length(value);
    if len >= 0 { Some(len as usize) } else { None }
}

fn parse_font_size(value: &str) -> Option<u32> {
    let value = value.trim();
    if let Some(px) = value.strip_suffix("px") {
        return px.trim().parse().ok();
    }
    if let Some(em) = value.strip_suffix("em") {
        return em.trim().parse::<f32>().ok().map(|v| (v * 16.0) as u32);
    }
    match value {
        "xx-small" => Some(9),
        "x-small" => Some(10),
        "small" => Some(13),
        "medium" => Some(16),
        "large" => Some(18),
        "x-large" => Some(24),
        "xx-large" => Some(32),
        _ => value.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_store_basic() {
        let store = StyleStore::new();

        let mut style = ComputedStyle::default();
        style.display = crate::layout::Display::Flex;
        style.opacity = 0.5;

        store.set_computed_style(1, style);

        assert_eq!(store.get_property_value(1, "display"), "flex");
        assert_eq!(store.get_property_value(1, "opacity"), "0.5");
    }

    #[test]
    fn test_style_store_mutations() {
        let store = StyleStore::new();
        store.set_computed_style(1, ComputedStyle::default());

        store.set_property_value(1, "display", "none");
        assert!(store.needs_relayout());

        let dirty = store.take_dirty_elements();
        assert_eq!(dirty, vec![1]);
        assert!(!store.needs_relayout());
    }
}
