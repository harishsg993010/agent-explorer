//! Taffy-based layout engine for CSS Flexbox and Grid.
//!
//! Uses the Taffy library to compute accurate CSS layout, then converts
//! the results to our Markdown-compatible block structure.

use super::block::{collect_inline_content, layout_block, BlockLayoutContext};
use super::float::FloatContext;
use super::tree::LayoutBox;
use super::{ComputedStyle, Display, FlexDirection, FlexWrap, JustifyContent, AlignItems};
use crate::ast::{Block, BlockKind, InlineContent, Span, SpanKind};
use crate::ids::NodeId;
use std::collections::HashMap;
use taffy::prelude::*;
use taffy::MinMax;

/// Result of Taffy-based layout
#[derive(Debug)]
pub struct TaffyLayoutResult {
    pub blocks: Vec<Block>,
}

/// Layout a flex or grid container using Taffy
pub fn layout_with_taffy(
    layout_box: &LayoutBox,
    ctx: &mut BlockLayoutContext,
) -> TaffyLayoutResult {
    let mut taffy = TaffyTree::new();
    let mut node_map: HashMap<NodeId, taffy::NodeId> = HashMap::new();
    let mut content_map: HashMap<taffy::NodeId, LayoutContent> = HashMap::new();

    // Build the Taffy tree
    let root_node = build_taffy_node(
        layout_box,
        &mut taffy,
        &mut node_map,
        &mut content_map,
        ctx,
    );

    // Set available width constraint
    let available_width = ctx.available_width as f32;

    // Compute layout
    let _ = taffy.compute_layout(
        root_node,
        Size {
            width: AvailableSpace::Definite(available_width),
            height: AvailableSpace::MaxContent,
        },
    );

    // Convert Taffy layout to blocks
    convert_taffy_to_blocks(
        root_node,
        &taffy,
        &content_map,
        layout_box.node_id,
    )
}

/// Content stored for each Taffy node
#[derive(Debug, Clone)]
enum LayoutContent {
    /// Text/inline content
    Inline(InlineContent, NodeId),
    /// Block content (already laid out)
    Block(Vec<Block>, NodeId),
    /// Container (flex or grid)
    Container(NodeId),
}

/// Build a Taffy node tree from our LayoutBox
fn build_taffy_node(
    layout_box: &LayoutBox,
    taffy: &mut TaffyTree,
    node_map: &mut HashMap<NodeId, taffy::NodeId>,
    content_map: &mut HashMap<taffy::NodeId, LayoutContent>,
    ctx: &BlockLayoutContext,
) -> taffy::NodeId {
    let style = convert_style(&layout_box.style);

    // Process children
    let mut child_nodes = Vec::new();

    for child in &layout_box.children {
        if !child.style.is_visible() {
            continue;
        }

        let child_node = if child.style.display == Display::Flex || child.style.display == Display::Grid {
            // Nested flex/grid container
            build_taffy_node(child, taffy, node_map, content_map, ctx)
        } else if child.is_block() && !child.children.is_empty() {
            // Block-level child
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

            // Calculate content size
            let content_width = estimate_block_width(&blocks);
            let content_height = blocks.len().max(1);

            let child_style = Style {
                size: Size {
                    width: Dimension::Length(content_width as f32),
                    height: Dimension::Length(content_height as f32),
                },
                ..convert_style(&child.style)
            };

            let node = taffy.new_leaf(child_style).unwrap();
            content_map.insert(node, LayoutContent::Block(blocks, child.node_id));
            node
        } else {
            // Inline content
            let content = collect_inline_content(child);
            let content_width = estimate_inline_width(&content);

            let child_style = Style {
                size: Size {
                    width: Dimension::Length(content_width as f32),
                    height: Dimension::Length(1.0),
                },
                min_size: Size {
                    width: Dimension::Length(1.0),
                    height: Dimension::Length(1.0),
                },
                ..convert_style(&child.style)
            };

            let node = taffy.new_leaf(child_style).unwrap();
            content_map.insert(node, LayoutContent::Inline(content, child.node_id));
            node
        };

        child_nodes.push(child_node);
        node_map.insert(child.node_id, child_node);
    }

    // Create container node
    let container_node = taffy.new_with_children(style, &child_nodes).unwrap();
    content_map.insert(container_node, LayoutContent::Container(layout_box.node_id));
    node_map.insert(layout_box.node_id, container_node);

    container_node
}

/// Convert our ComputedStyle to Taffy Style
fn convert_style(style: &ComputedStyle) -> Style {
    let mut taffy_style = Style::default();

    // Display and layout mode
    match style.display {
        Display::Flex => {
            taffy_style.display = taffy::Display::Flex;
        }
        Display::Grid => {
            taffy_style.display = taffy::Display::Grid;
        }
        Display::Block | Display::Inline | Display::InlineBlock => {
            taffy_style.display = taffy::Display::Block;
        }
        Display::None => {
            taffy_style.display = taffy::Display::None;
        }
        _ => {
            taffy_style.display = taffy::Display::Block;
        }
    }

    // Flex properties
    taffy_style.flex_direction = match style.flex_direction {
        FlexDirection::Row => taffy::FlexDirection::Row,
        FlexDirection::RowReverse => taffy::FlexDirection::RowReverse,
        FlexDirection::Column => taffy::FlexDirection::Column,
        FlexDirection::ColumnReverse => taffy::FlexDirection::ColumnReverse,
    };

    taffy_style.flex_wrap = match style.flex_wrap {
        FlexWrap::NoWrap => taffy::FlexWrap::NoWrap,
        FlexWrap::Wrap => taffy::FlexWrap::Wrap,
        FlexWrap::WrapReverse => taffy::FlexWrap::WrapReverse,
    };

    taffy_style.justify_content = Some(match style.justify_content {
        JustifyContent::FlexStart => taffy::JustifyContent::FlexStart,
        JustifyContent::FlexEnd => taffy::JustifyContent::FlexEnd,
        JustifyContent::Center => taffy::JustifyContent::Center,
        JustifyContent::SpaceBetween => taffy::JustifyContent::SpaceBetween,
        JustifyContent::SpaceAround => taffy::JustifyContent::SpaceAround,
        JustifyContent::SpaceEvenly => taffy::JustifyContent::SpaceEvenly,
    });

    taffy_style.align_items = Some(match style.align_items {
        AlignItems::FlexStart => taffy::AlignItems::FlexStart,
        AlignItems::FlexEnd => taffy::AlignItems::FlexEnd,
        AlignItems::Center => taffy::AlignItems::Center,
        AlignItems::Baseline => taffy::AlignItems::Baseline,
        AlignItems::Stretch => taffy::AlignItems::Stretch,
    });

    // Flex item properties
    taffy_style.flex_grow = style.flex_grow;
    taffy_style.flex_shrink = style.flex_shrink;

    if let Some(basis) = style.flex_basis {
        taffy_style.flex_basis = Dimension::Length(basis as f32);
    }

    // Gap
    if let Some(gap) = style.gap {
        taffy_style.gap = Size {
            width: LengthPercentage::Length(gap as f32),
            height: LengthPercentage::Length(gap as f32),
        };
    }

    // Grid properties
    if !style.grid_template_columns.is_empty() {
        taffy_style.grid_template_columns = style
            .grid_template_columns
            .iter()
            .map(|&size| convert_grid_track(size))
            .collect();
    }

    if !style.grid_template_rows.is_empty() {
        taffy_style.grid_template_rows = style
            .grid_template_rows
            .iter()
            .map(|&size| convert_grid_track(size))
            .collect();
    }

    // Grid item placement
    if let Some(col_start) = style.grid_column_start {
        taffy_style.grid_column = Line {
            start: GridPlacement::Line((col_start as i16).into()),
            end: GridPlacement::Auto,
        };
    }

    if let Some(row_start) = style.grid_row_start {
        taffy_style.grid_row = Line {
            start: GridPlacement::Line((row_start as i16).into()),
            end: GridPlacement::Auto,
        };
    }

    // Size constraints
    if let Some(width) = style.width {
        taffy_style.size.width = Dimension::Length(width as f32);
    }

    if let Some(min_width) = style.min_width {
        taffy_style.min_size.width = Dimension::Length(min_width as f32);
    }

    if let Some(max_width) = style.max_width {
        taffy_style.max_size.width = Dimension::Length(max_width as f32);
    }

    taffy_style
}

/// Convert a grid track size
fn convert_grid_track(size: GridTrackSize) -> TrackSizingFunction {
    match size {
        GridTrackSize::Fixed(px) => {
            TrackSizingFunction::Single(NonRepeatedTrackSizingFunction::from(
                MinMax {
                    min: MinTrackSizingFunction::Fixed(LengthPercentage::Length(px as f32)),
                    max: MaxTrackSizingFunction::Fixed(LengthPercentage::Length(px as f32)),
                }
            ))
        }
        GridTrackSize::Fr(fr) => {
            TrackSizingFunction::Single(NonRepeatedTrackSizingFunction::from(
                MinMax {
                    min: MinTrackSizingFunction::Fixed(LengthPercentage::Length(0.0)),
                    max: MaxTrackSizingFunction::Fraction(fr),
                }
            ))
        }
        GridTrackSize::MinContent => {
            TrackSizingFunction::Single(NonRepeatedTrackSizingFunction::from(
                MinMax {
                    min: MinTrackSizingFunction::MinContent,
                    max: MaxTrackSizingFunction::MinContent,
                }
            ))
        }
        GridTrackSize::MaxContent => {
            TrackSizingFunction::Single(NonRepeatedTrackSizingFunction::from(
                MinMax {
                    min: MinTrackSizingFunction::MaxContent,
                    max: MaxTrackSizingFunction::MaxContent,
                }
            ))
        }
        GridTrackSize::Auto => {
            TrackSizingFunction::Single(NonRepeatedTrackSizingFunction::from(
                MinMax {
                    min: MinTrackSizingFunction::Auto,
                    max: MaxTrackSizingFunction::Auto,
                }
            ))
        }
    }
}

/// Convert Taffy layout results to blocks
fn convert_taffy_to_blocks(
    root_node: taffy::NodeId,
    taffy: &TaffyTree,
    content_map: &HashMap<taffy::NodeId, LayoutContent>,
    container_id: NodeId,
) -> TaffyLayoutResult {
    let root_layout = taffy.layout(root_node).unwrap();
    let is_row = is_row_direction(taffy, root_node);

    // Get all children with their layouts
    let children = taffy.children(root_node).unwrap();
    let mut positioned_items: Vec<(taffy::NodeId, Layout)> = children
        .iter()
        .map(|&child| (child, *taffy.layout(child).unwrap()))
        .collect();

    // Sort by position (y for column, x for row)
    if is_row {
        positioned_items.sort_by(|a, b| {
            a.1.location.y.partial_cmp(&b.1.location.y)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    a.1.location.x.partial_cmp(&b.1.location.x)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
    } else {
        positioned_items.sort_by(|a, b| {
            a.1.location.y.partial_cmp(&b.1.location.y)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // Group items by row (for row layouts) or render sequentially (for column)
    let blocks = if is_row {
        render_row_layout(&positioned_items, content_map, container_id)
    } else {
        render_column_layout(&positioned_items, content_map, container_id)
    };

    TaffyLayoutResult { blocks }
}

/// Check if the container uses row direction
fn is_row_direction(taffy: &TaffyTree, node: taffy::NodeId) -> bool {
    let style = taffy.style(node).unwrap();
    matches!(
        style.flex_direction,
        taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse
    ) || style.display == taffy::Display::Grid
}

/// Render items in row layout (combine horizontally)
fn render_row_layout(
    items: &[(taffy::NodeId, Layout)],
    content_map: &HashMap<taffy::NodeId, LayoutContent>,
    container_id: NodeId,
) -> Vec<Block> {
    if items.is_empty() {
        return Vec::new();
    }

    // Group items by y position (row)
    let mut rows: Vec<Vec<(taffy::NodeId, Layout)>> = Vec::new();
    let mut current_row: Vec<(taffy::NodeId, Layout)> = Vec::new();
    let mut current_y = items[0].1.location.y;
    let tolerance = 5.0; // Allow small y differences

    for (node, layout) in items {
        if (layout.location.y - current_y).abs() > tolerance && !current_row.is_empty() {
            rows.push(std::mem::take(&mut current_row));
            current_y = layout.location.y;
        }
        current_row.push((*node, *layout));
    }
    if !current_row.is_empty() {
        rows.push(current_row);
    }

    // Render each row
    let mut blocks = Vec::new();

    for row in rows {
        // Sort row items by x position
        let mut row_items = row;
        row_items.sort_by(|a, b| {
            a.1.location.x.partial_cmp(&b.1.location.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut combined = InlineContent::new();
        let mut prev_end = 0.0f32;

        for (node, layout) in &row_items {
            // Add spacing based on gap
            let gap = (layout.location.x - prev_end).max(0.0) as usize;
            if gap > 0 && !combined.is_empty() {
                combined.push(Span {
                    kind: SpanKind::Text,
                    content: " ".repeat(gap.min(10)), // Cap at 10 spaces
                    source: None,
                });
            }

            // Add content
            if let Some(content) = content_map.get(node) {
                match content {
                    LayoutContent::Inline(inline, source) => {
                        for span in &inline.spans {
                            combined.push(span.clone());
                        }
                    }
                    LayoutContent::Block(blks, source) => {
                        // Summarize block content for row
                        let text = summarize_blocks(blks);
                        if !text.is_empty() {
                            combined.push(Span {
                                kind: SpanKind::Text,
                                content: text,
                                source: Some(*source),
                            });
                        }
                    }
                    LayoutContent::Container(_) => {
                        // Nested container - would need recursive handling
                    }
                }
            }

            prev_end = layout.location.x + layout.size.width;
        }

        if !combined.is_empty() {
            blocks.push(Block {
                kind: BlockKind::Paragraph { content: combined },
                source: Some(container_id),
            });
        }
    }

    blocks
}

/// Render items in column layout (stack vertically)
fn render_column_layout(
    items: &[(taffy::NodeId, Layout)],
    content_map: &HashMap<taffy::NodeId, LayoutContent>,
    container_id: NodeId,
) -> Vec<Block> {
    let mut blocks = Vec::new();

    for (node, _layout) in items {
        if let Some(content) = content_map.get(node) {
            match content {
                LayoutContent::Inline(inline, source) => {
                    if !inline.is_empty() {
                        blocks.push(Block {
                            kind: BlockKind::Paragraph {
                                content: inline.clone(),
                            },
                            source: Some(*source),
                        });
                    }
                }
                LayoutContent::Block(blks, _) => {
                    blocks.extend(blks.clone());
                }
                LayoutContent::Container(_) => {
                    // Nested container
                }
            }
        }
    }

    blocks
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
            _ => 10,
        })
        .max()
        .unwrap_or(1)
}

/// Summarize blocks as single-line text
fn summarize_blocks(blocks: &[Block]) -> String {
    let mut parts = Vec::new();

    for block in blocks {
        match &block.kind {
            BlockKind::Paragraph { content } => {
                let text = content.plain_text();
                if !text.is_empty() {
                    parts.push(text);
                }
            }
            BlockKind::Heading { content, .. } => {
                let text = content.plain_text();
                if !text.is_empty() {
                    parts.push(text);
                }
            }
            _ => {}
        }
    }

    parts.join(" ")
}

/// Grid track size representation
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridTrackSize {
    /// Fixed size in characters
    Fixed(usize),
    /// Fractional unit
    Fr(f32),
    /// Min-content
    MinContent,
    /// Max-content
    MaxContent,
    /// Auto
    Auto,
}

impl Default for GridTrackSize {
    fn default() -> Self {
        GridTrackSize::Auto
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_taffy_flex_layout() {
        let mut style = ComputedStyle::default();
        style.display = Display::Flex;
        style.flex_direction = FlexDirection::Row;

        let taffy_style = convert_style(&style);
        assert_eq!(taffy_style.display, taffy::Display::Flex);
        assert_eq!(taffy_style.flex_direction, taffy::FlexDirection::Row);
    }

    #[test]
    fn test_taffy_grid_layout() {
        let mut style = ComputedStyle::default();
        style.display = Display::Grid;
        style.grid_template_columns = vec![
            GridTrackSize::Fr(1.0),
            GridTrackSize::Fr(1.0),
        ];

        let taffy_style = convert_style(&style);
        assert_eq!(taffy_style.display, taffy::Display::Grid);
        assert_eq!(taffy_style.grid_template_columns.len(), 2);
    }
}
