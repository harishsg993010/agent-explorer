//! Console panel widget for JS errors and logs

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use crate::app::App;
use crate::style::Theme;

/// Console entry types
#[derive(Debug, Clone)]
pub enum ConsoleEntry {
    /// Log message
    Log {
        level: LogLevel,
        message: String,
    },
    /// JavaScript error
    JsError {
        error_type: String,
        message: String,
        source_url: Option<String>,
        line: Option<u32>,
        column: Option<u32>,
    },
    /// Network error
    NetworkError {
        url: String,
        status: u16,
        message: String,
    },
}

/// Log levels
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogLevel {
    Log,
    Info,
    Warn,
    Error,
    Debug,
}

impl ConsoleEntry {
    /// Create a log entry
    pub fn log(message: impl Into<String>) -> Self {
        Self::Log {
            level: LogLevel::Log,
            message: message.into(),
        }
    }

    /// Create an info entry
    pub fn info(message: impl Into<String>) -> Self {
        Self::Log {
            level: LogLevel::Info,
            message: message.into(),
        }
    }

    /// Create a warning entry
    pub fn warn(message: impl Into<String>) -> Self {
        Self::Log {
            level: LogLevel::Warn,
            message: message.into(),
        }
    }

    /// Create an error log entry
    pub fn error(message: impl Into<String>) -> Self {
        Self::Log {
            level: LogLevel::Error,
            message: message.into(),
        }
    }

    /// Create a JS error entry
    pub fn js_error(
        error_type: impl Into<String>,
        message: impl Into<String>,
        source_url: Option<String>,
        line: Option<u32>,
        column: Option<u32>,
    ) -> Self {
        Self::JsError {
            error_type: error_type.into(),
            message: message.into(),
            source_url,
            line,
            column,
        }
    }

    /// Create a network error entry
    pub fn network_error(
        url: impl Into<String>,
        status: u16,
        message: impl Into<String>,
    ) -> Self {
        Self::NetworkError {
            url: url.into(),
            status,
            message: message.into(),
        }
    }

    /// Get the icon for this entry
    fn icon(&self) -> &'static str {
        match self {
            Self::Log { level, .. } => match level {
                LogLevel::Log => "○",
                LogLevel::Info => "ℹ",
                LogLevel::Warn => "⚠",
                LogLevel::Error => "✗",
                LogLevel::Debug => "·",
            },
            Self::JsError { .. } => "✗",
            Self::NetworkError { .. } => "⚡",
        }
    }

    /// Get the style for this entry
    fn style(&self, theme: &Theme) -> Style {
        match self {
            Self::Log { level, .. } => match level {
                LogLevel::Log => Style::default().fg(theme.muted),
                LogLevel::Info => Style::default().fg(Color::Blue),
                LogLevel::Warn => Style::default().fg(theme.warning),
                LogLevel::Error => Style::default().fg(theme.error),
                LogLevel::Debug => Style::default().fg(theme.muted).add_modifier(Modifier::DIM),
            },
            Self::JsError { .. } => Style::default().fg(theme.error),
            Self::NetworkError { .. } => Style::default().fg(theme.warning),
        }
    }

    /// Render the entry to a Line
    fn render(&self, theme: &Theme) -> Vec<Line<'static>> {
        let icon_style = self.style(theme);
        let icon = self.icon();

        match self {
            Self::Log { message, .. } => {
                vec![Line::from(vec![
                    Span::styled(format!(" {} ", icon), icon_style),
                    Span::styled(message.clone(), self.style(theme)),
                ])]
            }
            Self::JsError {
                error_type,
                message,
                source_url,
                line,
                column,
            } => {
                let mut lines = vec![Line::from(vec![
                    Span::styled(format!(" {} ", icon), icon_style),
                    Span::styled(
                        format!("{}: {}", error_type, message),
                        self.style(theme),
                    ),
                ])];

                if let Some(url) = source_url {
                    let location = match (line, column) {
                        (Some(l), Some(c)) => format!("{}:{}:{}", url, l, c),
                        (Some(l), None) => format!("{}:{}", url, l),
                        _ => url.clone(),
                    };
                    lines.push(Line::from(vec![
                        Span::raw("     "),
                        Span::styled(
                            format!("at {}", location),
                            Style::default().fg(theme.muted),
                        ),
                    ]));
                }

                lines
            }
            Self::NetworkError { url, status, message } => {
                vec![Line::from(vec![
                    Span::styled(format!(" {} ", icon), icon_style),
                    Span::styled(
                        format!("[{}] {} - {}", status, message, url),
                        self.style(theme),
                    ),
                ])]
            }
        }
    }
}

/// Console panel widget
pub struct ConsolePanel<'a> {
    app: &'a App,
    theme: &'a Theme,
}

impl<'a> ConsolePanel<'a> {
    pub fn new(app: &'a App, theme: &'a Theme) -> Self {
        Self { app, theme }
    }
}

impl<'a> Widget for ConsolePanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.app.console_visible {
            return;
        }

        let block = Block::default()
            .title(" CONSOLE (` to toggle) ")
            .title_style(
                Style::default()
                    .fg(self.theme.fg)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::TOP)
            .border_style(Style::default().fg(self.theme.border))
            .style(Style::default().bg(Color::Rgb(25, 25, 25)));

        let inner = block.inner(area);
        block.render(area, buf);

        if self.app.console.is_empty() {
            let empty_msg = Paragraph::new(" No console output")
                .style(Style::default().fg(self.theme.muted));
            empty_msg.render(inner, buf);
            return;
        }

        // Collect all lines from entries
        let mut all_lines: Vec<Line> = Vec::new();
        for entry in &self.app.console {
            all_lines.extend(entry.render(self.theme));
        }

        // Calculate visible lines
        let height = inner.height as usize;
        let total = all_lines.len();
        let start = if total > height {
            total - height
        } else {
            0
        };

        let visible_lines: Vec<Line> = all_lines.into_iter().skip(start).collect();

        let paragraph = Paragraph::new(visible_lines);
        paragraph.render(inner, buf);

        // Error/warning count in header
        let error_count = self.app.console.iter().filter(|e| {
            matches!(e, ConsoleEntry::JsError { .. } | ConsoleEntry::Log { level: LogLevel::Error, .. })
        }).count();

        let warn_count = self.app.console.iter().filter(|e| {
            matches!(e, ConsoleEntry::Log { level: LogLevel::Warn, .. } | ConsoleEntry::NetworkError { .. })
        }).count();

        if error_count > 0 || warn_count > 0 {
            let counts = format!(" Errors: {} │ Warnings: {} ", error_count, warn_count);
            let x = area.right().saturating_sub(counts.len() as u16 + 1);
            if x > area.left() {
                buf.set_string(
                    x,
                    area.top(),
                    &counts,
                    Style::default()
                        .fg(if error_count > 0 { self.theme.error } else { self.theme.warning })
                        .bg(Color::Rgb(25, 25, 25)),
                );
            }
        }
    }
}
