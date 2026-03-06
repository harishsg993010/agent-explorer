//! Application state and main state machine

use crate::render::{RenderedPage, Interactive, NumberedLink};
use crate::widgets::console::ConsoleEntry;
use std::collections::VecDeque;

/// Main application state
pub struct App {
    /// Current browser state
    pub state: AppState,

    /// Currently rendered page
    pub page: Option<RenderedPage>,

    /// URL bar content
    pub url_input: String,

    /// URL bar cursor position
    pub url_cursor: usize,

    /// Scroll offset in content area
    pub scroll_offset: usize,

    /// Currently focused interactive element index
    pub focus_index: Option<usize>,

    /// Navigation history
    pub history: Vec<HistoryEntry>,

    /// Current position in history
    pub history_index: usize,

    /// Console entries (JS logs, errors)
    pub console: Vec<ConsoleEntry>,

    /// Console panel visible
    pub console_visible: bool,

    /// Console scroll offset
    pub console_scroll: usize,

    /// Current hover target (for mouse)
    pub hover_target: Option<usize>,

    /// Search query
    pub search_query: String,

    /// Search results (line indices)
    pub search_results: Vec<usize>,

    /// Current search result index
    pub search_index: usize,

    /// Form field values (element_id -> value)
    pub form_values: std::collections::HashMap<u64, String>,

    /// Form field cursor positions (element_id -> cursor position)
    pub form_cursors: std::collections::HashMap<u64, usize>,

    /// Loading state
    pub loading: bool,

    /// Loading progress (0.0 - 1.0)
    pub loading_progress: f32,

    /// Error message to display
    pub error_message: Option<String>,

    /// Should quit
    pub should_quit: bool,

    /// Terminal size
    pub terminal_size: (u16, u16),
}

/// Browser state machine
#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    /// Normal browsing mode
    Normal,

    /// Editing URL bar
    UrlEditing,

    /// Editing a form field
    FormInput { element_id: u64 },

    /// Searching in page
    Searching,

    /// Showing help overlay
    Help,

    /// Showing error modal
    Error,

    /// Loading a page
    Loading,
}

/// History entry
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub scroll_position: usize,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Create a new app instance
    pub fn new() -> Self {
        Self {
            state: AppState::Normal,
            page: None,
            url_input: String::new(),
            url_cursor: 0,
            scroll_offset: 0,
            focus_index: None,
            history: Vec::new(),
            history_index: 0,
            console: Vec::new(),
            console_visible: false,
            console_scroll: 0,
            hover_target: None,
            search_query: String::new(),
            search_results: Vec::new(),
            search_index: 0,
            form_values: std::collections::HashMap::new(),
            form_cursors: std::collections::HashMap::new(),
            loading: false,
            loading_progress: 0.0,
            error_message: None,
            should_quit: false,
            terminal_size: (80, 24),
        }
    }

    /// Navigate to a URL
    pub fn navigate(&mut self, url: &str) {
        // Save current scroll position in history
        if let Some(entry) = self.history.get_mut(self.history_index) {
            entry.scroll_position = self.scroll_offset;
        }

        // Add to history
        if self.history_index < self.history.len() {
            self.history.truncate(self.history_index + 1);
        }

        self.history.push(HistoryEntry {
            url: url.to_string(),
            title: String::new(),
            scroll_position: 0,
        });
        self.history_index = self.history.len() - 1;

        // Update URL bar
        self.url_input = url.to_string();
        self.url_cursor = url.len();

        // Reset state for new page
        self.scroll_offset = 0;
        self.focus_index = None;
        self.form_values.clear();
        self.form_cursors.clear();
        self.loading = true;
        self.loading_progress = 0.0;
        self.state = AppState::Loading;
    }

    /// Go back in history
    pub fn go_back(&mut self) -> Option<String> {
        if self.history_index > 0 {
            // Save current scroll position
            if let Some(entry) = self.history.get_mut(self.history_index) {
                entry.scroll_position = self.scroll_offset;
            }

            self.history_index -= 1;
            let entry = &self.history[self.history_index];
            let url = entry.url.clone();
            self.scroll_offset = entry.scroll_position;
            self.url_input = url.clone();
            self.url_cursor = url.len();
            Some(url)
        } else {
            None
        }
    }

    /// Go forward in history
    pub fn go_forward(&mut self) -> Option<String> {
        if self.history_index + 1 < self.history.len() {
            // Save current scroll position
            if let Some(entry) = self.history.get_mut(self.history_index) {
                entry.scroll_position = self.scroll_offset;
            }

            self.history_index += 1;
            let entry = &self.history[self.history_index];
            let url = entry.url.clone();
            self.scroll_offset = entry.scroll_position;
            self.url_input = url.clone();
            self.url_cursor = url.len();
            Some(url)
        } else {
            None
        }
    }

    /// Scroll the content area
    pub fn scroll(&mut self, delta: i32) {
        let max_scroll = self.max_scroll();
        let new_offset = (self.scroll_offset as i32 + delta).max(0) as usize;
        self.scroll_offset = new_offset.min(max_scroll);
    }

    /// Get maximum scroll offset
    pub fn max_scroll(&self) -> usize {
        if let Some(page) = &self.page {
            let content_height = self.terminal_size.1 as usize - 4; // URL bar + status bar + borders
            if page.lines.len() > content_height {
                page.lines.len() - content_height
            } else {
                0
            }
        } else {
            0
        }
    }

    /// Move focus to next interactive element
    pub fn focus_next(&mut self) {
        if let Some(page) = &self.page {
            if page.interactives.is_empty() {
                return;
            }

            self.focus_index = Some(match self.focus_index {
                Some(i) => (i + 1) % page.interactives.len(),
                None => 0,
            });

            // Scroll to make focused element visible
            self.scroll_to_focused();
        }
    }

    /// Move focus to previous interactive element
    pub fn focus_prev(&mut self) {
        if let Some(page) = &self.page {
            if page.interactives.is_empty() {
                return;
            }

            self.focus_index = Some(match self.focus_index {
                Some(0) => page.interactives.len() - 1,
                Some(i) => i - 1,
                None => page.interactives.len() - 1,
            });

            // Scroll to make focused element visible
            self.scroll_to_focused();
        }
    }

    /// Scroll to make the focused element visible
    fn scroll_to_focused(&mut self) {
        if let (Some(page), Some(focus_idx)) = (&self.page, self.focus_index) {
            if let Some(interactive) = page.interactives.get(focus_idx) {
                let content_height = self.terminal_size.1 as usize - 4;
                let line_idx = interactive.line_index;

                if line_idx < self.scroll_offset {
                    self.scroll_offset = line_idx;
                } else if line_idx >= self.scroll_offset + content_height {
                    self.scroll_offset = line_idx - content_height + 1;
                }
            }
        }
    }

    /// Jump to numbered link
    pub fn jump_to_link(&mut self, number: usize) -> Option<String> {
        if let Some(page) = &self.page {
            if let Some(link) = page.numbered_links.get(number - 1) {
                return Some(link.url.clone());
            }
        }
        None
    }

    /// Start URL editing mode
    pub fn start_url_edit(&mut self) {
        self.state = AppState::UrlEditing;
        self.url_cursor = self.url_input.len();
    }

    /// Start form input mode
    pub fn start_form_input(&mut self, element_id: u64) {
        self.state = AppState::FormInput { element_id };
    }

    /// Start search mode
    pub fn start_search(&mut self) {
        self.state = AppState::Searching;
        self.search_query.clear();
        self.search_results.clear();
        self.search_index = 0;
    }

    /// Perform search
    pub fn search(&mut self) {
        self.search_results.clear();
        self.search_index = 0;

        if self.search_query.is_empty() {
            return;
        }

        if let Some(page) = &self.page {
            let query_lower = self.search_query.to_lowercase();
            for (idx, line) in page.lines.iter().enumerate() {
                if line.text_content().to_lowercase().contains(&query_lower) {
                    self.search_results.push(idx);
                }
            }
        }

        // Jump to first result
        self.jump_to_search_result();
    }

    /// Jump to next search result
    pub fn next_search_result(&mut self) {
        if !self.search_results.is_empty() {
            self.search_index = (self.search_index + 1) % self.search_results.len();
            self.jump_to_search_result();
        }
    }

    /// Jump to previous search result
    pub fn prev_search_result(&mut self) {
        if !self.search_results.is_empty() {
            self.search_index = if self.search_index == 0 {
                self.search_results.len() - 1
            } else {
                self.search_index - 1
            };
            self.jump_to_search_result();
        }
    }

    /// Jump to current search result
    fn jump_to_search_result(&mut self) {
        if let Some(&line_idx) = self.search_results.get(self.search_index) {
            let content_height = self.terminal_size.1 as usize - 4;
            // Center the result on screen
            self.scroll_offset = line_idx.saturating_sub(content_height / 2);
        }
    }

    /// Toggle console visibility
    pub fn toggle_console(&mut self) {
        self.console_visible = !self.console_visible;
    }

    /// Add console entry
    pub fn add_console_entry(&mut self, entry: ConsoleEntry) {
        self.console.push(entry);
        // Auto-show on error if configured
        if matches!(self.console.last(), Some(ConsoleEntry::JsError { .. })) {
            self.console_visible = true;
        }
    }

    /// Clear console
    pub fn clear_console(&mut self) {
        self.console.clear();
        self.console_scroll = 0;
    }

    /// Set page loaded
    pub fn set_page(&mut self, page: RenderedPage) {
        // Update history title
        if let Some(entry) = self.history.get_mut(self.history_index) {
            entry.title = page.title.clone();
        }

        self.page = Some(page);
        self.loading = false;
        self.loading_progress = 1.0;
        self.state = AppState::Normal;
        self.error_message = None;
    }

    /// Set loading error
    pub fn set_error(&mut self, message: String) {
        self.error_message = Some(message);
        self.loading = false;
        self.state = AppState::Error;
    }

    /// Get current URL
    pub fn current_url(&self) -> Option<&str> {
        self.history.get(self.history_index).map(|e| e.url.as_str())
    }

    /// Check if can go back
    pub fn can_go_back(&self) -> bool {
        self.history_index > 0
    }

    /// Check if can go forward
    pub fn can_go_forward(&self) -> bool {
        self.history_index + 1 < self.history.len()
    }

    /// Handle URL input character
    pub fn url_input_char(&mut self, c: char) {
        self.url_input.insert(self.url_cursor, c);
        self.url_cursor += 1;
    }

    /// Handle URL input backspace
    pub fn url_input_backspace(&mut self) {
        if self.url_cursor > 0 {
            self.url_cursor -= 1;
            self.url_input.remove(self.url_cursor);
        }
    }

    /// Handle URL input delete
    pub fn url_input_delete(&mut self) {
        if self.url_cursor < self.url_input.len() {
            self.url_input.remove(self.url_cursor);
        }
    }

    /// Move URL cursor left
    pub fn url_cursor_left(&mut self) {
        if self.url_cursor > 0 {
            self.url_cursor -= 1;
        }
    }

    /// Move URL cursor right
    pub fn url_cursor_right(&mut self) {
        if self.url_cursor < self.url_input.len() {
            self.url_cursor += 1;
        }
    }

    /// Clear URL input
    pub fn url_clear(&mut self) {
        self.url_input.clear();
        self.url_cursor = 0;
    }

    /// Get form value
    pub fn get_form_value(&self, element_id: u64) -> &str {
        self.form_values.get(&element_id).map(|s| s.as_str()).unwrap_or("")
    }

    /// Set form value
    pub fn set_form_value(&mut self, element_id: u64, value: String) {
        let len = value.len();
        self.form_values.insert(element_id, value);
        self.form_cursors.insert(element_id, len);
    }

    /// Get form cursor position
    pub fn get_form_cursor(&self, element_id: u64) -> usize {
        *self.form_cursors.get(&element_id).unwrap_or(&0)
    }

    /// Handle form input character
    pub fn form_input_char(&mut self, element_id: u64, c: char) {
        let value = self.form_values.entry(element_id).or_default();
        let cursor = self.form_cursors.entry(element_id).or_insert(0);

        if *cursor >= value.len() {
            value.push(c);
        } else {
            value.insert(*cursor, c);
        }
        *cursor += 1;
    }

    /// Handle form input backspace
    pub fn form_input_backspace(&mut self, element_id: u64) {
        let cursor = self.form_cursors.entry(element_id).or_insert(0);
        if *cursor > 0 {
            if let Some(value) = self.form_values.get_mut(&element_id) {
                *cursor -= 1;
                if *cursor < value.len() {
                    value.remove(*cursor);
                }
            }
        }
    }

    /// Handle form input delete
    pub fn form_input_delete(&mut self, element_id: u64) {
        let cursor = *self.form_cursors.get(&element_id).unwrap_or(&0);
        if let Some(value) = self.form_values.get_mut(&element_id) {
            if cursor < value.len() {
                value.remove(cursor);
            }
        }
    }

    /// Move form cursor left
    pub fn form_cursor_left(&mut self, element_id: u64) {
        let cursor = self.form_cursors.entry(element_id).or_insert(0);
        if *cursor > 0 {
            *cursor -= 1;
        }
    }

    /// Move form cursor right
    pub fn form_cursor_right(&mut self, element_id: u64) {
        let cursor = self.form_cursors.entry(element_id).or_insert(0);
        let len = self.form_values.get(&element_id).map(|s| s.len()).unwrap_or(0);
        if *cursor < len {
            *cursor += 1;
        }
    }

    /// Move form cursor to start
    pub fn form_cursor_home(&mut self, element_id: u64) {
        self.form_cursors.insert(element_id, 0);
    }

    /// Move form cursor to end
    pub fn form_cursor_end(&mut self, element_id: u64) {
        let len = self.form_values.get(&element_id).map(|s| s.len()).unwrap_or(0);
        self.form_cursors.insert(element_id, len);
    }

    /// Clear form input
    pub fn form_clear(&mut self, element_id: u64) {
        self.form_values.insert(element_id, String::new());
        self.form_cursors.insert(element_id, 0);
    }

    /// Toggle checkbox
    pub fn toggle_checkbox(&mut self, element_id: u64) {
        let current = self.form_values.get(&element_id).map(|s| s == "true").unwrap_or(false);
        self.form_values.insert(element_id, if current { "false" } else { "true" }.to_string());
    }
}
