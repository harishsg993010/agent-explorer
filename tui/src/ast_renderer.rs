//! AST Renderer - Convert markdown Block AST directly to RenderedPage
//!
//! This bypasses the markdown string intermediary for better performance
//! and preserves widget IDs, source node IDs, and link IDs from the layout.

use crate::render::{
    Interactive, InteractiveKind, Line, LineContent, NumberedLink, RenderedPage, SpanStyle,
    StyledSpan,
};
use markdown::{
    ast::{Alignment, Block, BlockKind, InlineContent, ListItem, Span, SpanKind, TableCell},
    ids::{WidgetId, WidgetInfo, WidgetMap, WidgetType},
    PipelineOutput,
};
use ratatui::style::Color;

/// Convert a PipelineOutput directly to RenderedPage
pub fn render_pipeline_output(
    output: &PipelineOutput,
    widget_map: &WidgetMap,
    url: &str,
) -> RenderedPage {
    let mut page = RenderedPage::new();
    page.url = url.to_string();

    let mut ctx = RenderContext {
        page: &mut page,
        widget_map,
        current_form_action: None,
        current_form_method: "get".to_string(),
    };

    // Render main content blocks
    for block in &output.layout_plan.blocks {
        render_block(block, &mut ctx, 0);
    }

    // Render overlays if any (inline fallback mode)
    for overlay in &output.layout_plan.overlays.overlays {
        if overlay.visible {
            ctx.page.add_empty();
            ctx.page.lines.push(Line {
                content: LineContent::Markdown(vec![StyledSpan {
                    text: format!("--- {} ---", overlay_kind_label(&overlay.kind)),
                    style: SpanStyle {
                        fg: Some(Color::DarkGray),
                        dim: true,
                        ..Default::default()
                    },
                    link: None,
                }]),
                indent: 0,
            });
            for block in &overlay.content.blocks {
                render_block(block, &mut ctx, 0);
            }
        }
    }

    page
}

fn overlay_kind_label(kind: &markdown::ast::OverlayKind) -> &'static str {
    use markdown::ast::OverlayKind;
    match kind {
        OverlayKind::Fixed => "Fixed",
        OverlayKind::Absolute => "Popup",
        OverlayKind::Modal => "Modal",
        OverlayKind::Dropdown => "Dropdown",
        OverlayKind::Tooltip => "Tooltip",
        OverlayKind::Toast => "Toast",
        OverlayKind::Popup => "Popup",
    }
}

/// Render context passed through the rendering process
struct RenderContext<'a> {
    page: &'a mut RenderedPage,
    widget_map: &'a WidgetMap,
    current_form_action: Option<String>,
    current_form_method: String,
}

/// Render a block to the page
fn render_block(block: &Block, ctx: &mut RenderContext, indent: usize) {
    match &block.kind {
        BlockKind::Heading { level, content } => {
            render_heading(*level, content, ctx);
        }

        BlockKind::Paragraph { content } => {
            render_paragraph(content, ctx, indent);
        }

        BlockKind::CodeBlock { language, code } => {
            ctx.page.lines.push(Line {
                content: LineContent::CodeBlock {
                    code: code.clone(),
                    language: language.clone(),
                },
                indent: indent as u16,
            });
        }

        BlockKind::ThematicBreak => {
            ctx.page.add_hr();
        }

        BlockKind::BlankLines { count } => {
            for _ in 0..*count {
                ctx.page.add_empty();
            }
        }

        BlockKind::UnorderedList { items } => {
            for item in items {
                render_list_item(item, ctx, indent, None);
            }
        }

        BlockKind::OrderedList { start, items } => {
            for (i, item) in items.iter().enumerate() {
                render_list_item(item, ctx, indent, Some(*start + i));
            }
        }

        BlockKind::Blockquote { blocks } => {
            for inner_block in blocks {
                render_block(inner_block, ctx, indent + 2);
            }
        }

        BlockKind::Table {
            headers,
            rows,
            alignments,
        } => {
            render_table(headers, rows, alignments, ctx);
        }

        BlockKind::Widget { widget_id, display } => {
            render_widget(*widget_id, display, ctx);
        }

        BlockKind::Form {
            action,
            method,
            widgets: _,
        } => {
            ctx.current_form_action = Some(action.clone());
            ctx.current_form_method = method.clone();
            ctx.page.form_action = Some(action.clone());
            ctx.page.form_method = method.clone();
        }

        BlockKind::Container { blocks, indent: container_indent } => {
            for inner_block in blocks {
                render_block(inner_block, ctx, indent + container_indent);
            }
        }

        BlockKind::Details {
            summary,
            blocks,
            open,
        } => {
            let prefix = if *open { "▼ " } else { "▶ " };
            let mut spans = vec![StyledSpan {
                text: prefix.to_string(),
                style: SpanStyle {
                    fg: Some(Color::Cyan),
                    ..Default::default()
                },
                link: None,
            }];
            spans.extend(render_inline_content(summary, ctx));

            ctx.page.lines.push(Line {
                content: LineContent::Markdown(spans),
                indent: indent as u16,
            });

            if *open {
                for inner_block in blocks {
                    render_block(inner_block, ctx, indent + 2);
                }
            }
        }

        BlockKind::HtmlBlock { content } => {
            for line in content.lines() {
                ctx.page.lines.push(Line {
                    content: LineContent::Markdown(vec![StyledSpan {
                        text: line.to_string(),
                        style: SpanStyle {
                            dim: true,
                            ..Default::default()
                        },
                        link: None,
                    }]),
                    indent: indent as u16,
                });
            }
        }
    }
}

/// Render a heading
fn render_heading(level: u8, content: &InlineContent, ctx: &mut RenderContext) {
    let prefix = "#".repeat(level as usize);
    let color = match level {
        1 => Color::Cyan,
        2 => Color::Blue,
        3 => Color::Magenta,
        _ => Color::White,
    };

    if level == 1 {
        ctx.page.title = content.plain_text();
    }

    let mut spans = vec![StyledSpan {
        text: format!("{} ", prefix),
        style: SpanStyle {
            fg: Some(color),
            bold: true,
            ..Default::default()
        },
        link: None,
    }];

    for span in render_inline_content(content, ctx) {
        spans.push(StyledSpan {
            text: span.text,
            style: SpanStyle {
                fg: Some(color),
                bold: true,
                italic: span.style.italic,
                underline: span.style.underline,
                ..span.style
            },
            link: span.link,
        });
    }

    ctx.page.lines.push(Line {
        content: LineContent::Markdown(spans),
        indent: 0,
    });
}

/// Render a paragraph
fn render_paragraph(content: &InlineContent, ctx: &mut RenderContext, indent: usize) {
    let spans = render_inline_content(content, ctx);
    if !spans.is_empty() {
        ctx.page.lines.push(Line {
            content: LineContent::Markdown(spans),
            indent: indent as u16,
        });
    }
}

/// Render inline content to styled spans
fn render_inline_content(content: &InlineContent, ctx: &mut RenderContext) -> Vec<StyledSpan> {
    let mut result = Vec::new();

    for span in &content.spans {
        match &span.kind {
            SpanKind::Text => {
                result.push(StyledSpan {
                    text: span.content.clone(),
                    style: SpanStyle::default(),
                    link: None,
                });
            }

            SpanKind::Strong => {
                result.push(StyledSpan {
                    text: span.content.clone(),
                    style: SpanStyle {
                        bold: true,
                        ..Default::default()
                    },
                    link: None,
                });
            }

            SpanKind::Emphasis => {
                result.push(StyledSpan {
                    text: span.content.clone(),
                    style: SpanStyle {
                        italic: true,
                        ..Default::default()
                    },
                    link: None,
                });
            }

            SpanKind::StrongEmphasis => {
                result.push(StyledSpan {
                    text: span.content.clone(),
                    style: SpanStyle {
                        bold: true,
                        italic: true,
                        ..Default::default()
                    },
                    link: None,
                });
            }

            SpanKind::Code => {
                result.push(StyledSpan {
                    text: span.content.clone(),
                    style: SpanStyle {
                        bg: Some(Color::DarkGray),
                        fg: Some(Color::Yellow),
                        ..Default::default()
                    },
                    link: None,
                });
            }

            SpanKind::Strikethrough => {
                result.push(StyledSpan {
                    text: span.content.clone(),
                    style: SpanStyle {
                        dim: true,
                        ..Default::default()
                    },
                    link: None,
                });
            }

            SpanKind::Underline => {
                result.push(StyledSpan {
                    text: span.content.clone(),
                    style: SpanStyle {
                        underline: true,
                        ..Default::default()
                    },
                    link: None,
                });
            }

            SpanKind::Link { url, title: _, link_id } => {
                let number = ctx.page.numbered_links.len() + 1;
                let line_index = ctx.page.lines.len();

                ctx.page.numbered_links.push(NumberedLink {
                    number,
                    url: url.clone(),
                    text: span.content.clone(),
                    line_index,
                });

                result.push(StyledSpan {
                    text: format!("[{}]", number),
                    style: SpanStyle {
                        fg: Some(Color::DarkGray),
                        ..Default::default()
                    },
                    link: None,
                });

                result.push(StyledSpan {
                    text: span.content.clone(),
                    style: SpanStyle {
                        fg: Some(Color::Blue),
                        underline: true,
                        ..Default::default()
                    },
                    link: Some(number),
                });

                ctx.page.interactives.push(Interactive {
                    line_index,
                    element_id: link_id.0,
                    kind: InteractiveKind::Link(number),
                });
            }

            SpanKind::Image { url: _, alt } => {
                result.push(StyledSpan {
                    text: format!("[img: {}]", alt),
                    style: SpanStyle {
                        fg: Some(Color::Gray),
                        ..Default::default()
                    },
                    link: None,
                });
            }

            SpanKind::LineBreak { hard: _ } => {
                // Line breaks are handled by the line splitting logic
            }

            SpanKind::Superscript => {
                result.push(StyledSpan {
                    text: format!("^{}", span.content),
                    style: SpanStyle::default(),
                    link: None,
                });
            }

            SpanKind::Subscript => {
                result.push(StyledSpan {
                    text: format!("_{}", span.content),
                    style: SpanStyle::default(),
                    link: None,
                });
            }

            SpanKind::Highlight => {
                result.push(StyledSpan {
                    text: span.content.clone(),
                    style: SpanStyle {
                        bg: Some(Color::Yellow),
                        fg: Some(Color::Black),
                        ..Default::default()
                    },
                    link: None,
                });
            }

            SpanKind::Kbd => {
                result.push(StyledSpan {
                    text: format!("[{}]", span.content),
                    style: SpanStyle {
                        bg: Some(Color::DarkGray),
                        fg: Some(Color::White),
                        ..Default::default()
                    },
                    link: None,
                });
            }

            SpanKind::WidgetRef { widget_id } => {
                if let Some(widget) = ctx.widget_map.get(widget_id) {
                    render_inline_widget(*widget_id, widget, &mut result, ctx);
                }
            }
        }
    }

    result
}

/// Render an inline widget
fn render_inline_widget(
    widget_id: WidgetId,
    widget: &WidgetInfo,
    result: &mut Vec<StyledSpan>,
    ctx: &mut RenderContext,
) {
    let label = widget.label.as_deref().unwrap_or("Widget");
    result.push(StyledSpan {
        text: format!("[{}]", label),
        style: SpanStyle {
            fg: Some(Color::Cyan),
            ..Default::default()
        },
        link: None,
    });
}

/// Render a widget block
fn render_widget(widget_id: WidgetId, display: &str, ctx: &mut RenderContext) {
    let line_index = ctx.page.lines.len();

    if let Some(widget) = ctx.widget_map.get(&widget_id) {
        let label = widget.label.clone().unwrap_or_else(|| display.to_string());
        let placeholder = widget.placeholder.clone().unwrap_or_default();

        match &widget.widget_type {
            WidgetType::TextInput | WidgetType::EmailInput | WidgetType::SearchInput |
            WidgetType::UrlInput | WidgetType::TelInput | WidgetType::NumberInput |
            WidgetType::TextArea => {
                ctx.page.lines.push(Line {
                    content: LineContent::TextInput {
                        id: widget_id.0,
                        value: widget.value.clone(),
                        placeholder: if placeholder.is_empty() { label.clone() } else { placeholder },
                        password: false,
                        width: 30,
                    },
                    indent: 2,
                });

                ctx.page.interactives.push(Interactive {
                    line_index,
                    element_id: widget_id.0,
                    kind: InteractiveKind::TextInput,
                });

                if let Some(name) = &widget.name {
                    ctx.page.form_fields.insert(widget_id.0, name.clone());
                }
            }

            WidgetType::PasswordInput => {
                ctx.page.lines.push(Line {
                    content: LineContent::TextInput {
                        id: widget_id.0,
                        value: widget.value.clone(),
                        placeholder: if placeholder.is_empty() { "Password".to_string() } else { placeholder },
                        password: true,
                        width: 30,
                    },
                    indent: 2,
                });

                ctx.page.interactives.push(Interactive {
                    line_index,
                    element_id: widget_id.0,
                    kind: InteractiveKind::Password,
                });

                if let Some(name) = &widget.name {
                    ctx.page.form_fields.insert(widget_id.0, name.clone());
                }
            }

            WidgetType::Button | WidgetType::SubmitButton => {
                ctx.page.lines.push(Line {
                    content: LineContent::Button {
                        id: widget_id.0,
                        label,
                    },
                    indent: 2,
                });

                ctx.page.interactives.push(Interactive {
                    line_index,
                    element_id: widget_id.0,
                    kind: InteractiveKind::Button,
                });
            }

            WidgetType::Checkbox => {
                ctx.page.lines.push(Line {
                    content: LineContent::Checkbox {
                        id: widget_id.0,
                        label,
                        checked: widget.checked,
                    },
                    indent: 2,
                });

                ctx.page.interactives.push(Interactive {
                    line_index,
                    element_id: widget_id.0,
                    kind: InteractiveKind::Checkbox,
                });
            }

            WidgetType::Radio => {
                let name = widget.name.clone().unwrap_or_default();
                ctx.page.lines.push(Line {
                    content: LineContent::Radio {
                        id: widget_id.0,
                        name: name.clone(),
                        label,
                        value: widget.value.clone(),
                        checked: widget.checked,
                    },
                    indent: 2,
                });

                ctx.page.interactives.push(Interactive {
                    line_index,
                    element_id: widget_id.0,
                    kind: InteractiveKind::Radio,
                });
            }

            WidgetType::Select => {
                // For select, we'd need options - for now render as text
                ctx.page.lines.push(Line {
                    content: LineContent::Markdown(vec![StyledSpan {
                        text: format!("[select: {}]", label),
                        style: SpanStyle {
                            fg: Some(Color::Cyan),
                            ..Default::default()
                        },
                        link: None,
                    }]),
                    indent: 2,
                });

                ctx.page.interactives.push(Interactive {
                    line_index,
                    element_id: widget_id.0,
                    kind: InteractiveKind::Select,
                });
            }

            WidgetType::Hidden => {
                // Hidden fields are not rendered
            }
        }
    } else {
        // Widget not found in map, use display string
        ctx.page.lines.push(Line {
            content: LineContent::Markdown(vec![StyledSpan {
                text: display.to_string(),
                style: SpanStyle {
                    fg: Some(Color::Cyan),
                    ..Default::default()
                },
                link: None,
            }]),
            indent: 0,
        });
    }
}

/// Render a list item
fn render_list_item(
    item: &ListItem,
    ctx: &mut RenderContext,
    indent: usize,
    number: Option<usize>,
) {
    let bullet = if let Some(n) = number {
        format!("{}. ", n)
    } else if let Some(checked) = item.checked {
        if checked {
            "[x] ".to_string()
        } else {
            "[ ] ".to_string()
        }
    } else {
        "- ".to_string()
    };

    if let Some(first) = item.blocks.first() {
        match &first.kind {
            BlockKind::Paragraph { content } => {
                let mut spans = vec![StyledSpan {
                    text: bullet,
                    style: SpanStyle::default(),
                    link: None,
                }];
                spans.extend(render_inline_content(content, ctx));

                ctx.page.lines.push(Line {
                    content: LineContent::Markdown(spans),
                    indent: indent as u16,
                });
            }
            _ => {
                ctx.page.lines.push(Line {
                    content: LineContent::Markdown(vec![StyledSpan {
                        text: bullet,
                        style: SpanStyle::default(),
                        link: None,
                    }]),
                    indent: indent as u16,
                });
                render_block(first, ctx, indent + 2);
            }
        }

        for block in item.blocks.iter().skip(1) {
            render_block(block, ctx, indent + 2);
        }
    }
}

/// Render a table
fn render_table(
    headers: &[TableCell],
    rows: &[Vec<TableCell>],
    alignments: &[Alignment],
    ctx: &mut RenderContext,
) {
    let num_cols = headers.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
    let mut col_widths: Vec<usize> = vec![0; num_cols];

    for (i, header) in headers.iter().enumerate() {
        col_widths[i] = col_widths[i].max(header.content.plain_text().len());
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < col_widths.len() {
                col_widths[i] = col_widths[i].max(cell.content.plain_text().len());
            }
        }
    }

    // Header row
    let mut header_spans = vec![StyledSpan {
        text: "| ".to_string(),
        style: SpanStyle::default(),
        link: None,
    }];
    for (i, header) in headers.iter().enumerate() {
        let text = header.content.plain_text();
        let width = col_widths.get(i).copied().unwrap_or(text.len());
        header_spans.push(StyledSpan {
            text: format!("{:width$}", text, width = width),
            style: SpanStyle {
                bold: true,
                ..Default::default()
            },
            link: None,
        });
        header_spans.push(StyledSpan {
            text: " | ".to_string(),
            style: SpanStyle::default(),
            link: None,
        });
    }
    ctx.page.lines.push(Line {
        content: LineContent::Markdown(header_spans),
        indent: 0,
    });

    // Separator row
    let mut sep = "| ".to_string();
    for (i, width) in col_widths.iter().enumerate() {
        let alignment = alignments.get(i).copied().unwrap_or(Alignment::Default);
        let dashes = match alignment {
            Alignment::Left => format!(":{}", "-".repeat(width.saturating_sub(1))),
            Alignment::Right => format!("{}:", "-".repeat(width.saturating_sub(1))),
            Alignment::Center => {
                format!(":{}:", "-".repeat(width.saturating_sub(2).max(1)))
            }
            Alignment::Default => "-".repeat(*width),
        };
        sep.push_str(&format!("{} | ", dashes));
    }
    ctx.page.lines.push(Line {
        content: LineContent::Markdown(vec![StyledSpan {
            text: sep,
            style: SpanStyle {
                fg: Some(Color::DarkGray),
                ..Default::default()
            },
            link: None,
        }]),
        indent: 0,
    });

    // Data rows
    for row in rows {
        let mut row_spans = vec![StyledSpan {
            text: "| ".to_string(),
            style: SpanStyle::default(),
            link: None,
        }];
        for (i, cell) in row.iter().enumerate() {
            let text = cell.content.plain_text();
            let width = col_widths.get(i).copied().unwrap_or(text.len());
            row_spans.push(StyledSpan {
                text: format!("{:width$}", text, width = width),
                style: SpanStyle::default(),
                link: None,
            });
            row_spans.push(StyledSpan {
                text: " | ".to_string(),
                style: SpanStyle::default(),
                link: None,
            });
        }
        ctx.page.lines.push(Line {
            content: LineContent::Markdown(row_spans),
            indent: 0,
        });
    }
}
