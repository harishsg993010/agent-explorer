//! Content area widget - displays the rendered page

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use crate::app::{App, AppState};
use crate::render::{LineContent, StyledSpan};
use crate::style::Theme;

/// Content area widget
pub struct ContentArea<'a> {
    app: &'a App,
    theme: &'a Theme,
}

impl<'a> ContentArea<'a> {
    pub fn new(app: &'a App, theme: &'a Theme) -> Self {
        Self { app, theme }
    }

    /// Render a styled span to ratatui Span
    fn render_styled_span(&self, span: &StyledSpan, focused: bool) -> Span<'static> {
        let mut style = span.style.to_ratatui_style();

        // Override for links
        if span.link.is_some() {
            style = style.fg(self.theme.accent).add_modifier(Modifier::UNDERLINED);
            if focused {
                style = style.bg(self.theme.selection);
            }
        }

        Span::styled(span.text.clone(), style)
    }

    /// Render a line content to ratatui Line
    fn render_line(&self, line: &crate::render::Line, line_idx: usize) -> Line<'static> {
        let is_focused = self.app.focus_index
            .and_then(|idx| self.app.page.as_ref()?.interactives.get(idx))
            .map(|i| i.line_index == line_idx)
            .unwrap_or(false);

        let indent = " ".repeat(line.indent as usize);

        match &line.content {
            LineContent::Markdown(spans) => {
                let mut ratatui_spans: Vec<Span> = vec![Span::raw(indent)];
                for span in spans {
                    let focused = is_focused && span.link.is_some();
                    ratatui_spans.push(self.render_styled_span(span, focused));
                }
                Line::from(ratatui_spans)
            }

            LineContent::TextInput { id, placeholder, password, width, .. } => {
                let is_editing = matches!(self.app.state, AppState::FormInput { element_id } if element_id == *id);
                let display_value = self.app.get_form_value(*id);
                let cursor_pos = self.app.get_form_cursor(*id);
                let display_width = *width as usize;

                let prefix = if is_focused || is_editing { "> " } else { "  " };
                let prefix_style = if is_focused || is_editing {
                    Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.muted)
                };

                let mut spans = vec![
                    Span::raw(indent),
                    Span::styled(prefix, prefix_style),
                    Span::styled("[", Style::default().fg(self.theme.muted)),
                ];

                if is_editing {
                    // Show content with cursor - use char-based indexing for proper UTF-8 handling
                    let content = if *password {
                        "*".repeat(display_value.chars().count())
                    } else {
                        display_value.to_string()
                    };

                    let char_count = content.chars().count();
                    let cursor = cursor_pos.min(char_count);

                    // Get character boundaries properly
                    let chars: Vec<char> = content.chars().collect();
                    let before: String = chars[..cursor].iter().collect();
                    let at_cursor = chars.get(cursor).copied();
                    let after: String = if cursor < char_count { chars[cursor + 1..].iter().collect() } else { String::new() };

                    let text_style = Style::default().fg(self.theme.fg).bg(self.theme.selection);
                    let cursor_style = Style::default().fg(Color::Black).bg(self.theme.fg).add_modifier(Modifier::BOLD);

                    spans.push(Span::styled(before.clone(), text_style));

                    if let Some(c) = at_cursor {
                        spans.push(Span::styled(c.to_string(), cursor_style));
                    } else {
                        spans.push(Span::styled(" ", cursor_style));
                    }

                    spans.push(Span::styled(after.clone(), text_style));

                    // Padding - use char count not byte length
                    let current_len = before.chars().count() + 1 + after.chars().count();
                    if current_len < display_width {
                        spans.push(Span::styled(
                            "_".repeat(display_width - current_len),
                            Style::default().fg(self.theme.muted).bg(self.theme.selection),
                        ));
                    }
                } else {
                    // Normal display (not editing)
                    let content = if display_value.is_empty() {
                        placeholder.clone()
                    } else if *password {
                        "*".repeat(display_value.chars().count())
                    } else {
                        display_value.to_string()
                    };

                    let style = if is_focused {
                        Style::default().fg(self.theme.fg).bg(self.theme.selection)
                    } else if display_value.is_empty() {
                        Style::default().fg(self.theme.muted)
                    } else {
                        Style::default().fg(self.theme.fg)
                    };

                    // Use char count for proper display width calculation
                    let char_count = content.chars().count();
                    let padded = if char_count < display_width {
                        format!("{}{}", content, "_".repeat(display_width - char_count))
                    } else {
                        content.chars().take(display_width).collect()
                    };

                    spans.push(Span::styled(padded, style));
                }

                spans.push(Span::styled("]", Style::default().fg(self.theme.muted)));

                Line::from(spans)
            }

            LineContent::Button { id, label } => {
                let style = if is_focused {
                    Style::default()
                        .fg(Color::Black)
                        .bg(self.theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(self.theme.fg)
                        .add_modifier(Modifier::BOLD)
                };

                let prefix = if is_focused { "> " } else { "  " };

                Line::from(vec![
                    Span::raw(indent),
                    Span::styled(prefix, if is_focused {
                        Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.theme.muted)
                    }),
                    Span::styled("[ ", Style::default().fg(self.theme.muted)),
                    Span::styled(label.clone(), style),
                    Span::styled(" ]", Style::default().fg(self.theme.muted)),
                ])
            }

            LineContent::Checkbox { id, label, checked } => {
                let check_char = if *checked { "✓" } else { " " };
                let check_style = if *checked {
                    Style::default().fg(self.theme.success)
                } else {
                    Style::default().fg(self.theme.muted)
                };

                let style = if is_focused {
                    Style::default().bg(self.theme.selection)
                } else {
                    Style::default()
                };

                let prefix = if is_focused { "> " } else { "  " };

                Line::from(vec![
                    Span::raw(indent),
                    Span::styled(prefix, if is_focused {
                        Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.theme.muted)
                    }),
                    Span::styled("[", Style::default().fg(self.theme.muted)),
                    Span::styled(check_char, check_style),
                    Span::styled("] ", Style::default().fg(self.theme.muted)),
                    Span::styled(label.clone(), style),
                ])
            }

            LineContent::Radio { id, name, label, value, checked } => {
                let check_char = if *checked { "•" } else { " " };
                let check_style = if *checked {
                    Style::default().fg(self.theme.accent)
                } else {
                    Style::default().fg(self.theme.muted)
                };

                let style = if is_focused {
                    Style::default().bg(self.theme.selection)
                } else {
                    Style::default()
                };

                let prefix = if is_focused { "> " } else { "  " };

                Line::from(vec![
                    Span::raw(indent),
                    Span::styled(prefix, if is_focused {
                        Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.theme.muted)
                    }),
                    Span::styled("(", Style::default().fg(self.theme.muted)),
                    Span::styled(check_char, check_style),
                    Span::styled(") ", Style::default().fg(self.theme.muted)),
                    Span::styled(label.clone(), style),
                ])
            }

            LineContent::Select { id, options, selected } => {
                let selected_text = options.get(*selected).cloned().unwrap_or_default();

                let style = if is_focused {
                    Style::default().bg(self.theme.selection)
                } else {
                    Style::default()
                };

                let prefix = if is_focused { "> " } else { "  " };

                Line::from(vec![
                    Span::raw(indent),
                    Span::styled(prefix, if is_focused {
                        Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.theme.muted)
                    }),
                    Span::styled("[", Style::default().fg(self.theme.muted)),
                    Span::styled(selected_text, style),
                    Span::styled(" ▼]", Style::default().fg(self.theme.muted)),
                ])
            }

            LineContent::CodeBlock { code, language } => {
                let style = Style::default()
                    .fg(Color::Yellow)
                    .bg(Color::Rgb(30, 30, 30));

                Line::from(vec![
                    Span::raw(indent),
                    Span::styled(code.clone(), style),
                ])
            }

            LineContent::HorizontalRule => {
                let width = 50;
                Line::from(vec![
                    Span::raw(indent),
                    Span::styled(
                        "─".repeat(width),
                        Style::default().fg(self.theme.muted),
                    ),
                ])
            }

            LineContent::Empty => Line::from(""),
        }
    }
}

impl<'a> Widget for ContentArea<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Loading state
        if self.app.loading {
            let loading_text = format!(
                "Loading... {:.0}%",
                self.app.loading_progress * 100.0
            );
            let loading = Paragraph::new(loading_text)
                .style(Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD))
                .block(Block::default());
            loading.render(area, buf);
            return;
        }

        // No page loaded
        let page = match &self.app.page {
            Some(p) => p,
            None => {
                let welcome = vec![
                    Line::from(Span::styled(
                        "Semantic Browser",
                        Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD),
                    )),
                    Line::from(""),
                    Line::from("Press 'o' to enter a URL"),
                    Line::from("Press '?' for help"),
                ];
                let paragraph = Paragraph::new(welcome)
                    .block(Block::default());
                paragraph.render(area, buf);
                return;
            }
        };

        // Calculate visible lines
        let height = area.height as usize;
        let start = self.app.scroll_offset;
        let end = (start + height).min(page.lines.len());

        // Render visible lines
        let lines: Vec<Line> = page.lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| self.render_line(line, start + i))
            .collect();

        // Highlight search results
        let search_line = if !self.app.search_results.is_empty() {
            self.app.search_results.get(self.app.search_index).copied()
        } else {
            None
        };

        let paragraph = Paragraph::new(lines)
            .block(Block::default())
            .wrap(Wrap { trim: false });

        paragraph.render(area, buf);

        // Render scroll indicator
        if page.lines.len() > height {
            let scroll_percentage = if self.app.max_scroll() > 0 {
                (self.app.scroll_offset as f32 / self.app.max_scroll() as f32 * 100.0) as u16
            } else {
                0
            };

            let indicator = format!(" {}% ", scroll_percentage);
            let x = area.right().saturating_sub(indicator.len() as u16 + 1);
            let y = area.bottom().saturating_sub(1);

            if x > area.left() && y >= area.top() {
                buf.set_string(
                    x,
                    y,
                    &indicator,
                    Style::default().fg(self.theme.muted).bg(Color::Rgb(40, 40, 40)),
                );
            }
        }
    }
}
