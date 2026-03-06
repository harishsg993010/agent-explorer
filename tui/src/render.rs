//! Page rendering - converting DOM to displayable content

use std::rc::Rc;
use ratatui::style::{Color, Modifier, Style};

/// A rendered page ready for TUI display
#[derive(Debug, Clone)]
pub struct RenderedPage {
    /// All lines to display (markdown + widgets)
    pub lines: Vec<Line>,

    /// Interactive elements in Tab order
    pub interactives: Vec<Interactive>,

    /// Quick-jump links [1] [2] [3]...
    pub numbered_links: Vec<NumberedLink>,

    /// Form field names (element_id -> field name) for form submission
    pub form_fields: std::collections::HashMap<u64, String>,

    /// Form action URL (from the most recent form)
    pub form_action: Option<String>,

    /// Form method (get/post)
    pub form_method: String,

    /// Page title
    pub title: String,

    /// Page URL
    pub url: String,
}

/// A single line of display content
#[derive(Debug, Clone)]
pub struct Line {
    /// Content of the line
    pub content: LineContent,

    /// Indentation level (spaces)
    pub indent: u16,
}

impl Line {
    /// Get text content for searching
    pub fn text_content(&self) -> String {
        match &self.content {
            LineContent::Markdown(spans) => {
                spans.iter().map(|s| s.text.as_str()).collect()
            }
            LineContent::TextInput { value, placeholder, .. } => {
                if value.is_empty() { placeholder.clone() } else { value.clone() }
            }
            LineContent::Button { label, .. } => label.clone(),
            LineContent::Checkbox { label, .. } => label.clone(),
            LineContent::Radio { label, .. } => label.clone(),
            LineContent::Select { options, selected, .. } => {
                options.get(*selected).cloned().unwrap_or_default()
            }
            LineContent::HorizontalRule => String::new(),
            LineContent::Empty => String::new(),
            LineContent::CodeBlock { code, .. } => code.clone(),
        }
    }
}

/// What a line contains
#[derive(Debug, Clone)]
pub enum LineContent {
    /// Styled markdown text
    Markdown(Vec<StyledSpan>),

    /// Text input field
    TextInput {
        id: u64,
        value: String,
        placeholder: String,
        password: bool,
        width: u16,
    },

    /// Button
    Button {
        id: u64,
        label: String,
    },

    /// Checkbox
    Checkbox {
        id: u64,
        label: String,
        checked: bool,
    },

    /// Radio button
    Radio {
        id: u64,
        name: String,
        label: String,
        value: String,
        checked: bool,
    },

    /// Dropdown select
    Select {
        id: u64,
        options: Vec<String>,
        selected: usize,
    },

    /// Code block
    CodeBlock {
        code: String,
        language: Option<String>,
    },

    /// Horizontal rule
    HorizontalRule,

    /// Empty line
    Empty,
}

/// A styled text span (for markdown content)
#[derive(Debug, Clone)]
pub struct StyledSpan {
    pub text: String,
    pub style: SpanStyle,
    pub link: Option<usize>, // Link number if clickable
}

/// Text styling
#[derive(Debug, Clone, Default)]
pub struct SpanStyle {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
}

impl SpanStyle {
    pub fn to_ratatui_style(&self) -> Style {
        let mut style = Style::default();

        if let Some(fg) = self.fg {
            style = style.fg(fg);
        }
        if let Some(bg) = self.bg {
            style = style.bg(bg);
        }

        let mut modifiers = Modifier::empty();
        if self.bold {
            modifiers |= Modifier::BOLD;
        }
        if self.italic {
            modifiers |= Modifier::ITALIC;
        }
        if self.underline {
            modifiers |= Modifier::UNDERLINED;
        }
        if self.dim {
            modifiers |= Modifier::DIM;
        }

        style.add_modifier(modifiers)
    }
}

/// Interactive element reference
#[derive(Debug, Clone)]
pub struct Interactive {
    pub line_index: usize,
    pub element_id: u64,
    pub kind: InteractiveKind,
}

/// Kind of interactive element
#[derive(Debug, Clone)]
pub enum InteractiveKind {
    TextInput,
    Password,
    Checkbox,
    Radio,
    Select,
    Button,
    Link(usize), // Link number
}

/// Quick-jump link
#[derive(Debug, Clone)]
pub struct NumberedLink {
    pub number: usize,
    pub url: String,
    pub text: String,
    pub line_index: usize,
}

/// Clickable region on screen
#[derive(Debug, Clone)]
pub struct HitBox {
    /// Screen coordinates (0-based)
    pub row: u16,
    pub col_start: u16,
    pub col_end: u16,

    /// What this region represents
    pub target: ClickTarget,
}

/// Click target type
#[derive(Debug, Clone)]
pub enum ClickTarget {
    Link { url: String, number: usize },
    Button { element_id: u64 },
    TextInput { element_id: u64 },
    Checkbox { element_id: u64 },
    Radio { element_id: u64, value: String },
    Select { element_id: u64 },
    UrlBar,
    ConsoleEntry { index: usize },
}

/// Hit test map for mouse interaction
#[derive(Debug, Default)]
pub struct HitTestMap {
    /// All clickable regions, sorted by row then col
    pub regions: Vec<HitBox>,
}

impl HitTestMap {
    /// Find what's at screen position (row, col)
    pub fn hit_test(&self, row: u16, col: u16) -> Option<&ClickTarget> {
        self.regions
            .iter()
            .find(|r| r.row == row && col >= r.col_start && col < r.col_end)
            .map(|r| &r.target)
    }

    /// Clear all regions
    pub fn clear(&mut self) {
        self.regions.clear();
    }

    /// Add a hit region
    pub fn add(&mut self, row: u16, col_start: u16, col_end: u16, target: ClickTarget) {
        self.regions.push(HitBox {
            row,
            col_start,
            col_end,
            target,
        });
    }
}

impl Default for RenderedPage {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            interactives: Vec::new(),
            numbered_links: Vec::new(),
            form_fields: std::collections::HashMap::new(),
            form_action: None,
            form_method: "get".to_string(),
            title: String::new(),
            url: String::new(),
        }
    }
}

impl RenderedPage {
    /// Create a new empty page
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a heading line
    pub fn add_heading(&mut self, level: u8, text: &str) {
        let prefix = "#".repeat(level as usize);
        let style = SpanStyle {
            bold: true,
            fg: Some(match level {
                1 => Color::Cyan,
                2 => Color::Blue,
                3 => Color::Magenta,
                _ => Color::White,
            }),
            ..Default::default()
        };

        self.lines.push(Line {
            content: LineContent::Markdown(vec![StyledSpan {
                text: format!("{} {}", prefix, text),
                style,
                link: None,
            }]),
            indent: 0,
        });
    }

    /// Add a paragraph
    pub fn add_paragraph(&mut self, spans: Vec<StyledSpan>) {
        self.lines.push(Line {
            content: LineContent::Markdown(spans),
            indent: 0,
        });
    }

    /// Add a text line
    pub fn add_text(&mut self, text: &str) {
        self.lines.push(Line {
            content: LineContent::Markdown(vec![StyledSpan {
                text: text.to_string(),
                style: SpanStyle::default(),
                link: None,
            }]),
            indent: 0,
        });
    }

    /// Add a link
    pub fn add_link(&mut self, text: &str, url: &str) {
        let number = self.numbered_links.len() + 1;
        let line_index = self.lines.len();

        self.numbered_links.push(NumberedLink {
            number,
            url: url.to_string(),
            text: text.to_string(),
            line_index,
        });

        let style = SpanStyle {
            fg: Some(Color::Blue),
            underline: true,
            ..Default::default()
        };

        self.lines.push(Line {
            content: LineContent::Markdown(vec![
                StyledSpan {
                    text: format!("[{}] ", number),
                    style: SpanStyle {
                        fg: Some(Color::DarkGray),
                        ..Default::default()
                    },
                    link: None,
                },
                StyledSpan {
                    text: text.to_string(),
                    style,
                    link: Some(number),
                },
            ]),
            indent: 0,
        });

        self.interactives.push(Interactive {
            line_index,
            element_id: number as u64,
            kind: InteractiveKind::Link(number),
        });
    }

    /// Add a text input
    pub fn add_text_input(&mut self, id: u64, placeholder: &str, password: bool, name: Option<&str>) {
        let line_index = self.lines.len();

        self.lines.push(Line {
            content: LineContent::TextInput {
                id,
                value: String::new(),
                placeholder: placeholder.to_string(),
                password,
                width: 30,
            },
            indent: 2,
        });

        self.interactives.push(Interactive {
            line_index,
            element_id: id,
            kind: if password {
                InteractiveKind::Password
            } else {
                InteractiveKind::TextInput
            },
        });

        // Store field name for form submission
        if let Some(n) = name {
            if !n.is_empty() {
                self.form_fields.insert(id, n.to_string());
            }
        }
    }

    /// Add a button
    pub fn add_button(&mut self, id: u64, label: &str) {
        let line_index = self.lines.len();

        self.lines.push(Line {
            content: LineContent::Button {
                id,
                label: label.to_string(),
            },
            indent: 2,
        });

        self.interactives.push(Interactive {
            line_index,
            element_id: id,
            kind: InteractiveKind::Button,
        });
    }

    /// Add a checkbox
    pub fn add_checkbox(&mut self, id: u64, label: &str, checked: bool) {
        let line_index = self.lines.len();

        self.lines.push(Line {
            content: LineContent::Checkbox {
                id,
                label: label.to_string(),
                checked,
            },
            indent: 2,
        });

        self.interactives.push(Interactive {
            line_index,
            element_id: id,
            kind: InteractiveKind::Checkbox,
        });
    }

    /// Add a horizontal rule
    pub fn add_hr(&mut self) {
        self.lines.push(Line {
            content: LineContent::HorizontalRule,
            indent: 0,
        });
    }

    /// Add an empty line
    pub fn add_empty(&mut self) {
        self.lines.push(Line {
            content: LineContent::Empty,
            indent: 0,
        });
    }

    /// Add a code block
    pub fn add_code_block(&mut self, code: &str, language: Option<&str>) {
        self.lines.push(Line {
            content: LineContent::CodeBlock {
                code: code.to_string(),
                language: language.map(String::from),
            },
            indent: 0,
        });
    }
}

/// Convert markdown string to RenderedPage
pub fn render_markdown(markdown: &str, url: &str) -> RenderedPage {
    let mut page = RenderedPage::new();
    page.url = url.to_string();

    let mut in_code_block = false;
    let mut code_block = String::new();
    let mut code_language = None;

    for line in markdown.lines() {
        // Handle code blocks
        if line.starts_with("```") {
            if in_code_block {
                // End of code block
                page.add_code_block(&code_block, code_language.as_deref());
                code_block.clear();
                code_language = None;
                in_code_block = false;
            } else {
                // Start of code block
                in_code_block = true;
                let lang = line.trim_start_matches("```").trim();
                if !lang.is_empty() {
                    code_language = Some(lang.to_string());
                }
            }
            continue;
        }

        if in_code_block {
            if !code_block.is_empty() {
                code_block.push('\n');
            }
            code_block.push_str(line);
            continue;
        }

        let trimmed = line.trim();

        // Empty line
        if trimmed.is_empty() {
            page.add_empty();
            continue;
        }

        // Form marker: {{FORM:action:method}}
        if trimmed.starts_with("{{FORM:") && trimmed.ends_with("}}") {
            if let Some((action, method)) = parse_form_marker(trimmed) {
                page.form_action = Some(action);
                page.form_method = method;
            }
            continue;
        }

        // Form element markers: {{INPUT:id:type:name:placeholder:value}}
        if trimmed.starts_with("{{INPUT:") && trimmed.ends_with("}}") {
            if let Some(input) = parse_input_marker(trimmed) {
                let is_password = input.input_type == "password";
                // Use placeholder if available, otherwise use name as hint
                let placeholder = if !input.placeholder.is_empty() {
                    input.placeholder.clone()
                } else if !input.name.is_empty() {
                    // Common input names get friendly labels
                    match input.name.as_str() {
                        "q" | "query" | "search" => "Search...".to_string(),
                        "email" => "Email".to_string(),
                        "password" | "pass" | "passwd" => "Password".to_string(),
                        "username" | "user" => "Username".to_string(),
                        n => n.to_string(),
                    }
                } else {
                    match input.input_type.as_str() {
                        "search" => "Search...".to_string(),
                        "email" => "Email".to_string(),
                        "password" => "Password".to_string(),
                        _ => "Enter text...".to_string(),
                    }
                };
                let name = if !input.name.is_empty() { Some(input.name.as_str()) } else { None };
                page.add_text_input(input.id, &placeholder, is_password, name);
            }
            continue;
        }

        // Button marker: {{BUTTON:id:label}}
        if trimmed.starts_with("{{BUTTON:") && trimmed.ends_with("}}") {
            if let Some((id, label)) = parse_button_marker(trimmed) {
                page.add_button(id, &label);
            }
            continue;
        }

        // Checkbox marker: {{CHECKBOX:id:name:label:checked}}
        if trimmed.starts_with("{{CHECKBOX:") && trimmed.ends_with("}}") {
            if let Some((id, label, checked)) = parse_checkbox_marker(trimmed) {
                page.add_checkbox(id, &label, checked);
            }
            continue;
        }

        // Headings
        if let Some(rest) = trimmed.strip_prefix("# ") {
            page.title = rest.to_string();
            page.add_heading(1, rest);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            page.add_heading(2, rest);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            page.add_heading(3, rest);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("#### ") {
            page.add_heading(4, rest);
            continue;
        }

        // Horizontal rule
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            page.add_hr();
            continue;
        }

        // Links (simplified markdown link parsing)
        if trimmed.starts_with("- [") || trimmed.starts_with("* [") {
            // List item with link
            if let Some((text, url)) = parse_markdown_link(&trimmed[2..]) {
                page.add_link(&text, &url);
                continue;
            }
        }

        if trimmed.starts_with("[") {
            if let Some((text, url)) = parse_markdown_link(trimmed) {
                page.add_link(&text, &url);
                continue;
            }
        }

        // Regular text with inline formatting
        let spans = parse_inline_markdown(trimmed);
        page.add_paragraph(spans);
    }

    page
}

/// Parsed input marker data
struct InputMarker {
    id: u64,
    input_type: String,
    name: String,
    placeholder: String,
    value: String,
}

/// Parse {{INPUT:id:type:name:placeholder:value}} marker
fn parse_input_marker(s: &str) -> Option<InputMarker> {
    let inner = s.strip_prefix("{{INPUT:")?.strip_suffix("}}")?;
    let parts = split_escaped_fields(inner);

    // Handle both 5-part format and shorter formats
    if parts.len() >= 2 {
        Some(InputMarker {
            id: parts[0].parse().ok()?,
            input_type: parts.get(1).cloned().unwrap_or_else(|| "text".to_string()),
            name: parts.get(2).cloned().unwrap_or_default(),
            placeholder: parts.get(3).cloned().unwrap_or_default(),
            value: parts.get(4).cloned().unwrap_or_default(),
        })
    } else {
        None
    }
}

/// Parse {{FORM:action:method}} marker
fn parse_form_marker(s: &str) -> Option<(String, String)> {
    let inner = s.strip_prefix("{{FORM:")?.strip_suffix("}}")?;
    let parts = split_escaped_fields(inner);

    if parts.len() >= 2 {
        Some((parts[0].clone(), parts[1].clone()))
    } else if parts.len() == 1 {
        Some((parts[0].clone(), "get".to_string()))
    } else {
        None
    }
}

/// Parse {{BUTTON:id:label}} marker
fn parse_button_marker(s: &str) -> Option<(u64, String)> {
    let inner = s.strip_prefix("{{BUTTON:")?.strip_suffix("}}")?;
    let parts = split_escaped_fields(inner);

    if parts.len() >= 2 {
        let id: u64 = parts[0].parse().ok()?;
        let label = parts[1].clone();
        Some((id, label))
    } else {
        None
    }
}

/// Parse {{CHECKBOX:id:name:label:checked}} marker
fn parse_checkbox_marker(s: &str) -> Option<(u64, String, bool)> {
    let inner = s.strip_prefix("{{CHECKBOX:")?.strip_suffix("}}")?;
    let parts = split_escaped_fields(inner);

    if parts.len() >= 4 {
        let id: u64 = parts[0].parse().ok()?;
        let label = parts[2].clone();
        let checked = parts[3] == "true";
        Some((id, label, checked))
    } else {
        None
    }
}

/// Split fields that may contain escaped colons (\:) and braces (\})
fn split_escaped_fields(s: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            // Escaped character
            if let Some(&next) = chars.peek() {
                if next == ':' || next == '}' {
                    current.push(chars.next().unwrap());
                    continue;
                }
            }
            current.push(c);
        } else if c == ':' {
            fields.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }

    fields.push(current);
    fields
}

/// Parse a markdown link [text](url)
fn parse_markdown_link(s: &str) -> Option<(String, String)> {
    let s = s.trim();
    if !s.starts_with('[') {
        return None;
    }

    let bracket_end = s.find(']')?;
    let text = s[1..bracket_end].to_string();

    let rest = &s[bracket_end + 1..];
    if !rest.starts_with('(') {
        return None;
    }

    let paren_end = rest.find(')')?;
    let url = rest[1..paren_end].to_string();

    Some((text, url))
}

/// Parse inline markdown formatting
fn parse_inline_markdown(s: &str) -> Vec<StyledSpan> {
    let mut spans = Vec::new();
    let mut current_text = String::new();
    let mut current_style = SpanStyle::default();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '*' => {
                if chars.peek() == Some(&'*') {
                    // Bold
                    chars.next();
                    if !current_text.is_empty() {
                        spans.push(StyledSpan {
                            text: std::mem::take(&mut current_text),
                            style: current_style.clone(),
                            link: None,
                        });
                    }
                    current_style.bold = !current_style.bold;
                } else {
                    // Italic
                    if !current_text.is_empty() {
                        spans.push(StyledSpan {
                            text: std::mem::take(&mut current_text),
                            style: current_style.clone(),
                            link: None,
                        });
                    }
                    current_style.italic = !current_style.italic;
                }
            }
            '`' => {
                // Inline code
                if !current_text.is_empty() {
                    spans.push(StyledSpan {
                        text: std::mem::take(&mut current_text),
                        style: current_style.clone(),
                        link: None,
                    });
                }
                // Read until next backtick
                let mut code = String::new();
                while let Some(c) = chars.next() {
                    if c == '`' {
                        break;
                    }
                    code.push(c);
                }
                spans.push(StyledSpan {
                    text: code,
                    style: SpanStyle {
                        bg: Some(Color::DarkGray),
                        fg: Some(Color::Yellow),
                        ..Default::default()
                    },
                    link: None,
                });
            }
            _ => {
                current_text.push(c);
            }
        }
    }

    if !current_text.is_empty() {
        spans.push(StyledSpan {
            text: current_text,
            style: current_style,
            link: None,
        });
    }

    if spans.is_empty() {
        spans.push(StyledSpan {
            text: s.to_string(),
            style: SpanStyle::default(),
            link: None,
        });
    }

    spans
}
