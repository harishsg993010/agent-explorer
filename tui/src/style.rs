//! Color and style definitions for the TUI

use ratatui::style::{Color, Modifier, Style};

/// Theme colors
pub struct Theme {
    /// Background color
    pub bg: Color,
    /// Primary foreground
    pub fg: Color,
    /// Accent color (links, highlights)
    pub accent: Color,
    /// Secondary accent
    pub accent2: Color,
    /// Error color
    pub error: Color,
    /// Warning color
    pub warning: Color,
    /// Success color
    pub success: Color,
    /// Muted/dim color
    pub muted: Color,
    /// Border color
    pub border: Color,
    /// Selection/focus background
    pub selection: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Dark theme
    pub fn dark() -> Self {
        Self {
            bg: Color::Reset,
            fg: Color::White,
            accent: Color::Cyan,
            accent2: Color::Blue,
            error: Color::Red,
            warning: Color::Yellow,
            success: Color::Green,
            muted: Color::DarkGray,
            border: Color::Gray,
            selection: Color::DarkGray,
        }
    }

    /// Light theme
    pub fn light() -> Self {
        Self {
            bg: Color::White,
            fg: Color::Black,
            accent: Color::Blue,
            accent2: Color::Magenta,
            error: Color::Red,
            warning: Color::Rgb(180, 120, 0),
            success: Color::Green,
            muted: Color::Gray,
            border: Color::DarkGray,
            selection: Color::LightBlue,
        }
    }
}

/// Style for URL bar
pub fn url_bar_style(theme: &Theme, focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(theme.fg)
            .bg(theme.selection)
    } else {
        Style::default()
            .fg(theme.fg)
    }
}

/// Style for content area
pub fn content_style(theme: &Theme) -> Style {
    Style::default().fg(theme.fg)
}

/// Style for links
pub fn link_style(theme: &Theme, focused: bool) -> Style {
    let style = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::UNDERLINED);

    if focused {
        style.bg(theme.selection)
    } else {
        style
    }
}

/// Style for link numbers
pub fn link_number_style(theme: &Theme) -> Style {
    Style::default().fg(theme.muted)
}

/// Style for headings
pub fn heading_style(theme: &Theme, level: u8) -> Style {
    let color = match level {
        1 => theme.accent,
        2 => theme.accent2,
        3 => Color::Magenta,
        _ => theme.fg,
    };

    Style::default()
        .fg(color)
        .add_modifier(Modifier::BOLD)
}

/// Style for inline code
pub fn code_style(theme: &Theme) -> Style {
    Style::default()
        .fg(Color::Yellow)
        .bg(Color::Rgb(40, 40, 40))
}

/// Style for code blocks
pub fn code_block_style(theme: &Theme) -> Style {
    Style::default()
        .fg(Color::White)
        .bg(Color::Rgb(30, 30, 30))
}

/// Style for buttons
pub fn button_style(theme: &Theme, focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(theme.bg)
            .bg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme.fg)
            .add_modifier(Modifier::BOLD)
    }
}

/// Style for text inputs
pub fn input_style(theme: &Theme, focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(theme.fg)
            .bg(theme.selection)
    } else {
        Style::default()
            .fg(theme.fg)
    }
}

/// Style for input placeholder
pub fn placeholder_style(theme: &Theme) -> Style {
    Style::default().fg(theme.muted)
}

/// Style for checkbox
pub fn checkbox_style(theme: &Theme, checked: bool, focused: bool) -> Style {
    let style = if checked {
        Style::default().fg(theme.success)
    } else {
        Style::default().fg(theme.fg)
    };

    if focused {
        style.bg(theme.selection)
    } else {
        style
    }
}

/// Style for status bar
pub fn status_bar_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.muted)
        .bg(Color::Rgb(30, 30, 30))
}

/// Style for error messages
pub fn error_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.error)
        .add_modifier(Modifier::BOLD)
}

/// Style for warning messages
pub fn warning_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.warning)
}

/// Style for success messages
pub fn success_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.success)
}

/// Style for console panel header
pub fn console_header_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.fg)
        .bg(Color::Rgb(40, 40, 40))
        .add_modifier(Modifier::BOLD)
}

/// Style for console log entries
pub fn console_log_style(theme: &Theme) -> Style {
    Style::default().fg(theme.muted)
}

/// Style for console error entries
pub fn console_error_style(theme: &Theme) -> Style {
    Style::default().fg(theme.error)
}

/// Style for console warning entries
pub fn console_warn_style(theme: &Theme) -> Style {
    Style::default().fg(theme.warning)
}

/// Style for horizontal rule
pub fn hr_style(theme: &Theme) -> Style {
    Style::default().fg(theme.muted)
}

/// Style for bold text
pub fn bold_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.fg)
        .add_modifier(Modifier::BOLD)
}

/// Style for italic text
pub fn italic_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.fg)
        .add_modifier(Modifier::ITALIC)
}

/// Style for loading indicator
pub fn loading_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD)
}

/// Style for help overlay
pub fn help_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.fg)
        .bg(Color::Rgb(20, 20, 20))
}

/// Style for help key bindings
pub fn help_key_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD)
}
