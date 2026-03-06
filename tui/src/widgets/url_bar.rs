//! URL bar widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use crate::app::{App, AppState};
use crate::style::Theme;

/// URL bar widget
pub struct UrlBar<'a> {
    app: &'a App,
    theme: &'a Theme,
}

impl<'a> UrlBar<'a> {
    pub fn new(app: &'a App, theme: &'a Theme) -> Self {
        Self { app, theme }
    }
}

impl<'a> Widget for UrlBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let is_editing = matches!(self.app.state, AppState::UrlEditing);

        // Navigation buttons
        let back_style = if self.app.can_go_back() {
            Style::default().fg(self.theme.fg)
        } else {
            Style::default().fg(self.theme.muted)
        };

        let forward_style = if self.app.can_go_forward() {
            Style::default().fg(self.theme.fg)
        } else {
            Style::default().fg(self.theme.muted)
        };

        let refresh_style = Style::default().fg(self.theme.fg);

        // Build the URL bar content
        let mut spans = vec![
            Span::styled(" [", Style::default().fg(self.theme.muted)),
            Span::styled("◀", back_style),
            Span::styled("] [", Style::default().fg(self.theme.muted)),
            Span::styled("▶", forward_style),
            Span::styled("] [", Style::default().fg(self.theme.muted)),
            Span::styled("↻", refresh_style),
            Span::styled("] ", Style::default().fg(self.theme.muted)),
        ];

        // URL input area
        let url_style = if is_editing {
            Style::default().fg(self.theme.fg).bg(self.theme.selection)
        } else {
            Style::default().fg(self.theme.accent)
        };

        // Calculate available width for URL
        let prefix_width = 16; // "[◀] [▶] [↻] "
        let suffix_width = 5;  // " [⏎]"
        let url_width = area.width.saturating_sub(prefix_width + suffix_width) as usize;

        if is_editing {
            // Show URL with cursor
            let url = &self.app.url_input;
            let cursor = self.app.url_cursor;

            // Ensure URL fits in available space
            let (display_url, cursor_pos) = if url.len() > url_width {
                // Scroll the URL to keep cursor visible
                let start = if cursor > url_width - 5 {
                    cursor.saturating_sub(url_width - 5)
                } else {
                    0
                };
                let end = (start + url_width).min(url.len());
                (&url[start..end], cursor - start)
            } else {
                (url.as_str(), cursor)
            };

            // Build URL with cursor
            let before_cursor = &display_url[..cursor_pos.min(display_url.len())];
            let at_cursor = display_url.chars().nth(cursor_pos).map(|c| c.to_string());
            let after_cursor = if cursor_pos < display_url.len() {
                &display_url[cursor_pos + 1..]
            } else {
                ""
            };

            spans.push(Span::styled(before_cursor, url_style));

            // Cursor character (highlighted)
            if let Some(c) = at_cursor {
                spans.push(Span::styled(
                    c,
                    Style::default()
                        .fg(Color::Black)
                        .bg(self.theme.fg)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                // Cursor at end - show block
                spans.push(Span::styled(
                    " ",
                    Style::default().bg(self.theme.fg),
                ));
            }

            spans.push(Span::styled(after_cursor, url_style));

            // Padding
            let current_len = before_cursor.len() + 1 + after_cursor.len();
            if current_len < url_width {
                spans.push(Span::styled(
                    "_".repeat(url_width - current_len),
                    Style::default().fg(self.theme.muted),
                ));
            }
        } else {
            // Show URL normally
            let url = &self.app.url_input;
            let display_url = if url.len() > url_width {
                &url[..url_width]
            } else {
                url.as_str()
            };

            spans.push(Span::styled(display_url, url_style));

            // Padding
            if display_url.len() < url_width {
                spans.push(Span::styled(
                    "_".repeat(url_width - display_url.len()),
                    Style::default().fg(self.theme.muted),
                ));
            }
        }

        // Enter button
        spans.push(Span::styled(" [", Style::default().fg(self.theme.muted)));
        spans.push(Span::styled(
            "⏎",
            if is_editing {
                Style::default().fg(self.theme.success)
            } else {
                Style::default().fg(self.theme.muted)
            },
        ));
        spans.push(Span::styled("]", Style::default().fg(self.theme.muted)));

        let line = Line::from(spans);
        let block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(self.theme.border));

        let paragraph = Paragraph::new(line).block(block);
        paragraph.render(area, buf);
    }
}
