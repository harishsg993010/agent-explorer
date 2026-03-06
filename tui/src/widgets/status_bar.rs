//! Status bar widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use crate::app::{App, AppState};
use crate::style::Theme;

/// Status bar widget
pub struct StatusBar<'a> {
    app: &'a App,
    theme: &'a Theme,
}

impl<'a> StatusBar<'a> {
    pub fn new(app: &'a App, theme: &'a Theme) -> Self {
        Self { app, theme }
    }
}

impl<'a> Widget for StatusBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let style = Style::default()
            .fg(self.theme.muted)
            .bg(Color::Rgb(30, 30, 30));

        // Build status content based on state
        let content: Vec<Span> = match &self.app.state {
            AppState::Normal => {
                let mut spans = vec![];

                // Mode indicator
                spans.push(Span::styled(
                    " NORMAL ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ));

                spans.push(Span::styled(" │ ", style));

                // Key hints
                spans.push(Span::styled("j/k", Style::default().fg(self.theme.accent)));
                spans.push(Span::styled(": scroll │ ", style));

                spans.push(Span::styled("Tab", Style::default().fg(self.theme.accent)));
                spans.push(Span::styled(": next │ ", style));

                spans.push(Span::styled("Enter", Style::default().fg(self.theme.accent)));
                spans.push(Span::styled(": activate │ ", style));

                spans.push(Span::styled("1-9", Style::default().fg(self.theme.accent)));
                spans.push(Span::styled(": link │ ", style));

                spans.push(Span::styled("o", Style::default().fg(self.theme.accent)));
                spans.push(Span::styled(": URL │ ", style));

                spans.push(Span::styled("?", Style::default().fg(self.theme.accent)));
                spans.push(Span::styled(": help", style));

                spans
            }

            AppState::UrlEditing => {
                vec![
                    Span::styled(
                        " URL ",
                        Style::default()
                            .fg(Color::Black)
                            .bg(self.theme.success)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" │ ", style),
                    Span::styled("Enter", Style::default().fg(self.theme.accent)),
                    Span::styled(": navigate │ ", style),
                    Span::styled("Esc", Style::default().fg(self.theme.accent)),
                    Span::styled(": cancel │ ", style),
                    Span::styled("Ctrl+U", Style::default().fg(self.theme.accent)),
                    Span::styled(": clear", style),
                ]
            }

            AppState::FormInput { .. } => {
                vec![
                    Span::styled(
                        " INPUT ",
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" │ ", style),
                    Span::styled("Enter", Style::default().fg(self.theme.accent)),
                    Span::styled(": next field │ ", style),
                    Span::styled("Tab", Style::default().fg(self.theme.accent)),
                    Span::styled(": next │ ", style),
                    Span::styled("Esc", Style::default().fg(self.theme.accent)),
                    Span::styled(": exit input", style),
                ]
            }

            AppState::Searching => {
                let search_info = if self.app.search_results.is_empty() {
                    if self.app.search_query.is_empty() {
                        String::new()
                    } else {
                        " (no results)".to_string()
                    }
                } else {
                    format!(
                        " ({}/{})",
                        self.app.search_index + 1,
                        self.app.search_results.len()
                    )
                };

                vec![
                    Span::styled(
                        " SEARCH ",
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" │ /", style),
                    Span::styled(
                        self.app.search_query.clone(),
                        Style::default().fg(self.theme.fg),
                    ),
                    Span::styled(search_info, Style::default().fg(self.theme.muted)),
                    Span::styled(" │ ", style),
                    Span::styled("Enter", Style::default().fg(self.theme.accent)),
                    Span::styled(": search │ ", style),
                    Span::styled("Esc", Style::default().fg(self.theme.accent)),
                    Span::styled(": cancel", style),
                ]
            }

            AppState::Help => {
                vec![
                    Span::styled(
                        " HELP ",
                        Style::default()
                            .fg(Color::Black)
                            .bg(self.theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" │ Press ", style),
                    Span::styled("Esc", Style::default().fg(self.theme.accent)),
                    Span::styled(" or ", style),
                    Span::styled("?", Style::default().fg(self.theme.accent)),
                    Span::styled(" to close", style),
                ]
            }

            AppState::Error => {
                vec![
                    Span::styled(
                        " ERROR ",
                        Style::default()
                            .fg(Color::White)
                            .bg(self.theme.error)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" │ Press ", style),
                    Span::styled("Esc", Style::default().fg(self.theme.accent)),
                    Span::styled(" or ", style),
                    Span::styled("Enter", Style::default().fg(self.theme.accent)),
                    Span::styled(" to dismiss", style),
                ]
            }

            AppState::Loading => {
                vec![
                    Span::styled(
                        " LOADING ",
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" │ {:.0}%", self.app.loading_progress * 100.0),
                        style,
                    ),
                    Span::styled(" │ Press ", style),
                    Span::styled("Esc", Style::default().fg(self.theme.accent)),
                    Span::styled(" to cancel", style),
                ]
            }
        };

        // Right side info
        let right_info = if let Some(page) = &self.app.page {
            format!(
                " {} links │ {} ",
                page.numbered_links.len(),
                if self.app.console_visible { "Console: ON" } else { "` for console" }
            )
        } else {
            String::new()
        };

        let line = Line::from(content);
        let block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(self.theme.border));

        // Render main content
        let paragraph = Paragraph::new(line)
            .style(style)
            .block(block);
        paragraph.render(area, buf);

        // Render right side info
        if !right_info.is_empty() {
            let x = area.right().saturating_sub(right_info.len() as u16);
            if x > area.left() {
                buf.set_string(x, area.top() + 1, &right_info, style);
            }
        }
    }
}
