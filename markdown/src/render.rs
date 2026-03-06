//! Markdown renderer.
//!
//! Converts a LayoutPlan (collection of Blocks) into a Markdown string.
//! Handles:
//! - Block-level Markdown syntax (headings, lists, blockquotes, code blocks)
//! - Inline formatting (bold, italic, code, links)
//! - Table rendering
//! - Source mapping via line records

use crate::ast::{Alignment, Block, BlockKind, InlineContent, ListItem, Span, SpanKind, TableCell};
use crate::ids::NodeId;
use crate::layout::{LayoutPlan, LineRecord, Viewport};

/// Render configuration
#[derive(Debug, Clone)]
pub struct RenderConfig {
    /// Maximum line width for wrapping
    pub max_width: usize,
    /// Whether to add trailing newlines
    pub trailing_newline: bool,
    /// Whether to use reference-style links
    pub reference_links: bool,
    /// Indent string for nested content
    pub indent_string: String,
    /// Whether to emit source mapping
    pub emit_source_map: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        RenderConfig {
            max_width: 80,
            trailing_newline: true,
            reference_links: false,
            indent_string: "  ".to_string(),
            emit_source_map: false,
        }
    }
}

/// Render result with source mapping
#[derive(Debug)]
pub struct RenderResult {
    /// The rendered Markdown string
    pub markdown: String,
    /// Line-to-node mapping for source maps
    pub line_map: Vec<LineRecord>,
}

/// Render a layout plan to Markdown
pub fn render(plan: &LayoutPlan, config: &RenderConfig) -> RenderResult {
    let mut ctx = RenderContext::new(config);

    for block in &plan.blocks {
        render_block(block, &mut ctx);
    }

    // Handle trailing newline
    let mut markdown = ctx.output;

    // Collapse excessive blank lines and clean up hard line breaks
    // Replace trailing spaces + newlines patterns
    markdown = markdown.replace("  \n  \n", "\n");
    markdown = markdown.replace("  \n\n", "\n\n");

    // Collapse more than 2 consecutive newlines
    while markdown.contains("\n\n\n") {
        markdown = markdown.replace("\n\n\n", "\n\n");
    }

    // Remove consecutive thematic breaks
    while markdown.contains("---\n\n---") {
        markdown = markdown.replace("---\n\n---", "---");
    }

    // Remove trailing thematic breaks
    markdown = markdown.trim_end().to_string();
    while markdown.ends_with("\n---") || markdown.ends_with("\n\n---") {
        markdown = markdown.trim_end_matches("---").trim_end().to_string();
    }

    if config.trailing_newline && !markdown.ends_with('\n') {
        markdown.push('\n');
    }

    RenderResult {
        markdown,
        line_map: ctx.line_records,
    }
}

/// Render a layout plan to a simple string (convenience function)
pub fn render_to_string(plan: &LayoutPlan, viewport: &Viewport) -> String {
    let config = RenderConfig {
        max_width: viewport.width,
        ..Default::default()
    };
    let result = render(plan, &config);
    result.markdown
}

/// Render context
struct RenderContext<'a> {
    config: &'a RenderConfig,
    output: String,
    current_line: usize,
    line_records: Vec<LineRecord>,
    /// Stack of list counters for nested ordered lists
    list_counter_stack: Vec<usize>,
}

impl<'a> RenderContext<'a> {
    fn new(config: &'a RenderConfig) -> Self {
        RenderContext {
            config,
            output: String::new(),
            current_line: 1,
            line_records: Vec::new(),
            list_counter_stack: Vec::new(),
        }
    }

    fn push_line(&mut self, line: &str, source_node: Option<NodeId>) {
        self.output.push_str(line);
        self.output.push('\n');

        if self.config.emit_source_map {
            self.line_records.push(LineRecord {
                line_number: self.current_line,
                node_id: source_node,
                text: line.to_string(),
            });
        }

        self.current_line += 1;
    }

    fn push_blank_line(&mut self) {
        if !self.output.ends_with("\n\n") && !self.output.is_empty() {
            self.output.push('\n');
            self.current_line += 1;
        }
    }

    fn indent_string(&self, level: usize) -> String {
        self.config.indent_string.repeat(level)
    }
}

/// Render a single block
fn render_block(block: &Block, ctx: &mut RenderContext) {
    let source = block.source;

    match &block.kind {
        BlockKind::Heading { level, content } => {
            let prefix = "#".repeat(*level as usize);
            let text = render_inline(content);
            ctx.push_blank_line();
            ctx.push_line(&format!("{} {}", prefix, text), source);
            ctx.push_blank_line();
        }

        BlockKind::Paragraph { content } => {
            let text = render_inline(content);
            if !text.is_empty() {
                ctx.push_blank_line();
                ctx.push_line(&text, source);
            }
        }

        BlockKind::Blockquote { blocks } => {
            ctx.push_blank_line();
            for inner_block in blocks {
                let rendered = render_block_to_string(inner_block);
                for line in rendered.lines() {
                    ctx.push_line(&format!("> {}", line), source);
                }
            }
        }

        BlockKind::CodeBlock { language, code } => {
            let fence = "```";
            let lang = language.as_deref().unwrap_or("");
            ctx.push_blank_line();
            ctx.push_line(&format!("{}{}", fence, lang), source);
            for line in code.lines() {
                ctx.push_line(line, source);
            }
            ctx.push_line(fence, source);
            ctx.push_blank_line();
        }

        BlockKind::UnorderedList { items } => {
            ctx.push_blank_line();
            for item in items {
                render_list_item(item, "-", ctx);
            }
        }

        BlockKind::OrderedList { start, items } => {
            ctx.push_blank_line();
            ctx.list_counter_stack.push(*start);
            for item in items {
                let counter = ctx.list_counter_stack.last_mut().unwrap();
                let marker = format!("{}.", *counter);
                *counter += 1;
                render_list_item(item, &marker, ctx);
            }
            ctx.list_counter_stack.pop();
        }

        BlockKind::ThematicBreak => {
            ctx.push_blank_line();
            ctx.push_line("---", source);
            ctx.push_blank_line();
        }

        BlockKind::Table { headers, rows, alignments } => {
            render_table(headers, rows, alignments, source, ctx);
        }

        BlockKind::Widget { widget_id, display } => {
            if display.is_empty() {
                ctx.push_line(&format!("{{{{WIDGET:{}}}}}", widget_id.as_u64()), source);
            } else {
                ctx.push_line(display, source);
            }
        }

        BlockKind::Form { action, method, widgets: _ } => {
            ctx.push_blank_line();
            ctx.push_line(&format!("{{{{FORM:{}:{}}}}}", action, method), source);
        }

        BlockKind::HtmlBlock { content } => {
            ctx.push_blank_line();
            for line in content.lines() {
                ctx.push_line(line, source);
            }
        }

        BlockKind::BlankLines { count } => {
            for _ in 0..*count {
                ctx.push_line("", source);
            }
        }

        BlockKind::Container { blocks, indent } => {
            let indent_str = ctx.indent_string(*indent);
            for inner_block in blocks {
                let rendered = render_block_to_string(inner_block);
                for line in rendered.lines() {
                    ctx.push_line(&format!("{}{}", indent_str, line), source);
                }
            }
        }

        BlockKind::Details { summary, blocks, open } => {
            ctx.push_blank_line();
            let marker = if *open { "▼" } else { "▶" };
            let summary_text = render_inline(summary);
            ctx.push_line(&format!("{} **{}**", marker, summary_text), source);
            if *open {
                for inner_block in blocks {
                    render_block(inner_block, ctx);
                }
            }
        }
    }
}

/// Render a block to a string (for nesting)
fn render_block_to_string(block: &Block) -> String {
    let config = RenderConfig {
        trailing_newline: false,
        ..Default::default()
    };
    let plan = LayoutPlan {
        blocks: vec![block.clone()],
        overlays: Default::default(),
    };
    let result = render(&plan, &config);
    result.markdown
}

/// Render a list item
fn render_list_item(item: &ListItem, marker: &str, ctx: &mut RenderContext) {
    let source = item.source;

    // Handle task list checkbox
    let prefix = if let Some(checked) = item.checked {
        let checkbox = if checked { "[x]" } else { "[ ]" };
        format!("{} {}", marker, checkbox)
    } else {
        marker.to_string()
    };

    // Render first block on same line as marker
    if let Some(first_block) = item.blocks.first() {
        let first_content = render_block_to_string(first_block);
        let first_line = first_content.lines().next().unwrap_or("");
        ctx.push_line(&format!("{} {}", prefix, first_line), source);

        // Indent subsequent lines
        for line in first_content.lines().skip(1) {
            let indent = " ".repeat(prefix.len() + 1);
            ctx.push_line(&format!("{}{}", indent, line), source);
        }

        // Render remaining blocks with indent
        for block in item.blocks.iter().skip(1) {
            let content = render_block_to_string(block);
            for line in content.lines() {
                let indent = " ".repeat(prefix.len() + 1);
                ctx.push_line(&format!("{}{}", indent, line), source);
            }
        }
    } else {
        ctx.push_line(&prefix, source);
    }
}

/// Render inline content to a formatted string
pub fn render_inline(content: &InlineContent) -> String {
    let mut result = String::new();

    for span in &content.spans {
        result.push_str(&render_span(span));
    }

    result
}

/// Render a single span with formatting
fn render_span(span: &Span) -> String {
    match &span.kind {
        SpanKind::Text => span.content.clone(),
        SpanKind::Strong => format!("**{}**", span.content),
        SpanKind::Emphasis => format!("*{}*", span.content),
        SpanKind::StrongEmphasis => format!("***{}***", span.content),
        SpanKind::Code => format!("`{}`", span.content),
        SpanKind::Strikethrough => format!("~~{}~~", span.content),
        SpanKind::Underline => format!("<u>{}</u>", span.content),

        SpanKind::Link { url, title, link_id: _ } => {
            if let Some(t) = title {
                format!("[{}]({} \"{}\")", span.content, url, t)
            } else {
                format!("[{}]({})", span.content, url)
            }
        }

        SpanKind::Image { url, alt } => {
            format!("![{}]({})", alt, url)
        }

        SpanKind::LineBreak { hard } => {
            if *hard {
                "  \n".to_string()
            } else {
                "\n".to_string()
            }
        }

        SpanKind::Superscript => format!("^{}", span.content),
        SpanKind::Subscript => format!("~{}", span.content),
        SpanKind::Highlight => format!("=={}", span.content),
        SpanKind::Kbd => format!("<kbd>{}</kbd>", span.content),
        SpanKind::WidgetRef { widget_id } => format!("{{{{WIDGET:{}}}}}", widget_id.as_u64()),
    }
}

/// Render a Markdown table
fn render_table(
    headers: &[TableCell],
    rows: &[Vec<TableCell>],
    alignments: &[Alignment],
    source: Option<NodeId>,
    ctx: &mut RenderContext,
) {
    if headers.is_empty() && rows.is_empty() {
        return;
    }

    ctx.push_blank_line();

    // Calculate column widths
    let num_cols = headers.len().max(rows.first().map(|r| r.len()).unwrap_or(0));
    let mut widths: Vec<usize> = vec![3; num_cols];

    for (i, cell) in headers.iter().enumerate() {
        let content = render_inline(&cell.content);
        widths[i] = widths[i].max(unicode_width::UnicodeWidthStr::width(content.as_str()));
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            let content = render_inline(&cell.content);
            if i < widths.len() {
                widths[i] = widths[i].max(unicode_width::UnicodeWidthStr::width(content.as_str()));
            }
        }
    }

    // Render header
    if !headers.is_empty() {
        let header_cells: Vec<String> = headers
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let content = render_inline(&cell.content);
                let width = widths.get(i).copied().unwrap_or(3);
                pad_cell(&content, width, alignments.get(i).copied().unwrap_or(Alignment::Default))
            })
            .collect();

        ctx.push_line(&format!("| {} |", header_cells.join(" | ")), source);

        // Separator with alignment
        let separator: Vec<String> = (0..num_cols)
            .map(|i| {
                let width = widths.get(i).copied().unwrap_or(3);
                let alignment = alignments.get(i).copied().unwrap_or(Alignment::Default);
                match alignment {
                    Alignment::Left => format!(":{}", "-".repeat(width - 1)),
                    Alignment::Right => format!("{}:", "-".repeat(width - 1)),
                    Alignment::Center => format!(":{}:", "-".repeat(width.saturating_sub(2))),
                    Alignment::Default => "-".repeat(width),
                }
            })
            .collect();
        ctx.push_line(&format!("| {} |", separator.join(" | ")), source);
    }

    // Render rows
    for row in rows {
        let row_cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let content = render_inline(&cell.content);
                let width = widths.get(i).copied().unwrap_or(3);
                let alignment = alignments.get(i).copied().unwrap_or(Alignment::Default);
                pad_cell(&content, width, alignment)
            })
            .collect();

        ctx.push_line(&format!("| {} |", row_cells.join(" | ")), source);
    }

    ctx.push_blank_line();
}

/// Pad a cell to a given width with alignment
fn pad_cell(content: &str, width: usize, alignment: Alignment) -> String {
    use unicode_width::UnicodeWidthStr;
    let content_width = UnicodeWidthStr::width(content);
    if content_width >= width {
        return content.to_string();
    }

    let padding = width - content_width;
    match alignment {
        Alignment::Right => format!("{}{}", " ".repeat(padding), content),
        Alignment::Center => {
            let left_pad = padding / 2;
            let right_pad = padding - left_pad;
            format!("{}{}{}", " ".repeat(left_pad), content, " ".repeat(right_pad))
        }
        _ => format!("{}{}", content, " ".repeat(padding)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_heading() {
        let blocks = vec![Block {
            kind: BlockKind::Heading {
                level: 1,
                content: InlineContent::text("Hello World"),
            },
            source: None,
        }];

        let plan = LayoutPlan {
            blocks,
            overlays: Default::default(),
        };

        let config = RenderConfig::default();
        let result = render(&plan, &config);

        assert!(result.markdown.contains("# Hello World"));
    }

    #[test]
    fn test_render_paragraph() {
        let blocks = vec![Block {
            kind: BlockKind::Paragraph {
                content: InlineContent::text("This is a paragraph."),
            },
            source: None,
        }];

        let plan = LayoutPlan {
            blocks,
            overlays: Default::default(),
        };

        let config = RenderConfig::default();
        let result = render(&plan, &config);

        assert!(result.markdown.contains("This is a paragraph."));
    }

    #[test]
    fn test_render_code_block() {
        let blocks = vec![Block {
            kind: BlockKind::CodeBlock {
                language: Some("rust".to_string()),
                code: "fn main() {\n    println!(\"Hello\");\n}".to_string(),
            },
            source: None,
        }];

        let plan = LayoutPlan {
            blocks,
            overlays: Default::default(),
        };

        let config = RenderConfig::default();
        let result = render(&plan, &config);

        assert!(result.markdown.contains("```rust"));
        assert!(result.markdown.contains("fn main()"));
        assert!(result.markdown.contains("```\n"));
    }

    #[test]
    fn test_render_list() {
        let blocks = vec![Block {
            kind: BlockKind::UnorderedList {
                items: vec![
                    ListItem {
                        blocks: vec![Block {
                            kind: BlockKind::Paragraph {
                                content: InlineContent::text("First item"),
                            },
                            source: None,
                        }],
                        source: None,
                        checked: None,
                    },
                    ListItem {
                        blocks: vec![Block {
                            kind: BlockKind::Paragraph {
                                content: InlineContent::text("Second item"),
                            },
                            source: None,
                        }],
                        source: None,
                        checked: None,
                    },
                ],
            },
            source: None,
        }];

        let plan = LayoutPlan {
            blocks,
            overlays: Default::default(),
        };

        let config = RenderConfig::default();
        let result = render(&plan, &config);

        assert!(result.markdown.contains("- First item"));
        assert!(result.markdown.contains("- Second item"));
    }

    #[test]
    fn test_render_inline_formatting() {
        let mut content = InlineContent::new();
        content.push(Span {
            kind: SpanKind::Text,
            content: "Hello ".to_string(),
            source: None,
        });
        content.push(Span {
            kind: SpanKind::Strong,
            content: "bold".to_string(),
            source: None,
        });
        content.push(Span {
            kind: SpanKind::Text,
            content: " and ".to_string(),
            source: None,
        });
        content.push(Span {
            kind: SpanKind::Emphasis,
            content: "italic".to_string(),
            source: None,
        });

        let rendered = render_inline(&content);
        assert_eq!(rendered, "Hello **bold** and *italic*");
    }

    #[test]
    fn test_render_link() {
        let mut content = InlineContent::new();
        content.push(Span {
            kind: SpanKind::Link {
                url: "https://example.com".to_string(),
                title: None,
                link_id: crate::ids::LinkId::new(),
            },
            content: "Example".to_string(),
            source: None,
        });

        let rendered = render_inline(&content);
        assert_eq!(rendered, "[Example](https://example.com)");
    }

    #[test]
    fn test_render_table() {
        let blocks = vec![Block {
            kind: BlockKind::Table {
                headers: vec![
                    TableCell { content: InlineContent::text("Name"), source: None },
                    TableCell { content: InlineContent::text("Age"), source: None },
                ],
                rows: vec![
                    vec![
                        TableCell { content: InlineContent::text("Alice"), source: None },
                        TableCell { content: InlineContent::text("30"), source: None },
                    ],
                    vec![
                        TableCell { content: InlineContent::text("Bob"), source: None },
                        TableCell { content: InlineContent::text("25"), source: None },
                    ],
                ],
                alignments: vec![Alignment::Left, Alignment::Right],
            },
            source: None,
        }];

        let plan = LayoutPlan {
            blocks,
            overlays: Default::default(),
        };

        let config = RenderConfig::default();
        let result = render(&plan, &config);

        assert!(result.markdown.contains("| Name"));
        assert!(result.markdown.contains("| Age"));
        assert!(result.markdown.contains("| Alice"));
        assert!(result.markdown.contains("| Bob"));
    }
}
