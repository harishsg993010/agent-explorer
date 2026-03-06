//! Inline-block layout with shrink-to-fit width calculation.
//!
//! Handles inline-block elements that participate in inline flow
//! but establish their own block formatting context internally.

use super::block::{layout_block, BlockLayoutContext};
use super::tree::LayoutBox;
use super::{Display, Viewport};
use crate::ast::Block;

/// Inline-block layout result
#[derive(Debug)]
pub struct InlineBlockResult {
    /// The laid out blocks
    pub blocks: Vec<Block>,
    /// Shrink-to-fit width
    pub width: usize,
    /// Height in lines
    pub height: usize,
}

/// Layout an inline-block element
pub fn layout_inline_block(
    layout_box: &LayoutBox,
    viewport: &Viewport,
    available_width: usize,
) -> InlineBlockResult {
    // Calculate shrink-to-fit width
    let preferred_width = calculate_preferred_width(layout_box, viewport);
    let min_width = calculate_min_width(layout_box);

    // Apply explicit width constraints
    let explicit_width = layout_box.style.width;
    let min_w = layout_box.style.min_width.unwrap_or(0);
    let max_w = layout_box.style.max_width.unwrap_or(usize::MAX);

    // Shrink-to-fit algorithm
    let mut width = if let Some(w) = explicit_width {
        w
    } else {
        // Use preferred width, constrained by available width
        preferred_width.min(available_width)
    };

    // Apply min/max constraints
    width = width.max(min_w).min(max_w).max(min_width);

    // Layout content with calculated width
    let mut ctx = BlockLayoutContext::new(viewport);
    ctx.available_width = width;

    let blocks = if layout_box.style.display == Display::InlineBlock {
        // Layout as block internally
        layout_block(layout_box, &mut ctx)
    } else {
        Vec::new()
    };

    let height = estimate_height(&blocks);

    InlineBlockResult {
        blocks,
        width,
        height,
    }
}

/// Calculate preferred width (content would ideally like)
fn calculate_preferred_width(layout_box: &LayoutBox, viewport: &Viewport) -> usize {
    if layout_box.children.is_empty() {
        if let Some(text) = &layout_box.text {
            return unicode_width::UnicodeWidthStr::width(text.as_str());
        }
        return 0;
    }

    let mut max_child_width = 0usize;

    for child in &layout_box.children {
        let child_width = if child.is_block() {
            // Block children: measure their preferred width
            let child_preferred = calculate_preferred_width(child, viewport);
            let margins = (child.style.margin_left + child.style.margin_right).max(0) as usize;
            let padding = (child.style.padding_left + child.style.padding_right).max(0) as usize;
            child_preferred + margins + padding
        } else {
            // Inline children: measure text width
            if let Some(text) = &child.text {
                unicode_width::UnicodeWidthStr::width(text.as_str())
            } else {
                calculate_preferred_width(child, viewport)
            }
        };

        max_child_width = max_child_width.max(child_width);
    }

    // Add own padding
    let padding = (layout_box.style.padding_left + layout_box.style.padding_right).max(0) as usize;

    max_child_width + padding
}

/// Calculate minimum width (narrowest the content can be)
fn calculate_min_width(layout_box: &LayoutBox) -> usize {
    if layout_box.children.is_empty() {
        if let Some(text) = &layout_box.text {
            // Minimum width is the longest word
            return text
                .split_whitespace()
                .map(|w| unicode_width::UnicodeWidthStr::width(w))
                .max()
                .unwrap_or(0);
        }
        return 0;
    }

    let mut max_min_width = 0usize;

    for child in &layout_box.children {
        let child_min = calculate_min_width(child);
        max_min_width = max_min_width.max(child_min);
    }

    // Add own padding
    let padding = (layout_box.style.padding_left + layout_box.style.padding_right).max(0) as usize;

    max_min_width + padding
}

/// Estimate height in lines
fn estimate_height(blocks: &[Block]) -> usize {
    blocks.len().max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::NodeId;
    use crate::layout::ComputedStyle;

    #[test]
    fn test_calculate_min_width() {
        let node_id = NodeId::new(1);
        let mut layout_box = LayoutBox::new(node_id, "div", ComputedStyle::default());
        layout_box.text = Some("hello world test".to_string());

        let min_width = calculate_min_width(&layout_box);
        assert_eq!(min_width, 5); // "hello" or "world" is the longest word
    }

    #[test]
    fn test_shrink_to_fit() {
        let node_id = NodeId::new(1);
        let mut style = ComputedStyle::default();
        style.display = Display::InlineBlock;
        let mut layout_box = LayoutBox::new(node_id, "span", style);
        layout_box.text = Some("test".to_string());

        let viewport = Viewport::new(80);
        let result = layout_inline_block(&layout_box, &viewport, 40);

        assert!(result.width <= 40);
    }
}
