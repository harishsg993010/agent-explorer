//! Help overlay widget

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};
use crate::style::Theme;

/// Help overlay widget
pub struct HelpOverlay<'a> {
    theme: &'a Theme,
}

impl<'a> HelpOverlay<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self { theme }
    }
}

impl<'a> Widget for HelpOverlay<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate centered overlay area
        let width = 60.min(area.width - 4);
        let height = 28.min(area.height - 4);
        let x = (area.width - width) / 2;
        let y = (area.height - height) / 2;

        let overlay_area = Rect::new(x, y, width, height);

        // Clear the background
        Clear.render(overlay_area, buf);

        let block = Block::default()
            .title(" Help ")
            .title_alignment(Alignment::Center)
            .title_style(
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.border))
            .style(Style::default().bg(Color::Rgb(20, 20, 20)));

        let key_style = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let desc_style = Style::default().fg(self.theme.fg);
        let section_style = Style::default()
            .fg(self.theme.fg)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);

        let help_content = vec![
            Line::from(Span::styled("Navigation", section_style)),
            Line::from(""),
            Line::from(vec![
                Span::styled("  j / ↓      ", key_style),
                Span::styled("Scroll down", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  k / ↑      ", key_style),
                Span::styled("Scroll up", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+d     ", key_style),
                Span::styled("Page down", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+u     ", key_style),
                Span::styled("Page up", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  g / Home   ", key_style),
                Span::styled("Go to top", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  G / End    ", key_style),
                Span::styled("Go to bottom", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  H          ", key_style),
                Span::styled("Go back", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  L          ", key_style),
                Span::styled("Go forward", desc_style),
            ]),
            Line::from(""),
            Line::from(Span::styled("Interaction", section_style)),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Tab        ", key_style),
                Span::styled("Next focusable element", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  Shift+Tab  ", key_style),
                Span::styled("Previous focusable element", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  Enter      ", key_style),
                Span::styled("Activate focused element", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  1-9        ", key_style),
                Span::styled("Quick jump to numbered link", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  o          ", key_style),
                Span::styled("Open URL bar", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  O          ", key_style),
                Span::styled("Open URL bar (clear)", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  /          ", key_style),
                Span::styled("Search in page", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  n / N      ", key_style),
                Span::styled("Next / previous search result", desc_style),
            ]),
            Line::from(""),
            Line::from(Span::styled("Other", section_style)),
            Line::from(""),
            Line::from(vec![
                Span::styled("  `          ", key_style),
                Span::styled("Toggle console panel", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  r          ", key_style),
                Span::styled("Refresh page", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  ?          ", key_style),
                Span::styled("Toggle help", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  q          ", key_style),
                Span::styled("Quit", desc_style),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Press Esc or ? to close",
                Style::default().fg(self.theme.muted),
            )),
        ];

        let paragraph = Paragraph::new(help_content)
            .block(block)
            .alignment(Alignment::Left);

        paragraph.render(overlay_area, buf);
    }
}
