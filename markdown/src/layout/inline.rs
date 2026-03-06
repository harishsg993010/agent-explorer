//! Inline layout with line boxes and text wrapping.
//!
//! Handles:
//! - Text tokenization and word breaking
//! - Line box creation
//! - Text wrapping at word boundaries
//! - Inline formatting (bold, italic, code, links)
//! - Text alignment and justification

use super::{TextAlign, TextOverflow, Viewport, WhiteSpace};
use crate::ast::{InlineContent, Span, SpanKind};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// A line box containing inline content
#[derive(Debug, Clone)]
pub struct LineBox {
    /// Spans on this line
    pub spans: Vec<LineSpan>,
    /// Total width of the line
    pub width: usize,
    /// Text alignment
    pub align: TextAlign,
}

/// A span within a line box
#[derive(Debug, Clone)]
pub struct LineSpan {
    pub kind: SpanKind,
    pub text: String,
    pub width: usize,
}

impl LineBox {
    pub fn new(align: TextAlign) -> Self {
        LineBox {
            spans: Vec::new(),
            width: 0,
            align,
        }
    }

    pub fn push(&mut self, span: LineSpan) {
        self.width += span.width;
        self.spans.push(span);
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }
}

/// Inline layout context
pub struct InlineLayoutContext<'a> {
    pub viewport: &'a Viewport,
    pub available_width: usize,
    pub white_space: WhiteSpace,
    pub text_align: TextAlign,
    pub text_overflow: TextOverflow,
    /// Whether to clip content that overflows (single line mode)
    pub single_line: bool,
}

impl<'a> InlineLayoutContext<'a> {
    pub fn new(viewport: &'a Viewport, available_width: usize) -> Self {
        InlineLayoutContext {
            viewport,
            available_width,
            white_space: WhiteSpace::Normal,
            text_align: TextAlign::Left,
            text_overflow: TextOverflow::Clip,
            single_line: false,
        }
    }

    pub fn with_white_space(mut self, ws: WhiteSpace) -> Self {
        self.white_space = ws;
        self
    }

    pub fn with_align(mut self, align: TextAlign) -> Self {
        self.text_align = align;
        self
    }

    pub fn with_text_overflow(mut self, overflow: TextOverflow) -> Self {
        self.text_overflow = overflow;
        self
    }

    pub fn with_single_line(mut self, single: bool) -> Self {
        self.single_line = single;
        self
    }

    /// Apply text overflow truncation if needed
    pub fn truncate_if_needed(&self, text: &str) -> String {
        if !self.single_line {
            return text.to_string();
        }

        let width = text.width();
        if width <= self.available_width {
            return text.to_string();
        }

        match self.text_overflow {
            TextOverflow::Clip => {
                // Just truncate without indicator
                truncate_to_width(text, self.available_width)
            }
            TextOverflow::Ellipsis => {
                // Truncate with ellipsis
                if self.available_width < 2 {
                    return String::new();
                }
                let truncated = truncate_to_width(text, self.available_width - 1);
                format!("{}…", truncated)
            }
        }
    }
}

/// Truncate text to fit within a given display width
fn truncate_to_width(text: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut width = 0;

    for grapheme in text.graphemes(true) {
        let g_width = grapheme.width();
        if width + g_width > max_width {
            break;
        }
        result.push_str(grapheme);
        width += g_width;
    }

    result
}

/// Layout inline content into line boxes
pub fn layout_inline(content: &InlineContent, ctx: &InlineLayoutContext) -> Vec<LineBox> {
    let mut lines = Vec::new();
    let mut current_line = LineBox::new(ctx.text_align);
    let mut remaining_width = ctx.available_width;

    for span in &content.spans {
        match &span.kind {
            SpanKind::LineBreak { hard } => {
                if *hard || ctx.white_space.preserves_whitespace() {
                    lines.push(std::mem::replace(
                        &mut current_line,
                        LineBox::new(ctx.text_align),
                    ));
                    remaining_width = ctx.available_width;
                }
            }

            SpanKind::Link { url, title: _, link_id } => {
                let text = &span.content;
                let formatted = format_link_for_width(text, url, remaining_width);
                let width = display_width(&formatted);

                if width > remaining_width && !current_line.is_empty() && ctx.white_space.wraps() {
                    lines.push(std::mem::replace(
                        &mut current_line,
                        LineBox::new(ctx.text_align),
                    ));
                    remaining_width = ctx.available_width;
                }

                current_line.push(LineSpan {
                    kind: SpanKind::Link {
                        url: url.clone(),
                        title: None,
                        link_id: *link_id,
                    },
                    text: formatted.clone(),
                    width: display_width(&formatted),
                });
                remaining_width = remaining_width.saturating_sub(display_width(&formatted));
            }

            SpanKind::Image { url, alt } => {
                let text = format!("![{}]", alt);
                let width = display_width(&text);

                if width > remaining_width && !current_line.is_empty() && ctx.white_space.wraps() {
                    lines.push(std::mem::replace(
                        &mut current_line,
                        LineBox::new(ctx.text_align),
                    ));
                    remaining_width = ctx.available_width;
                }

                current_line.push(LineSpan {
                    kind: SpanKind::Image {
                        url: url.clone(),
                        alt: alt.clone(),
                    },
                    text,
                    width,
                });
                remaining_width = remaining_width.saturating_sub(width);
            }

            _ => {
                let text = &span.content;
                if text.is_empty() {
                    continue;
                }

                // Tokenize text into words
                let tokens = tokenize_text(text, ctx.white_space);

                for token in tokens {
                    let width = display_width(&token);

                    if width > remaining_width && !current_line.is_empty() && ctx.white_space.wraps()
                    {
                        lines.push(std::mem::replace(
                            &mut current_line,
                            LineBox::new(ctx.text_align),
                        ));
                        remaining_width = ctx.available_width;
                    }

                    // Handle tokens wider than available width
                    if width > ctx.available_width && ctx.white_space.wraps() {
                        let broken = break_word(&token, ctx.available_width);
                        for (i, part) in broken.into_iter().enumerate() {
                            if i > 0 {
                                lines.push(std::mem::replace(
                                    &mut current_line,
                                    LineBox::new(ctx.text_align),
                                ));
                                remaining_width = ctx.available_width;
                            }
                            let part_width = display_width(&part);
                            current_line.push(LineSpan {
                                kind: span.kind.clone(),
                                text: part,
                                width: part_width,
                            });
                            remaining_width = remaining_width.saturating_sub(part_width);
                        }
                    } else {
                        current_line.push(LineSpan {
                            kind: span.kind.clone(),
                            text: token,
                            width,
                        });
                        remaining_width = remaining_width.saturating_sub(width);
                    }
                }
            }
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines
}

/// Tokenize text into words and whitespace based on white-space mode
fn tokenize_text(text: &str, white_space: WhiteSpace) -> Vec<String> {
    let mut tokens = Vec::new();

    if white_space.preserves_whitespace() {
        // Preserve whitespace: split on newlines only
        for line in text.split('\n') {
            if !tokens.is_empty() {
                tokens.push("\n".to_string());
            }
            if !line.is_empty() {
                tokens.push(line.to_string());
            }
        }
    } else {
        // Normal mode: split on whitespace, collapse multiple spaces
        let mut current_word = String::new();
        let mut prev_was_space = false;

        for c in text.chars() {
            if c.is_whitespace() {
                if !current_word.is_empty() {
                    tokens.push(current_word.clone());
                    current_word.clear();
                }
                if !prev_was_space && !tokens.is_empty() {
                    tokens.push(" ".to_string());
                }
                prev_was_space = true;
            } else {
                current_word.push(c);
                prev_was_space = false;
            }
        }

        if !current_word.is_empty() {
            tokens.push(current_word);
        }
    }

    tokens
}

/// Break a word that's too long into parts
fn break_word(word: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![word.to_string()];
    }

    let mut parts = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for grapheme in word.graphemes(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);

        if current_width + grapheme_width > max_width && !current.is_empty() {
            parts.push(std::mem::take(&mut current));
            current_width = 0;
        }

        current.push_str(grapheme);
        current_width += grapheme_width;
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

/// Calculate display width of text (handling CJK and emoji)
pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Format a link for a given width
fn format_link_for_width(text: &str, url: &str, _max_width: usize) -> String {
    format!("[{}]({})", text, url)
}

/// Render line boxes to a string
pub fn render_lines(lines: &[LineBox], available_width: usize) -> String {
    let mut output = String::new();

    for line in lines {
        let line_text = render_line(line);

        // Apply alignment
        let aligned = match line.align {
            TextAlign::Left | TextAlign::Start => line_text,
            TextAlign::Right | TextAlign::End => {
                let padding = available_width.saturating_sub(line.width);
                format!("{:>width$}", line_text, width = line_text.len() + padding)
            }
            TextAlign::Center => {
                let padding = available_width.saturating_sub(line.width);
                let left_pad = padding / 2;
                format!("{:>width$}", line_text, width = line_text.len() + left_pad)
            }
            TextAlign::Justify => {
                // For justify, we'd need to add extra spaces between words
                // For now, just left-align
                line_text
            }
        };

        output.push_str(&aligned);
        output.push('\n');
    }

    output.trim_end_matches('\n').to_string()
}

/// Render a single line box to a string
fn render_line(line: &LineBox) -> String {
    let mut output = String::new();

    for span in &line.spans {
        let formatted = format_span(span);
        output.push_str(&formatted);
    }

    output
}

/// Format a single span with markdown formatting
fn format_span(span: &LineSpan) -> String {
    match &span.kind {
        SpanKind::Text => span.text.clone(),
        SpanKind::Strong => format!("**{}**", span.text),
        SpanKind::Emphasis => format!("*{}*", span.text),
        SpanKind::StrongEmphasis => format!("***{}***", span.text),
        SpanKind::Code => format!("`{}`", span.text),
        SpanKind::Strikethrough => format!("~~{}~~", span.text),
        SpanKind::Link { .. } => span.text.clone(), // Already formatted
        SpanKind::Image { alt, .. } => format!("![{}]", alt),
        SpanKind::LineBreak { hard } => {
            if *hard {
                "  \n".to_string()
            } else {
                "\n".to_string()
            }
        }
        SpanKind::Underline => format!("<u>{}</u>", span.text),
        SpanKind::Highlight => format!("=={}", span.text),
        SpanKind::Superscript => format!("^{}", span.text),
        SpanKind::Subscript => format!("~{}", span.text),
        SpanKind::Kbd => format!("`{}`", span.text),
        SpanKind::WidgetRef { widget_id } => format!("{{{{WIDGET:{}}}}}", widget_id.as_u64()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_width() {
        assert_eq!(display_width("hello"), 5);
        assert_eq!(display_width("hello world"), 11);
        assert_eq!(display_width("日本語"), 6); // CJK characters are double-width
    }

    #[test]
    fn test_tokenize_normal() {
        let tokens = tokenize_text("hello  world", WhiteSpace::Normal);
        assert_eq!(tokens, vec!["hello", " ", "world"]);
    }

    #[test]
    fn test_tokenize_pre() {
        let tokens = tokenize_text("hello  world", WhiteSpace::Pre);
        assert_eq!(tokens, vec!["hello  world"]);
    }

    #[test]
    fn test_break_word() {
        let parts = break_word("hello", 3);
        assert_eq!(parts, vec!["hel", "lo"]);
    }

    #[test]
    fn test_layout_simple() {
        let viewport = Viewport::new(80);
        let ctx = InlineLayoutContext::new(&viewport, 80);

        let content = InlineContent::text("Hello world");
        let lines = layout_inline(&content, &ctx);

        assert_eq!(lines.len(), 1);
        assert!(!lines[0].is_empty());
    }
}
