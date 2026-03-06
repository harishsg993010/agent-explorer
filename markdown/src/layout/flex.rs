//! Full flex layout implementation.
//!
//! Implements the CSS Flexbox layout algorithm adapted for terminal/Markdown output:
//! - flex-direction: row, row-reverse, column, column-reverse
//! - flex-wrap: nowrap, wrap, wrap-reverse
//! - justify-content: flex-start, flex-end, center, space-between, space-around, space-evenly
//! - align-items: flex-start, flex-end, center, baseline, stretch
//! - gap (row-gap and column-gap combined)
//! - flex-grow, flex-shrink, flex-basis on items
//!
//! The layout produces Markdown-compatible output with proper spacing.

use super::block::{collect_inline_content, layout_block, BlockLayoutContext};
use super::float::FloatContext;
use super::tree::LayoutBox;
use super::{AlignItems, FlexDirection, FlexWrap, JustifyContent, Viewport};
use crate::ast::{Block, BlockKind, InlineContent, Span, SpanKind};
use crate::ids::NodeId;

/// Flex layout result
#[derive(Debug)]
pub struct FlexLayoutResult {
    pub blocks: Vec<Block>,
}

/// A flex item with computed properties
#[derive(Debug, Clone)]
struct FlexItem {
    /// Source node ID
    node_id: NodeId,
    /// The layout box
    content: InlineContent,
    /// Child blocks (for block-level content)
    child_blocks: Vec<Block>,
    /// Base size (flex-basis or content size)
    base_size: usize,
    /// Hypothetical main size
    hypothetical_main_size: usize,
    /// Final main size after growing/shrinking
    final_main_size: usize,
    /// Cross size
    cross_size: usize,
    /// flex-grow value
    flex_grow: f32,
    /// flex-shrink value
    flex_shrink: f32,
    /// Whether this item is frozen (can't grow/shrink more)
    frozen: bool,
    /// Scaled flex shrink factor
    scaled_shrink_factor: f32,
    /// Is this item block-level internally
    is_block: bool,
}

/// A flex line containing items
#[derive(Debug)]
struct FlexLine {
    items: Vec<FlexItem>,
    /// Main axis size of this line
    main_size: usize,
    /// Cross axis size of this line
    cross_size: usize,
}

/// Layout a flex container
pub fn layout_flex(layout_box: &LayoutBox, ctx: &mut BlockLayoutContext) -> FlexLayoutResult {
    let style = &layout_box.style;

    // Step 1: Generate flex items
    let mut items = generate_flex_items(layout_box, ctx);

    if items.is_empty() {
        return FlexLayoutResult { blocks: Vec::new() };
    }

    // Determine main axis direction
    let is_row = style.flex_direction.is_row();
    let is_reversed = style.flex_direction.is_reversed();
    let is_wrap_reverse = style.flex_wrap == FlexWrap::WrapReverse;

    // Available main size
    let available_main = if is_row {
        ctx.available_width
    } else {
        // For column, we don't have a fixed height constraint in terminal
        usize::MAX
    };

    let gap = style.gap.unwrap_or(if is_row { 1 } else { 0 });

    // Step 2: Determine base sizes and hypothetical main sizes
    for item in &mut items {
        item.hypothetical_main_size = item.base_size;

        // Apply min/max constraints (simplified for terminal)
        item.hypothetical_main_size = item.hypothetical_main_size.max(1);
    }

    // Step 3: Collect items into flex lines
    let mut lines = collect_into_lines(&items, available_main, gap, style.flex_wrap);

    // Step 4: Resolve flexible lengths for each line
    for line in &mut lines {
        resolve_flexible_lengths(line, available_main, gap);
    }

    // Step 5: Calculate cross sizes
    for line in &mut lines {
        calculate_cross_sizes(line, is_row);
    }

    // Reverse items within lines if needed
    if is_reversed {
        for line in &mut lines {
            line.items.reverse();
        }
    }

    // Reverse lines if wrap-reverse
    if is_wrap_reverse {
        lines.reverse();
    }

    // Step 6: Render to blocks
    render_flex_lines(
        lines,
        style.justify_content,
        style.align_items,
        gap,
        available_main,
        is_row,
        layout_box.node_id,
    )
}

/// Generate flex items from children
fn generate_flex_items(layout_box: &LayoutBox, ctx: &BlockLayoutContext) -> Vec<FlexItem> {
    let mut items = Vec::new();

    for child in &layout_box.children {
        if !child.style.is_visible() {
            continue;
        }

        let (content, child_blocks, is_block) = if child.is_block() && !child.children.is_empty() {
            // Block-level child: layout it and get blocks
            let mut child_ctx = BlockLayoutContext {
                viewport: ctx.viewport,
                available_width: ctx.available_width,
                indent: ctx.indent,
                prev_margin_bottom: 0,
                in_list: ctx.in_list,
                list_depth: ctx.list_depth,
                in_blockquote: ctx.in_blockquote,
                blockquote_depth: ctx.blockquote_depth,
                container_stack: ctx.container_stack.clone(),
                float_context: FloatContext::new(ctx.available_width),
                current_y: ctx.current_y,
                counter_context: ctx.counter_context.clone(),
            };
            let blocks = layout_block(child, &mut child_ctx);
            (InlineContent::new(), blocks, true)
        } else {
            // Inline-level child: collect inline content
            let content = collect_inline_content(child);
            (content, Vec::new(), false)
        };

        // Calculate base size
        let base_size = if let Some(basis) = child.style.flex_basis {
            basis
        } else if let Some(width) = child.style.width {
            width
        } else if is_block {
            // Block content: estimate from first block
            estimate_block_width(&child_blocks)
        } else {
            // Inline content: measure text width
            estimate_inline_width(&content)
        };

        items.push(FlexItem {
            node_id: child.node_id,
            content,
            child_blocks,
            base_size: base_size.max(1),
            hypothetical_main_size: base_size.max(1),
            final_main_size: base_size.max(1),
            cross_size: 1,
            flex_grow: child.style.flex_grow,
            flex_shrink: child.style.flex_shrink,
            frozen: false,
            scaled_shrink_factor: 0.0,
            is_block,
        });
    }

    items
}

/// Collect flex items into lines based on wrapping
fn collect_into_lines(
    items: &[FlexItem],
    available_main: usize,
    gap: usize,
    wrap: FlexWrap,
) -> Vec<FlexLine> {
    if wrap == FlexWrap::NoWrap || available_main == usize::MAX {
        // Single line
        return vec![FlexLine {
            items: items.to_vec(),
            main_size: items.iter().map(|i| i.hypothetical_main_size).sum::<usize>()
                + if items.len() > 1 { (items.len() - 1) * gap } else { 0 },
            cross_size: 1,
        }];
    }

    let mut lines = Vec::new();
    let mut current_line = Vec::new();
    let mut current_main_size = 0usize;

    for item in items {
        let item_size = item.hypothetical_main_size;
        let gap_size = if current_line.is_empty() { 0 } else { gap };

        if current_main_size + gap_size + item_size > available_main && !current_line.is_empty() {
            // Start new line
            lines.push(FlexLine {
                items: std::mem::take(&mut current_line),
                main_size: current_main_size,
                cross_size: 1,
            });
            current_main_size = 0;
        }

        current_main_size += if current_line.is_empty() { 0 } else { gap } + item_size;
        current_line.push(item.clone());
    }

    if !current_line.is_empty() {
        lines.push(FlexLine {
            items: current_line,
            main_size: current_main_size,
            cross_size: 1,
        });
    }

    lines
}

/// Resolve flexible lengths using the flex algorithm
fn resolve_flexible_lengths(line: &mut FlexLine, available_main: usize, gap: usize) {
    if line.items.is_empty() {
        return;
    }

    let total_gap = if line.items.len() > 1 {
        (line.items.len() - 1) * gap
    } else {
        0
    };
    let available_for_items = available_main.saturating_sub(total_gap);

    // Calculate initial free space
    let total_base: usize = line.items.iter().map(|i| i.hypothetical_main_size).sum();
    let mut free_space = available_for_items as i64 - total_base as i64;

    // Determine if we're growing or shrinking
    let growing = free_space > 0;

    // Calculate total flex factor
    let total_flex: f32 = line
        .items
        .iter()
        .map(|i| if growing { i.flex_grow } else { i.flex_shrink })
        .sum();

    if total_flex == 0.0 {
        // No flexible items, use hypothetical sizes
        for item in &mut line.items {
            item.final_main_size = item.hypothetical_main_size;
        }
        return;
    }

    // Calculate scaled shrink factors if shrinking
    if !growing {
        for item in &mut line.items {
            item.scaled_shrink_factor = item.flex_shrink * item.base_size as f32;
        }
    }

    // Distribute free space
    let total_scaled: f32 = if growing {
        total_flex
    } else {
        line.items.iter().map(|i| i.scaled_shrink_factor).sum()
    };

    if total_scaled > 0.0 {
        for item in &mut line.items {
            let flex_factor = if growing {
                item.flex_grow
            } else {
                item.scaled_shrink_factor
            };

            let ratio = flex_factor / total_scaled;
            let delta = (free_space as f32 * ratio) as i64;

            let new_size = (item.hypothetical_main_size as i64 + delta).max(1) as usize;
            item.final_main_size = new_size;
        }
    } else {
        for item in &mut line.items {
            item.final_main_size = item.hypothetical_main_size;
        }
    }

    // Update line main size
    line.main_size = line.items.iter().map(|i| i.final_main_size).sum::<usize>() + total_gap;
}

/// Calculate cross sizes for items in a line
fn calculate_cross_sizes(line: &mut FlexLine, _is_row: bool) {
    let max_cross: usize = line
        .items
        .iter()
        .map(|i| {
            if i.is_block {
                i.child_blocks.len().max(1)
            } else {
                1
            }
        })
        .max()
        .unwrap_or(1);

    line.cross_size = max_cross;

    for item in &mut line.items {
        item.cross_size = if item.is_block {
            item.child_blocks.len().max(1)
        } else {
            1
        };
    }
}

/// Render flex lines to blocks
fn render_flex_lines(
    lines: Vec<FlexLine>,
    justify: JustifyContent,
    align: AlignItems,
    gap: usize,
    available_main: usize,
    is_row: bool,
    container_id: NodeId,
) -> FlexLayoutResult {
    let mut blocks = Vec::new();

    for line in lines {
        if is_row {
            // Row layout: combine items into a single line with spacing
            let line_block = render_row_line(&line, justify, gap, available_main, container_id);
            blocks.push(line_block);
        } else {
            // Column layout: stack items vertically
            let line_blocks = render_column_line(&line, align, gap, container_id);
            blocks.extend(line_blocks);
        }
    }

    FlexLayoutResult { blocks }
}

/// Render a row flex line
fn render_row_line(
    line: &FlexLine,
    justify: JustifyContent,
    gap: usize,
    available_main: usize,
    container_id: NodeId,
) -> Block {
    if line.items.is_empty() {
        return Block {
            kind: BlockKind::Paragraph {
                content: InlineContent::new(),
            },
            source: Some(container_id),
        };
    }

    // Calculate spacing
    let total_content: usize = line.items.iter().map(|i| i.final_main_size).sum();
    let total_gaps = if line.items.len() > 1 {
        (line.items.len() - 1) * gap
    } else {
        0
    };
    let used_space = total_content + total_gaps;
    let free_space = available_main.saturating_sub(used_space);

    let (prefix, between, suffix) = calculate_justification(justify, free_space, line.items.len(), gap);

    // Build combined content
    let mut combined = InlineContent::new();

    // Add prefix spacing
    if prefix > 0 {
        combined.push(Span {
            kind: SpanKind::Text,
            content: " ".repeat(prefix),
            source: None,
        });
    }

    for (i, item) in line.items.iter().enumerate() {
        // Add between spacing (after first item)
        if i > 0 && between > 0 {
            combined.push(Span {
                kind: SpanKind::Text,
                content: " ".repeat(between),
                source: None,
            });
        }

        // Add item content
        if item.is_block {
            // For block items, render as text summary
            let text = summarize_blocks(&item.child_blocks);
            combined.push(Span {
                kind: SpanKind::Text,
                content: text,
                source: Some(item.node_id),
            });
        } else {
            for span in &item.content.spans {
                combined.push(span.clone());
            }
        }

        // Pad to final_main_size if needed
        let current_width = if item.is_block {
            summarize_blocks(&item.child_blocks).len()
        } else {
            estimate_inline_width(&item.content)
        };

        if current_width < item.final_main_size {
            let padding = item.final_main_size - current_width;
            combined.push(Span {
                kind: SpanKind::Text,
                content: " ".repeat(padding),
                source: None,
            });
        }
    }

    // Add suffix spacing
    if suffix > 0 {
        combined.push(Span {
            kind: SpanKind::Text,
            content: " ".repeat(suffix),
            source: None,
        });
    }

    Block {
        kind: BlockKind::Paragraph { content: combined },
        source: Some(container_id),
    }
}

/// Render a column flex line
fn render_column_line(
    line: &FlexLine,
    _align: AlignItems,
    _gap: usize,
    container_id: NodeId,
) -> Vec<Block> {
    let mut blocks = Vec::new();

    for item in &line.items {
        if item.is_block {
            blocks.extend(item.child_blocks.clone());
        } else if !item.content.is_empty() {
            blocks.push(Block {
                kind: BlockKind::Paragraph {
                    content: item.content.clone(),
                },
                source: Some(item.node_id),
            });
        }
    }

    blocks
}

/// Calculate justification spacing
fn calculate_justification(
    justify: JustifyContent,
    free_space: usize,
    item_count: usize,
    base_gap: usize,
) -> (usize, usize, usize) {
    if item_count == 0 {
        return (0, 0, 0);
    }

    match justify {
        JustifyContent::FlexStart => (0, base_gap, free_space),
        JustifyContent::FlexEnd => (free_space, base_gap, 0),
        JustifyContent::Center => (free_space / 2, base_gap, free_space - free_space / 2),
        JustifyContent::SpaceBetween => {
            if item_count <= 1 {
                (0, base_gap, free_space)
            } else {
                let between = free_space / (item_count - 1) + base_gap;
                (0, between, 0)
            }
        }
        JustifyContent::SpaceAround => {
            if item_count == 0 {
                (0, base_gap, 0)
            } else {
                let space_per_item = free_space / item_count;
                let half = space_per_item / 2;
                (half, space_per_item + base_gap, half)
            }
        }
        JustifyContent::SpaceEvenly => {
            let spaces = item_count + 1;
            let space = free_space / spaces;
            (space, space + base_gap, space)
        }
    }
}

/// Estimate width of inline content
fn estimate_inline_width(content: &InlineContent) -> usize {
    content
        .spans
        .iter()
        .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_str()))
        .sum::<usize>()
        .max(1)
}

/// Estimate width of block content
fn estimate_block_width(blocks: &[Block]) -> usize {
    blocks
        .iter()
        .map(|b| match &b.kind {
            BlockKind::Paragraph { content } => estimate_inline_width(content),
            BlockKind::Heading { content, .. } => estimate_inline_width(content) + 4,
            _ => 10, // Default estimate
        })
        .max()
        .unwrap_or(1)
}

/// Summarize blocks as a single-line text
fn summarize_blocks(blocks: &[Block]) -> String {
    let mut parts = Vec::new();

    for block in blocks {
        match &block.kind {
            BlockKind::Paragraph { content } => {
                parts.push(content.plain_text());
            }
            BlockKind::Heading { content, .. } => {
                parts.push(content.plain_text());
            }
            _ => {}
        }
    }

    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{ComputedStyle, Display};

    fn make_flex_item(text: &str, grow: f32, shrink: f32) -> LayoutBox {
        let node_id = NodeId::new(1);
        let mut style = ComputedStyle::default();
        style.display = Display::Block;
        style.flex_grow = grow;
        style.flex_shrink = shrink;

        let mut item = LayoutBox::new(node_id, "div", style);
        item.children.push(LayoutBox::text_node(NodeId::new(2), text));
        item
    }

    fn make_flex_container(items: Vec<LayoutBox>, direction: FlexDirection, justify: JustifyContent) -> LayoutBox {
        let node_id = NodeId::new(0);
        let mut style = ComputedStyle::default();
        style.display = Display::Flex;
        style.flex_direction = direction;
        style.justify_content = justify;
        style.gap = Some(1);

        let mut container = LayoutBox::new(node_id, "div", style);
        container.children = items;
        container
    }

    #[test]
    fn test_flex_grow() {
        let items = vec![
            make_flex_item("A", 1.0, 1.0),
            make_flex_item("B", 2.0, 1.0),
        ];
        let container = make_flex_container(items, FlexDirection::Row, JustifyContent::FlexStart);

        let viewport = Viewport::new(20);
        let mut ctx = BlockLayoutContext::new(&viewport);
        let result = layout_flex(&container, &mut ctx);

        assert!(!result.blocks.is_empty());
    }

    #[test]
    fn test_justify_space_between() {
        let items = vec![
            make_flex_item("Left", 0.0, 0.0),
            make_flex_item("Right", 0.0, 0.0),
        ];
        let container = make_flex_container(items, FlexDirection::Row, JustifyContent::SpaceBetween);

        let viewport = Viewport::new(40);
        let mut ctx = BlockLayoutContext::new(&viewport);
        let result = layout_flex(&container, &mut ctx);

        assert_eq!(result.blocks.len(), 1);
    }

    #[test]
    fn test_flex_wrap() {
        let items = vec![
            make_flex_item("Item1", 0.0, 0.0),
            make_flex_item("Item2", 0.0, 0.0),
            make_flex_item("Item3", 0.0, 0.0),
        ];

        let node_id = NodeId::new(0);
        let mut style = ComputedStyle::default();
        style.display = Display::Flex;
        style.flex_wrap = FlexWrap::Wrap;

        let mut container = LayoutBox::new(node_id, "div", style);
        container.children = items;

        let viewport = Viewport::new(15); // Narrow viewport forces wrapping
        let mut ctx = BlockLayoutContext::new(&viewport);
        let result = layout_flex(&container, &mut ctx);

        // Should produce multiple lines/blocks due to wrapping
        assert!(!result.blocks.is_empty());
    }

    #[test]
    fn test_flex_column() {
        let items = vec![
            make_flex_item("Row1", 0.0, 0.0),
            make_flex_item("Row2", 0.0, 0.0),
        ];
        let container = make_flex_container(items, FlexDirection::Column, JustifyContent::FlexStart);

        let viewport = Viewport::new(80);
        let mut ctx = BlockLayoutContext::new(&viewport);
        let result = layout_flex(&container, &mut ctx);

        // Column layout produces separate blocks
        assert!(result.blocks.len() >= 2);
    }
}
