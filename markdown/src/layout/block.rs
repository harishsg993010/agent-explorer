//! Block layout with margin collapsing.
//!
//! Handles block-level layout including:
//! - Vertical stacking of blocks
//! - Margin collapsing between adjacent blocks
//! - Padding and border handling
//! - Block-level semantic elements (headings, paragraphs, lists, etc.)

use super::float::FloatContext;
use super::taffy_layout::layout_with_taffy;
use super::tree::LayoutBox;
use super::{Clear, ContainerType, Display, Float, Viewport};
use crate::ast::{
    Alignment, Block, BlockKind, InlineContent, ListItem, Span, SpanKind, TableCell,
};

/// Information about a container for container queries
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    /// Container name (if any)
    pub name: Option<String>,
    /// Container type
    pub container_type: ContainerType,
    /// Container width in characters
    pub width: usize,
    /// Container height (estimated in lines, 0 if unknown)
    pub height: usize,
}

/// CSS Counter context for tracking counter values
#[derive(Debug, Clone, Default)]
pub struct CounterContext {
    /// Counter values keyed by counter name
    pub counters: std::collections::HashMap<String, Vec<i32>>,
}

impl CounterContext {
    pub fn new() -> Self {
        CounterContext {
            counters: std::collections::HashMap::new(),
        }
    }

    /// Reset a counter (creates new scope if already exists)
    pub fn reset(&mut self, name: &str, value: i32) {
        self.counters
            .entry(name.to_string())
            .or_insert_with(Vec::new)
            .push(value);
    }

    /// Increment a counter by the given amount
    pub fn increment(&mut self, name: &str, amount: i32) {
        if let Some(stack) = self.counters.get_mut(name) {
            if let Some(last) = stack.last_mut() {
                *last += amount;
            }
        } else {
            // Auto-create counter if not exists
            self.counters.insert(name.to_string(), vec![amount]);
        }
    }

    /// Get the current value of a counter
    pub fn get(&self, name: &str) -> i32 {
        self.counters
            .get(name)
            .and_then(|stack| stack.last().copied())
            .unwrap_or(0)
    }

    /// Get all values for counter() with style (for counters() function)
    pub fn get_all(&self, name: &str) -> Vec<i32> {
        self.counters
            .get(name)
            .cloned()
            .unwrap_or_default()
    }

    /// Exit a counter scope (pop the last value)
    pub fn exit_scope(&mut self, name: &str) {
        if let Some(stack) = self.counters.get_mut(name) {
            if stack.len() > 1 {
                stack.pop();
            }
        }
    }
}

/// Block layout context
pub struct BlockLayoutContext<'a> {
    pub viewport: &'a Viewport,
    /// Available width for content
    pub available_width: usize,
    /// Current indent level
    pub indent: usize,
    /// Previous bottom margin for collapsing
    pub prev_margin_bottom: i32,
    /// Whether we're inside a list
    pub in_list: bool,
    /// Current list depth
    pub list_depth: usize,
    /// Whether we're inside a blockquote
    pub in_blockquote: bool,
    /// Blockquote depth
    pub blockquote_depth: usize,
    /// Stack of container query containers
    pub container_stack: Vec<ContainerInfo>,
    /// Float context for tracking floated elements
    pub float_context: FloatContext,
    /// Current y position (line number) for float tracking
    pub current_y: usize,
    /// CSS Counter context
    pub counter_context: CounterContext,
}

impl<'a> BlockLayoutContext<'a> {
    pub fn new(viewport: &'a Viewport) -> Self {
        BlockLayoutContext {
            viewport,
            available_width: viewport.width,
            indent: 0,
            prev_margin_bottom: 0,
            in_list: false,
            list_depth: 0,
            in_blockquote: false,
            blockquote_depth: 0,
            container_stack: Vec::new(),
            float_context: FloatContext::new(viewport.width),
            current_y: 0,
            counter_context: CounterContext::new(),
        }
    }

    /// Process counter-reset and counter-increment for an element
    pub fn process_counters(&mut self, style: &super::ComputedStyle) {
        // Process counter-reset
        for (name, value) in &style.counter_reset {
            self.counter_context.reset(name, *value);
        }
        // Process counter-increment
        for (name, amount) in &style.counter_increment {
            self.counter_context.increment(name, *amount);
        }
    }

    /// Get the current value of a counter
    pub fn get_counter(&self, name: &str) -> i32 {
        self.counter_context.get(name)
    }

    pub fn with_indent(&self, additional: usize) -> Self {
        let new_width = self.available_width.saturating_sub(additional);
        BlockLayoutContext {
            viewport: self.viewport,
            available_width: new_width,
            indent: self.indent + additional,
            prev_margin_bottom: 0,
            in_list: self.in_list,
            list_depth: self.list_depth,
            in_blockquote: self.in_blockquote,
            blockquote_depth: self.blockquote_depth,
            container_stack: self.container_stack.clone(),
            float_context: FloatContext::new(new_width),
            current_y: self.current_y,
            counter_context: self.counter_context.clone(),
        }
    }

    pub fn enter_list(&self) -> Self {
        let new_width = self.available_width.saturating_sub(2);
        BlockLayoutContext {
            viewport: self.viewport,
            available_width: new_width,
            indent: self.indent + 2,
            prev_margin_bottom: 0,
            in_list: true,
            list_depth: self.list_depth + 1,
            in_blockquote: self.in_blockquote,
            blockquote_depth: self.blockquote_depth,
            container_stack: self.container_stack.clone(),
            float_context: FloatContext::new(new_width),
            current_y: self.current_y,
            counter_context: self.counter_context.clone(),
        }
    }

    /// Push a container onto the container stack
    pub fn push_container(&mut self, name: Option<String>, container_type: ContainerType, width: usize) {
        self.container_stack.push(ContainerInfo {
            name,
            container_type,
            width,
            height: 0, // Height not known until after layout
        });
    }

    /// Pop a container from the stack
    pub fn pop_container(&mut self) {
        self.container_stack.pop();
    }

    /// Find the nearest container that matches a query name
    pub fn find_container(&self, name: Option<&str>) -> Option<&ContainerInfo> {
        if let Some(target_name) = name {
            // Find by name
            self.container_stack.iter().rev().find(|c| {
                c.container_type.is_container() &&
                c.name.as_deref() == Some(target_name)
            })
        } else {
            // Find nearest container
            self.container_stack.iter().rev().find(|c| c.container_type.is_container())
        }
    }

    /// Get the current container width (for container queries)
    pub fn current_container_width(&self) -> usize {
        self.container_stack.last()
            .filter(|c| c.container_type.is_container())
            .map(|c| c.width)
            .unwrap_or(self.viewport.width)
    }

    pub fn enter_blockquote(&self) -> Self {
        let new_width = self.available_width.saturating_sub(2);
        BlockLayoutContext {
            viewport: self.viewport,
            available_width: new_width,
            indent: self.indent + 2,
            prev_margin_bottom: 0,
            in_list: self.in_list,
            list_depth: self.list_depth,
            in_blockquote: true,
            blockquote_depth: self.blockquote_depth + 1,
            container_stack: self.container_stack.clone(),
            float_context: FloatContext::new(new_width),
            current_y: self.current_y,
            counter_context: self.counter_context.clone(),
        }
    }

    /// Get available width at current y position, accounting for floats
    pub fn available_width_with_floats(&self) -> usize {
        self.float_context.available_width_at(self.current_y, 1)
    }

    /// Apply clear property and return new y position
    pub fn apply_clear(&mut self, clear: Clear) {
        let new_y = self.float_context.clear_y(clear, self.current_y);
        self.current_y = new_y;
    }

    /// Advance y position by a number of lines
    pub fn advance_y(&mut self, lines: usize) {
        self.current_y += lines;
    }
}

/// Compute collapsed margin between two elements
pub fn collapse_margin(margin_a: i32, margin_b: i32) -> i32 {
    if margin_a >= 0 && margin_b >= 0 {
        // Both positive: take the larger
        margin_a.max(margin_b)
    } else if margin_a < 0 && margin_b < 0 {
        // Both negative: take the more negative
        margin_a.min(margin_b)
    } else {
        // Mixed: add them together
        margin_a + margin_b
    }
}

/// Layout a block-level element
pub fn layout_block(layout_box: &LayoutBox, ctx: &mut BlockLayoutContext) -> Vec<Block> {
    let mut blocks = Vec::new();
    let style = &layout_box.style;

    // Handle visibility
    if !style.is_visible() {
        return blocks;
    }

    // Apply clear property - move below any floats if needed
    if style.clear != Clear::None {
        ctx.apply_clear(style.clear);
    }

    // Handle floated elements
    if style.float.is_floated() {
        return layout_floated_element(layout_box, ctx);
    }

    // Track container query containers
    let is_container = style.container_type.is_container();
    if is_container {
        ctx.push_container(
            style.container_name.clone(),
            style.container_type,
            ctx.available_width,
        );
    }

    // Dispatch to Taffy for Flex and Grid containers
    if style.display == Display::Flex || style.display == Display::Grid {
        let result = layout_with_taffy(layout_box, ctx);
        ctx.prev_margin_bottom = style.margin_bottom;
        if is_container {
            ctx.pop_container();
        }
        return result.blocks;
    }

    // Handle multi-column layout
    if style.is_multicol() {
        let result = layout_multicol(layout_box, ctx);
        ctx.prev_margin_bottom = style.margin_bottom;
        if is_container {
            ctx.pop_container();
        }
        return result;
    }

    // Calculate collapsed margin
    let collapsed = collapse_margin(ctx.prev_margin_bottom, style.margin_top);
    if collapsed > 0 {
        blocks.push(Block {
            kind: BlockKind::BlankLines {
                count: (collapsed as usize).min(2),
            },
            source: Some(layout_box.node_id),
        });
        ctx.advance_y(collapsed as usize);
    }

    // Handle specific block types
    match layout_box.tag.as_str() {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            if let Some(level) = layout_box.is_heading() {
                let content = collect_inline_content(layout_box);
                // Skip empty headings or headings with just UI labels
                let text = content.plain_text();
                // Remove all whitespace including non-breaking spaces
                let text_clean: String = text.chars()
                    .filter(|c| !c.is_whitespace() && *c != '\u{00A0}')
                    .collect();
                let text_lower = text_clean.to_lowercase();
                if content.is_empty()
                    || text_clean.is_empty()
                    || text_lower == "tooltip"
                    || text_lower == "menu"
                    || text_lower == "popover"
                    || text_lower == "navigation"
                    || text_lower == "nav"
                    || text_lower == "sidebar"
                    || text_lower == "close"
                    || text_lower == "search"
                    || text_clean.len() <= 1
                {
                    // Skip this heading
                } else {
                    let capped_level = level.min(ctx.viewport.max_heading_depth as u8);
                    blocks.push(Block {
                        kind: BlockKind::Heading {
                            level: capped_level,
                            content,
                        },
                        source: Some(layout_box.node_id),
                    });
                }
            }
        }

        "p" => {
            let content = collect_inline_content(layout_box);
            if !content.is_empty() {
                blocks.push(Block {
                    kind: BlockKind::Paragraph { content },
                    source: Some(layout_box.node_id),
                });
            }
        }

        "pre" => {
            let (language, code) = collect_code_content(layout_box);
            blocks.push(Block {
                kind: BlockKind::CodeBlock { language, code },
                source: Some(layout_box.node_id),
            });
        }

        "blockquote" => {
            let inner_ctx = ctx.enter_blockquote();
            let inner_blocks = layout_children(layout_box, &inner_ctx);
            blocks.push(Block {
                kind: BlockKind::Blockquote {
                    blocks: inner_blocks,
                },
                source: Some(layout_box.node_id),
            });
        }

        "ul" | "menu" => {
            let items = layout_list_items(layout_box, ctx, false);
            blocks.push(Block {
                kind: BlockKind::UnorderedList { items },
                source: Some(layout_box.node_id),
            });
        }

        "ol" => {
            let start = layout_box
                .attrs
                .get("start")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
            let items = layout_list_items(layout_box, ctx, true);
            blocks.push(Block {
                kind: BlockKind::OrderedList { items, start },
                source: Some(layout_box.node_id),
            });
        }

        "hr" => {
            blocks.push(Block {
                kind: BlockKind::ThematicBreak,
                source: Some(layout_box.node_id),
            });
        }

        "table" => {
            let table_block = layout_table(layout_box, ctx);
            blocks.push(table_block);
        }

        "details" => {
            let (summary, inner_blocks) = layout_details(layout_box, ctx);
            let open = layout_box.attrs.get("open").is_some();
            blocks.push(Block {
                kind: BlockKind::Details {
                    summary,
                    blocks: inner_blocks,
                    open,
                },
                source: Some(layout_box.node_id),
            });
        }

        "br" => {
            // Line break in block context creates a blank line
            blocks.push(Block {
                kind: BlockKind::BlankLines { count: 1 },
                source: Some(layout_box.node_id),
            });
        }

        "div" | "section" | "article" | "main" | "aside" | "nav" | "header" | "footer"
        | "figure" | "figcaption" | "address" | "form" => {
            // Generic container - layout children
            let inner_blocks = layout_children(layout_box, ctx);
            if inner_blocks.len() == 1 {
                // Unwrap single-block containers
                blocks.extend(inner_blocks);
            } else if !inner_blocks.is_empty() {
                blocks.push(Block {
                    kind: BlockKind::Container {
                        blocks: inner_blocks,
                        indent: ctx.indent,
                    },
                    source: Some(layout_box.node_id),
                });
            }
        }

        _ => {
            // Unknown block element - treat as container or paragraph
            if layout_box.children.is_empty() {
                if let Some(text) = &layout_box.text {
                    if !text.trim().is_empty() {
                        blocks.push(Block {
                            kind: BlockKind::Paragraph {
                                content: InlineContent::text(text),
                            },
                            source: Some(layout_box.node_id),
                        });
                    }
                }
            } else {
                let inner_blocks = layout_children(layout_box, ctx);
                blocks.extend(inner_blocks);
            }
        }
    }

    // Update previous margin for next element
    ctx.prev_margin_bottom = style.margin_bottom;

    // Pop container from stack if we pushed one
    if is_container {
        ctx.pop_container();
    }

    blocks
}

/// Layout a floated element.
///
/// Floated elements are taken out of normal flow and positioned to the left
/// or right of their container. In a terminal context, we render them inline
/// with a marker and let subsequent content flow around them.
fn layout_floated_element(layout_box: &LayoutBox, ctx: &mut BlockLayoutContext) -> Vec<Block> {
    let mut blocks = Vec::new();
    let style = &layout_box.style;

    // Estimate the dimensions of the floated content
    let content = collect_inline_content(layout_box);
    let content_text = content.plain_text();
    let float_width = style.width.unwrap_or_else(|| {
        // Estimate width from content, max 40% of available width
        content_text.len().min(ctx.available_width * 2 / 5)
    });
    let float_height = estimate_line_count(&content_text, float_width);

    // Place the float in the float context (position info not used for terminal rendering)
    let (_x, _y) = ctx.float_context.place_float(
        style.float,
        float_width,
        float_height,
        ctx.current_y,
    );

    // For terminal rendering, we'll render the floated content as a block
    // with a visual indicator of its float status
    if !content.is_empty() {
        let float_marker = match style.float {
            Float::Left | Float::InlineStart => "[<< ",
            Float::Right | Float::InlineEnd => "[>> ",
            Float::None => "",
        };
        let float_end = "]";

        // Create a modified content with float markers
        let mut marked_content = InlineContent::new();
        marked_content.push(Span {
            kind: SpanKind::Text,
            content: float_marker.to_string(),
            source: None,
        });
        for span in content.spans {
            marked_content.push(span);
        }
        marked_content.push(Span {
            kind: SpanKind::Text,
            content: float_end.to_string(),
            source: None,
        });

        blocks.push(Block {
            kind: BlockKind::Paragraph { content: marked_content },
            source: Some(layout_box.node_id),
        });
    }

    // Don't advance ctx.current_y since floats don't take up space in normal flow
    // The float context will handle spacing for subsequent content

    blocks
}

/// Estimate the number of lines content will take at a given width
fn estimate_line_count(text: &str, width: usize) -> usize {
    if width == 0 {
        return 1;
    }
    let char_count = text.chars().count();
    (char_count + width - 1) / width.max(1)
}

/// Layout all children of a block
fn layout_children(parent: &LayoutBox, ctx: &BlockLayoutContext) -> Vec<Block> {
    let mut blocks = Vec::new();
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
        float_context: ctx.float_context.clone(),
        current_y: ctx.current_y,
        counter_context: ctx.counter_context.clone(),
    };

    // Check if all children are inline
    if parent.all_children_inline() && !parent.children.is_empty() {
        let content = collect_inline_content(parent);
        if !content.is_empty() {
            blocks.push(Block {
                kind: BlockKind::Paragraph { content },
                source: Some(parent.node_id),
            });
        }
        return blocks;
    }

    for child in &parent.children {
        if child.is_block() {
            let child_blocks = layout_block(child, &mut child_ctx);
            blocks.extend(child_blocks);
        } else {
            // Inline content that wasn't wrapped - create paragraph
            let content = collect_inline_from_box(child);
            if !content.is_empty() {
                blocks.push(Block {
                    kind: BlockKind::Paragraph { content },
                    source: Some(child.node_id),
                });
            }
        }
    }

    blocks
}

/// Layout list items
fn layout_list_items(
    list_box: &LayoutBox,
    ctx: &BlockLayoutContext,
    _ordered: bool,
) -> Vec<ListItem> {
    let mut items = Vec::new();
    let item_ctx = ctx.enter_list();

    for child in &list_box.children {
        if child.tag == "li" || child.style.display == super::Display::ListItem {
            let item_blocks = layout_children(child, &item_ctx);
            let checked = child
                .attrs
                .get("data-checked")
                .map(|v| v == "true" || v == "checked");
            items.push(ListItem {
                blocks: item_blocks,
                source: Some(child.node_id),
                checked,
            });
        }
    }

    items
}

/// Layout a table
fn layout_table(table_box: &LayoutBox, _ctx: &BlockLayoutContext) -> Block {
    let mut headers = Vec::new();
    let mut rows = Vec::new();
    let mut alignments = Vec::new();

    // Find thead, tbody, tfoot
    for child in &table_box.children {
        match child.tag.as_str() {
            "thead" => {
                for row in &child.children {
                    if row.tag == "tr" {
                        for cell in &row.children {
                            if cell.tag == "th" || cell.tag == "td" {
                                let content = collect_inline_content(cell);
                                headers.push(TableCell {
                                    content,
                                    source: Some(cell.node_id),
                                });
                                alignments.push(get_cell_alignment(cell));
                            }
                        }
                    }
                }
            }
            "tbody" | "tfoot" => {
                for row in &child.children {
                    if row.tag == "tr" {
                        let mut row_cells = Vec::new();
                        for cell in &row.children {
                            if cell.tag == "td" || cell.tag == "th" {
                                let content = collect_inline_content(cell);
                                row_cells.push(TableCell {
                                    content,
                                    source: Some(cell.node_id),
                                });
                            }
                        }
                        if !row_cells.is_empty() {
                            rows.push(row_cells);
                        }
                    }
                }
            }
            "tr" => {
                // Direct tr child (no thead/tbody)
                let mut row_cells = Vec::new();
                let mut is_header = false;
                for cell in &child.children {
                    if cell.tag == "th" {
                        is_header = true;
                    }
                    if cell.tag == "td" || cell.tag == "th" {
                        let content = collect_inline_content(cell);
                        row_cells.push(TableCell {
                            content,
                            source: Some(cell.node_id),
                        });
                        if headers.is_empty() {
                            alignments.push(get_cell_alignment(cell));
                        }
                    }
                }
                if !row_cells.is_empty() {
                    if is_header && headers.is_empty() {
                        headers = row_cells;
                    } else {
                        rows.push(row_cells);
                    }
                }
            }
            _ => {}
        }
    }

    Block {
        kind: BlockKind::Table {
            headers,
            rows,
            alignments,
        },
        source: Some(table_box.node_id),
    }
}

/// Get alignment for a table cell
fn get_cell_alignment(cell: &LayoutBox) -> Alignment {
    match cell.style.text_align {
        super::TextAlign::Left | super::TextAlign::Start => Alignment::Left,
        super::TextAlign::Right | super::TextAlign::End => Alignment::Right,
        super::TextAlign::Center => Alignment::Center,
        super::TextAlign::Justify => Alignment::Left,
    }
}

/// Layout details/summary element
fn layout_details(
    details_box: &LayoutBox,
    ctx: &BlockLayoutContext,
) -> (InlineContent, Vec<Block>) {
    let mut summary = InlineContent::text("Details");
    let mut inner_blocks = Vec::new();

    for child in &details_box.children {
        if child.tag == "summary" {
            summary = collect_inline_content(child);
        } else {
            let child_blocks = layout_children(child, ctx);
            inner_blocks.extend(child_blocks);
        }
    }

    (summary, inner_blocks)
}

/// Collect inline content from a layout box and its children
pub fn collect_inline_content(layout_box: &LayoutBox) -> InlineContent {
    let mut content = InlineContent::new();
    collect_inline_recursive(layout_box, &mut content);
    content
}

fn collect_inline_recursive(layout_box: &LayoutBox, content: &mut InlineContent) {
    // Handle text nodes
    if let Some(text) = &layout_box.text {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            // Add space before text if it doesn't start with punctuation
            // and we don't already end with whitespace
            if !content.ends_with_whitespace() {
                let first_char = trimmed.chars().next().unwrap_or(' ');
                if !matches!(first_char, '.' | ',' | '!' | '?' | ':' | ';' | ')' | ']' | '}' | '|') {
                    content.push(Span {
                        kind: SpanKind::Text,
                        content: " ".to_string(),
                        source: None,
                    });
                }
            }
            let kind = determine_span_kind(layout_box);
            content.push(Span {
                kind,
                content: trimmed.to_string(),
                source: Some(layout_box.node_id),
            });
        }
        return;
    }

    // Handle specific inline elements
    match layout_box.tag.as_str() {
        // Block-level elements within inline context should add line breaks
        "div" | "p" | "tr" | "li" | "dd" | "dt" => {
            // Add line break before block content in inline context
            if !content.is_empty() {
                content.push(Span {
                    kind: SpanKind::LineBreak { hard: true },
                    content: String::new(),
                    source: Some(layout_box.node_id),
                });
            }
            for child in &layout_box.children {
                collect_inline_recursive(child, content);
            }
            // Add line break after
            content.push(Span {
                kind: SpanKind::LineBreak { hard: true },
                content: String::new(),
                source: Some(layout_box.node_id),
            });
        }

        // Table cells should add spacing
        "td" | "th" => {
            // Add space before cell content (except first)
            if !content.is_empty() && !content.ends_with_whitespace() {
                content.push(Span {
                    kind: SpanKind::Text,
                    content: " ".to_string(),
                    source: None,
                });
            }
            for child in &layout_box.children {
                collect_inline_recursive(child, content);
            }
        }

        "br" => {
            content.push(Span {
                kind: SpanKind::LineBreak { hard: true },
                content: String::new(),
                source: Some(layout_box.node_id),
            });
        }
        "a" if layout_box.is_link() => {
            let mut link_content = InlineContent::new();
            for child in &layout_box.children {
                collect_inline_recursive(child, &mut link_content);
            }
            let text = link_content.plain_text().trim().to_string();

            // Skip empty links (vote buttons, spacers, etc.)
            if text.is_empty() {
                return;
            }

            // Add space before link if needed
            if !content.ends_with_whitespace() {
                content.push(Span {
                    kind: SpanKind::Text,
                    content: " ".to_string(),
                    source: None,
                });
            }
            let url = layout_box.href().unwrap_or("").to_string();
            let title = layout_box.attrs.get("title").cloned();
            content.push(Span {
                kind: SpanKind::Link {
                    url,
                    title,
                    link_id: crate::ids::LinkId::new(),
                },
                content: text,
                source: Some(layout_box.node_id),
            });
        }
        "img" => {
            let alt = layout_box.alt().unwrap_or("").to_string();
            let url = layout_box.src().unwrap_or("").to_string();

            // Skip spacer images and images with no alt text and trivial src
            if url.ends_with(".gif") && alt.is_empty() {
                return;
            }
            if url.contains("spacer") || url.contains("1x1") || url == "s.gif" {
                return;
            }

            if !alt.is_empty() {
                content.push(Span {
                    kind: SpanKind::Image { url, alt: alt.clone() },
                    content: alt,
                    source: Some(layout_box.node_id),
                });
            }
        }
        "code" | "kbd" | "samp" | "tt" => {
            let mut code_content = InlineContent::new();
            for child in &layout_box.children {
                collect_inline_recursive(child, &mut code_content);
            }
            let text = code_content.plain_text();
            let kind = if layout_box.tag == "kbd" {
                SpanKind::Kbd
            } else {
                SpanKind::Code
            };
            content.push(Span {
                kind,
                content: text,
                source: Some(layout_box.node_id),
            });
        }
        "strong" | "b" => {
            // Add space before bold if needed
            if !content.ends_with_whitespace() {
                content.push(Span {
                    kind: SpanKind::Text,
                    content: " ".to_string(),
                    source: None,
                });
            }
            let mut inner = InlineContent::new();
            for child in &layout_box.children {
                collect_inline_recursive(child, &mut inner);
            }
            for span in inner.spans {
                content.push(Span {
                    kind: if span.kind == SpanKind::Emphasis {
                        SpanKind::StrongEmphasis
                    } else {
                        SpanKind::Strong
                    },
                    content: span.content,
                    source: span.source,
                });
            }
        }
        "em" | "i" => {
            // Add space before italic if needed
            if !content.ends_with_whitespace() {
                content.push(Span {
                    kind: SpanKind::Text,
                    content: " ".to_string(),
                    source: None,
                });
            }
            let mut inner = InlineContent::new();
            for child in &layout_box.children {
                collect_inline_recursive(child, &mut inner);
            }
            for span in inner.spans {
                content.push(Span {
                    kind: if span.kind == SpanKind::Strong {
                        SpanKind::StrongEmphasis
                    } else {
                        SpanKind::Emphasis
                    },
                    content: span.content,
                    source: span.source,
                });
            }
        }
        "s" | "strike" | "del" => {
            let mut inner = InlineContent::new();
            for child in &layout_box.children {
                collect_inline_recursive(child, &mut inner);
            }
            for span in inner.spans {
                content.push(Span {
                    kind: SpanKind::Strikethrough,
                    content: span.content,
                    source: span.source,
                });
            }
        }
        "u" | "ins" => {
            let mut inner = InlineContent::new();
            for child in &layout_box.children {
                collect_inline_recursive(child, &mut inner);
            }
            for span in inner.spans {
                content.push(Span {
                    kind: SpanKind::Underline,
                    content: span.content,
                    source: span.source,
                });
            }
        }
        "mark" => {
            let mut inner = InlineContent::new();
            for child in &layout_box.children {
                collect_inline_recursive(child, &mut inner);
            }
            for span in inner.spans {
                content.push(Span {
                    kind: SpanKind::Highlight,
                    content: span.content,
                    source: span.source,
                });
            }
        }
        "sub" => {
            let mut inner = InlineContent::new();
            for child in &layout_box.children {
                collect_inline_recursive(child, &mut inner);
            }
            for span in inner.spans {
                content.push(Span {
                    kind: SpanKind::Subscript,
                    content: span.content,
                    source: span.source,
                });
            }
        }
        "sup" => {
            let mut inner = InlineContent::new();
            for child in &layout_box.children {
                collect_inline_recursive(child, &mut inner);
            }
            for span in inner.spans {
                content.push(Span {
                    kind: SpanKind::Superscript,
                    content: span.content,
                    source: span.source,
                });
            }
        }
        // Span elements - add space before if needed
        "span" => {
            // Add space before span content if we have content and it doesn't end with whitespace
            if !content.ends_with_whitespace() {
                content.push(Span {
                    kind: SpanKind::Text,
                    content: " ".to_string(),
                    source: None,
                });
            }
            for child in &layout_box.children {
                collect_inline_recursive(child, content);
            }
        }

        _ => {
            // Generic inline element - process children
            for child in &layout_box.children {
                collect_inline_recursive(child, content);
            }
        }
    }
}

fn determine_span_kind(layout_box: &LayoutBox) -> SpanKind {
    let style = &layout_box.style;
    let is_bold = style.font_weight.is_bold();
    let is_italic = style.font_style.is_italic();

    if is_bold && is_italic {
        SpanKind::StrongEmphasis
    } else if is_bold {
        SpanKind::Strong
    } else if is_italic {
        SpanKind::Emphasis
    } else {
        SpanKind::Text
    }
}

fn collect_inline_from_box(layout_box: &LayoutBox) -> InlineContent {
    collect_inline_content(layout_box)
}

/// Collect code content from a pre element
fn collect_code_content(pre_box: &LayoutBox) -> (Option<String>, String) {
    let mut language = None;
    let mut code = String::new();

    // Check for code child with language class
    for child in &pre_box.children {
        if child.tag == "code" {
            // Check for language-xxx or lang-xxx class
            if let Some(class) = child.attrs.get("class") {
                for cls in class.split_whitespace() {
                    if let Some(lang) = cls.strip_prefix("language-") {
                        language = Some(lang.to_string());
                        break;
                    }
                    if let Some(lang) = cls.strip_prefix("lang-") {
                        language = Some(lang.to_string());
                        break;
                    }
                }
            }
            code = collect_text_content(child);
        } else {
            code.push_str(&collect_text_content(child));
        }
    }

    if code.is_empty() {
        code = collect_text_content(pre_box);
    }

    (language, code)
}

/// Collect plain text content from a box
fn collect_text_content(layout_box: &LayoutBox) -> String {
    if let Some(text) = &layout_box.text {
        return text.clone();
    }

    let mut result = String::new();
    for child in &layout_box.children {
        result.push_str(&collect_text_content(child));
    }
    result
}

/// Layout multi-column content.
///
/// Multi-column layout divides content into multiple columns. In terminal context,
/// we render columns side-by-side with a separator (|) between them.
fn layout_multicol(layout_box: &LayoutBox, ctx: &mut BlockLayoutContext) -> Vec<Block> {
    let style = &layout_box.style;
    let mut blocks = Vec::new();

    // Calculate actual number of columns
    let num_columns = style.effective_column_count(ctx.available_width);
    if num_columns <= 1 {
        // Fall back to normal block layout if only one column
        return layout_children(layout_box, ctx);
    }

    // Calculate column width (accounting for gaps and separators)
    // Each gap is: space + separator (|) + space = 3 chars, or just column_gap
    let separator_width = if style.column_rule_width > 0 { 3 } else { style.column_gap.max(1) };
    let total_separators = separator_width * (num_columns - 1);
    let available_for_columns = ctx.available_width.saturating_sub(total_separators);
    let column_width = available_for_columns / num_columns;

    if column_width < 5 {
        // Columns too narrow, fall back to single column
        return layout_children(layout_box, ctx);
    }

    // Layout children in a narrower context
    let narrow_ctx = BlockLayoutContext {
        viewport: ctx.viewport,
        available_width: column_width,
        indent: 0, // No additional indent within columns
        prev_margin_bottom: 0,
        in_list: ctx.in_list,
        list_depth: ctx.list_depth,
        in_blockquote: ctx.in_blockquote,
        blockquote_depth: ctx.blockquote_depth,
        container_stack: ctx.container_stack.clone(),
        float_context: FloatContext::new(column_width),
        current_y: ctx.current_y,
        counter_context: ctx.counter_context.clone(),
    };

    // Get all child blocks
    let child_blocks = layout_children(layout_box, &narrow_ctx);

    if child_blocks.is_empty() {
        return blocks;
    }

    // Estimate total lines needed for content distribution
    let total_lines = estimate_block_lines(&child_blocks);
    let lines_per_column = (total_lines + num_columns - 1) / num_columns;

    // Distribute blocks across columns
    let mut columns: Vec<Vec<Block>> = vec![Vec::new(); num_columns];
    let mut current_column = 0;
    let mut current_lines = 0;

    for block in child_blocks {
        let block_lines = estimate_single_block_lines(&block);

        // Check if this block would overflow current column
        if current_lines + block_lines > lines_per_column && current_column < num_columns - 1 {
            // Move to next column if not last
            current_column += 1;
            current_lines = 0;
        }

        columns[current_column].push(block);
        current_lines += block_lines;
    }

    // Now we need to render each column to text and combine them side-by-side
    // Create a pre-formatted code block with the multi-column layout
    let rendered_columns: Vec<Vec<String>> = columns
        .iter()
        .map(|col_blocks| render_blocks_to_lines(col_blocks, column_width))
        .collect();

    // Find max height
    let max_lines = rendered_columns.iter().map(|c| c.len()).max().unwrap_or(0);

    // Build combined output
    let mut combined_lines = Vec::with_capacity(max_lines);
    let separator = if style.column_rule_width > 0 { " │ " } else { "   " };

    for line_idx in 0..max_lines {
        let mut line = String::new();
        for (col_idx, col_lines) in rendered_columns.iter().enumerate() {
            if col_idx > 0 {
                line.push_str(separator);
            }
            // Get line or empty padding
            let col_line = col_lines.get(line_idx).map(|s| s.as_str()).unwrap_or("");
            // Pad to column width
            line.push_str(col_line);
            let padding = column_width.saturating_sub(display_width(col_line));
            for _ in 0..padding {
                line.push(' ');
            }
        }
        combined_lines.push(line);
    }

    // Output as a code block to preserve formatting
    let combined_text = combined_lines.join("\n");
    blocks.push(Block {
        kind: BlockKind::CodeBlock {
            language: None,
            code: combined_text,
        },
        source: Some(layout_box.node_id),
    });

    blocks
}

/// Estimate total lines for a list of blocks
fn estimate_block_lines(blocks: &[Block]) -> usize {
    blocks.iter().map(estimate_single_block_lines).sum()
}

/// Estimate lines for a single block
fn estimate_single_block_lines(block: &Block) -> usize {
    match &block.kind {
        BlockKind::Paragraph { content } => {
            let text = content.plain_text();
            // Rough estimate: 1 line per 60 chars
            (text.len() / 60).max(1)
        }
        BlockKind::Heading { .. } => 2, // Heading + blank line
        BlockKind::CodeBlock { code, .. } => code.lines().count().max(1) + 1,
        BlockKind::Blockquote { blocks } => estimate_block_lines(blocks) + 1,
        BlockKind::UnorderedList { items } | BlockKind::OrderedList { items, .. } => {
            items.iter().map(|item| item.blocks.len().max(1)).sum::<usize>() + 1
        }
        BlockKind::Table { rows, .. } => rows.len() + 2, // Header + separator + rows
        BlockKind::ThematicBreak => 2,
        BlockKind::BlankLines { count } => *count,
        BlockKind::Container { blocks, .. } => estimate_block_lines(blocks),
        BlockKind::Details { blocks, .. } => estimate_block_lines(blocks) + 1,
        BlockKind::Widget { .. } => 1,
        BlockKind::Form { widgets, .. } => widgets.len() + 1,
        BlockKind::HtmlBlock { content } => content.lines().count().max(1),
    }
}

/// Render blocks to lines of text
fn render_blocks_to_lines(blocks: &[Block], max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();

    for block in blocks {
        render_block_to_lines(block, max_width, &mut lines);
    }

    lines
}

/// Render a single block to lines
fn render_block_to_lines(block: &Block, max_width: usize, lines: &mut Vec<String>) {
    match &block.kind {
        BlockKind::Paragraph { content } => {
            let text = content.plain_text();
            // Word wrap the text
            wrap_text(&text, max_width, lines);
            lines.push(String::new()); // Blank line after paragraph
        }
        BlockKind::Heading { level, content } => {
            let text = content.plain_text();
            let prefix = "#".repeat(*level as usize);
            let heading = format!("{} {}", prefix, text);
            wrap_text(&heading, max_width, lines);
            lines.push(String::new());
        }
        BlockKind::CodeBlock { code, .. } => {
            for line in code.lines() {
                lines.push(truncate_line(line, max_width));
            }
            lines.push(String::new());
        }
        BlockKind::Blockquote { blocks } => {
            let inner_lines = render_blocks_to_lines(blocks, max_width.saturating_sub(2));
            for line in inner_lines {
                lines.push(format!("> {}", line));
            }
        }
        BlockKind::UnorderedList { items } => {
            for item in items {
                for (i, item_block) in item.blocks.iter().enumerate() {
                    let prefix = if i == 0 { "• " } else { "  " };
                    let mut item_lines = Vec::new();
                    render_block_to_lines(item_block, max_width.saturating_sub(2), &mut item_lines);
                    for (j, line) in item_lines.into_iter().enumerate() {
                        if j == 0 && i == 0 {
                            lines.push(format!("{}{}", prefix, line));
                        } else {
                            lines.push(format!("  {}", line));
                        }
                    }
                }
            }
        }
        BlockKind::OrderedList { items, start } => {
            for (idx, item) in items.iter().enumerate() {
                let num = start + idx;
                for (i, item_block) in item.blocks.iter().enumerate() {
                    let prefix = if i == 0 {
                        format!("{}. ", num)
                    } else {
                        "   ".to_string()
                    };
                    let mut item_lines = Vec::new();
                    render_block_to_lines(item_block, max_width.saturating_sub(3), &mut item_lines);
                    for (j, line) in item_lines.into_iter().enumerate() {
                        if j == 0 && i == 0 {
                            lines.push(format!("{}{}", prefix, line));
                        } else {
                            lines.push(format!("   {}", line));
                        }
                    }
                }
            }
        }
        BlockKind::ThematicBreak => {
            lines.push("─".repeat(max_width.min(40)));
            lines.push(String::new());
        }
        BlockKind::BlankLines { count } => {
            for _ in 0..*count {
                lines.push(String::new());
            }
        }
        BlockKind::Container { blocks, .. } => {
            let inner = render_blocks_to_lines(blocks, max_width);
            lines.extend(inner);
        }
        BlockKind::Table { headers, rows, alignments } => {
            // Simple table rendering
            if !headers.is_empty() {
                let header_line: String = headers
                    .iter()
                    .map(|c| c.content.plain_text())
                    .collect::<Vec<_>>()
                    .join(" | ");
                lines.push(truncate_line(&header_line, max_width));
                lines.push("-".repeat(max_width.min(header_line.len())));
            }
            for row in rows {
                let row_line: String = row
                    .iter()
                    .map(|c| c.content.plain_text())
                    .collect::<Vec<_>>()
                    .join(" | ");
                lines.push(truncate_line(&row_line, max_width));
            }
            let _ = alignments; // Unused but acknowledged
            lines.push(String::new());
        }
        BlockKind::Details { summary, blocks, open } => {
            let marker = if *open { "▼" } else { "▶" };
            lines.push(format!("{} {}", marker, summary.plain_text()));
            if *open {
                let inner = render_blocks_to_lines(blocks, max_width.saturating_sub(2));
                for line in inner {
                    lines.push(format!("  {}", line));
                }
            }
        }
        BlockKind::Widget { display, .. } => {
            lines.push(format!("[{}]", display));
        }
        BlockKind::Form { widgets, .. } => {
            lines.push(format!("[form: {} widgets]", widgets.len()));
        }
        BlockKind::HtmlBlock { content } => {
            for line in content.lines() {
                lines.push(truncate_line(line, max_width));
            }
        }
    }
}

/// Word wrap text to fit within max_width
fn wrap_text(text: &str, max_width: usize, lines: &mut Vec<String>) {
    if max_width == 0 {
        lines.push(text.to_string());
        return;
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let mut current_line = String::new();

    for word in words {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else if display_width(&current_line) + 1 + display_width(word) <= max_width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }
}

/// Truncate a line to max_width
fn truncate_line(line: &str, max_width: usize) -> String {
    if display_width(line) <= max_width {
        line.to_string()
    } else {
        let mut result = String::new();
        let mut width = 0;
        for ch in line.chars() {
            let ch_width = unicode_width(ch);
            if width + ch_width > max_width.saturating_sub(1) {
                result.push('…');
                break;
            }
            result.push(ch);
            width += ch_width;
        }
        result
    }
}

/// Calculate display width of a string
fn display_width(s: &str) -> usize {
    s.chars().map(unicode_width).sum()
}

/// Get width of a single character
fn unicode_width(ch: char) -> usize {
    // Simplified width calculation
    if ch.is_ascii() {
        1
    } else if ch >= '\u{1100}' {
        // CJK and other wide characters
        2
    } else {
        1
    }
}
