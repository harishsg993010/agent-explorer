//! Event handling for keyboard and mouse input

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind, MouseButton};
use std::time::Duration;
use crate::app::{App, AppState};
use crate::render::ClickTarget;

/// Event handler
pub struct EventHandler;

/// Action to perform after handling an event
#[derive(Debug, Clone)]
pub enum Action {
    /// No action needed
    None,
    /// Navigate to URL (adds to history)
    Navigate(String),
    /// Load URL without adding to history (for back/forward)
    Load(String),
    /// Refresh current page
    Refresh,
    /// Submit form
    SubmitForm(u64),
    /// Click button
    ClickButton(u64),
    /// Quit application
    Quit,
}

impl EventHandler {
    /// Poll for events with timeout
    pub fn poll(timeout: Duration) -> std::io::Result<Option<Event>> {
        if event::poll(timeout)? {
            Ok(Some(event::read()?))
        } else {
            Ok(None)
        }
    }

    /// Handle an event and return action
    pub fn handle(app: &mut App, event: Event) -> Action {
        match event {
            Event::Key(key) => {
                // Only handle key press events, not release (fixes double input on Windows)
                if key.kind != KeyEventKind::Press {
                    return Action::None;
                }
                Self::handle_key(app, key)
            }
            Event::Mouse(mouse) => Self::handle_mouse(app, mouse),
            Event::Resize(width, height) => {
                app.terminal_size = (width, height);
                Action::None
            }
            _ => Action::None,
        }
    }

    /// Handle keyboard event
    fn handle_key(app: &mut App, key: KeyEvent) -> Action {
        match app.state {
            AppState::Normal => Self::handle_normal_key(app, key),
            AppState::UrlEditing => Self::handle_url_key(app, key),
            AppState::FormInput { element_id } => Self::handle_form_key(app, key, element_id),
            AppState::Searching => Self::handle_search_key(app, key),
            AppState::Help => Self::handle_help_key(app, key),
            AppState::Error => Self::handle_error_key(app, key),
            AppState::Loading => Self::handle_loading_key(app, key),
        }
    }

    /// Handle keys in normal mode
    fn handle_normal_key(app: &mut App, key: KeyEvent) -> Action {
        match (key.modifiers, key.code) {
            // Quit
            (KeyModifiers::NONE, KeyCode::Char('q')) => {
                app.should_quit = true;
                Action::Quit
            }

            // Help
            (KeyModifiers::NONE, KeyCode::Char('?')) => {
                app.state = AppState::Help;
                Action::None
            }

            // Scroll
            (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
                app.scroll(1);
                Action::None
            }
            (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
                app.scroll(-1);
                Action::None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('d')) | (KeyModifiers::NONE, KeyCode::PageDown) => {
                let half_page = (app.terminal_size.1 / 2) as i32;
                app.scroll(half_page);
                Action::None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('u')) | (KeyModifiers::NONE, KeyCode::PageUp) => {
                let half_page = (app.terminal_size.1 / 2) as i32;
                app.scroll(-half_page);
                Action::None
            }
            (KeyModifiers::NONE, KeyCode::Char('g')) | (KeyModifiers::NONE, KeyCode::Home) => {
                app.scroll_offset = 0;
                Action::None
            }
            (KeyModifiers::SHIFT, KeyCode::Char('G')) | (KeyModifiers::NONE, KeyCode::End) => {
                app.scroll_offset = app.max_scroll();
                Action::None
            }

            // Focus navigation
            (KeyModifiers::NONE, KeyCode::Tab) => {
                app.focus_next();
                Action::None
            }
            (KeyModifiers::SHIFT, KeyCode::BackTab) => {
                app.focus_prev();
                Action::None
            }

            // Activate focused element
            (KeyModifiers::NONE, KeyCode::Enter) => {
                if let Some(focus_idx) = app.focus_index {
                    if let Some(page) = &app.page {
                        if let Some(interactive) = page.interactives.get(focus_idx) {
                            match &interactive.kind {
                                crate::render::InteractiveKind::Link(num) => {
                                    if let Some(link) = page.numbered_links.get(*num - 1) {
                                        return Action::Navigate(link.url.clone());
                                    }
                                }
                                crate::render::InteractiveKind::Button => {
                                    return Action::ClickButton(interactive.element_id);
                                }
                                crate::render::InteractiveKind::TextInput |
                                crate::render::InteractiveKind::Password => {
                                    app.start_form_input(interactive.element_id);
                                }
                                crate::render::InteractiveKind::Checkbox => {
                                    // Toggle checkbox
                                    if let Some(line) = page.lines.get(interactive.line_index) {
                                        if let crate::render::LineContent::Checkbox { checked, .. } = &line.content {
                                            // Would need mutable access to toggle
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Action::None
            }

            // Quick-jump to links (1-9)
            (KeyModifiers::NONE, KeyCode::Char(c)) if c.is_ascii_digit() && c != '0' => {
                let num = c.to_digit(10).unwrap() as usize;
                if let Some(url) = app.jump_to_link(num) {
                    Action::Navigate(url)
                } else {
                    Action::None
                }
            }

            // URL editing
            (KeyModifiers::NONE, KeyCode::Char('o')) => {
                app.start_url_edit();
                Action::None
            }
            (KeyModifiers::SHIFT, KeyCode::Char('O')) => {
                app.url_clear();
                app.start_url_edit();
                Action::None
            }

            // Navigation (back/forward use Load to avoid adding to history)
            (KeyModifiers::SHIFT, KeyCode::Char('H')) => {
                if let Some(url) = app.go_back() {
                    Action::Load(url)
                } else {
                    Action::None
                }
            }
            (KeyModifiers::SHIFT, KeyCode::Char('L')) => {
                if let Some(url) = app.go_forward() {
                    Action::Load(url)
                } else {
                    Action::None
                }
            }
            (KeyModifiers::NONE, KeyCode::Char('r')) => {
                Action::Refresh
            }

            // Search
            (KeyModifiers::NONE, KeyCode::Char('/')) => {
                app.start_search();
                Action::None
            }
            (KeyModifiers::NONE, KeyCode::Char('n')) => {
                app.next_search_result();
                Action::None
            }
            (KeyModifiers::SHIFT, KeyCode::Char('N')) => {
                app.prev_search_result();
                Action::None
            }

            // Console
            (KeyModifiers::NONE, KeyCode::Char('`')) => {
                app.toggle_console();
                Action::None
            }

            _ => Action::None,
        }
    }

    /// Handle keys in URL editing mode
    fn handle_url_key(app: &mut App, key: KeyEvent) -> Action {
        match (key.modifiers, key.code) {
            // Cancel
            (KeyModifiers::NONE, KeyCode::Esc) => {
                app.state = AppState::Normal;
                Action::None
            }

            // Submit
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let url = app.url_input.clone();
                app.state = AppState::Normal;
                if !url.is_empty() {
                    // Add https:// if no protocol
                    let url = if !url.contains("://") {
                        format!("https://{}", url)
                    } else {
                        url
                    };
                    Action::Navigate(url)
                } else {
                    Action::None
                }
            }

            // Character input
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                app.url_input_char(c);
                Action::None
            }

            // Backspace
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                app.url_input_backspace();
                Action::None
            }

            // Delete
            (KeyModifiers::NONE, KeyCode::Delete) => {
                app.url_input_delete();
                Action::None
            }

            // Cursor movement
            (KeyModifiers::NONE, KeyCode::Left) => {
                app.url_cursor_left();
                Action::None
            }
            (KeyModifiers::NONE, KeyCode::Right) => {
                app.url_cursor_right();
                Action::None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('a')) | (KeyModifiers::NONE, KeyCode::Home) => {
                app.url_cursor = 0;
                Action::None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('e')) | (KeyModifiers::NONE, KeyCode::End) => {
                app.url_cursor = app.url_input.len();
                Action::None
            }

            // Clear line
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                app.url_clear();
                Action::None
            }

            _ => Action::None,
        }
    }

    /// Handle keys in form input mode
    fn handle_form_key(app: &mut App, key: KeyEvent, element_id: u64) -> Action {
        match (key.modifiers, key.code) {
            // Cancel / Exit form mode
            (KeyModifiers::NONE, KeyCode::Esc) => {
                app.state = AppState::Normal;
                Action::None
            }

            // Submit / Next field
            (KeyModifiers::NONE, KeyCode::Enter) => {
                app.state = AppState::Normal;
                app.focus_next();
                Action::None
            }

            // Tab to next field
            (KeyModifiers::NONE, KeyCode::Tab) => {
                app.state = AppState::Normal;
                app.focus_next();
                // Re-enter form mode if next element is also input
                if let Some(focus_idx) = app.focus_index {
                    if let Some(page) = &app.page {
                        if let Some(interactive) = page.interactives.get(focus_idx) {
                            match interactive.kind {
                                crate::render::InteractiveKind::TextInput |
                                crate::render::InteractiveKind::Password => {
                                    app.start_form_input(interactive.element_id);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Action::None
            }

            // Shift+Tab to previous field
            (KeyModifiers::SHIFT, KeyCode::BackTab) => {
                app.state = AppState::Normal;
                app.focus_prev();
                Action::None
            }

            // Character input
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                app.form_input_char(element_id, c);
                Action::None
            }

            // Backspace
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                app.form_input_backspace(element_id);
                Action::None
            }

            // Delete
            (KeyModifiers::NONE, KeyCode::Delete) => {
                app.form_input_delete(element_id);
                Action::None
            }

            // Cursor movement
            (KeyModifiers::NONE, KeyCode::Left) => {
                app.form_cursor_left(element_id);
                Action::None
            }
            (KeyModifiers::NONE, KeyCode::Right) => {
                app.form_cursor_right(element_id);
                Action::None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('a')) | (KeyModifiers::NONE, KeyCode::Home) => {
                app.form_cursor_home(element_id);
                Action::None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('e')) | (KeyModifiers::NONE, KeyCode::End) => {
                app.form_cursor_end(element_id);
                Action::None
            }

            // Clear line
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                app.form_clear(element_id);
                Action::None
            }

            _ => Action::None,
        }
    }

    /// Handle keys in search mode
    fn handle_search_key(app: &mut App, key: KeyEvent) -> Action {
        match (key.modifiers, key.code) {
            // Cancel
            (KeyModifiers::NONE, KeyCode::Esc) => {
                app.state = AppState::Normal;
                app.search_query.clear();
                app.search_results.clear();
                Action::None
            }

            // Submit search
            (KeyModifiers::NONE, KeyCode::Enter) => {
                app.search();
                app.state = AppState::Normal;
                Action::None
            }

            // Character input
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                app.search_query.push(c);
                Action::None
            }

            // Backspace
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                app.search_query.pop();
                Action::None
            }

            _ => Action::None,
        }
    }

    /// Handle keys in help mode
    fn handle_help_key(app: &mut App, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                app.state = AppState::Normal;
                Action::None
            }
            _ => Action::None,
        }
    }

    /// Handle keys in error mode
    fn handle_error_key(app: &mut App, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                app.state = AppState::Normal;
                app.error_message = None;
                Action::None
            }
            _ => Action::None,
        }
    }

    /// Handle keys while loading
    fn handle_loading_key(app: &mut App, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                // Cancel loading
                app.loading = false;
                app.state = AppState::Normal;
                Action::None
            }
            _ => Action::None,
        }
    }

    /// Handle mouse event
    fn handle_mouse(app: &mut App, mouse: MouseEvent) -> Action {
        match mouse.kind {
            // Left click
            MouseEventKind::Down(MouseButton::Left) => {
                let row = mouse.row;
                let col = mouse.column;

                // Calculate content area bounds
                // Layout: URL bar (2 rows) | Content | Console (8 if visible) | Status (2 rows)
                let url_bar_height = 2u16;
                let status_bar_height = 2u16;
                let console_height = if app.console_visible { 8u16 } else { 0u16 };
                let content_start = url_bar_height;
                let content_end = app.terminal_size.1.saturating_sub(status_bar_height + console_height);

                // Check if click is in content area
                if row >= content_start && row < content_end {
                    let content_row = (row - content_start) as usize;
                    let line_index = app.scroll_offset + content_row;

                    // Check if there's a link on this line
                    if let Some(page) = &app.page {
                        for link in &page.numbered_links {
                            if link.line_index == line_index {
                                return Action::Navigate(link.url.clone());
                            }
                        }

                        // Check for interactive elements (buttons, inputs)
                        for (idx, interactive) in page.interactives.iter().enumerate() {
                            if interactive.line_index == line_index {
                                match &interactive.kind {
                                    crate::render::InteractiveKind::Link(num) => {
                                        if let Some(link) = page.numbered_links.get(*num - 1) {
                                            return Action::Navigate(link.url.clone());
                                        }
                                    }
                                    crate::render::InteractiveKind::Button => {
                                        return Action::ClickButton(interactive.element_id);
                                    }
                                    crate::render::InteractiveKind::TextInput |
                                    crate::render::InteractiveKind::Password => {
                                        app.focus_index = Some(idx);
                                        app.start_form_input(interactive.element_id);
                                    }
                                    crate::render::InteractiveKind::Checkbox => {
                                        app.focus_index = Some(idx);
                                        // Toggle checkbox would go here
                                    }
                                    _ => {
                                        app.focus_index = Some(idx);
                                    }
                                }
                                break;
                            }
                        }
                    }
                }

                // Check if click is in URL bar area
                if row < url_bar_height {
                    app.start_url_edit();
                }

                Action::None
            }

            // Scroll wheel
            MouseEventKind::ScrollUp => {
                app.scroll(-3);
                Action::None
            }
            MouseEventKind::ScrollDown => {
                app.scroll(3);
                Action::None
            }

            _ => Action::None,
        }
    }
}
