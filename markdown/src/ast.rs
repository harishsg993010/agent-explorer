//! Markdown AST with source mapping back to DOM nodes.
//!
//! This AST represents the final Markdown structure after layout,
//! with links back to source DOM nodes for interaction.

use crate::ids::{LinkId, NodeId, OverlayId, WidgetId};

/// A Markdown document consisting of blocks
#[derive(Debug, Clone)]
pub struct Document {
    pub blocks: Vec<Block>,
}

impl Document {
    pub fn new() -> Self {
        Document { blocks: Vec::new() }
    }

    pub fn push(&mut self, block: Block) {
        self.blocks.push(block);
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

/// A block-level element in Markdown
#[derive(Debug, Clone)]
pub struct Block {
    pub kind: BlockKind,
    pub source: Option<NodeId>,
}

#[derive(Debug, Clone)]
pub enum BlockKind {
    /// Heading with level (1-6)
    Heading {
        level: u8,
        content: InlineContent,
    },

    /// Paragraph of inline content
    Paragraph {
        content: InlineContent,
    },

    /// Blockquote containing blocks
    Blockquote {
        blocks: Vec<Block>,
    },

    /// Code block with optional language
    CodeBlock {
        language: Option<String>,
        code: String,
    },

    /// Unordered list
    UnorderedList {
        items: Vec<ListItem>,
    },

    /// Ordered list with starting number
    OrderedList {
        start: usize,
        items: Vec<ListItem>,
    },

    /// Horizontal rule
    ThematicBreak,

    /// Table with headers and rows
    Table {
        headers: Vec<TableCell>,
        rows: Vec<Vec<TableCell>>,
        alignments: Vec<Alignment>,
    },

    /// Widget placeholder (input, button, etc.)
    Widget {
        widget_id: WidgetId,
        display: String,
    },

    /// Form container
    Form {
        action: String,
        method: String,
        widgets: Vec<WidgetId>,
    },

    /// Raw HTML passthrough (for complex structures)
    HtmlBlock {
        content: String,
    },

    /// Blank line(s) for spacing
    BlankLines {
        count: usize,
    },

    /// Container for nested blocks (like divs)
    Container {
        blocks: Vec<Block>,
        indent: usize,
    },

    /// Details/summary expandable section
    Details {
        summary: InlineContent,
        blocks: Vec<Block>,
        open: bool,
    },
}

/// A list item containing blocks
#[derive(Debug, Clone)]
pub struct ListItem {
    pub blocks: Vec<Block>,
    pub source: Option<NodeId>,
    /// For task lists
    pub checked: Option<bool>,
}

/// A table cell
#[derive(Debug, Clone)]
pub struct TableCell {
    pub content: InlineContent,
    pub source: Option<NodeId>,
}

/// Text alignment for table columns
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
    Default,
}

/// Inline content composed of spans
#[derive(Debug, Clone)]
pub struct InlineContent {
    pub spans: Vec<Span>,
}

impl InlineContent {
    pub fn new() -> Self {
        InlineContent { spans: Vec::new() }
    }

    pub fn push(&mut self, span: Span) {
        self.spans.push(span);
    }

    pub fn push_text(&mut self, text: impl Into<String>) {
        self.spans.push(Span {
            kind: SpanKind::Text,
            content: text.into(),
            source: None,
        });
    }

    pub fn text(text: impl Into<String>) -> Self {
        InlineContent {
            spans: vec![Span {
                kind: SpanKind::Text,
                content: text.into(),
                source: None,
            }],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty() || self.spans.iter().all(|s| s.content.is_empty())
    }

    /// Get plain text content without formatting
    pub fn plain_text(&self) -> String {
        self.spans.iter().map(|s| s.content.as_str()).collect()
    }

    /// Check if content ends with whitespace or is empty
    pub fn ends_with_whitespace(&self) -> bool {
        if self.spans.is_empty() {
            return true;
        }
        self.spans
            .last()
            .map(|s| {
                s.content.is_empty()
                    || s.content.ends_with(char::is_whitespace)
                    || matches!(s.kind, SpanKind::LineBreak { .. })
            })
            .unwrap_or(true)
    }
}

impl Default for InlineContent {
    fn default() -> Self {
        Self::new()
    }
}

/// A span of inline content with formatting
#[derive(Debug, Clone)]
pub struct Span {
    pub kind: SpanKind,
    pub content: String,
    pub source: Option<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanKind {
    /// Plain text
    Text,

    /// Strong/bold text
    Strong,

    /// Emphasis/italic text
    Emphasis,

    /// Strong + emphasis
    StrongEmphasis,

    /// Inline code
    Code,

    /// Strikethrough text
    Strikethrough,

    /// Underline (non-standard, often rendered as underline in terminals)
    Underline,

    /// Link with URL and optional title
    Link {
        url: String,
        title: Option<String>,
        link_id: LinkId,
    },

    /// Image with alt text
    Image {
        url: String,
        alt: String,
    },

    /// Line break (soft or hard)
    LineBreak {
        hard: bool,
    },

    /// Superscript
    Superscript,

    /// Subscript
    Subscript,

    /// Highlighted/marked text
    Highlight,

    /// Keyboard shortcut
    Kbd,

    /// Widget reference (inline)
    WidgetRef {
        widget_id: WidgetId,
    },
}

/// Line record for source mapping
#[derive(Debug, Clone)]
pub struct LineRecord {
    /// Line number in output (0-indexed)
    pub line_number: usize,
    /// Starting column (0-indexed)
    pub start_column: usize,
    /// Ending column (exclusive)
    pub end_column: usize,
    /// Source DOM node if any
    pub source_node: Option<NodeId>,
    /// Content on this line
    pub content: String,
    /// Indent level in characters
    pub indent: usize,
}

/// Overlay content for fixed/absolute positioned elements
#[derive(Debug, Clone)]
pub struct Overlay {
    pub id: OverlayId,
    pub source_node: NodeId,
    pub kind: OverlayKind,
    pub content: Document,
    /// Z-index for stacking
    pub z_index: i32,
    /// Whether the overlay is currently visible
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayKind {
    /// Fixed position (stays in place during scroll)
    Fixed,
    /// Absolute position (relative to positioned ancestor)
    Absolute,
    /// Modal dialog
    Modal,
    /// Dropdown menu
    Dropdown,
    /// Tooltip
    Tooltip,
    /// Toast notification
    Toast,
    /// Popup/popover
    Popup,
}

/// Plan for how to render overlays
#[derive(Debug, Clone)]
pub struct OverlayPlan {
    pub overlays: Vec<Overlay>,
    pub mode: OverlayRenderMode,
}

impl OverlayPlan {
    pub fn new(mode: OverlayRenderMode) -> Self {
        OverlayPlan {
            overlays: Vec::new(),
            mode,
        }
    }

    pub fn push(&mut self, overlay: Overlay) {
        self.overlays.push(overlay);
    }

    pub fn is_empty(&self) -> bool {
        self.overlays.is_empty()
    }
}

impl Default for OverlayPlan {
    fn default() -> Self {
        Self::new(OverlayRenderMode::InlineFallback)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayRenderMode {
    /// Render overlays inline after main content
    InlineFallback,
    /// Keep track of overlay regions for pinned display
    PinnedRegions,
    /// Focus on modal when present, dim main content indicator
    ModalFocus,
}
