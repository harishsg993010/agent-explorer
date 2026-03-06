//! Layout engine for converting DOM with computed CSS to Markdown.
//!
//! This module provides:
//! - Block and inline layout
//! - Margin collapsing approximation
//! - Flex layout (via Taffy)
//! - Grid layout (via Taffy)
//! - Table layout
//! - Overlay detection and handling

pub mod block;
pub mod flex;
pub mod float;
pub mod inline;
pub mod inline_block;
pub mod overlay;
pub mod table;
pub mod taffy_layout;
pub mod tree;

pub use taffy_layout::GridTrackSize;

use crate::ast::OverlayPlan;
use crate::ids::NodeId;
use std::collections::HashMap;

pub use tree::{LayoutBox, LayoutTree};

/// Computed CSS styles for an element (already cascaded by upstream)
#[derive(Debug, Clone)]
pub struct ComputedStyle {
    // Display and positioning
    pub display: Display,
    pub position: Position,
    pub visibility: Visibility,

    // Position offsets (for sticky, fixed, absolute, relative)
    pub top: Option<i32>,
    pub right: Option<i32>,
    pub bottom: Option<i32>,
    pub left: Option<i32>,

    // Text properties
    pub white_space: WhiteSpace,
    pub overflow_x: Overflow,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub text_decoration: TextDecoration,
    pub text_align: TextAlign,

    // Box model (in ch units as integers)
    pub margin_top: i32,
    pub margin_right: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
    pub padding_top: i32,
    pub padding_right: i32,
    pub padding_bottom: i32,
    pub padding_left: i32,
    pub border_top_width: i32,
    pub border_right_width: i32,
    pub border_bottom_width: i32,
    pub border_left_width: i32,

    // Sizing (in ch units, None = auto)
    pub width: Option<usize>,
    pub min_width: Option<usize>,
    pub max_width: Option<usize>,
    pub height: Option<usize>,

    // Flex properties
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub gap: Option<usize>,

    // Flex item properties
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Option<usize>,

    // Grid container properties
    pub grid_template_columns: Vec<GridTrackSize>,
    pub grid_template_rows: Vec<GridTrackSize>,
    pub grid_auto_columns: GridTrackSize,
    pub grid_auto_rows: GridTrackSize,

    // Grid item properties
    pub grid_column_start: Option<i32>,
    pub grid_column_end: Option<i32>,
    pub grid_row_start: Option<i32>,
    pub grid_row_end: Option<i32>,

    // List properties
    pub list_style_type: ListStyleType,

    // Z-index for stacking
    pub z_index: Option<i32>,

    // Container Query properties
    pub container_type: ContainerType,
    pub container_name: Option<String>,

    // Float properties
    pub float: Float,
    pub clear: Clear,

    // Text properties
    pub line_height: Option<f32>,
    pub letter_spacing: i32,
    pub word_spacing: i32,
    pub text_indent: i32,

    // CSS Custom Properties (variables)
    pub css_variables: HashMap<String, String>,

    // Generated content (::before/::after)
    pub content_before: Option<GeneratedContent>,
    pub content_after: Option<GeneratedContent>,

    // Multi-column layout
    pub column_count: Option<usize>,
    pub column_width: Option<usize>,
    pub column_gap: usize,
    pub column_rule_width: usize,

    // CSS Counters
    pub counter_reset: Vec<(String, i32)>,
    pub counter_increment: Vec<(String, i32)>,

    // Text overflow
    pub text_overflow: TextOverflow,

    // Aspect ratio
    pub aspect_ratio: Option<(f32, f32)>,

    // Writing modes and direction
    pub writing_mode: WritingMode,
    pub direction: Direction,
    pub text_orientation: TextOrientation,

    // Logical properties (resolved based on writing-mode/direction)
    pub margin_inline_start: Option<i32>,
    pub margin_inline_end: Option<i32>,
    pub margin_block_start: Option<i32>,
    pub margin_block_end: Option<i32>,
    pub padding_inline_start: Option<i32>,
    pub padding_inline_end: Option<i32>,
    pub padding_block_start: Option<i32>,
    pub padding_block_end: Option<i32>,
    pub inset_inline_start: Option<i32>,
    pub inset_inline_end: Option<i32>,
    pub inset_block_start: Option<i32>,
    pub inset_block_end: Option<i32>,

    // Object fit/position (for images)
    pub object_fit: ObjectFit,
    pub object_position: (ObjectPosition, ObjectPosition),

    // Table cell properties
    pub colspan: usize,
    pub rowspan: usize,
    pub vertical_align: VerticalAlign,

    // Hyphenation
    pub hyphens: Hyphens,

    // First-letter/first-line styling
    pub first_letter_style: Option<Box<FirstLetterStyle>>,
    pub first_line_style: Option<Box<FirstLineStyle>>,

    // List marker styling
    pub list_style_position: ListStylePosition,
    pub marker_content: Option<String>,

    // Transforms (limited in TUI)
    pub transform: Option<Transform>,

    // Clipping
    pub clip: Option<ClipRect>,
    pub overflow_clip_margin: i32,

    // Box decoration break
    pub box_decoration_break: BoxDecorationBreak,

    // Break properties (for pagination/columns)
    pub break_before: BreakValue,
    pub break_after: BreakValue,
    pub break_inside: BreakInside,

    // Orphans and widows
    pub orphans: usize,
    pub widows: usize,

    // Word breaking
    pub word_break: WordBreak,
    pub overflow_wrap: OverflowWrap,

    // Box sizing
    pub box_sizing: BoxSizing,

    // Outline
    pub outline_width: i32,
    pub outline_style: OutlineStyle,
    pub outline_offset: i32,

    // Tab size
    pub tab_size: usize,

    // Content sizing
    pub width_sizing: ContentSizing,
    pub height_sizing: ContentSizing,

    // Text transform
    pub text_transform: TextTransform,

    // Resize
    pub resize: Resize,

    // Pointer events
    pub pointer_events: PointerEvents,

    // User select
    pub user_select: UserSelect,

    // Quotes
    pub quotes: Option<Vec<(String, String)>>,

    // Table border properties
    pub border_collapse: BorderCollapse,
    pub border_spacing: (i32, i32), // horizontal, vertical
    pub empty_cells: EmptyCells,
    pub caption_side: CaptionSide,
    pub table_layout: TableLayout,

    // Text emphasis properties
    pub text_emphasis_style: TextEmphasisStyle,
    pub text_emphasis_position: TextEmphasisPosition,
    pub text_underline_position: TextUnderlinePosition,
    pub text_underline_offset: i32,
    pub ruby_position: RubyPosition,

    // Typography properties
    pub hanging_punctuation: HangingPunctuation,
    pub initial_letter: Option<(f32, Option<usize>)>, // size, sink

    // Accessibility & theming
    pub color_scheme: ColorScheme,
    pub forced_color_adjust: ForcedColorAdjust,
    pub accent_color: AccentColor,

    // Cursor
    pub cursor: Cursor,
    pub caret_color: CaretColor,

    // Scroll properties
    pub scroll_margin_top: i32,
    pub scroll_margin_right: i32,
    pub scroll_margin_bottom: i32,
    pub scroll_margin_left: i32,
    pub scroll_padding_top: i32,
    pub scroll_padding_right: i32,
    pub scroll_padding_bottom: i32,
    pub scroll_padding_left: i32,

    // Performance/containment
    pub contain: Contain,
    pub content_visibility: ContentVisibility,

    // Flex/Grid item ordering and alignment
    pub order: i32,
    pub align_self: AlignSelf,
    pub justify_self: JustifySelf,
    pub row_gap: usize, // For flex/grid - use existing column_gap for column gaps

    // Scroll snap
    pub scroll_snap_type: ScrollSnapType,
    pub scroll_snap_align: ScrollSnapAlign,
    pub scroll_snap_stop: ScrollSnapStop,
    pub scroll_behavior: ScrollBehavior,

    // Pseudo-element styles
    pub marker_style: Option<Box<MarkerStyle>>,
    pub selection_style: Option<Box<SelectionStyle>>,
    pub placeholder_style: Option<Box<PlaceholderStyle>>,

    // Filter and opacity
    pub opacity: f32,
    pub filter: Filter,
    pub mix_blend_mode: MixBlendMode,

    // Important flags (bitfield for common properties)
    pub important_flags: u64,

    // Transitions and animations
    pub transitions: Vec<Transition>,
    pub animations: Vec<Animation>,

    // Anchor positioning
    pub anchor_name: Option<String>,
    pub position_anchor: Option<String>,

    // Subgrid
    pub grid_template_columns_subgrid: SubgridValue,
    pub grid_template_rows_subgrid: SubgridValue,

    // CSS Masking
    pub mask_image: MaskImage,
    pub mask_mode: MaskMode,
    pub mask_repeat: MaskRepeat,
    pub mask_position: MaskPosition,
    pub mask_size: MaskSize,
    pub mask_composite: MaskComposite,
    pub mask_clip: MaskClip,
    pub mask_origin: MaskOrigin,

    // Backdrop filter
    pub backdrop_filter: BackdropFilter,

    // View Transitions
    pub view_transition_name: ViewTransitionName,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        ComputedStyle {
            display: Display::Block,
            position: Position::Static,
            visibility: Visibility::Visible,
            top: None,
            right: None,
            bottom: None,
            left: None,
            white_space: WhiteSpace::Normal,
            overflow_x: Overflow::Visible,
            font_weight: FontWeight::Normal,
            font_style: FontStyle::Normal,
            text_decoration: TextDecoration::None,
            text_align: TextAlign::Left,
            margin_top: 0,
            margin_right: 0,
            margin_bottom: 0,
            margin_left: 0,
            padding_top: 0,
            padding_right: 0,
            padding_bottom: 0,
            padding_left: 0,
            border_top_width: 0,
            border_right_width: 0,
            border_bottom_width: 0,
            border_left_width: 0,
            width: None,
            min_width: None,
            max_width: None,
            height: None,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Stretch,
            gap: None,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: None,
            grid_template_columns: Vec::new(),
            grid_template_rows: Vec::new(),
            grid_auto_columns: GridTrackSize::Auto,
            grid_auto_rows: GridTrackSize::Auto,
            grid_column_start: None,
            grid_column_end: None,
            grid_row_start: None,
            grid_row_end: None,
            list_style_type: ListStyleType::Disc,
            z_index: None,
            container_type: ContainerType::Normal,
            container_name: None,
            float: Float::None,
            clear: Clear::None,
            line_height: None,
            letter_spacing: 0,
            word_spacing: 0,
            text_indent: 0,
            css_variables: HashMap::new(),
            content_before: None,
            content_after: None,
            column_count: None,
            column_width: None,
            column_gap: 1,
            column_rule_width: 0,
            counter_reset: Vec::new(),
            counter_increment: Vec::new(),
            text_overflow: TextOverflow::Clip,
            aspect_ratio: None,
            writing_mode: WritingMode::HorizontalTb,
            direction: Direction::Ltr,
            text_orientation: TextOrientation::Mixed,
            margin_inline_start: None,
            margin_inline_end: None,
            margin_block_start: None,
            margin_block_end: None,
            padding_inline_start: None,
            padding_inline_end: None,
            padding_block_start: None,
            padding_block_end: None,
            inset_inline_start: None,
            inset_inline_end: None,
            inset_block_start: None,
            inset_block_end: None,
            object_fit: ObjectFit::Fill,
            object_position: (ObjectPosition::Center, ObjectPosition::Center),
            colspan: 1,
            rowspan: 1,
            vertical_align: VerticalAlign::Baseline,
            hyphens: Hyphens::None,
            first_letter_style: None,
            first_line_style: None,
            list_style_position: ListStylePosition::Outside,
            marker_content: None,
            transform: None,
            clip: None,
            overflow_clip_margin: 0,
            box_decoration_break: BoxDecorationBreak::Slice,
            break_before: BreakValue::Auto,
            break_after: BreakValue::Auto,
            break_inside: BreakInside::Auto,
            orphans: 2,
            widows: 2,
            word_break: WordBreak::Normal,
            overflow_wrap: OverflowWrap::Normal,
            box_sizing: BoxSizing::ContentBox,
            outline_width: 0,
            outline_style: OutlineStyle::None,
            outline_offset: 0,
            tab_size: 8,
            width_sizing: ContentSizing::Auto,
            height_sizing: ContentSizing::Auto,
            text_transform: TextTransform::None,
            resize: Resize::None,
            pointer_events: PointerEvents::Auto,
            user_select: UserSelect::Auto,
            quotes: None,
            // Table border properties
            border_collapse: BorderCollapse::Separate,
            border_spacing: (0, 0),
            empty_cells: EmptyCells::Show,
            caption_side: CaptionSide::Top,
            table_layout: TableLayout::Auto,
            // Text emphasis properties
            text_emphasis_style: TextEmphasisStyle::None,
            text_emphasis_position: TextEmphasisPosition::Over,
            text_underline_position: TextUnderlinePosition::Auto,
            text_underline_offset: 0,
            ruby_position: RubyPosition::Over,
            // Typography properties
            hanging_punctuation: HangingPunctuation::None,
            initial_letter: None,
            // Accessibility & theming
            color_scheme: ColorScheme::Normal,
            forced_color_adjust: ForcedColorAdjust::Auto,
            accent_color: AccentColor::Auto,
            // Cursor
            cursor: Cursor::Auto,
            caret_color: CaretColor::Auto,
            // Scroll properties
            scroll_margin_top: 0,
            scroll_margin_right: 0,
            scroll_margin_bottom: 0,
            scroll_margin_left: 0,
            scroll_padding_top: 0,
            scroll_padding_right: 0,
            scroll_padding_bottom: 0,
            scroll_padding_left: 0,
            // Performance/containment
            contain: Contain::None,
            content_visibility: ContentVisibility::Visible,
            // Flex/Grid item ordering and alignment
            order: 0,
            align_self: AlignSelf::Auto,
            justify_self: JustifySelf::Auto,
            row_gap: 0,
            // Scroll snap
            scroll_snap_type: ScrollSnapType::None,
            scroll_snap_align: ScrollSnapAlign::None,
            scroll_snap_stop: ScrollSnapStop::Normal,
            scroll_behavior: ScrollBehavior::Auto,
            // Pseudo-element styles
            marker_style: None,
            selection_style: None,
            placeholder_style: None,
            // Filter and opacity
            opacity: 1.0,
            filter: Filter::None,
            mix_blend_mode: MixBlendMode::Normal,
            // Important flags
            important_flags: 0,
            // Transitions and animations
            transitions: Vec::new(),
            animations: Vec::new(),
            // Anchor positioning
            anchor_name: None,
            position_anchor: None,
            // Subgrid
            grid_template_columns_subgrid: SubgridValue::None,
            grid_template_rows_subgrid: SubgridValue::None,
            // CSS Masking
            mask_image: MaskImage::None,
            mask_mode: MaskMode::MatchSource,
            mask_repeat: MaskRepeat::Repeat,
            mask_position: MaskPosition::default(),
            mask_size: MaskSize::Auto,
            mask_composite: MaskComposite::Add,
            mask_clip: MaskClip::BorderBox,
            mask_origin: MaskOrigin::BorderBox,
            // Backdrop filter
            backdrop_filter: BackdropFilter::None,
            // View Transitions
            view_transition_name: ViewTransitionName::None,
        }
    }
}

impl ComputedStyle {
    /// Check if element should be displayed
    pub fn is_visible(&self) -> bool {
        self.display != Display::None && self.visibility == Visibility::Visible
    }

    /// Check if element creates a new block formatting context
    pub fn is_block_level(&self) -> bool {
        matches!(
            self.display,
            Display::Block | Display::ListItem | Display::Table | Display::Flex | Display::Grid
        )
    }

    /// Check if element is inline-level
    pub fn is_inline_level(&self) -> bool {
        matches!(self.display, Display::Inline | Display::InlineBlock)
    }

    /// Check if element creates an overlay (fixed/absolute)
    pub fn is_overlay(&self) -> bool {
        matches!(self.position, Position::Fixed | Position::Absolute)
    }

    /// Check if element is sticky positioned
    pub fn is_sticky(&self) -> bool {
        self.position == Position::Sticky
    }

    /// Check if element has positioned offsets
    pub fn has_position_offset(&self) -> bool {
        self.top.is_some() || self.right.is_some() ||
        self.bottom.is_some() || self.left.is_some()
    }

    /// Get the effective top offset (defaults to 0 for sticky)
    pub fn sticky_top(&self) -> i32 {
        self.top.unwrap_or(0)
    }

    /// Check if multi-column layout is enabled
    pub fn is_multicol(&self) -> bool {
        self.column_count.is_some() || self.column_width.is_some()
    }

    /// Get effective column count for layout
    pub fn effective_column_count(&self, available_width: usize) -> usize {
        match (self.column_count, self.column_width) {
            (Some(count), None) => count.max(1),
            (None, Some(width)) => (available_width / width.max(1)).max(1),
            (Some(count), Some(width)) => {
                // Use whichever results in fewer columns
                let by_width = available_width / width.max(1);
                count.min(by_width).max(1)
            }
            (None, None) => 1,
        }
    }

    /// Resolve logical margin-inline-start to physical margin based on direction
    pub fn resolved_margin_left(&self) -> i32 {
        if self.direction == Direction::Ltr {
            self.margin_inline_start.unwrap_or(self.margin_left)
        } else {
            self.margin_inline_end.unwrap_or(self.margin_left)
        }
    }

    /// Resolve logical margin-inline-end to physical margin based on direction
    pub fn resolved_margin_right(&self) -> i32 {
        if self.direction == Direction::Ltr {
            self.margin_inline_end.unwrap_or(self.margin_right)
        } else {
            self.margin_inline_start.unwrap_or(self.margin_right)
        }
    }

    /// Resolve logical padding-inline-start to physical padding
    pub fn resolved_padding_left(&self) -> i32 {
        if self.direction == Direction::Ltr {
            self.padding_inline_start.unwrap_or(self.padding_left)
        } else {
            self.padding_inline_end.unwrap_or(self.padding_left)
        }
    }

    /// Resolve logical padding-inline-end to physical padding
    pub fn resolved_padding_right(&self) -> i32 {
        if self.direction == Direction::Ltr {
            self.padding_inline_end.unwrap_or(self.padding_right)
        } else {
            self.padding_inline_start.unwrap_or(self.padding_right)
        }
    }

    /// Total horizontal margin
    pub fn margin_horizontal(&self) -> i32 {
        self.margin_left + self.margin_right
    }

    /// Total vertical margin
    pub fn margin_vertical(&self) -> i32 {
        self.margin_top + self.margin_bottom
    }

    /// Total horizontal padding
    pub fn padding_horizontal(&self) -> i32 {
        self.padding_left + self.padding_right
    }

    /// Total vertical padding
    pub fn padding_vertical(&self) -> i32 {
        self.padding_top + self.padding_bottom
    }

    /// Total horizontal border
    pub fn border_horizontal(&self) -> i32 {
        self.border_left_width + self.border_right_width
    }

    /// Total inner box offset (padding + border)
    pub fn inner_offset_left(&self) -> i32 {
        self.padding_left + self.border_left_width
    }

    pub fn inner_offset_right(&self) -> i32 {
        self.padding_right + self.border_right_width
    }

    /// Calculate height from width based on aspect ratio
    pub fn height_from_width(&self, width: usize) -> Option<usize> {
        self.aspect_ratio.map(|(w, h)| {
            ((width as f32 * h) / w).round() as usize
        })
    }

    /// Calculate width from height based on aspect ratio
    pub fn width_from_height(&self, height: usize) -> Option<usize> {
        self.aspect_ratio.map(|(w, h)| {
            ((height as f32 * w) / h).round() as usize
        })
    }

    /// Check if this is a vertical writing mode
    pub fn is_vertical(&self) -> bool {
        matches!(
            self.writing_mode,
            WritingMode::VerticalRl | WritingMode::VerticalLr | WritingMode::SidewaysRl | WritingMode::SidewaysLr
        )
    }

    /// Check if this is right-to-left direction
    pub fn is_rtl(&self) -> bool {
        self.direction == Direction::Rtl
    }

    /// Get resolved top/inset-block-start offset
    pub fn resolved_top(&self) -> Option<i32> {
        if self.is_vertical() {
            self.inset_inline_start.or(self.top)
        } else {
            self.inset_block_start.or(self.top)
        }
    }

    /// Get resolved left/inset-inline-start offset
    pub fn resolved_left(&self) -> Option<i32> {
        if self.is_rtl() {
            self.inset_inline_end.or(self.left)
        } else {
            self.inset_inline_start.or(self.left)
        }
    }

    /// Calculate object fit dimensions for an image
    /// Returns (display_width, display_height, offset_x, offset_y)
    pub fn calculate_object_fit(
        &self,
        container_width: usize,
        container_height: usize,
        intrinsic_width: usize,
        intrinsic_height: usize,
    ) -> (usize, usize, usize, usize) {
        if intrinsic_width == 0 || intrinsic_height == 0 {
            return (container_width, container_height, 0, 0);
        }

        let (w, h) = match self.object_fit {
            ObjectFit::Fill => (container_width, container_height),
            ObjectFit::Contain => {
                let scale = (container_width as f32 / intrinsic_width as f32)
                    .min(container_height as f32 / intrinsic_height as f32);
                let w = (intrinsic_width as f32 * scale).round() as usize;
                let h = (intrinsic_height as f32 * scale).round() as usize;
                (w, h)
            }
            ObjectFit::Cover => {
                let scale = (container_width as f32 / intrinsic_width as f32)
                    .max(container_height as f32 / intrinsic_height as f32);
                let w = (intrinsic_width as f32 * scale).round() as usize;
                let h = (intrinsic_height as f32 * scale).round() as usize;
                (w, h)
            }
            ObjectFit::None => (intrinsic_width, intrinsic_height),
            ObjectFit::ScaleDown => {
                // Use none unless it would overflow
                if intrinsic_width <= container_width && intrinsic_height <= container_height {
                    (intrinsic_width, intrinsic_height)
                } else {
                    // Behave like contain
                    let scale = (container_width as f32 / intrinsic_width as f32)
                        .min(container_height as f32 / intrinsic_height as f32);
                    let w = (intrinsic_width as f32 * scale).round() as usize;
                    let h = (intrinsic_height as f32 * scale).round() as usize;
                    (w, h)
                }
            }
        };

        // Calculate offset based on object-position
        let calc_offset = |pos: &ObjectPosition, container: usize, content: usize| -> usize {
            match pos {
                ObjectPosition::Center => container.saturating_sub(content) / 2,
                ObjectPosition::Top | ObjectPosition::Left => 0,
                ObjectPosition::Bottom | ObjectPosition::Right => container.saturating_sub(content),
                ObjectPosition::Percent(p) => ((container.saturating_sub(content)) as f32 * p).round() as usize,
                ObjectPosition::Length(l) => (*l).max(0) as usize,
            }
        };

        let offset_x = calc_offset(&self.object_position.0, container_width, w);
        let offset_y = calc_offset(&self.object_position.1, container_height, h);

        (w, h, offset_x, offset_y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    None,
    Block,
    Inline,
    InlineBlock,
    ListItem,
    Table,
    TableRow,
    TableCell,
    TableHeaderGroup,
    TableRowGroup,
    TableFooterGroup,
    Flex,
    InlineFlex,
    Grid,
    Contents,
}

impl Display {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "none" => Display::None,
            "block" => Display::Block,
            "inline" => Display::Inline,
            "inline-block" => Display::InlineBlock,
            "list-item" => Display::ListItem,
            "table" => Display::Table,
            "table-row" => Display::TableRow,
            "table-cell" => Display::TableCell,
            "table-header-group" => Display::TableHeaderGroup,
            "table-row-group" => Display::TableRowGroup,
            "table-footer-group" => Display::TableFooterGroup,
            "flex" => Display::Flex,
            "inline-flex" => Display::InlineFlex,
            "grid" => Display::Grid,
            "contents" => Display::Contents,
            _ => Display::Block,
        }
    }
}

/// CSS Container Query container type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContainerType {
    /// Normal - not a query container
    #[default]
    Normal,
    /// Size containment on inline axis only
    InlineSize,
    /// Size containment on both axes
    Size,
}

impl ContainerType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "inline-size" => ContainerType::InlineSize,
            "size" => ContainerType::Size,
            "normal" | "" => ContainerType::Normal,
            _ => ContainerType::Normal,
        }
    }

    /// Check if this is a query container
    pub fn is_container(&self) -> bool {
        matches!(self, ContainerType::InlineSize | ContainerType::Size)
    }
}

/// Container query condition
#[derive(Debug, Clone)]
pub struct ContainerQuery {
    /// Optional container name to match
    pub name: Option<String>,
    /// Query conditions
    pub conditions: Vec<ContainerCondition>,
}

/// A single container query condition
#[derive(Debug, Clone)]
pub enum ContainerCondition {
    /// min-width: <value>
    MinWidth(usize),
    /// max-width: <value>
    MaxWidth(usize),
    /// min-height: <value>
    MinHeight(usize),
    /// max-height: <value>
    MaxHeight(usize),
    /// width: <value> (exact match)
    Width(usize),
    /// height: <value> (exact match)
    Height(usize),
}

impl ContainerQuery {
    /// Parse a container query string like "(min-width: 400px)" or "sidebar (min-width: 300px)"
    pub fn parse(query: &str) -> Option<Self> {
        let query = query.trim();

        // Check for container name
        let (name, condition_str) = if let Some(paren_pos) = query.find('(') {
            let name_part = query[..paren_pos].trim();
            let name = if name_part.is_empty() {
                None
            } else {
                Some(name_part.to_string())
            };
            (name, &query[paren_pos..])
        } else {
            (None, query)
        };

        // Parse conditions
        let mut conditions = Vec::new();
        let condition_str = condition_str.trim_matches(|c| c == '(' || c == ')');

        for part in condition_str.split(" and ") {
            let part = part.trim();
            if let Some(condition) = Self::parse_condition(part) {
                conditions.push(condition);
            }
        }

        if conditions.is_empty() {
            return None;
        }

        Some(ContainerQuery { name, conditions })
    }

    fn parse_condition(s: &str) -> Option<ContainerCondition> {
        let s = s.trim();

        // Parse "property: value" format
        let (prop, value) = s.split_once(':')?;
        let prop = prop.trim().to_lowercase();
        let value = value.trim();

        // Parse value to pixels/chars
        let chars = Self::parse_length(value)?;

        match prop.as_str() {
            "min-width" => Some(ContainerCondition::MinWidth(chars)),
            "max-width" => Some(ContainerCondition::MaxWidth(chars)),
            "min-height" => Some(ContainerCondition::MinHeight(chars)),
            "max-height" => Some(ContainerCondition::MaxHeight(chars)),
            "width" => Some(ContainerCondition::Width(chars)),
            "height" => Some(ContainerCondition::Height(chars)),
            _ => None,
        }
    }

    fn parse_length(s: &str) -> Option<usize> {
        let s = s.trim();
        if let Some(px) = s.strip_suffix("px") {
            // Convert px to chars (approximately 8px per char)
            px.trim().parse::<f32>().ok().map(|v| (v / 8.0) as usize)
        } else if let Some(ch) = s.strip_suffix("ch") {
            ch.trim().parse().ok()
        } else if let Some(em) = s.strip_suffix("em") {
            // Assume 1em = 2ch
            em.trim().parse::<f32>().ok().map(|v| (v * 2.0) as usize)
        } else if let Some(rem) = s.strip_suffix("rem") {
            rem.trim().parse::<f32>().ok().map(|v| (v * 2.0) as usize)
        } else {
            // Try parsing as number
            s.parse().ok()
        }
    }

    /// Evaluate the query against container dimensions
    pub fn matches(&self, container_width: usize, container_height: usize) -> bool {
        self.conditions.iter().all(|cond| match cond {
            ContainerCondition::MinWidth(min) => container_width >= *min,
            ContainerCondition::MaxWidth(max) => container_width <= *max,
            ContainerCondition::MinHeight(min) => container_height >= *min,
            ContainerCondition::MaxHeight(max) => container_height <= *max,
            ContainerCondition::Width(w) => container_width == *w,
            ContainerCondition::Height(h) => container_height == *h,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

impl Position {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "static" => Position::Static,
            "relative" => Position::Relative,
            "absolute" => Position::Absolute,
            "fixed" => Position::Fixed,
            "sticky" => Position::Sticky,
            _ => Position::Static,
        }
    }
}

/// CSS float property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Float {
    #[default]
    None,
    Left,
    Right,
    InlineStart,
    InlineEnd,
}

impl Float {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "left" => Float::Left,
            "right" => Float::Right,
            "inline-start" => Float::InlineStart,
            "inline-end" => Float::InlineEnd,
            "none" | "" => Float::None,
            _ => Float::None,
        }
    }

    /// Check if element is floated
    pub fn is_floated(&self) -> bool {
        !matches!(self, Float::None)
    }
}

/// CSS clear property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Clear {
    #[default]
    None,
    Left,
    Right,
    Both,
    InlineStart,
    InlineEnd,
}

impl Clear {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "left" => Clear::Left,
            "right" => Clear::Right,
            "both" => Clear::Both,
            "inline-start" => Clear::InlineStart,
            "inline-end" => Clear::InlineEnd,
            "none" | "" => Clear::None,
            _ => Clear::None,
        }
    }
}

/// Generated content for ::before and ::after pseudo-elements
#[derive(Debug, Clone)]
pub enum GeneratedContent {
    /// Literal string content
    String(String),
    /// attr(attribute-name) - value of an attribute
    Attr(String),
    /// counter(name) - value of a CSS counter
    Counter(String),
    /// Multiple content items
    Multiple(Vec<GeneratedContent>),
    /// URL/image reference
    Url(String),
    /// Open quote character
    OpenQuote,
    /// Close quote character
    CloseQuote,
    /// No content (none/normal)
    None,
}

impl GeneratedContent {
    /// Parse a CSS content value
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();

        // Handle special keywords
        match value.to_lowercase().as_str() {
            "none" | "normal" => return Some(GeneratedContent::None),
            "open-quote" => return Some(GeneratedContent::OpenQuote),
            "close-quote" => return Some(GeneratedContent::CloseQuote),
            _ => {}
        }

        // Handle quoted strings
        if (value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\''))
        {
            let inner = &value[1..value.len() - 1];
            return Some(GeneratedContent::String(Self::unescape_string(inner)));
        }

        // Handle attr()
        if let Some(inner) = value.strip_prefix("attr(").and_then(|s| s.strip_suffix(')')) {
            return Some(GeneratedContent::Attr(inner.trim().to_string()));
        }

        // Handle counter()
        if let Some(inner) = value.strip_prefix("counter(").and_then(|s| s.strip_suffix(')')) {
            return Some(GeneratedContent::Counter(inner.trim().to_string()));
        }

        // Handle url()
        if let Some(inner) = value.strip_prefix("url(").and_then(|s| s.strip_suffix(')')) {
            let url = inner.trim().trim_matches(|c| c == '"' || c == '\'');
            return Some(GeneratedContent::Url(url.to_string()));
        }

        // Handle multiple values (space-separated)
        let parts: Vec<&str> = Self::split_content_values(value);
        if parts.len() > 1 {
            let items: Vec<GeneratedContent> = parts
                .iter()
                .filter_map(|p| Self::parse(p))
                .collect();
            if !items.is_empty() {
                return Some(GeneratedContent::Multiple(items));
            }
        }

        // Fallback: treat as string
        if !value.is_empty() {
            Some(GeneratedContent::String(value.to_string()))
        } else {
            None
        }
    }

    /// Split content value respecting quotes
    fn split_content_values(s: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut in_quotes = false;
        let mut quote_char = ' ';
        let mut paren_depth: usize = 0;

        for (i, c) in s.char_indices() {
            match c {
                '"' | '\'' if !in_quotes => {
                    in_quotes = true;
                    quote_char = c;
                }
                c if in_quotes && c == quote_char => {
                    in_quotes = false;
                }
                '(' if !in_quotes => paren_depth += 1,
                ')' if !in_quotes => paren_depth = paren_depth.saturating_sub(1),
                ' ' if !in_quotes && paren_depth == 0 => {
                    let part = s[start..i].trim();
                    if !part.is_empty() {
                        parts.push(part);
                    }
                    start = i + 1;
                }
                _ => {}
            }
        }

        // Don't forget the last part
        let last = s[start..].trim();
        if !last.is_empty() {
            parts.push(last);
        }

        parts
    }

    /// Unescape CSS string escapes
    fn unescape_string(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(&next) = chars.peek() {
                    match next {
                        'n' => { chars.next(); result.push('\n'); }
                        't' => { chars.next(); result.push('\t'); }
                        'r' => { chars.next(); result.push('\r'); }
                        '"' => { chars.next(); result.push('"'); }
                        '\'' => { chars.next(); result.push('\''); }
                        '\\' => { chars.next(); result.push('\\'); }
                        _ => result.push(c),
                    }
                } else {
                    result.push(c);
                }
            } else {
                result.push(c);
            }
        }

        result
    }

    /// Resolve the content to a string
    pub fn resolve(&self, attrs: &HashMap<String, String>) -> String {
        match self {
            GeneratedContent::String(s) => s.clone(),
            GeneratedContent::Attr(name) => {
                attrs.get(name).cloned().unwrap_or_default()
            }
            GeneratedContent::Counter(name) => {
                // Counters need context - return placeholder
                format!("[{}]", name)
            }
            GeneratedContent::Multiple(items) => {
                items.iter().map(|i| i.resolve(attrs)).collect()
            }
            GeneratedContent::Url(url) => format!("[{}]", url),
            GeneratedContent::OpenQuote => "\u{201C}".to_string(), // "
            GeneratedContent::CloseQuote => "\u{201D}".to_string(), // "
            GeneratedContent::None => String::new(),
        }
    }
}

/// Text overflow behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

impl TextOverflow {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "ellipsis" => TextOverflow::Ellipsis,
            "clip" | _ => TextOverflow::Clip,
        }
    }
}

/// CSS writing-mode property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WritingMode {
    #[default]
    HorizontalTb,
    VerticalRl,
    VerticalLr,
    SidewaysRl,
    SidewaysLr,
}

impl WritingMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "vertical-rl" => WritingMode::VerticalRl,
            "vertical-lr" => WritingMode::VerticalLr,
            "sideways-rl" => WritingMode::SidewaysRl,
            "sideways-lr" => WritingMode::SidewaysLr,
            "horizontal-tb" | _ => WritingMode::HorizontalTb,
        }
    }

    /// Check if writing mode is vertical
    pub fn is_vertical(&self) -> bool {
        matches!(self, WritingMode::VerticalRl | WritingMode::VerticalLr |
                      WritingMode::SidewaysRl | WritingMode::SidewaysLr)
    }
}

/// CSS direction property (for RTL support)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "rtl" => Direction::Rtl,
            "ltr" | _ => Direction::Ltr,
        }
    }
}

/// CSS text-orientation property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextOrientation {
    #[default]
    Mixed,
    Upright,
    Sideways,
}

impl TextOrientation {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "upright" => TextOrientation::Upright,
            "sideways" => TextOrientation::Sideways,
            "mixed" | _ => TextOrientation::Mixed,
        }
    }
}

/// CSS object-fit property for images
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ObjectFit {
    #[default]
    Fill,
    Contain,
    Cover,
    None,
    ScaleDown,
}

impl ObjectFit {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "contain" => ObjectFit::Contain,
            "cover" => ObjectFit::Cover,
            "none" => ObjectFit::None,
            "scale-down" => ObjectFit::ScaleDown,
            "fill" | _ => ObjectFit::Fill,
        }
    }
}

/// CSS object-position values
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ObjectPosition {
    #[default]
    Center,
    Top,
    Bottom,
    Left,
    Right,
    Percent(f32),
    Length(i32),
}

impl ObjectPosition {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "top" => ObjectPosition::Top,
            "bottom" => ObjectPosition::Bottom,
            "left" => ObjectPosition::Left,
            "right" => ObjectPosition::Right,
            "center" => ObjectPosition::Center,
            _ => {
                if let Some(pct) = s.strip_suffix('%') {
                    if let Ok(v) = pct.trim().parse::<f32>() {
                        return ObjectPosition::Percent(v / 100.0);
                    }
                }
                ObjectPosition::Center
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Visible,
    Hidden,
    Collapse,
}

impl Visibility {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "visible" => Visibility::Visible,
            "hidden" => Visibility::Hidden,
            "collapse" => Visibility::Collapse,
            _ => Visibility::Visible,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiteSpace {
    Normal,
    NoWrap,
    Pre,
    PreWrap,
    PreLine,
    BreakSpaces,
}

impl WhiteSpace {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "normal" => WhiteSpace::Normal,
            "nowrap" => WhiteSpace::NoWrap,
            "pre" => WhiteSpace::Pre,
            "pre-wrap" => WhiteSpace::PreWrap,
            "pre-line" => WhiteSpace::PreLine,
            "break-spaces" => WhiteSpace::BreakSpaces,
            _ => WhiteSpace::Normal,
        }
    }

    /// Whether whitespace should be preserved
    pub fn preserves_whitespace(&self) -> bool {
        matches!(self, WhiteSpace::Pre | WhiteSpace::PreWrap | WhiteSpace::PreLine | WhiteSpace::BreakSpaces)
    }

    /// Whether text should wrap
    pub fn wraps(&self) -> bool {
        matches!(self, WhiteSpace::Normal | WhiteSpace::PreWrap | WhiteSpace::PreLine | WhiteSpace::BreakSpaces)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    Visible,
    Hidden,
    Scroll,
    Auto,
}

impl Overflow {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "visible" => Overflow::Visible,
            "hidden" => Overflow::Hidden,
            "scroll" => Overflow::Scroll,
            "auto" => Overflow::Auto,
            _ => Overflow::Visible,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    Normal,
    Bold,
    Lighter,
    Bolder,
    Numeric(u16),
}

impl FontWeight {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "normal" | "400" => FontWeight::Normal,
            "bold" | "700" => FontWeight::Bold,
            "lighter" => FontWeight::Lighter,
            "bolder" => FontWeight::Bolder,
            _ => {
                if let Ok(n) = s.parse::<u16>() {
                    if n >= 600 {
                        FontWeight::Bold
                    } else {
                        FontWeight::Normal
                    }
                } else {
                    FontWeight::Normal
                }
            }
        }
    }

    pub fn is_bold(&self) -> bool {
        match self {
            FontWeight::Bold | FontWeight::Bolder => true,
            FontWeight::Numeric(n) => *n >= 600,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

impl FontStyle {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "normal" => FontStyle::Normal,
            "italic" => FontStyle::Italic,
            "oblique" => FontStyle::Oblique,
            _ => FontStyle::Normal,
        }
    }

    pub fn is_italic(&self) -> bool {
        matches!(self, FontStyle::Italic | FontStyle::Oblique)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDecoration {
    None,
    Underline,
    LineThrough,
    Overline,
    UnderlineLineThrough,
}

impl TextDecoration {
    pub fn from_str(s: &str) -> Self {
        let lower = s.to_lowercase();
        let has_underline = lower.contains("underline");
        let has_line_through = lower.contains("line-through");

        if has_underline && has_line_through {
            TextDecoration::UnderlineLineThrough
        } else if has_underline {
            TextDecoration::Underline
        } else if has_line_through {
            TextDecoration::LineThrough
        } else if lower.contains("overline") {
            TextDecoration::Overline
        } else {
            TextDecoration::None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
    Start,
    End,
}

impl TextAlign {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "left" | "start" => TextAlign::Left,
            "right" | "end" => TextAlign::Right,
            "center" => TextAlign::Center,
            "justify" => TextAlign::Justify,
            _ => TextAlign::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl FlexDirection {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "row" => FlexDirection::Row,
            "row-reverse" => FlexDirection::RowReverse,
            "column" => FlexDirection::Column,
            "column-reverse" => FlexDirection::ColumnReverse,
            _ => FlexDirection::Row,
        }
    }

    pub fn is_row(&self) -> bool {
        matches!(self, FlexDirection::Row | FlexDirection::RowReverse)
    }

    pub fn is_reversed(&self) -> bool {
        matches!(self, FlexDirection::RowReverse | FlexDirection::ColumnReverse)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

impl FlexWrap {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "nowrap" => FlexWrap::NoWrap,
            "wrap" => FlexWrap::Wrap,
            "wrap-reverse" => FlexWrap::WrapReverse,
            _ => FlexWrap::NoWrap,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl JustifyContent {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "flex-start" | "start" => JustifyContent::FlexStart,
            "flex-end" | "end" => JustifyContent::FlexEnd,
            "center" => JustifyContent::Center,
            "space-between" => JustifyContent::SpaceBetween,
            "space-around" => JustifyContent::SpaceAround,
            "space-evenly" => JustifyContent::SpaceEvenly,
            _ => JustifyContent::FlexStart,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
    Stretch,
}

impl AlignItems {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "flex-start" | "start" => AlignItems::FlexStart,
            "flex-end" | "end" => AlignItems::FlexEnd,
            "center" => AlignItems::Center,
            "baseline" => AlignItems::Baseline,
            "stretch" => AlignItems::Stretch,
            _ => AlignItems::Stretch,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListStyleType {
    Disc,
    Circle,
    Square,
    Decimal,
    DecimalLeadingZero,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
    None,
}

impl ListStyleType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "disc" => ListStyleType::Disc,
            "circle" => ListStyleType::Circle,
            "square" => ListStyleType::Square,
            "decimal" => ListStyleType::Decimal,
            "decimal-leading-zero" => ListStyleType::DecimalLeadingZero,
            "lower-alpha" | "lower-latin" => ListStyleType::LowerAlpha,
            "upper-alpha" | "upper-latin" => ListStyleType::UpperAlpha,
            "lower-roman" => ListStyleType::LowerRoman,
            "upper-roman" => ListStyleType::UpperRoman,
            "none" => ListStyleType::None,
            _ => ListStyleType::Disc,
        }
    }

    /// Get the marker for a given index (0-based)
    pub fn marker(&self, index: usize) -> String {
        match self {
            ListStyleType::Disc => "•".to_string(),
            ListStyleType::Circle => "◦".to_string(),
            ListStyleType::Square => "▪".to_string(),
            ListStyleType::Decimal => format!("{}.", index + 1),
            ListStyleType::DecimalLeadingZero => format!("{:02}.", index + 1),
            ListStyleType::LowerAlpha => {
                let c = (b'a' + (index % 26) as u8) as char;
                format!("{}.", c)
            }
            ListStyleType::UpperAlpha => {
                let c = (b'A' + (index % 26) as u8) as char;
                format!("{}.", c)
            }
            ListStyleType::LowerRoman => format!("{}.", to_roman(index + 1).to_lowercase()),
            ListStyleType::UpperRoman => format!("{}.", to_roman(index + 1)),
            ListStyleType::None => String::new(),
        }
    }
}

fn to_roman(mut n: usize) -> String {
    let numerals = [
        (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"),
        (100, "C"), (90, "XC"), (50, "L"), (40, "XL"),
        (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I"),
    ];
    let mut result = String::new();
    for (value, numeral) in numerals {
        while n >= value {
            result.push_str(numeral);
            n -= value;
        }
    }
    result
}

/// Viewport constraints for layout
#[derive(Debug, Clone)]
pub struct Viewport {
    /// Width in characters
    pub width: usize,
    /// Height in lines (None = unlimited)
    pub height: Option<usize>,
    /// Whether to soft-wrap long lines
    pub soft_wrap: bool,
    /// Maximum heading depth to render (1-6)
    pub max_heading_depth: usize,
    /// Color scheme preference
    pub color_scheme: ColorScheme,
    /// Reduced motion preference
    pub prefers_reduced_motion: bool,
    /// Device pixel ratio (for high-DPI detection)
    pub device_pixel_ratio: f32,
}

/// Color scheme preference for prefers-color-scheme media query
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorScheme {
    #[default]
    Normal,
    Light,
    Dark,
    LightDark,
}

impl ColorScheme {
    pub fn from_str(s: &str) -> Self {
        let s = s.to_lowercase();
        if s.contains("light") && s.contains("dark") {
            ColorScheme::LightDark
        } else if s.contains("dark") {
            ColorScheme::Dark
        } else if s.contains("light") {
            ColorScheme::Light
        } else {
            ColorScheme::Normal
        }
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport {
            width: 80,
            height: None,
            soft_wrap: true,
            max_heading_depth: 6,
            color_scheme: ColorScheme::Normal,
            prefers_reduced_motion: false,
            device_pixel_ratio: 1.0,
        }
    }
}

impl Viewport {
    pub fn new(width: usize) -> Self {
        Viewport {
            width,
            ..Default::default()
        }
    }

    pub fn with_height(mut self, height: usize) -> Self {
        self.height = Some(height);
        self
    }

    pub fn with_soft_wrap(mut self, soft_wrap: bool) -> Self {
        self.soft_wrap = soft_wrap;
        self
    }

    pub fn with_color_scheme(mut self, scheme: ColorScheme) -> Self {
        self.color_scheme = scheme;
        self
    }

    pub fn with_reduced_motion(mut self, reduced: bool) -> Self {
        self.prefers_reduced_motion = reduced;
        self
    }

    /// Evaluate a media query against this viewport
    pub fn matches_media_query(&self, query: &MediaQuery) -> bool {
        query.matches(self)
    }
}

/// CSS Media Query
#[derive(Debug, Clone)]
pub struct MediaQuery {
    /// Query conditions (all must match)
    pub conditions: Vec<MediaCondition>,
    /// Whether this is a "not" query
    pub negated: bool,
}

/// A single media query condition
#[derive(Debug, Clone)]
pub enum MediaCondition {
    /// min-width: <chars>
    MinWidth(usize),
    /// max-width: <chars>
    MaxWidth(usize),
    /// min-height: <lines>
    MinHeight(usize),
    /// max-height: <lines>
    MaxHeight(usize),
    /// prefers-color-scheme: light|dark
    ColorScheme(ColorScheme),
    /// prefers-reduced-motion: reduce|no-preference
    ReducedMotion(bool),
    /// screen (always true for terminal)
    Screen,
    /// print (always false for terminal)
    Print,
    /// all (always true)
    All,
}

impl MediaQuery {
    /// Parse a media query string like "(min-width: 600px)" or "screen and (max-width: 800px)"
    pub fn parse(query: &str) -> Option<Self> {
        let query = query.trim();

        // Handle "not" prefix
        let (negated, query) = if query.to_lowercase().starts_with("not ") {
            (true, &query[4..])
        } else {
            (false, query)
        };

        let mut conditions = Vec::new();

        // Split by "and"
        for part in query.split(" and ") {
            let part = part.trim();

            // Handle media type
            let part_lower = part.to_lowercase();
            if part_lower == "screen" {
                conditions.push(MediaCondition::Screen);
                continue;
            } else if part_lower == "print" {
                conditions.push(MediaCondition::Print);
                continue;
            } else if part_lower == "all" {
                conditions.push(MediaCondition::All);
                continue;
            }

            // Handle parenthesized conditions
            let inner = part.trim_matches(|c| c == '(' || c == ')');
            if let Some(cond) = Self::parse_condition(inner) {
                conditions.push(cond);
            }
        }

        if conditions.is_empty() {
            return None;
        }

        Some(MediaQuery { conditions, negated })
    }

    fn parse_condition(s: &str) -> Option<MediaCondition> {
        let (prop, value) = s.split_once(':')?;
        let prop = prop.trim().to_lowercase();
        let value = value.trim();

        match prop.as_str() {
            "min-width" => {
                let chars = Self::parse_length(value)?;
                Some(MediaCondition::MinWidth(chars))
            }
            "max-width" => {
                let chars = Self::parse_length(value)?;
                Some(MediaCondition::MaxWidth(chars))
            }
            "min-height" => {
                let lines = Self::parse_length(value)?;
                Some(MediaCondition::MinHeight(lines))
            }
            "max-height" => {
                let lines = Self::parse_length(value)?;
                Some(MediaCondition::MaxHeight(lines))
            }
            "prefers-color-scheme" => {
                let scheme = match value.to_lowercase().as_str() {
                    "dark" => ColorScheme::Dark,
                    "light" => ColorScheme::Light,
                    _ => return None,
                };
                Some(MediaCondition::ColorScheme(scheme))
            }
            "prefers-reduced-motion" => {
                let reduced = match value.to_lowercase().as_str() {
                    "reduce" => true,
                    "no-preference" => false,
                    _ => return None,
                };
                Some(MediaCondition::ReducedMotion(reduced))
            }
            _ => None,
        }
    }

    fn parse_length(s: &str) -> Option<usize> {
        let s = s.trim();
        if let Some(px) = s.strip_suffix("px") {
            // Convert px to chars (approximately 8px per char)
            px.trim().parse::<f32>().ok().map(|v| (v / 8.0) as usize)
        } else if let Some(ch) = s.strip_suffix("ch") {
            ch.trim().parse().ok()
        } else if let Some(em) = s.strip_suffix("em") {
            // Assume 1em = 2ch
            em.trim().parse::<f32>().ok().map(|v| (v * 2.0) as usize)
        } else {
            s.parse().ok()
        }
    }

    /// Evaluate the query against a viewport
    pub fn matches(&self, viewport: &Viewport) -> bool {
        let result = self.conditions.iter().all(|cond| match cond {
            MediaCondition::MinWidth(min) => viewport.width >= *min,
            MediaCondition::MaxWidth(max) => viewport.width <= *max,
            MediaCondition::MinHeight(min) => {
                viewport.height.map(|h| h >= *min).unwrap_or(true)
            }
            MediaCondition::MaxHeight(max) => {
                viewport.height.map(|h| h <= *max).unwrap_or(false)
            }
            MediaCondition::ColorScheme(scheme) => viewport.color_scheme == *scheme,
            MediaCondition::ReducedMotion(reduced) => viewport.prefers_reduced_motion == *reduced,
            MediaCondition::Screen => true,  // Terminal is always "screen"
            MediaCondition::Print => false,  // Terminal is never "print"
            MediaCondition::All => true,
        });

        if self.negated { !result } else { result }
    }
}

/// URL resolver for converting relative URLs to absolute
pub trait UrlResolver {
    fn base_url(&self) -> &str;
    fn resolve(&self, href: &str) -> String;
}

/// Simple URL resolver with a base URL
#[derive(Debug, Clone)]
pub struct SimpleUrlResolver {
    base: String,
}

impl SimpleUrlResolver {
    pub fn new(base: impl Into<String>) -> Self {
        SimpleUrlResolver { base: base.into() }
    }
}

impl UrlResolver for SimpleUrlResolver {
    fn base_url(&self) -> &str {
        &self.base
    }

    fn resolve(&self, href: &str) -> String {
        if href.starts_with("http://") || href.starts_with("https://") || href.starts_with("//") {
            return href.to_string();
        }
        if href.starts_with('/') {
            // Absolute path
            if let Some(idx) = self.base.find("://") {
                if let Some(end) = self.base[idx + 3..].find('/') {
                    return format!("{}{}", &self.base[..idx + 3 + end], href);
                }
            }
            return format!("{}{}", self.base.trim_end_matches('/'), href);
        }
        // Relative path
        let base = if self.base.ends_with('/') {
            self.base.clone()
        } else if let Some(idx) = self.base.rfind('/') {
            self.base[..=idx].to_string()
        } else {
            format!("{}/", self.base)
        };
        format!("{}{}", base, href)
    }
}

/// Line record for source mapping
#[derive(Debug, Clone)]
pub struct LineRecord {
    /// Line number (1-indexed)
    pub line_number: usize,
    /// Source node ID if mapped
    pub node_id: Option<NodeId>,
    /// The rendered line content
    pub text: String,
}

/// The complete layout plan
#[derive(Debug, Clone)]
pub struct LayoutPlan {
    /// Blocks to render
    pub blocks: Vec<crate::ast::Block>,
    /// Overlay plan
    pub overlays: OverlayPlan,
}

impl LayoutPlan {
    pub fn new() -> Self {
        LayoutPlan {
            blocks: Vec::new(),
            overlays: OverlayPlan::default(),
        }
    }
}

impl Default for LayoutPlan {
    fn default() -> Self {
        Self::new()
    }
}

/// Complete pipeline output
#[derive(Debug)]
pub struct PipelineOutput {
    /// Rendered markdown string
    pub markdown: String,
    /// The layout plan used to generate the markdown
    pub layout_plan: LayoutPlan,
    /// Source mapping from lines to DOM nodes
    pub line_map: Vec<LineRecord>,
}

// ============================================================================
// New layout properties for enhanced features
// ============================================================================

/// CSS vertical-align property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerticalAlign {
    #[default]
    Baseline,
    Top,
    Middle,
    Bottom,
    TextTop,
    TextBottom,
    Sub,
    Super,
}

impl VerticalAlign {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "top" => VerticalAlign::Top,
            "middle" => VerticalAlign::Middle,
            "bottom" => VerticalAlign::Bottom,
            "text-top" => VerticalAlign::TextTop,
            "text-bottom" => VerticalAlign::TextBottom,
            "sub" => VerticalAlign::Sub,
            "super" => VerticalAlign::Super,
            "baseline" | _ => VerticalAlign::Baseline,
        }
    }
}

/// CSS hyphens property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Hyphens {
    #[default]
    None,
    Manual,
    Auto,
}

impl Hyphens {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "manual" => Hyphens::Manual,
            "auto" => Hyphens::Auto,
            "none" | _ => Hyphens::None,
        }
    }
}

/// First-letter pseudo-element styling
#[derive(Debug, Clone, Default)]
pub struct FirstLetterStyle {
    pub font_weight: Option<FontWeight>,
    pub font_style: Option<FontStyle>,
    pub font_size_multiplier: Option<f32>,
    pub text_decoration: Option<TextDecoration>,
    pub float: Option<Float>,
}

/// First-line pseudo-element styling
#[derive(Debug, Clone, Default)]
pub struct FirstLineStyle {
    pub font_weight: Option<FontWeight>,
    pub font_style: Option<FontStyle>,
    pub text_decoration: Option<TextDecoration>,
    pub text_transform: Option<TextTransform>,
}

/// CSS text-transform property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextTransform {
    #[default]
    None,
    Capitalize,
    Uppercase,
    Lowercase,
}

impl TextTransform {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "capitalize" => TextTransform::Capitalize,
            "uppercase" => TextTransform::Uppercase,
            "lowercase" => TextTransform::Lowercase,
            "none" | _ => TextTransform::None,
        }
    }

    pub fn apply(&self, text: &str) -> String {
        match self {
            TextTransform::None => text.to_string(),
            TextTransform::Uppercase => text.to_uppercase(),
            TextTransform::Lowercase => text.to_lowercase(),
            TextTransform::Capitalize => {
                text.split_whitespace()
                    .map(|word| {
                        let mut chars = word.chars();
                        match chars.next() {
                            Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                            None => String::new(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        }
    }
}

/// CSS list-style-position property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListStylePosition {
    #[default]
    Outside,
    Inside,
}

impl ListStylePosition {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "inside" => ListStylePosition::Inside,
            "outside" | _ => ListStylePosition::Outside,
        }
    }
}

/// CSS transform (limited support for TUI)
#[derive(Debug, Clone, PartialEq)]
pub enum Transform {
    None,
    Rotate(f32),           // degrees
    Scale(f32, f32),       // x, y
    Translate(i32, i32),   // x, y in chars
    Matrix(f32, f32, f32, f32, f32, f32), // a, b, c, d, tx, ty
}

impl Default for Transform {
    fn default() -> Self {
        Transform::None
    }
}

impl Transform {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_lowercase();
        if s == "none" {
            return Some(Transform::None);
        }

        // Parse rotate(Xdeg)
        if let Some(inner) = s.strip_prefix("rotate(").and_then(|s| s.strip_suffix(")")) {
            let deg_str = inner.trim().trim_end_matches("deg");
            if let Ok(deg) = deg_str.parse::<f32>() {
                return Some(Transform::Rotate(deg));
            }
        }

        // Parse scale(X) or scale(X, Y)
        if let Some(inner) = s.strip_prefix("scale(").and_then(|s| s.strip_suffix(")")) {
            let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
            match parts.len() {
                1 => {
                    if let Ok(v) = parts[0].parse::<f32>() {
                        return Some(Transform::Scale(v, v));
                    }
                }
                2 => {
                    if let (Ok(x), Ok(y)) = (parts[0].parse::<f32>(), parts[1].parse::<f32>()) {
                        return Some(Transform::Scale(x, y));
                    }
                }
                _ => {}
            }
        }

        // Parse translate(X, Y)
        if let Some(inner) = s.strip_prefix("translate(").and_then(|s| s.strip_suffix(")")) {
            let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
            if parts.len() == 2 {
                let parse_val = |p: &str| -> Option<i32> {
                    p.trim_end_matches("px")
                        .trim_end_matches("ch")
                        .trim()
                        .parse::<i32>()
                        .ok()
                };
                if let (Some(x), Some(y)) = (parse_val(parts[0]), parse_val(parts[1])) {
                    return Some(Transform::Translate(x, y));
                }
            }
        }

        None
    }
}

/// Clip rectangle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClipRect {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
}

impl ClipRect {
    pub fn parse(s: &str) -> Option<Self> {
        // Parse rect(top, right, bottom, left)
        let s = s.trim().to_lowercase();
        if let Some(inner) = s.strip_prefix("rect(").and_then(|s| s.strip_suffix(")")) {
            let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
            if parts.len() == 4 {
                let parse_val = |p: &str| -> Option<i32> {
                    if p == "auto" {
                        Some(0)
                    } else {
                        p.trim_end_matches("px").trim().parse::<i32>().ok()
                    }
                };
                if let (Some(t), Some(r), Some(b), Some(l)) = (
                    parse_val(parts[0]),
                    parse_val(parts[1]),
                    parse_val(parts[2]),
                    parse_val(parts[3]),
                ) {
                    return Some(ClipRect { top: t, right: r, bottom: b, left: l });
                }
            }
        }
        None
    }
}

/// CSS box-decoration-break property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoxDecorationBreak {
    #[default]
    Slice,
    Clone,
}

impl BoxDecorationBreak {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "clone" => BoxDecorationBreak::Clone,
            "slice" | _ => BoxDecorationBreak::Slice,
        }
    }
}

/// CSS break-before/break-after property values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BreakValue {
    #[default]
    Auto,
    Avoid,
    Always,
    All,
    AvoidPage,
    Page,
    Left,
    Right,
    Recto,
    Verso,
    AvoidColumn,
    Column,
    AvoidRegion,
    Region,
}

impl BreakValue {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "avoid" => BreakValue::Avoid,
            "always" => BreakValue::Always,
            "all" => BreakValue::All,
            "avoid-page" => BreakValue::AvoidPage,
            "page" => BreakValue::Page,
            "left" => BreakValue::Left,
            "right" => BreakValue::Right,
            "recto" => BreakValue::Recto,
            "verso" => BreakValue::Verso,
            "avoid-column" => BreakValue::AvoidColumn,
            "column" => BreakValue::Column,
            "avoid-region" => BreakValue::AvoidRegion,
            "region" => BreakValue::Region,
            "auto" | _ => BreakValue::Auto,
        }
    }

    /// Check if this break value forces a break
    pub fn forces_break(&self) -> bool {
        matches!(self, BreakValue::Always | BreakValue::All | BreakValue::Page |
                       BreakValue::Column | BreakValue::Region |
                       BreakValue::Left | BreakValue::Right |
                       BreakValue::Recto | BreakValue::Verso)
    }

    /// Check if this break value avoids a break
    pub fn avoids_break(&self) -> bool {
        matches!(self, BreakValue::Avoid | BreakValue::AvoidPage |
                       BreakValue::AvoidColumn | BreakValue::AvoidRegion)
    }
}

/// CSS break-inside property values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BreakInside {
    #[default]
    Auto,
    Avoid,
    AvoidPage,
    AvoidColumn,
    AvoidRegion,
}

impl BreakInside {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "avoid" => BreakInside::Avoid,
            "avoid-page" => BreakInside::AvoidPage,
            "avoid-column" => BreakInside::AvoidColumn,
            "avoid-region" => BreakInside::AvoidRegion,
            "auto" | _ => BreakInside::Auto,
        }
    }

    /// Check if breaking inside should be avoided
    pub fn avoids_break(&self) -> bool {
        !matches!(self, BreakInside::Auto)
    }
}

// ============================================================================
// Additional CSS properties
// ============================================================================

/// CSS word-break property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordBreak {
    #[default]
    Normal,
    BreakAll,
    KeepAll,
    BreakWord,
}

impl WordBreak {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "break-all" => WordBreak::BreakAll,
            "keep-all" => WordBreak::KeepAll,
            "break-word" => WordBreak::BreakWord,
            "normal" | _ => WordBreak::Normal,
        }
    }

    /// Check if this mode allows breaking within words
    pub fn allows_word_break(&self) -> bool {
        matches!(self, WordBreak::BreakAll | WordBreak::BreakWord)
    }
}

/// CSS overflow-wrap property (formerly word-wrap)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverflowWrap {
    #[default]
    Normal,
    BreakWord,
    Anywhere,
}

impl OverflowWrap {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "break-word" => OverflowWrap::BreakWord,
            "anywhere" => OverflowWrap::Anywhere,
            "normal" | _ => OverflowWrap::Normal,
        }
    }

    /// Check if overflow wrapping is enabled
    pub fn allows_break(&self) -> bool {
        !matches!(self, OverflowWrap::Normal)
    }
}

/// CSS box-sizing property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}

impl BoxSizing {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "border-box" => BoxSizing::BorderBox,
            "content-box" | _ => BoxSizing::ContentBox,
        }
    }
}

/// CSS outline-style property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutlineStyle {
    #[default]
    None,
    Solid,
    Dotted,
    Dashed,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

impl OutlineStyle {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "solid" => OutlineStyle::Solid,
            "dotted" => OutlineStyle::Dotted,
            "dashed" => OutlineStyle::Dashed,
            "double" => OutlineStyle::Double,
            "groove" => OutlineStyle::Groove,
            "ridge" => OutlineStyle::Ridge,
            "inset" => OutlineStyle::Inset,
            "outset" => OutlineStyle::Outset,
            "none" | _ => OutlineStyle::None,
        }
    }

    /// Get character representation for TUI
    pub fn to_char(&self) -> Option<char> {
        match self {
            OutlineStyle::None => None,
            OutlineStyle::Solid => Some('─'),
            OutlineStyle::Dotted => Some('·'),
            OutlineStyle::Dashed => Some('-'),
            OutlineStyle::Double => Some('═'),
            OutlineStyle::Groove | OutlineStyle::Ridge => Some('│'),
            OutlineStyle::Inset | OutlineStyle::Outset => Some('┃'),
        }
    }
}

/// CSS content sizing keywords
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentSizing {
    #[default]
    Auto,
    MinContent,
    MaxContent,
    FitContent,
}

impl ContentSizing {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "min-content" => ContentSizing::MinContent,
            "max-content" => ContentSizing::MaxContent,
            "fit-content" => ContentSizing::FitContent,
            "auto" | _ => ContentSizing::Auto,
        }
    }

    /// Check if this is an intrinsic sizing keyword
    pub fn is_intrinsic(&self) -> bool {
        !matches!(self, ContentSizing::Auto)
    }
}

/// CSS resize property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Resize {
    #[default]
    None,
    Both,
    Horizontal,
    Vertical,
    Block,
    Inline,
}

impl Resize {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "both" => Resize::Both,
            "horizontal" => Resize::Horizontal,
            "vertical" => Resize::Vertical,
            "block" => Resize::Block,
            "inline" => Resize::Inline,
            "none" | _ => Resize::None,
        }
    }

    /// Check if horizontal resize is allowed
    pub fn allows_horizontal(&self) -> bool {
        matches!(self, Resize::Both | Resize::Horizontal | Resize::Inline)
    }

    /// Check if vertical resize is allowed
    pub fn allows_vertical(&self) -> bool {
        matches!(self, Resize::Both | Resize::Vertical | Resize::Block)
    }
}

/// CSS pointer-events property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PointerEvents {
    #[default]
    Auto,
    None,
    VisiblePainted,
    VisibleFill,
    VisibleStroke,
    Visible,
    Painted,
    Fill,
    Stroke,
    All,
}

impl PointerEvents {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "none" => PointerEvents::None,
            "visiblepainted" => PointerEvents::VisiblePainted,
            "visiblefill" => PointerEvents::VisibleFill,
            "visiblestroke" => PointerEvents::VisibleStroke,
            "visible" => PointerEvents::Visible,
            "painted" => PointerEvents::Painted,
            "fill" => PointerEvents::Fill,
            "stroke" => PointerEvents::Stroke,
            "all" => PointerEvents::All,
            "auto" | _ => PointerEvents::Auto,
        }
    }

    /// Check if pointer events are enabled
    pub fn is_interactive(&self) -> bool {
        !matches!(self, PointerEvents::None)
    }
}

/// CSS user-select property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UserSelect {
    #[default]
    Auto,
    None,
    Text,
    All,
    Contain,
}

impl UserSelect {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "none" => UserSelect::None,
            "text" => UserSelect::Text,
            "all" => UserSelect::All,
            "contain" => UserSelect::Contain,
            "auto" | _ => UserSelect::Auto,
        }
    }

    /// Check if text selection is allowed
    pub fn allows_selection(&self) -> bool {
        !matches!(self, UserSelect::None)
    }
}

/// Border collapse mode for tables
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderCollapse {
    #[default]
    Separate,
    Collapse,
}

impl BorderCollapse {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "collapse" => BorderCollapse::Collapse,
            "separate" | _ => BorderCollapse::Separate,
        }
    }
}

/// Empty cells visibility for tables
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmptyCells {
    #[default]
    Show,
    Hide,
}

impl EmptyCells {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "hide" => EmptyCells::Hide,
            "show" | _ => EmptyCells::Show,
        }
    }
}

/// Caption position for tables
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptionSide {
    #[default]
    Top,
    Bottom,
}

impl CaptionSide {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "bottom" => CaptionSide::Bottom,
            "top" | _ => CaptionSide::Top,
        }
    }
}

/// Table layout algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TableLayout {
    #[default]
    Auto,
    Fixed,
}

impl TableLayout {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "fixed" => TableLayout::Fixed,
            "auto" | _ => TableLayout::Auto,
        }
    }
}

/// Text emphasis style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextEmphasisStyle {
    #[default]
    None,
    Filled,
    Open,
    Dot,
    Circle,
    DoubleCircle,
    Triangle,
    Sesame,
}

impl TextEmphasisStyle {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "none" => TextEmphasisStyle::None,
            "filled" => TextEmphasisStyle::Filled,
            "open" => TextEmphasisStyle::Open,
            "dot" => TextEmphasisStyle::Dot,
            "circle" => TextEmphasisStyle::Circle,
            "double-circle" => TextEmphasisStyle::DoubleCircle,
            "triangle" => TextEmphasisStyle::Triangle,
            "sesame" => TextEmphasisStyle::Sesame,
            _ => TextEmphasisStyle::None,
        }
    }
}

/// Text emphasis position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextEmphasisPosition {
    #[default]
    Over,
    Under,
}

impl TextEmphasisPosition {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "under" => TextEmphasisPosition::Under,
            "over" | _ => TextEmphasisPosition::Over,
        }
    }
}

/// Text underline position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextUnderlinePosition {
    #[default]
    Auto,
    Under,
    Left,
    Right,
    FromFont,
}

impl TextUnderlinePosition {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "under" => TextUnderlinePosition::Under,
            "left" => TextUnderlinePosition::Left,
            "right" => TextUnderlinePosition::Right,
            "from-font" => TextUnderlinePosition::FromFont,
            "auto" | _ => TextUnderlinePosition::Auto,
        }
    }
}

/// Ruby annotation position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RubyPosition {
    #[default]
    Over,
    Under,
    InterCharacter,
}

impl RubyPosition {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "under" => RubyPosition::Under,
            "inter-character" => RubyPosition::InterCharacter,
            "over" | _ => RubyPosition::Over,
        }
    }
}

/// Hanging punctuation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HangingPunctuation {
    #[default]
    None,
    First,
    Last,
    ForceEnd,
    AllowEnd,
}

impl HangingPunctuation {
    pub fn from_str(s: &str) -> Self {
        let s = s.to_lowercase();
        if s.contains("first") {
            HangingPunctuation::First
        } else if s.contains("last") {
            HangingPunctuation::Last
        } else if s.contains("force-end") {
            HangingPunctuation::ForceEnd
        } else if s.contains("allow-end") {
            HangingPunctuation::AllowEnd
        } else {
            HangingPunctuation::None
        }
    }
}

/// Forced color adjust mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ForcedColorAdjust {
    #[default]
    Auto,
    None,
    PreserveParentColor,
}

impl ForcedColorAdjust {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "none" => ForcedColorAdjust::None,
            "preserve-parent-color" => ForcedColorAdjust::PreserveParentColor,
            "auto" | _ => ForcedColorAdjust::Auto,
        }
    }
}

/// Accent color for form controls
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccentColor {
    #[default]
    Auto,
    Custom, // In a real implementation, this would hold a color value
}

impl AccentColor {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "auto" => AccentColor::Auto,
            _ => AccentColor::Custom, // Any color value
        }
    }
}

/// Cursor type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cursor {
    #[default]
    Auto,
    Default,
    None,
    ContextMenu,
    Help,
    Pointer,
    Progress,
    Wait,
    Cell,
    Crosshair,
    Text,
    VerticalText,
    Alias,
    Copy,
    Move,
    NoDrop,
    NotAllowed,
    Grab,
    Grabbing,
    EResize,
    NResize,
    NeResize,
    NwResize,
    SResize,
    SeResize,
    SwResize,
    WResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    ColResize,
    RowResize,
    AllScroll,
    ZoomIn,
    ZoomOut,
}

impl Cursor {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "default" => Cursor::Default,
            "none" => Cursor::None,
            "context-menu" => Cursor::ContextMenu,
            "help" => Cursor::Help,
            "pointer" => Cursor::Pointer,
            "progress" => Cursor::Progress,
            "wait" => Cursor::Wait,
            "cell" => Cursor::Cell,
            "crosshair" => Cursor::Crosshair,
            "text" => Cursor::Text,
            "vertical-text" => Cursor::VerticalText,
            "alias" => Cursor::Alias,
            "copy" => Cursor::Copy,
            "move" => Cursor::Move,
            "no-drop" => Cursor::NoDrop,
            "not-allowed" => Cursor::NotAllowed,
            "grab" => Cursor::Grab,
            "grabbing" => Cursor::Grabbing,
            "e-resize" => Cursor::EResize,
            "n-resize" => Cursor::NResize,
            "ne-resize" => Cursor::NeResize,
            "nw-resize" => Cursor::NwResize,
            "s-resize" => Cursor::SResize,
            "se-resize" => Cursor::SeResize,
            "sw-resize" => Cursor::SwResize,
            "w-resize" => Cursor::WResize,
            "ew-resize" => Cursor::EwResize,
            "ns-resize" => Cursor::NsResize,
            "nesw-resize" => Cursor::NeswResize,
            "nwse-resize" => Cursor::NwseResize,
            "col-resize" => Cursor::ColResize,
            "row-resize" => Cursor::RowResize,
            "all-scroll" => Cursor::AllScroll,
            "zoom-in" => Cursor::ZoomIn,
            "zoom-out" => Cursor::ZoomOut,
            "auto" | _ => Cursor::Auto,
        }
    }

    /// Get a display representation of the cursor type
    pub fn as_indicator(&self) -> Option<&'static str> {
        match self {
            Cursor::Pointer => Some("[link]"),
            Cursor::Help => Some("[?]"),
            Cursor::Wait | Cursor::Progress => Some("[...]"),
            Cursor::NotAllowed | Cursor::NoDrop => Some("[X]"),
            Cursor::Text => Some("[I]"),
            Cursor::Move | Cursor::Grab | Cursor::Grabbing => Some("[+]"),
            Cursor::ZoomIn => Some("[+]"),
            Cursor::ZoomOut => Some("[-]"),
            _ => None,
        }
    }
}

/// Caret color for text inputs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaretColor {
    #[default]
    Auto,
    Transparent,
    Custom, // Would hold color value in real implementation
}

impl CaretColor {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "auto" => CaretColor::Auto,
            "transparent" => CaretColor::Transparent,
            _ => CaretColor::Custom,
        }
    }
}

/// CSS contain property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Contain {
    #[default]
    None,
    Strict,
    Content,
    Size,
    Layout,
    Style,
    Paint,
}

impl Contain {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "strict" => Contain::Strict,
            "content" => Contain::Content,
            "size" => Contain::Size,
            "layout" => Contain::Layout,
            "style" => Contain::Style,
            "paint" => Contain::Paint,
            "none" | _ => Contain::None,
        }
    }
}

/// Content visibility for rendering optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentVisibility {
    #[default]
    Visible,
    Auto,
    Hidden,
}

impl ContentVisibility {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "auto" => ContentVisibility::Auto,
            "hidden" => ContentVisibility::Hidden,
            "visible" | _ => ContentVisibility::Visible,
        }
    }

    /// Check if content should be rendered
    pub fn should_render(&self) -> bool {
        !matches!(self, ContentVisibility::Hidden)
    }
}

/// Align-self for flex/grid items
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignSelf {
    #[default]
    Auto,
    Normal,
    Start,
    End,
    Center,
    FlexStart,
    FlexEnd,
    Baseline,
    Stretch,
}

impl AlignSelf {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "auto" => AlignSelf::Auto,
            "normal" => AlignSelf::Normal,
            "start" | "self-start" => AlignSelf::Start,
            "end" | "self-end" => AlignSelf::End,
            "center" => AlignSelf::Center,
            "flex-start" => AlignSelf::FlexStart,
            "flex-end" => AlignSelf::FlexEnd,
            "baseline" => AlignSelf::Baseline,
            "stretch" => AlignSelf::Stretch,
            _ => AlignSelf::Auto,
        }
    }
}

/// Justify-self for grid items
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JustifySelf {
    #[default]
    Auto,
    Normal,
    Start,
    End,
    Center,
    Stretch,
    Baseline,
}

impl JustifySelf {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "auto" => JustifySelf::Auto,
            "normal" => JustifySelf::Normal,
            "start" | "self-start" => JustifySelf::Start,
            "end" | "self-end" => JustifySelf::End,
            "center" => JustifySelf::Center,
            "stretch" => JustifySelf::Stretch,
            "baseline" => JustifySelf::Baseline,
            _ => JustifySelf::Auto,
        }
    }
}

/// Scroll snap type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollSnapType {
    #[default]
    None,
    X,
    Y,
    Block,
    Inline,
    Both,
    XMandatory,
    YMandatory,
    BothMandatory,
}

impl ScrollSnapType {
    pub fn from_str(s: &str) -> Self {
        let s = s.to_lowercase();
        let mandatory = s.contains("mandatory");
        if s.contains("both") {
            if mandatory { ScrollSnapType::BothMandatory } else { ScrollSnapType::Both }
        } else if s.contains("x") {
            if mandatory { ScrollSnapType::XMandatory } else { ScrollSnapType::X }
        } else if s.contains("y") {
            if mandatory { ScrollSnapType::YMandatory } else { ScrollSnapType::Y }
        } else if s.contains("block") {
            ScrollSnapType::Block
        } else if s.contains("inline") {
            ScrollSnapType::Inline
        } else {
            ScrollSnapType::None
        }
    }
}

/// Scroll snap align
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollSnapAlign {
    #[default]
    None,
    Start,
    End,
    Center,
}

impl ScrollSnapAlign {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "start" => ScrollSnapAlign::Start,
            "end" => ScrollSnapAlign::End,
            "center" => ScrollSnapAlign::Center,
            "none" | _ => ScrollSnapAlign::None,
        }
    }
}

/// Scroll snap stop
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollSnapStop {
    #[default]
    Normal,
    Always,
}

impl ScrollSnapStop {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "always" => ScrollSnapStop::Always,
            "normal" | _ => ScrollSnapStop::Normal,
        }
    }
}

/// Scroll behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollBehavior {
    #[default]
    Auto,
    Smooth,
}

impl ScrollBehavior {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "smooth" => ScrollBehavior::Smooth,
            "auto" | _ => ScrollBehavior::Auto,
        }
    }
}

/// Marker pseudo-element style
#[derive(Debug, Clone, Default)]
pub struct MarkerStyle {
    pub content: Option<String>,
    pub color: Option<String>,
    pub font_size: Option<f32>,
}

/// Selection pseudo-element style
#[derive(Debug, Clone, Default)]
pub struct SelectionStyle {
    pub background: Option<String>,
    pub color: Option<String>,
}

/// Placeholder pseudo-element style
#[derive(Debug, Clone, Default)]
pub struct PlaceholderStyle {
    pub color: Option<String>,
    pub opacity: Option<f32>,
    pub font_style: Option<FontStyle>,
}

/// CSS filter
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Filter {
    #[default]
    None,
    Blur(f32),
    Brightness(f32),
    Contrast(f32),
    Grayscale(f32),
    Invert(f32),
    Opacity(f32),
    Saturate(f32),
    Sepia(f32),
}

impl Filter {
    pub fn from_str(s: &str) -> Self {
        let s = s.to_lowercase();
        if s == "none" {
            return Filter::None;
        }

        // Parse filter functions like blur(5px), grayscale(100%)
        if let Some(inner) = s.strip_prefix("blur(").and_then(|s| s.strip_suffix(')')) {
            if let Some(val) = parse_filter_value(inner) {
                return Filter::Blur(val);
            }
        } else if let Some(inner) = s.strip_prefix("brightness(").and_then(|s| s.strip_suffix(')')) {
            if let Some(val) = parse_filter_value(inner) {
                return Filter::Brightness(val);
            }
        } else if let Some(inner) = s.strip_prefix("contrast(").and_then(|s| s.strip_suffix(')')) {
            if let Some(val) = parse_filter_value(inner) {
                return Filter::Contrast(val);
            }
        } else if let Some(inner) = s.strip_prefix("grayscale(").and_then(|s| s.strip_suffix(')')) {
            if let Some(val) = parse_filter_value(inner) {
                return Filter::Grayscale(val);
            }
        } else if let Some(inner) = s.strip_prefix("invert(").and_then(|s| s.strip_suffix(')')) {
            if let Some(val) = parse_filter_value(inner) {
                return Filter::Invert(val);
            }
        } else if let Some(inner) = s.strip_prefix("opacity(").and_then(|s| s.strip_suffix(')')) {
            if let Some(val) = parse_filter_value(inner) {
                return Filter::Opacity(val);
            }
        } else if let Some(inner) = s.strip_prefix("saturate(").and_then(|s| s.strip_suffix(')')) {
            if let Some(val) = parse_filter_value(inner) {
                return Filter::Saturate(val);
            }
        } else if let Some(inner) = s.strip_prefix("sepia(").and_then(|s| s.strip_suffix(')')) {
            if let Some(val) = parse_filter_value(inner) {
                return Filter::Sepia(val);
            }
        }

        Filter::None
    }
}

fn parse_filter_value(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(pct) = s.strip_suffix('%') {
        pct.parse::<f32>().ok().map(|v| v / 100.0)
    } else if let Some(px) = s.strip_suffix("px") {
        px.parse::<f32>().ok()
    } else {
        s.parse::<f32>().ok()
    }
}

/// Mix blend mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MixBlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl MixBlendMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "multiply" => MixBlendMode::Multiply,
            "screen" => MixBlendMode::Screen,
            "overlay" => MixBlendMode::Overlay,
            "darken" => MixBlendMode::Darken,
            "lighten" => MixBlendMode::Lighten,
            "color-dodge" => MixBlendMode::ColorDodge,
            "color-burn" => MixBlendMode::ColorBurn,
            "hard-light" => MixBlendMode::HardLight,
            "soft-light" => MixBlendMode::SoftLight,
            "difference" => MixBlendMode::Difference,
            "exclusion" => MixBlendMode::Exclusion,
            "hue" => MixBlendMode::Hue,
            "saturation" => MixBlendMode::Saturation,
            "color" => MixBlendMode::Color,
            "luminosity" => MixBlendMode::Luminosity,
            "normal" | _ => MixBlendMode::Normal,
        }
    }
}

/// CSS value keywords for property inheritance
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CssValueKeyword {
    Inherit,
    Initial,
    Unset,
    Revert,
}

impl CssValueKeyword {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "inherit" => Some(CssValueKeyword::Inherit),
            "initial" => Some(CssValueKeyword::Initial),
            "unset" => Some(CssValueKeyword::Unset),
            "revert" => Some(CssValueKeyword::Revert),
            _ => None,
        }
    }
}

/// Important property flags (bit positions)
pub mod important_flags {
    pub const DISPLAY: u64 = 1 << 0;
    pub const POSITION: u64 = 1 << 1;
    pub const WIDTH: u64 = 1 << 2;
    pub const HEIGHT: u64 = 1 << 3;
    pub const MARGIN: u64 = 1 << 4;
    pub const PADDING: u64 = 1 << 5;
    pub const COLOR: u64 = 1 << 6;
    pub const BACKGROUND: u64 = 1 << 7;
    pub const FONT_SIZE: u64 = 1 << 8;
    pub const FONT_WEIGHT: u64 = 1 << 9;
    pub const TEXT_ALIGN: u64 = 1 << 10;
    pub const VISIBILITY: u64 = 1 << 11;
    pub const OVERFLOW: u64 = 1 << 12;
    pub const Z_INDEX: u64 = 1 << 13;
    pub const OPACITY: u64 = 1 << 14;
    pub const FLEX: u64 = 1 << 15;
    pub const GRID: u64 = 1 << 16;
}

/// Environment variable values
#[derive(Debug, Clone, PartialEq)]
pub enum EnvValue {
    SafeAreaInsetTop,
    SafeAreaInsetRight,
    SafeAreaInsetBottom,
    SafeAreaInsetLeft,
    TitlebarAreaX,
    TitlebarAreaY,
    TitlebarAreaWidth,
    TitlebarAreaHeight,
    Custom(String),
}

impl EnvValue {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "safe-area-inset-top" => Some(EnvValue::SafeAreaInsetTop),
            "safe-area-inset-right" => Some(EnvValue::SafeAreaInsetRight),
            "safe-area-inset-bottom" => Some(EnvValue::SafeAreaInsetBottom),
            "safe-area-inset-left" => Some(EnvValue::SafeAreaInsetLeft),
            "titlebar-area-x" => Some(EnvValue::TitlebarAreaX),
            "titlebar-area-y" => Some(EnvValue::TitlebarAreaY),
            "titlebar-area-width" => Some(EnvValue::TitlebarAreaWidth),
            "titlebar-area-height" => Some(EnvValue::TitlebarAreaHeight),
            _ => Some(EnvValue::Custom(s.to_string())),
        }
    }

    /// Get default value for this env variable (in ch units)
    pub fn default_value(&self) -> i32 {
        match self {
            // Safe area insets default to 0 in non-notched displays
            EnvValue::SafeAreaInsetTop => 0,
            EnvValue::SafeAreaInsetRight => 0,
            EnvValue::SafeAreaInsetBottom => 0,
            EnvValue::SafeAreaInsetLeft => 0,
            // Titlebar area defaults
            EnvValue::TitlebarAreaX => 0,
            EnvValue::TitlebarAreaY => 0,
            EnvValue::TitlebarAreaWidth => 0,
            EnvValue::TitlebarAreaHeight => 0,
            EnvValue::Custom(_) => 0,
        }
    }
}

/// @supports condition
#[derive(Debug, Clone, PartialEq)]
pub enum SupportsCondition {
    Property(String, String), // property, value
    Selector(String),
    And(Vec<SupportsCondition>),
    Or(Vec<SupportsCondition>),
    Not(Box<SupportsCondition>),
}

impl SupportsCondition {
    /// Check if feature is supported (simplified - we support most CSS)
    pub fn is_supported(&self) -> bool {
        match self {
            SupportsCondition::Property(prop, _) => {
                // List of supported properties
                let supported = [
                    "display", "position", "width", "height", "margin", "padding",
                    "flex", "grid", "float", "clear", "overflow", "visibility",
                    "opacity", "filter", "transform", "text-align", "font-weight",
                    "color", "background", "border", "outline", "gap", "order",
                ];
                supported.iter().any(|s| prop.contains(s))
            }
            SupportsCondition::Selector(_) => true, // We support most selectors
            SupportsCondition::And(conditions) => conditions.iter().all(|c| c.is_supported()),
            SupportsCondition::Or(conditions) => conditions.iter().any(|c| c.is_supported()),
            SupportsCondition::Not(condition) => !condition.is_supported(),
        }
    }
}

// ============================================================================
// CSS Selector Parsing and Specificity
// ============================================================================

/// CSS Specificity (a, b, c) where:
/// - a = ID selectors
/// - b = class selectors, attribute selectors, pseudo-classes
/// - c = type selectors, pseudo-elements
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Specificity(pub u32, pub u32, pub u32);

impl Specificity {
    pub fn new(ids: u32, classes: u32, elements: u32) -> Self {
        Specificity(ids, classes, elements)
    }

    /// Calculate numeric value for comparison (simplified)
    pub fn value(&self) -> u32 {
        self.0 * 10000 + self.1 * 100 + self.2
    }

    /// Add two specificities
    pub fn add(&self, other: &Specificity) -> Specificity {
        Specificity(self.0 + other.0, self.1 + other.1, self.2 + other.2)
    }
}

/// A parsed CSS selector
#[derive(Debug, Clone, PartialEq)]
pub struct CssSelector {
    pub parts: Vec<SelectorPart>,
    pub specificity: Specificity,
}

/// Part of a CSS selector
#[derive(Debug, Clone, PartialEq)]
pub enum SelectorPart {
    /// Universal selector (*)
    Universal,
    /// Type/tag selector (div, span, etc.)
    Type(String),
    /// Class selector (.class)
    Class(String),
    /// ID selector (#id)
    Id(String),
    /// Attribute selector ([attr], [attr=value], etc.)
    Attribute(AttributeSelector),
    /// Pseudo-class (:hover, :first-child, etc.)
    PseudoClass(PseudoClass),
    /// Pseudo-element (::before, ::after, etc.)
    PseudoElement(PseudoElement),
    /// Combinator (space, >, +, ~)
    Combinator(Combinator),
}

/// Attribute selector
#[derive(Debug, Clone, PartialEq)]
pub struct AttributeSelector {
    pub name: String,
    pub matcher: Option<AttributeMatcher>,
    pub value: Option<String>,
    pub case_insensitive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeMatcher {
    Exact,        // [attr=value]
    Contains,     // [attr*=value]
    StartsWith,   // [attr^=value]
    EndsWith,     // [attr$=value]
    WhiteSpace,   // [attr~=value]
    Hyphen,       // [attr|=value]
}

/// CSS Pseudo-classes
#[derive(Debug, Clone, PartialEq)]
pub enum PseudoClass {
    // Structural
    FirstChild,
    LastChild,
    OnlyChild,
    FirstOfType,
    LastOfType,
    OnlyOfType,
    NthChild(NthExpression),
    NthLastChild(NthExpression),
    NthOfType(NthExpression),
    NthLastOfType(NthExpression),
    Empty,
    Root,

    // User action (limited in terminal)
    Hover,
    Active,
    Focus,
    FocusVisible,
    FocusWithin,
    Visited,
    Link,

    // Input states
    Enabled,
    Disabled,
    Checked,
    Indeterminate,
    Required,
    Optional,
    Valid,
    Invalid,
    InRange,
    OutOfRange,
    ReadOnly,
    ReadWrite,
    PlaceholderShown,
    Default,

    // Selector functions
    Not(Box<CssSelector>),
    Is(Vec<CssSelector>),
    Where(Vec<CssSelector>),
    Has(Vec<CssSelector>),

    // Other
    Target,
    Lang(String),
    Dir(Direction),
}

/// Nth expression (an + b)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NthExpression {
    pub a: i32,  // coefficient
    pub b: i32,  // offset
}

impl NthExpression {
    pub fn new(a: i32, b: i32) -> Self {
        NthExpression { a, b }
    }

    /// Check if index n matches this expression (1-indexed)
    pub fn matches(&self, n: usize) -> bool {
        let n = n as i32;
        if self.a == 0 {
            return n == self.b;
        }
        let diff = n - self.b;
        if self.a > 0 {
            diff >= 0 && diff % self.a == 0
        } else {
            diff <= 0 && diff % self.a == 0
        }
    }

    /// Parse from string like "2n+1", "odd", "even", "3"
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_lowercase();
        match s.as_str() {
            "odd" => Some(NthExpression::new(2, 1)),
            "even" => Some(NthExpression::new(2, 0)),
            _ => {
                // Parse an+b format
                if let Some(n_pos) = s.find('n') {
                    let a_str = &s[..n_pos];
                    let a = if a_str.is_empty() || a_str == "+" {
                        1
                    } else if a_str == "-" {
                        -1
                    } else {
                        a_str.parse().ok()?
                    };

                    let b_str = s[n_pos + 1..].trim();
                    let b = if b_str.is_empty() {
                        0
                    } else {
                        b_str.replace(' ', "").parse().ok()?
                    };

                    Some(NthExpression::new(a, b))
                } else {
                    // Just a number
                    let b = s.parse().ok()?;
                    Some(NthExpression::new(0, b))
                }
            }
        }
    }
}

/// CSS Pseudo-elements
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PseudoElement {
    Before,
    After,
    FirstLine,
    FirstLetter,
    Marker,
    Selection,
    Placeholder,
    Backdrop,
}

/// Selector combinators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    Descendant,     // space
    Child,          // >
    NextSibling,    // +
    SubsequentSibling, // ~
}

impl CssSelector {
    /// Parse a CSS selector string
    pub fn parse(selector: &str) -> Option<Self> {
        let selector = selector.trim();
        if selector.is_empty() {
            return None;
        }

        let mut parts = Vec::new();
        let mut specificity = Specificity::default();
        let mut chars = selector.chars().peekable();
        let mut current = String::new();

        while let Some(c) = chars.next() {
            match c {
                '.' => {
                    // Flush current as type selector
                    if !current.is_empty() {
                        parts.push(SelectorPart::Type(current.clone()));
                        specificity.2 += 1;
                        current.clear();
                    }
                    // Parse class
                    let mut class_name = String::new();
                    while let Some(&ch) = chars.peek() {
                        if ch.is_alphanumeric() || ch == '-' || ch == '_' {
                            class_name.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                    if !class_name.is_empty() {
                        parts.push(SelectorPart::Class(class_name));
                        specificity.1 += 1;
                    }
                }
                '#' => {
                    if !current.is_empty() {
                        parts.push(SelectorPart::Type(current.clone()));
                        specificity.2 += 1;
                        current.clear();
                    }
                    let mut id_name = String::new();
                    while let Some(&ch) = chars.peek() {
                        if ch.is_alphanumeric() || ch == '-' || ch == '_' {
                            id_name.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                    if !id_name.is_empty() {
                        parts.push(SelectorPart::Id(id_name));
                        specificity.0 += 1;
                    }
                }
                '[' => {
                    if !current.is_empty() {
                        parts.push(SelectorPart::Type(current.clone()));
                        specificity.2 += 1;
                        current.clear();
                    }
                    // Parse attribute selector
                    let mut attr_content = String::new();
                    let mut depth = 1;
                    while let Some(ch) = chars.next() {
                        if ch == '[' {
                            depth += 1;
                        } else if ch == ']' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        attr_content.push(ch);
                    }
                    if let Some(attr) = parse_attribute_selector(&attr_content) {
                        parts.push(SelectorPart::Attribute(attr));
                        specificity.1 += 1;
                    }
                }
                ':' => {
                    if !current.is_empty() {
                        parts.push(SelectorPart::Type(current.clone()));
                        specificity.2 += 1;
                        current.clear();
                    }
                    // Check for pseudo-element (::)
                    let is_element = chars.peek() == Some(&':');
                    if is_element {
                        chars.next();
                    }
                    // Parse pseudo name
                    let mut pseudo_name = String::new();
                    while let Some(&ch) = chars.peek() {
                        if ch.is_alphanumeric() || ch == '-' {
                            pseudo_name.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                    // Check for functional pseudo
                    let mut pseudo_arg = None;
                    if chars.peek() == Some(&'(') {
                        chars.next();
                        let mut arg = String::new();
                        let mut depth = 1;
                        while let Some(ch) = chars.next() {
                            if ch == '(' {
                                depth += 1;
                            } else if ch == ')' {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            arg.push(ch);
                        }
                        pseudo_arg = Some(arg);
                    }

                    if is_element {
                        if let Some(pe) = parse_pseudo_element(&pseudo_name) {
                            parts.push(SelectorPart::PseudoElement(pe));
                            specificity.2 += 1;
                        }
                    } else {
                        if let Some(pc) = parse_pseudo_class(&pseudo_name, pseudo_arg) {
                            // :not(), :is(), :where() have special specificity rules
                            let pc_spec = pseudo_class_specificity(&pc);
                            specificity = specificity.add(&pc_spec);
                            parts.push(SelectorPart::PseudoClass(pc));
                        }
                    }
                }
                '*' => {
                    if !current.is_empty() {
                        parts.push(SelectorPart::Type(current.clone()));
                        specificity.2 += 1;
                        current.clear();
                    }
                    parts.push(SelectorPart::Universal);
                }
                ' ' | '>' | '+' | '~' => {
                    if !current.is_empty() {
                        parts.push(SelectorPart::Type(current.clone()));
                        specificity.2 += 1;
                        current.clear();
                    }
                    // Skip whitespace
                    while chars.peek() == Some(&' ') {
                        chars.next();
                    }
                    let comb = match c {
                        '>' => Combinator::Child,
                        '+' => Combinator::NextSibling,
                        '~' => Combinator::SubsequentSibling,
                        _ => {
                            // Check if next char is a combinator
                            match chars.peek() {
                                Some(&'>') => { chars.next(); Combinator::Child }
                                Some(&'+') => { chars.next(); Combinator::NextSibling }
                                Some(&'~') => { chars.next(); Combinator::SubsequentSibling }
                                _ => Combinator::Descendant,
                            }
                        }
                    };
                    // Skip whitespace after combinator
                    while chars.peek() == Some(&' ') {
                        chars.next();
                    }
                    parts.push(SelectorPart::Combinator(comb));
                }
                _ => {
                    current.push(c);
                }
            }
        }

        // Flush remaining
        if !current.is_empty() {
            parts.push(SelectorPart::Type(current));
            specificity.2 += 1;
        }

        if parts.is_empty() {
            None
        } else {
            Some(CssSelector { parts, specificity })
        }
    }

    /// Check if this selector matches an element
    pub fn matches(&self, element: &ElementInfo) -> bool {
        // Simplified matching - just check the last simple selector
        for part in self.parts.iter().rev() {
            match part {
                SelectorPart::Type(name) => {
                    if element.tag_name.to_lowercase() != name.to_lowercase() {
                        return false;
                    }
                }
                SelectorPart::Class(class) => {
                    if !element.classes.iter().any(|c| c == class) {
                        return false;
                    }
                }
                SelectorPart::Id(id) => {
                    if element.id.as_ref() != Some(id) {
                        return false;
                    }
                }
                SelectorPart::Universal => {}
                SelectorPart::Combinator(_) => break, // Stop at combinator
                SelectorPart::PseudoClass(pc) => {
                    if !matches_pseudo_class(pc, element) {
                        return false;
                    }
                }
                SelectorPart::Attribute(attr) => {
                    if !matches_attribute(attr, element) {
                        return false;
                    }
                }
                _ => {}
            }
        }
        true
    }
}

/// Element info for selector matching
#[derive(Debug, Clone, Default)]
pub struct ElementInfo {
    pub tag_name: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attributes: std::collections::HashMap<String, String>,
    pub index: usize,          // 1-indexed position among siblings
    pub sibling_count: usize,
    pub type_index: usize,     // 1-indexed position among same-type siblings
    pub type_count: usize,
    pub is_empty: bool,
    pub is_root: bool,
    pub is_link: bool,
    pub is_visited: bool,
    pub is_enabled: bool,
    pub is_disabled: bool,
    pub is_checked: bool,
    pub is_required: bool,
    pub is_valid: bool,
    pub is_focus: bool,
    pub lang: Option<String>,
    pub dir: Option<Direction>,
}

fn parse_attribute_selector(content: &str) -> Option<AttributeSelector> {
    let content = content.trim();

    // Check for matcher
    let matchers = [("*=", AttributeMatcher::Contains),
                    ("^=", AttributeMatcher::StartsWith),
                    ("$=", AttributeMatcher::EndsWith),
                    ("~=", AttributeMatcher::WhiteSpace),
                    ("|=", AttributeMatcher::Hyphen),
                    ("=", AttributeMatcher::Exact)];

    for (op, matcher) in matchers {
        if let Some(pos) = content.find(op) {
            let name = content[..pos].trim().to_string();
            let mut value = content[pos + op.len()..].trim().to_string();
            let case_insensitive = value.ends_with(" i") || value.ends_with(" I");
            if case_insensitive {
                value = value[..value.len() - 2].trim().to_string();
            }
            // Strip quotes
            if (value.starts_with('"') && value.ends_with('"')) ||
               (value.starts_with('\'') && value.ends_with('\'')) {
                value = value[1..value.len()-1].to_string();
            }
            return Some(AttributeSelector {
                name,
                matcher: Some(matcher),
                value: Some(value),
                case_insensitive,
            });
        }
    }

    // Just attribute presence
    Some(AttributeSelector {
        name: content.to_string(),
        matcher: None,
        value: None,
        case_insensitive: false,
    })
}

fn parse_pseudo_element(name: &str) -> Option<PseudoElement> {
    match name.to_lowercase().as_str() {
        "before" => Some(PseudoElement::Before),
        "after" => Some(PseudoElement::After),
        "first-line" => Some(PseudoElement::FirstLine),
        "first-letter" => Some(PseudoElement::FirstLetter),
        "marker" => Some(PseudoElement::Marker),
        "selection" => Some(PseudoElement::Selection),
        "placeholder" => Some(PseudoElement::Placeholder),
        "backdrop" => Some(PseudoElement::Backdrop),
        _ => None,
    }
}

fn parse_pseudo_class(name: &str, arg: Option<String>) -> Option<PseudoClass> {
    match name.to_lowercase().as_str() {
        // Structural
        "first-child" => Some(PseudoClass::FirstChild),
        "last-child" => Some(PseudoClass::LastChild),
        "only-child" => Some(PseudoClass::OnlyChild),
        "first-of-type" => Some(PseudoClass::FirstOfType),
        "last-of-type" => Some(PseudoClass::LastOfType),
        "only-of-type" => Some(PseudoClass::OnlyOfType),
        "empty" => Some(PseudoClass::Empty),
        "root" => Some(PseudoClass::Root),
        "nth-child" => arg.and_then(|a| NthExpression::parse(&a)).map(PseudoClass::NthChild),
        "nth-last-child" => arg.and_then(|a| NthExpression::parse(&a)).map(PseudoClass::NthLastChild),
        "nth-of-type" => arg.and_then(|a| NthExpression::parse(&a)).map(PseudoClass::NthOfType),
        "nth-last-of-type" => arg.and_then(|a| NthExpression::parse(&a)).map(PseudoClass::NthLastOfType),
        // User action
        "hover" => Some(PseudoClass::Hover),
        "active" => Some(PseudoClass::Active),
        "focus" => Some(PseudoClass::Focus),
        "focus-visible" => Some(PseudoClass::FocusVisible),
        "focus-within" => Some(PseudoClass::FocusWithin),
        "visited" => Some(PseudoClass::Visited),
        "link" => Some(PseudoClass::Link),
        // Input states
        "enabled" => Some(PseudoClass::Enabled),
        "disabled" => Some(PseudoClass::Disabled),
        "checked" => Some(PseudoClass::Checked),
        "indeterminate" => Some(PseudoClass::Indeterminate),
        "required" => Some(PseudoClass::Required),
        "optional" => Some(PseudoClass::Optional),
        "valid" => Some(PseudoClass::Valid),
        "invalid" => Some(PseudoClass::Invalid),
        "in-range" => Some(PseudoClass::InRange),
        "out-of-range" => Some(PseudoClass::OutOfRange),
        "read-only" => Some(PseudoClass::ReadOnly),
        "read-write" => Some(PseudoClass::ReadWrite),
        "placeholder-shown" => Some(PseudoClass::PlaceholderShown),
        "default" => Some(PseudoClass::Default),
        // Selector functions
        "not" => arg.and_then(|a| CssSelector::parse(&a))
            .map(|s| PseudoClass::Not(Box::new(s))),
        "is" | "matches" | "-webkit-any" | "-moz-any" => arg.map(|a| {
            let selectors: Vec<_> = a.split(',')
                .filter_map(|s| CssSelector::parse(s.trim()))
                .collect();
            PseudoClass::Is(selectors)
        }),
        "where" => arg.map(|a| {
            let selectors: Vec<_> = a.split(',')
                .filter_map(|s| CssSelector::parse(s.trim()))
                .collect();
            PseudoClass::Where(selectors)
        }),
        "has" => arg.map(|a| {
            let selectors: Vec<_> = a.split(',')
                .filter_map(|s| CssSelector::parse(s.trim()))
                .collect();
            PseudoClass::Has(selectors)
        }),
        // Other
        "target" => Some(PseudoClass::Target),
        "lang" => arg.map(PseudoClass::Lang),
        "dir" => arg.map(|a| {
            let dir = if a.to_lowercase() == "rtl" { Direction::Rtl } else { Direction::Ltr };
            PseudoClass::Dir(dir)
        }),
        _ => None,
    }
}

fn pseudo_class_specificity(pc: &PseudoClass) -> Specificity {
    match pc {
        // :where() has zero specificity
        PseudoClass::Where(_) => Specificity::default(),
        // :not() and :is() take the specificity of the most specific argument
        PseudoClass::Not(sel) => sel.specificity,
        PseudoClass::Is(selectors) | PseudoClass::Has(selectors) => {
            selectors.iter()
                .map(|s| s.specificity)
                .max()
                .unwrap_or_default()
        }
        // All other pseudo-classes have specificity (0, 1, 0)
        _ => Specificity(0, 1, 0),
    }
}

fn matches_pseudo_class(pc: &PseudoClass, elem: &ElementInfo) -> bool {
    match pc {
        PseudoClass::FirstChild => elem.index == 1,
        PseudoClass::LastChild => elem.index == elem.sibling_count,
        PseudoClass::OnlyChild => elem.sibling_count == 1,
        PseudoClass::FirstOfType => elem.type_index == 1,
        PseudoClass::LastOfType => elem.type_index == elem.type_count,
        PseudoClass::OnlyOfType => elem.type_count == 1,
        PseudoClass::NthChild(expr) => expr.matches(elem.index),
        PseudoClass::NthLastChild(expr) => expr.matches(elem.sibling_count - elem.index + 1),
        PseudoClass::NthOfType(expr) => expr.matches(elem.type_index),
        PseudoClass::NthLastOfType(expr) => expr.matches(elem.type_count - elem.type_index + 1),
        PseudoClass::Empty => elem.is_empty,
        PseudoClass::Root => elem.is_root,
        PseudoClass::Link => elem.is_link,
        PseudoClass::Visited => elem.is_visited,
        PseudoClass::Enabled => elem.is_enabled,
        PseudoClass::Disabled => elem.is_disabled,
        PseudoClass::Checked => elem.is_checked,
        PseudoClass::Required => elem.is_required,
        PseudoClass::Optional => !elem.is_required,
        PseudoClass::Valid => elem.is_valid,
        PseudoClass::Invalid => !elem.is_valid,
        PseudoClass::Focus => elem.is_focus,
        PseudoClass::Lang(lang) => elem.lang.as_ref().map_or(false, |l| l.starts_with(lang)),
        PseudoClass::Dir(dir) => elem.dir.as_ref() == Some(dir),
        PseudoClass::Not(sel) => !sel.matches(elem),
        PseudoClass::Is(selectors) => selectors.iter().any(|s| s.matches(elem)),
        PseudoClass::Where(selectors) => selectors.iter().any(|s| s.matches(elem)),
        PseudoClass::Has(_) => false, // Has requires checking descendants
        // User action pseudo-classes (always false in terminal)
        PseudoClass::Hover | PseudoClass::Active => false,
        _ => true, // Default to true for unknown/unsupported
    }
}

fn matches_attribute(attr: &AttributeSelector, elem: &ElementInfo) -> bool {
    let value = elem.attributes.get(&attr.name);

    match (&attr.matcher, &attr.value, value) {
        (None, _, Some(_)) => true, // [attr] - presence check
        (None, _, None) => false,
        (Some(_), _, None) => false,
        (Some(matcher), Some(expected), Some(actual)) => {
            let (expected, actual) = if attr.case_insensitive {
                (expected.to_lowercase(), actual.to_lowercase())
            } else {
                (expected.clone(), actual.clone())
            };
            match matcher {
                AttributeMatcher::Exact => actual == expected,
                AttributeMatcher::Contains => actual.contains(&expected),
                AttributeMatcher::StartsWith => actual.starts_with(&expected),
                AttributeMatcher::EndsWith => actual.ends_with(&expected),
                AttributeMatcher::WhiteSpace => actual.split_whitespace().any(|w| w == expected),
                AttributeMatcher::Hyphen => actual == expected || actual.starts_with(&format!("{}-", expected)),
            }
        }
        _ => false,
    }
}

// ============================================================================
// CSS Animations & Transitions
// ============================================================================

/// CSS Transition
#[derive(Debug, Clone, Default)]
pub struct Transition {
    pub property: TransitionProperty,
    pub duration: f32,      // seconds
    pub timing: TimingFunction,
    pub delay: f32,         // seconds
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TransitionProperty {
    #[default]
    All,
    None,
    Property(String),
}

impl TransitionProperty {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "all" => TransitionProperty::All,
            "none" => TransitionProperty::None,
            _ => TransitionProperty::Property(s.to_string()),
        }
    }
}

/// Timing function
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TimingFunction {
    #[default]
    Ease,
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    StepStart,
    StepEnd,
    CubicBezier(f32, f32, f32, f32),
    Steps(u32, StepPosition),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepPosition {
    #[default]
    End,
    Start,
    JumpNone,
    JumpBoth,
}

impl TimingFunction {
    pub fn from_str(s: &str) -> Self {
        let s = s.to_lowercase();
        match s.as_str() {
            "linear" => TimingFunction::Linear,
            "ease" => TimingFunction::Ease,
            "ease-in" => TimingFunction::EaseIn,
            "ease-out" => TimingFunction::EaseOut,
            "ease-in-out" => TimingFunction::EaseInOut,
            "step-start" => TimingFunction::StepStart,
            "step-end" => TimingFunction::StepEnd,
            _ if s.starts_with("cubic-bezier(") => {
                if let Some(inner) = s.strip_prefix("cubic-bezier(").and_then(|s| s.strip_suffix(')')) {
                    let parts: Vec<f32> = inner.split(',')
                        .filter_map(|p| p.trim().parse().ok())
                        .collect();
                    if parts.len() == 4 {
                        return TimingFunction::CubicBezier(parts[0], parts[1], parts[2], parts[3]);
                    }
                }
                TimingFunction::Ease
            }
            _ if s.starts_with("steps(") => {
                if let Some(inner) = s.strip_prefix("steps(").and_then(|s| s.strip_suffix(')')) {
                    let parts: Vec<&str> = inner.split(',').collect();
                    if let Some(steps) = parts.first().and_then(|p| p.trim().parse().ok()) {
                        let pos = parts.get(1).map_or(StepPosition::End, |p| {
                            match p.trim() {
                                "start" => StepPosition::Start,
                                "jump-none" => StepPosition::JumpNone,
                                "jump-both" => StepPosition::JumpBoth,
                                _ => StepPosition::End,
                            }
                        });
                        return TimingFunction::Steps(steps, pos);
                    }
                }
                TimingFunction::Ease
            }
            _ => TimingFunction::Ease,
        }
    }
}

/// CSS Animation
#[derive(Debug, Clone, Default)]
pub struct Animation {
    pub name: String,
    pub duration: f32,
    pub timing: TimingFunction,
    pub delay: f32,
    pub iteration_count: AnimationIterationCount,
    pub direction: AnimationDirection,
    pub fill_mode: AnimationFillMode,
    pub play_state: AnimationPlayState,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AnimationIterationCount {
    #[default]
    One,
    Infinite,
    Count(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationDirection {
    #[default]
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationFillMode {
    #[default]
    None,
    Forwards,
    Backwards,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationPlayState {
    #[default]
    Running,
    Paused,
}

/// Keyframe rule
#[derive(Debug, Clone)]
pub struct Keyframe {
    pub offset: f32, // 0.0 to 1.0
    pub properties: Vec<(String, String)>,
}

/// @keyframes rule
#[derive(Debug, Clone)]
pub struct KeyframesRule {
    pub name: String,
    pub keyframes: Vec<Keyframe>,
}

// ============================================================================
// Cascade Layers
// ============================================================================

/// @layer rule
#[derive(Debug, Clone, Default)]
pub struct CascadeLayer {
    pub name: Option<String>,
    pub order: usize,
}

/// Layer order for cascade
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LayerOrigin {
    UserAgent,
    User,
    Author,
    AuthorImportant,
    UserImportant,
    UserAgentImportant,
}

// ============================================================================
// CSS Nesting
// ============================================================================

/// Nested style rule
#[derive(Debug, Clone)]
pub struct NestedRule {
    pub selector: String,
    pub declarations: Vec<(String, String)>,
    pub nested: Vec<NestedRule>,
}

// ============================================================================
// Advanced Layout Types
// ============================================================================

/// Subgrid value
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SubgridValue {
    #[default]
    None,
    Subgrid,
}

impl SubgridValue {
    pub fn from_str(s: &str) -> Self {
        if s.to_lowercase() == "subgrid" {
            SubgridValue::Subgrid
        } else {
            SubgridValue::None
        }
    }
}

/// Masonry layout
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MasonryValue {
    #[default]
    None,
    Masonry,
}

/// Anchor positioning
#[derive(Debug, Clone, PartialEq)]
pub struct AnchorPosition {
    pub anchor_name: Option<String>,
    pub position_anchor: Option<String>,
}

impl Default for AnchorPosition {
    fn default() -> Self {
        AnchorPosition {
            anchor_name: None,
            position_anchor: None,
        }
    }
}

/// Media feature for @media queries
#[derive(Debug, Clone)]
pub enum MediaFeature {
    Width(MediaRange),
    Height(MediaRange),
    AspectRatio(MediaRange),
    Orientation(Orientation),
    Resolution(MediaRange),
    Color(MediaRange),
    ColorGamut(ColorGamut),
    ColorSchemeQuery(ColorSchemePreference),
    PrefersReducedMotion(ReducedMotion),
    PrefersContrast(ContrastPreference),
    PrefersReducedTransparency(bool),
    PrefersColorScheme(ColorSchemePreference),
    ForcedColors(bool),
    Hover(HoverCapability),
    Pointer(PointerCapability),
    AnyHover(HoverCapability),
    AnyPointer(PointerCapability),
    DisplayMode(DisplayModeValue),
    Scripting(ScriptingValue),
    Update(UpdateFrequency),
    OverflowBlock(OverflowMediaValue),
    OverflowInline(OverflowMediaValue),
    Grid(bool),
    Monochrome(MediaRange),
    InvertedColors(bool),
}

#[derive(Debug, Clone)]
pub struct MediaRange {
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub exact: Option<f32>,
}

impl MediaRange {
    pub fn new() -> Self {
        MediaRange { min: None, max: None, exact: None }
    }

    pub fn min(value: f32) -> Self {
        MediaRange { min: Some(value), max: None, exact: None }
    }

    pub fn max(value: f32) -> Self {
        MediaRange { min: None, max: Some(value), exact: None }
    }

    pub fn exact(value: f32) -> Self {
        MediaRange { min: None, max: None, exact: Some(value) }
    }

    pub fn between(min: f32, max: f32) -> Self {
        MediaRange { min: Some(min), max: Some(max), exact: None }
    }

    pub fn matches(&self, value: f32) -> bool {
        if let Some(exact) = self.exact {
            return (value - exact).abs() < 0.001;
        }
        let min_ok = self.min.map_or(true, |m| value >= m);
        let max_ok = self.max.map_or(true, |m| value <= m);
        min_ok && max_ok
    }
}

impl Default for MediaRange {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorGamut {
    #[default]
    Srgb,
    P3,
    Rec2020,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorSchemePreference {
    #[default]
    Light,
    Dark,
    NoPreference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReducedMotion {
    #[default]
    NoPreference,
    Reduce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContrastPreference {
    #[default]
    NoPreference,
    More,
    Less,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HoverCapability {
    #[default]
    None,
    Hover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PointerCapability {
    #[default]
    None,
    Coarse,
    Fine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayModeValue {
    #[default]
    Browser,
    Fullscreen,
    MinimalUi,
    Standalone,
    WindowControlsOverlay,
    Picture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScriptingValue {
    None,
    #[default]
    Enabled,
    InitialOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UpdateFrequency {
    None,
    Slow,
    #[default]
    Fast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverflowMediaValue {
    None,
    Scroll,
    #[default]
    Paged,
}

/// Media query context for evaluation
#[derive(Debug, Clone)]
pub struct MediaContext {
    pub width: f32,
    pub height: f32,
    pub device_pixel_ratio: f32,
    pub color_depth: u8,
    pub monochrome: bool,
    pub orientation: Orientation,
    pub prefers_color_scheme: ColorSchemePreference,
    pub prefers_reduced_motion: ReducedMotion,
    pub prefers_contrast: ContrastPreference,
    pub hover: HoverCapability,
    pub pointer: PointerCapability,
    pub scripting: ScriptingValue,
    pub forced_colors: bool,
}

impl Default for MediaContext {
    fn default() -> Self {
        MediaContext {
            width: 1920.0,
            height: 1080.0,
            device_pixel_ratio: 1.0,
            color_depth: 24,
            monochrome: false,
            orientation: Orientation::Landscape,
            prefers_color_scheme: ColorSchemePreference::Light,
            prefers_reduced_motion: ReducedMotion::NoPreference,
            prefers_contrast: ContrastPreference::NoPreference,
            hover: HoverCapability::Hover,
            pointer: PointerCapability::Fine,
            scripting: ScriptingValue::Enabled,
            forced_colors: false,
        }
    }
}


fn parse_media_feature(s: &str) -> Option<MediaFeature> {
    let s = s.trim();

    // Handle min-/max- prefixes
    if let Some(rest) = s.strip_prefix("min-width:") {
        let value = parse_length_f32(rest.trim())?;
        return Some(MediaFeature::Width(MediaRange::min(value)));
    }
    if let Some(rest) = s.strip_prefix("max-width:") {
        let value = parse_length_f32(rest.trim())?;
        return Some(MediaFeature::Width(MediaRange::max(value)));
    }
    if let Some(rest) = s.strip_prefix("width:") {
        let value = parse_length_f32(rest.trim())?;
        return Some(MediaFeature::Width(MediaRange::exact(value)));
    }
    if let Some(rest) = s.strip_prefix("min-height:") {
        let value = parse_length_f32(rest.trim())?;
        return Some(MediaFeature::Height(MediaRange::min(value)));
    }
    if let Some(rest) = s.strip_prefix("max-height:") {
        let value = parse_length_f32(rest.trim())?;
        return Some(MediaFeature::Height(MediaRange::max(value)));
    }
    if let Some(rest) = s.strip_prefix("height:") {
        let value = parse_length_f32(rest.trim())?;
        return Some(MediaFeature::Height(MediaRange::exact(value)));
    }

    // Boolean features
    if s == "prefers-reduced-motion: reduce" || s == "prefers-reduced-motion:reduce" {
        return Some(MediaFeature::PrefersReducedMotion(ReducedMotion::Reduce));
    }
    if s == "prefers-color-scheme: dark" || s == "prefers-color-scheme:dark" {
        return Some(MediaFeature::PrefersColorScheme(ColorSchemePreference::Dark));
    }
    if s == "prefers-color-scheme: light" || s == "prefers-color-scheme:light" {
        return Some(MediaFeature::PrefersColorScheme(ColorSchemePreference::Light));
    }
    if s == "prefers-contrast: more" || s == "prefers-contrast:more" {
        return Some(MediaFeature::PrefersContrast(ContrastPreference::More));
    }
    if s == "hover: hover" || s == "hover:hover" {
        return Some(MediaFeature::Hover(HoverCapability::Hover));
    }
    if s == "hover: none" || s == "hover:none" {
        return Some(MediaFeature::Hover(HoverCapability::None));
    }
    if s == "pointer: fine" || s == "pointer:fine" {
        return Some(MediaFeature::Pointer(PointerCapability::Fine));
    }
    if s == "pointer: coarse" || s == "pointer:coarse" {
        return Some(MediaFeature::Pointer(PointerCapability::Coarse));
    }
    if s == "orientation: portrait" || s == "orientation:portrait" {
        return Some(MediaFeature::Orientation(Orientation::Portrait));
    }
    if s == "orientation: landscape" || s == "orientation:landscape" {
        return Some(MediaFeature::Orientation(Orientation::Landscape));
    }

    None
}

fn parse_length_f32(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(px) = s.strip_suffix("px") {
        return px.trim().parse().ok();
    }
    if let Some(em) = s.strip_suffix("em") {
        return em.trim().parse::<f32>().ok().map(|v| v * 16.0);
    }
    if let Some(rem) = s.strip_suffix("rem") {
        return rem.trim().parse::<f32>().ok().map(|v| v * 16.0);
    }
    s.parse().ok()
}

fn evaluate_feature(feature: &MediaFeature, ctx: &MediaContext) -> bool {
    match feature {
        MediaFeature::Width(range) => range.matches(ctx.width),
        MediaFeature::Height(range) => range.matches(ctx.height),
        MediaFeature::Orientation(o) => ctx.orientation == *o,
        MediaFeature::PrefersColorScheme(p) => ctx.prefers_color_scheme == *p,
        MediaFeature::PrefersReducedMotion(r) => ctx.prefers_reduced_motion == *r,
        MediaFeature::PrefersContrast(c) => ctx.prefers_contrast == *c,
        MediaFeature::Hover(h) => ctx.hover == *h,
        MediaFeature::Pointer(p) => ctx.pointer == *p,
        MediaFeature::ForcedColors(f) => ctx.forced_colors == *f,
        MediaFeature::Scripting(s) => ctx.scripting == *s,
        MediaFeature::Color(range) => range.matches(ctx.color_depth as f32),
        MediaFeature::Monochrome(range) => {
            if ctx.monochrome {
                range.matches(1.0)
            } else {
                range.matches(0.0)
            }
        }
        _ => true, // Default to true for unhandled features
    }
}

// ============================================================================
// @supports Feature Queries - SupportedFeatures
// ============================================================================

/// Set of supported CSS features for @supports evaluation
#[derive(Debug, Clone)]
pub struct SupportedFeatures {
    pub properties: std::collections::HashSet<&'static str>,
    pub font_techs: std::collections::HashSet<&'static str>,
    pub font_formats: std::collections::HashSet<&'static str>,
}

impl Default for SupportedFeatures {
    fn default() -> Self {
        let mut properties = std::collections::HashSet::new();
        // Add all supported CSS properties
        for prop in &[
            "display", "position", "visibility", "width", "height", "margin", "padding",
            "flex", "flex-direction", "flex-wrap", "justify-content", "align-items",
            "grid", "grid-template-columns", "grid-template-rows", "gap",
            "color", "background", "background-color", "border", "border-radius",
            "font", "font-size", "font-weight", "font-style", "text-align",
            "transform", "transition", "animation", "opacity", "filter",
            "overflow", "z-index", "float", "clear", "clip-path",
            "container", "container-type", "aspect-ratio", "object-fit",
        ] {
            properties.insert(*prop);
        }

        let mut font_techs = std::collections::HashSet::new();
        font_techs.insert("color-COLRv0");
        font_techs.insert("color-COLRv1");
        font_techs.insert("variations");

        let mut font_formats = std::collections::HashSet::new();
        font_formats.insert("woff2");
        font_formats.insert("woff");
        font_formats.insert("truetype");
        font_formats.insert("opentype");

        SupportedFeatures { properties, font_techs, font_formats }
    }
}

// ============================================================================
// Container Query Context
// ============================================================================

/// Container context for evaluation
#[derive(Debug, Clone, Default)]
pub struct ContainerContext {
    pub name: Option<String>,
    pub width: f32,
    pub height: f32,
    pub container_type: ContainerType,
}

// ============================================================================
// @keyframes Full Parsing
// ============================================================================

/// Keyframe selector (from, to, or percentage)
#[derive(Debug, Clone)]
pub enum KeyframeSelector {
    From,
    To,
    Percentage(f32),
}

impl KeyframeSelector {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_lowercase();
        match s.as_str() {
            "from" => Some(KeyframeSelector::From),
            "to" => Some(KeyframeSelector::To),
            _ => {
                let pct = s.strip_suffix('%')?.trim().parse::<f32>().ok()?;
                Some(KeyframeSelector::Percentage(pct / 100.0))
            }
        }
    }

    pub fn to_offset(&self) -> f32 {
        match self {
            KeyframeSelector::From => 0.0,
            KeyframeSelector::To => 1.0,
            KeyframeSelector::Percentage(p) => *p,
        }
    }
}

/// Full keyframe with selector support
#[derive(Debug, Clone)]
pub struct FullKeyframe {
    pub selectors: Vec<KeyframeSelector>,
    pub properties: HashMap<String, String>,
}

/// Full @keyframes rule with parsing
#[derive(Debug, Clone)]
pub struct FullKeyframesRule {
    pub name: String,
    pub keyframes: Vec<FullKeyframe>,
}

impl FullKeyframesRule {
    pub fn parse(name: &str, body: &str) -> Option<Self> {
        let name = name.trim().to_string();
        let mut keyframes = Vec::new();

        // Simple parsing - split by } and parse each block
        for block in body.split('}') {
            let block = block.trim();
            if block.is_empty() {
                continue;
            }

            // Split selector from declarations
            if let Some((selector_part, decl_part)) = block.split_once('{') {
                let selectors: Vec<KeyframeSelector> = selector_part
                    .split(',')
                    .filter_map(|s| KeyframeSelector::parse(s.trim()))
                    .collect();

                if selectors.is_empty() {
                    continue;
                }

                let mut properties = HashMap::new();
                for decl in decl_part.split(';') {
                    if let Some((prop, val)) = decl.split_once(':') {
                        properties.insert(
                            prop.trim().to_string(),
                            val.trim().to_string(),
                        );
                    }
                }

                keyframes.push(FullKeyframe { selectors, properties });
            }
        }

        if keyframes.is_empty() {
            None
        } else {
            Some(FullKeyframesRule { name, keyframes })
        }
    }

    /// Get interpolated properties at a given progress (0.0 to 1.0)
    pub fn get_properties_at(&self, progress: f32) -> HashMap<String, String> {
        let progress = progress.clamp(0.0, 1.0);

        // Find surrounding keyframes
        let mut sorted_frames: Vec<(f32, &FullKeyframe)> = self.keyframes
            .iter()
            .flat_map(|kf| {
                kf.selectors.iter().map(move |s| (s.to_offset(), kf))
            })
            .collect();
        sorted_frames.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        // Find frames before and after progress
        let mut before: Option<(f32, &FullKeyframe)> = None;
        let mut after: Option<(f32, &FullKeyframe)> = None;

        for (offset, frame) in &sorted_frames {
            if *offset <= progress {
                before = Some((*offset, frame));
            }
            if *offset >= progress && after.is_none() {
                after = Some((*offset, frame));
            }
        }

        // Return properties from nearest frame
        if let Some((_, frame)) = before.or(after) {
            frame.properties.clone()
        } else {
            HashMap::new()
        }
    }
}

// ============================================================================
// CSS Cascade Improvements
// ============================================================================

/// Origin in the CSS cascade
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CascadeOrigin {
    UserAgent,
    User,
    Author,
}

/// A cascaded value with origin, importance, and specificity
#[derive(Debug, Clone)]
pub struct CascadedValue {
    pub value: String,
    pub origin: CascadeOrigin,
    pub important: bool,
    pub specificity: Specificity,
    pub layer_order: usize,
    pub source_order: usize,
}

impl CascadedValue {
    /// Compare cascade priority (higher priority wins)
    pub fn cascade_priority(&self) -> (bool, CascadeOrigin, usize, Specificity, usize) {
        // Important declarations have inverted origin order
        let effective_origin = if self.important {
            match self.origin {
                CascadeOrigin::UserAgent => CascadeOrigin::Author,
                CascadeOrigin::Author => CascadeOrigin::UserAgent,
                CascadeOrigin::User => CascadeOrigin::User,
            }
        } else {
            self.origin
        };

        (self.important, effective_origin, self.layer_order, self.specificity, self.source_order)
    }
}

/// Cascade for a single property
#[derive(Debug, Clone, Default)]
pub struct PropertyCascade {
    pub values: Vec<CascadedValue>,
}

impl PropertyCascade {
    pub fn add(&mut self, value: CascadedValue) {
        self.values.push(value);
    }

    /// Get the winning value from the cascade
    pub fn resolve(&self) -> Option<&str> {
        self.values
            .iter()
            .max_by(|a, b| a.cascade_priority().cmp(&b.cascade_priority()))
            .map(|v| v.value.as_str())
    }
}

/// Full cascade context
#[derive(Debug, Clone, Default)]
pub struct CascadeContext {
    pub properties: HashMap<String, PropertyCascade>,
    pub layer_order: Vec<String>, // Layer names in order
}

impl CascadeContext {
    pub fn add_declaration(
        &mut self,
        property: &str,
        value: &str,
        origin: CascadeOrigin,
        important: bool,
        specificity: Specificity,
        layer: Option<&str>,
        source_order: usize,
    ) {
        let layer_order = layer
            .and_then(|l| self.layer_order.iter().position(|n| n == l))
            .unwrap_or(self.layer_order.len());

        let cascade = self.properties
            .entry(property.to_string())
            .or_default();

        cascade.add(CascadedValue {
            value: value.to_string(),
            origin,
            important,
            specificity,
            layer_order,
            source_order,
        });
    }

    pub fn resolve_all(&self) -> HashMap<String, String> {
        self.properties
            .iter()
            .filter_map(|(prop, cascade)| {
                cascade.resolve().map(|v| (prop.clone(), v.to_string()))
            })
            .collect()
    }
}

// ============================================================================
// Complex Selector Matching
// ============================================================================

/// Full DOM tree context for selector matching
#[derive(Debug, Clone)]
pub struct DomContext {
    pub elements: Vec<ElementInfo>,
    pub parent_indices: Vec<Option<usize>>,
    pub sibling_indices: Vec<Vec<usize>>,
}

impl DomContext {
    pub fn new() -> Self {
        DomContext {
            elements: Vec::new(),
            parent_indices: Vec::new(),
            sibling_indices: Vec::new(),
        }
    }

    pub fn add_element(&mut self, info: ElementInfo, parent: Option<usize>) {
        let idx = self.elements.len();
        self.elements.push(info);
        self.parent_indices.push(parent);

        // Update sibling indices
        if let Some(parent_idx) = parent {
            while self.sibling_indices.len() <= parent_idx {
                self.sibling_indices.push(Vec::new());
            }
            self.sibling_indices[parent_idx].push(idx);
        }

        self.sibling_indices.push(Vec::new());
    }

    pub fn get_parent(&self, idx: usize) -> Option<&ElementInfo> {
        self.parent_indices.get(idx)?.as_ref().map(|&p| &self.elements[p])
    }

    pub fn get_ancestors(&self, idx: usize) -> Vec<&ElementInfo> {
        let mut ancestors = Vec::new();
        let mut current = self.parent_indices.get(idx).copied().flatten();
        while let Some(parent_idx) = current {
            ancestors.push(&self.elements[parent_idx]);
            current = self.parent_indices.get(parent_idx).copied().flatten();
        }
        ancestors
    }

    pub fn get_preceding_siblings(&self, idx: usize) -> Vec<&ElementInfo> {
        if let Some(Some(parent_idx)) = self.parent_indices.get(idx) {
            if let Some(siblings) = self.sibling_indices.get(*parent_idx) {
                return siblings
                    .iter()
                    .take_while(|&&i| i < idx)
                    .map(|&i| &self.elements[i])
                    .collect();
            }
        }
        Vec::new()
    }

    pub fn get_following_siblings(&self, idx: usize) -> Vec<&ElementInfo> {
        if let Some(Some(parent_idx)) = self.parent_indices.get(idx) {
            if let Some(siblings) = self.sibling_indices.get(*parent_idx) {
                return siblings
                    .iter()
                    .skip_while(|&&i| i <= idx)
                    .map(|&i| &self.elements[i])
                    .collect();
            }
        }
        Vec::new()
    }
}

impl Default for DomContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Full selector matcher with DOM traversal
pub struct SelectorMatcher<'a> {
    pub dom: &'a DomContext,
}

impl<'a> SelectorMatcher<'a> {
    pub fn new(dom: &'a DomContext) -> Self {
        SelectorMatcher { dom }
    }

    /// Match a selector against an element
    pub fn matches(&self, selector: &CssSelector, element_idx: usize) -> bool {
        if element_idx >= self.dom.elements.len() {
            return false;
        }

        self.match_parts(&selector.parts, element_idx, selector.parts.len())
    }

    fn match_parts(&self, parts: &[SelectorPart], element_idx: usize, end: usize) -> bool {
        if end == 0 {
            return true;
        }

        let element = &self.dom.elements[element_idx];
        let mut pos = end - 1;

        // Match the last simple selector
        loop {
            match &parts[pos] {
                SelectorPart::Type(name) => {
                    if element.tag_name.to_lowercase() != name.to_lowercase() {
                        return false;
                    }
                }
                SelectorPart::Class(class) => {
                    if !element.classes.iter().any(|c| c == class) {
                        return false;
                    }
                }
                SelectorPart::Id(id) => {
                    if element.id.as_ref() != Some(id) {
                        return false;
                    }
                }
                SelectorPart::Universal => {}
                SelectorPart::Attribute(attr) => {
                    if !matches_attribute(attr, element) {
                        return false;
                    }
                }
                SelectorPart::PseudoClass(pc) => {
                    if !self.matches_pseudo_class(pc, element_idx) {
                        return false;
                    }
                }
                SelectorPart::PseudoElement(_) => {
                    // Pseudo-elements always match at this stage
                }
                SelectorPart::Combinator(comb) => {
                    // Found a combinator - match the left side
                    return self.match_combinator(comb, parts, element_idx, pos);
                }
            }

            if pos == 0 {
                break;
            }
            pos -= 1;
        }

        true
    }

    fn match_combinator(
        &self,
        combinator: &Combinator,
        parts: &[SelectorPart],
        element_idx: usize,
        comb_pos: usize,
    ) -> bool {
        match combinator {
            Combinator::Descendant => {
                // Match any ancestor
                for &ancestor_idx in self.dom.parent_indices.get(element_idx)
                    .iter()
                    .filter_map(|&p| p.as_ref())
                {
                    if self.match_parts(parts, ancestor_idx, comb_pos) {
                        return true;
                    }
                    // Recurse to grandparents
                    if let Some(Some(grandparent)) = self.dom.parent_indices.get(ancestor_idx) {
                        if self.match_combinator(combinator, parts, *grandparent, comb_pos) {
                            return true;
                        }
                    }
                }
                false
            }
            Combinator::Child => {
                // Match direct parent only
                if let Some(Some(parent_idx)) = self.dom.parent_indices.get(element_idx) {
                    self.match_parts(parts, *parent_idx, comb_pos)
                } else {
                    false
                }
            }
            Combinator::NextSibling => {
                // Match immediately preceding sibling
                let preceding = self.dom.get_preceding_siblings(element_idx);
                if let Some(sibling) = preceding.last() {
                    // Find sibling index
                    for (i, el) in self.dom.elements.iter().enumerate() {
                        if std::ptr::eq(el, *sibling) {
                            return self.match_parts(parts, i, comb_pos);
                        }
                    }
                }
                false
            }
            Combinator::SubsequentSibling => {
                // Match any preceding sibling
                let preceding = self.dom.get_preceding_siblings(element_idx);
                for sibling in preceding {
                    for (i, el) in self.dom.elements.iter().enumerate() {
                        if std::ptr::eq(el, sibling) {
                            if self.match_parts(parts, i, comb_pos) {
                                return true;
                            }
                        }
                    }
                }
                false
            }
        }
    }

    fn matches_pseudo_class(&self, pc: &PseudoClass, element_idx: usize) -> bool {
        let element = &self.dom.elements[element_idx];

        match pc {
            PseudoClass::FirstChild => element.index == 1,
            PseudoClass::LastChild => element.index == element.sibling_count,
            PseudoClass::OnlyChild => element.sibling_count == 1,
            PseudoClass::NthChild(expr) => expr.matches(element.index),
            PseudoClass::NthLastChild(expr) => {
                let from_end = element.sibling_count - element.index + 1;
                expr.matches(from_end)
            }
            PseudoClass::FirstOfType => element.type_index == 1,
            PseudoClass::LastOfType => element.type_index == element.type_count,
            PseudoClass::OnlyOfType => element.type_count == 1,
            PseudoClass::NthOfType(expr) => expr.matches(element.type_index),
            PseudoClass::NthLastOfType(expr) => {
                let from_end = element.type_count - element.type_index + 1;
                expr.matches(from_end)
            }
            PseudoClass::Empty => element.is_empty,
            PseudoClass::Root => element.is_root,
            PseudoClass::Not(inner) => !self.matches(inner, element_idx),
            PseudoClass::Is(selectors) | PseudoClass::Where(selectors) => {
                selectors.iter().any(|s| self.matches(s, element_idx))
            }
            PseudoClass::Has(selectors) => {
                // :has() matches if any descendant matches the selectors
                self.has_matching_descendant(element_idx, selectors)
            }
            _ => matches_pseudo_class(pc, element),
        }
    }

    fn has_matching_descendant(&self, element_idx: usize, selectors: &[CssSelector]) -> bool {
        // Get all descendants
        if let Some(children) = self.dom.sibling_indices.get(element_idx) {
            for &child_idx in children {
                for selector in selectors {
                    if self.matches(selector, child_idx) {
                        return true;
                    }
                }
                // Recurse
                if self.has_matching_descendant(child_idx, selectors) {
                    return true;
                }
            }
        }
        false
    }
}

// ============================================================================
// Generated Content
// ============================================================================

/// CSS content property value
#[derive(Debug, Clone)]
pub enum ContentValue {
    Normal,
    None,
    String(String),
    Attr(String),
    Counter(String, ListStyleType),
    Counters(String, String, ListStyleType),
    OpenQuote,
    CloseQuote,
    NoOpenQuote,
    NoCloseQuote,
    Url(String),
    Image(String),
    LinearGradient(String),
    Multiple(Vec<ContentValue>),
}

impl ContentValue {
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();

        match value {
            "normal" => return Some(ContentValue::Normal),
            "none" => return Some(ContentValue::None),
            "open-quote" => return Some(ContentValue::OpenQuote),
            "close-quote" => return Some(ContentValue::CloseQuote),
            "no-open-quote" => return Some(ContentValue::NoOpenQuote),
            "no-close-quote" => return Some(ContentValue::NoCloseQuote),
            _ => {}
        }

        // String literal
        if (value.starts_with('"') && value.ends_with('"')) ||
           (value.starts_with('\'') && value.ends_with('\'')) {
            return Some(ContentValue::String(value[1..value.len()-1].to_string()));
        }

        // attr()
        if let Some(inner) = value.strip_prefix("attr(") {
            let inner = inner.strip_suffix(')')?;
            return Some(ContentValue::Attr(inner.trim().to_string()));
        }

        // counter()
        if let Some(inner) = value.strip_prefix("counter(") {
            let inner = inner.strip_suffix(')')?;
            let parts: Vec<&str> = inner.split(',').collect();
            let name = parts.first()?.trim().to_string();
            let style = parts.get(1)
                .map(|s| ListStyleType::from_str(s.trim()))
                .unwrap_or(ListStyleType::Decimal);
            return Some(ContentValue::Counter(name, style));
        }

        // counters()
        if let Some(inner) = value.strip_prefix("counters(") {
            let inner = inner.strip_suffix(')')?;
            let parts: Vec<&str> = inner.split(',').collect();
            let name = parts.first()?.trim().to_string();
            let separator = parts.get(1)?
                .trim()
                .trim_matches(|c| c == '"' || c == '\'')
                .to_string();
            let style = parts.get(2)
                .map(|s| ListStyleType::from_str(s.trim()))
                .unwrap_or(ListStyleType::Decimal);
            return Some(ContentValue::Counters(name, separator, style));
        }

        // url()
        if let Some(inner) = value.strip_prefix("url(") {
            let inner = inner.strip_suffix(')')?;
            let url = inner.trim().trim_matches(|c| c == '"' || c == '\'');
            return Some(ContentValue::Url(url.to_string()));
        }

        // Multiple values
        if value.contains(' ') {
            let mut values = Vec::new();
            let mut current = String::new();
            let mut in_string = false;
            let mut string_char = '"';
            let mut paren_depth = 0;

            for c in value.chars() {
                match c {
                    '"' | '\'' if !in_string => {
                        in_string = true;
                        string_char = c;
                        current.push(c);
                    }
                    c if c == string_char && in_string => {
                        in_string = false;
                        current.push(c);
                    }
                    '(' => {
                        paren_depth += 1;
                        current.push(c);
                    }
                    ')' => {
                        paren_depth -= 1;
                        current.push(c);
                    }
                    ' ' if !in_string && paren_depth == 0 => {
                        if !current.is_empty() {
                            if let Some(v) = ContentValue::parse(&current) {
                                values.push(v);
                            }
                            current.clear();
                        }
                    }
                    _ => current.push(c),
                }
            }

            if !current.is_empty() {
                if let Some(v) = ContentValue::parse(&current) {
                    values.push(v);
                }
            }

            if values.len() > 1 {
                return Some(ContentValue::Multiple(values));
            } else if values.len() == 1 {
                return Some(values.remove(0));
            }
        }

        None
    }

    pub fn generate(&self, counters: &CounterContext, quotes: &QuoteContext) -> String {
        match self {
            ContentValue::Normal | ContentValue::None => String::new(),
            ContentValue::String(s) => s.clone(),
            ContentValue::Attr(_attr) => String::new(), // Needs element context
            ContentValue::Counter(name, style) => {
                counters.get(name).map(|v| format_counter(v, *style)).unwrap_or_default()
            }
            ContentValue::Counters(name, sep, style) => {
                counters.get_all(name)
                    .iter()
                    .map(|v| format_counter(*v, *style))
                    .collect::<Vec<_>>()
                    .join(sep)
            }
            ContentValue::OpenQuote => quotes.open_quote(),
            ContentValue::CloseQuote => quotes.close_quote(),
            ContentValue::NoOpenQuote => {
                // Increments depth but no output
                String::new()
            }
            ContentValue::NoCloseQuote => {
                // Decrements depth but no output
                String::new()
            }
            ContentValue::Url(_) | ContentValue::Image(_) | ContentValue::LinearGradient(_) => {
                String::from("[image]")
            }
            ContentValue::Multiple(values) => {
                values.iter().map(|v| v.generate(counters, quotes)).collect()
            }
        }
    }
}

/// CSS counter context
#[derive(Debug, Clone, Default)]
pub struct CounterContext {
    counters: HashMap<String, Vec<i32>>,
}

impl CounterContext {
    pub fn new() -> Self {
        CounterContext { counters: HashMap::new() }
    }

    pub fn reset(&mut self, name: &str, value: i32) {
        self.counters.insert(name.to_string(), vec![value]);
    }

    pub fn increment(&mut self, name: &str, value: i32) {
        let counter = self.counters.entry(name.to_string()).or_insert_with(|| vec![0]);
        if let Some(last) = counter.last_mut() {
            *last += value;
        }
    }

    pub fn set(&mut self, name: &str, value: i32) {
        let counter = self.counters.entry(name.to_string()).or_insert_with(|| vec![0]);
        if let Some(last) = counter.last_mut() {
            *last = value;
        }
    }

    pub fn push(&mut self, name: &str, value: i32) {
        self.counters.entry(name.to_string()).or_default().push(value);
    }

    pub fn pop(&mut self, name: &str) {
        if let Some(stack) = self.counters.get_mut(name) {
            if stack.len() > 1 {
                stack.pop();
            }
        }
    }

    pub fn get(&self, name: &str) -> Option<i32> {
        self.counters.get(name).and_then(|v| v.last().copied())
    }

    pub fn get_all(&self, name: &str) -> Vec<i32> {
        self.counters.get(name).cloned().unwrap_or_default()
    }
}

/// Quote context for open-quote/close-quote
#[derive(Debug, Clone)]
pub struct QuoteContext {
    pub quotes: Vec<(String, String)>,
    pub depth: usize,
}

impl Default for QuoteContext {
    fn default() -> Self {
        QuoteContext {
            quotes: vec![
                ("\u{201C}".to_string(), "\u{201D}".to_string()), // " and "
                ("\u{2018}".to_string(), "\u{2019}".to_string()), // ' and '
            ],
            depth: 0,
        }
    }
}

impl QuoteContext {
    pub fn open_quote(&self) -> String {
        let idx = self.depth.min(self.quotes.len().saturating_sub(1));
        self.quotes.get(idx).map(|(o, _)| o.clone()).unwrap_or_default()
    }

    pub fn close_quote(&self) -> String {
        let idx = self.depth.saturating_sub(1).min(self.quotes.len().saturating_sub(1));
        self.quotes.get(idx).map(|(_, c)| c.clone()).unwrap_or_default()
    }
}

fn format_counter(value: i32, style: ListStyleType) -> String {
    match style {
        ListStyleType::Decimal => value.to_string(),
        ListStyleType::DecimalLeadingZero => format!("{:02}", value),
        ListStyleType::LowerAlpha => {
            if value >= 1 && value <= 26 {
                char::from_u32('a' as u32 + (value - 1) as u32)
                    .map(|c| c.to_string())
                    .unwrap_or_default()
            } else {
                value.to_string()
            }
        }
        ListStyleType::UpperAlpha => {
            if value >= 1 && value <= 26 {
                char::from_u32('A' as u32 + (value - 1) as u32)
                    .map(|c| c.to_string())
                    .unwrap_or_default()
            } else {
                value.to_string()
            }
        }
        ListStyleType::LowerRoman => to_roman_lower(value),
        ListStyleType::UpperRoman => to_roman_upper(value),
        _ => value.to_string(),
    }
}

fn to_roman_lower(n: i32) -> String {
    to_roman_upper(n).to_lowercase()
}

fn to_roman_upper(n: i32) -> String {
    if n <= 0 || n > 3999 {
        return n.to_string();
    }

    let mut result = String::new();
    let mut n = n as usize;

    let numerals = [
        (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"),
        (100, "C"), (90, "XC"), (50, "L"), (40, "XL"),
        (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I"),
    ];

    for (value, numeral) in numerals {
        while n >= value {
            result.push_str(numeral);
            n -= value;
        }
    }

    result
}

// ============================================================================
// CSS Shapes & Clipping
// ============================================================================

/// clip-path value
#[derive(Debug, Clone, PartialEq)]
pub enum ClipPathValue {
    None,
    Url(String),
    BasicShape(BasicShape),
    GeometryBox(GeometryBox),
    ShapeBox(BasicShape, GeometryBox),
}

impl Default for ClipPathValue {
    fn default() -> Self {
        ClipPathValue::None
    }
}

/// Basic shape for clip-path and shape-outside
#[derive(Debug, Clone, PartialEq)]
pub enum BasicShape {
    Inset(InsetShape),
    Circle(CircleShape),
    Ellipse(EllipseShape),
    Polygon(PolygonShape),
    Path(PathShape),
}

#[derive(Debug, Clone, PartialEq)]
pub struct InsetShape {
    pub top: LengthPercentage,
    pub right: LengthPercentage,
    pub bottom: LengthPercentage,
    pub left: LengthPercentage,
    pub border_radius: Option<BorderRadiusValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CircleShape {
    pub radius: ShapeRadius,
    pub position: ShapePosition,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EllipseShape {
    pub rx: ShapeRadius,
    pub ry: ShapeRadius,
    pub position: ShapePosition,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolygonShape {
    pub fill_rule: FillRule,
    pub points: Vec<(LengthPercentage, LengthPercentage)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathShape {
    pub fill_rule: FillRule,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShapeRadius {
    Length(LengthPercentage),
    ClosestSide,
    FarthestSide,
}

impl Default for ShapeRadius {
    fn default() -> Self {
        ShapeRadius::ClosestSide
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShapePosition {
    pub x: LengthPercentage,
    pub y: LengthPercentage,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LengthPercentage {
    Length(f32),
    Percentage(f32),
}

impl Default for LengthPercentage {
    fn default() -> Self {
        LengthPercentage::Percentage(50.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BorderRadiusValue {
    pub top_left: (LengthPercentage, LengthPercentage),
    pub top_right: (LengthPercentage, LengthPercentage),
    pub bottom_right: (LengthPercentage, LengthPercentage),
    pub bottom_left: (LengthPercentage, LengthPercentage),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GeometryBox {
    #[default]
    BorderBox,
    PaddingBox,
    ContentBox,
    MarginBox,
    FillBox,
    StrokeBox,
    ViewBox,
}

/// shape-outside value
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeOutside {
    None,
    MarginBox,
    BorderBox,
    PaddingBox,
    ContentBox,
    BasicShape(BasicShape, Option<GeometryBox>),
    Image(String),
}

impl Default for ShapeOutside {
    fn default() -> Self {
        ShapeOutside::None
    }
}

impl ClipPathValue {
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();

        if value == "none" {
            return Some(ClipPathValue::None);
        }

        // url()
        if let Some(inner) = value.strip_prefix("url(") {
            let inner = inner.strip_suffix(')')?.trim().trim_matches(|c| c == '"' || c == '\'');
            return Some(ClipPathValue::Url(inner.to_string()));
        }

        // Basic shapes
        if let Some(shape) = parse_basic_shape(value) {
            return Some(ClipPathValue::BasicShape(shape));
        }

        // Geometry box
        if let Some(box_type) = parse_geometry_box(value) {
            return Some(ClipPathValue::GeometryBox(box_type));
        }

        None
    }
}

fn parse_basic_shape(value: &str) -> Option<BasicShape> {
    let value = value.trim();

    // circle()
    if let Some(inner) = value.strip_prefix("circle(") {
        let inner = inner.strip_suffix(')')?;
        return Some(BasicShape::Circle(parse_circle(inner)?));
    }

    // ellipse()
    if let Some(inner) = value.strip_prefix("ellipse(") {
        let inner = inner.strip_suffix(')')?;
        return Some(BasicShape::Ellipse(parse_ellipse(inner)?));
    }

    // inset()
    if let Some(inner) = value.strip_prefix("inset(") {
        let inner = inner.strip_suffix(')')?;
        return Some(BasicShape::Inset(parse_inset(inner)?));
    }

    // polygon()
    if let Some(inner) = value.strip_prefix("polygon(") {
        let inner = inner.strip_suffix(')')?;
        return Some(BasicShape::Polygon(parse_polygon(inner)?));
    }

    // path()
    if let Some(inner) = value.strip_prefix("path(") {
        let inner = inner.strip_suffix(')')?;
        return Some(BasicShape::Path(PathShape {
            fill_rule: FillRule::NonZero,
            path: inner.trim().trim_matches('"').to_string(),
        }));
    }

    None
}

fn parse_circle(s: &str) -> Option<CircleShape> {
    // Simple parsing: radius at x y
    let parts: Vec<&str> = s.split(" at ").collect();

    let radius = if let Some(r) = parts.first() {
        parse_shape_radius(r.trim())
    } else {
        ShapeRadius::ClosestSide
    };

    let position = if let Some(pos) = parts.get(1) {
        parse_shape_position(pos.trim())
    } else {
        ShapePosition::default()
    };

    Some(CircleShape { radius, position })
}

fn parse_ellipse(s: &str) -> Option<EllipseShape> {
    let parts: Vec<&str> = s.split(" at ").collect();

    let (rx, ry) = if let Some(radii) = parts.first() {
        let r: Vec<&str> = radii.split_whitespace().collect();
        let rx = r.first().map(|r| parse_shape_radius(r)).unwrap_or(ShapeRadius::ClosestSide);
        let ry = r.get(1).map(|r| parse_shape_radius(r)).unwrap_or(ShapeRadius::ClosestSide);
        (rx, ry)
    } else {
        (ShapeRadius::ClosestSide, ShapeRadius::ClosestSide)
    };

    let position = if let Some(pos) = parts.get(1) {
        parse_shape_position(pos.trim())
    } else {
        ShapePosition::default()
    };

    Some(EllipseShape { rx, ry, position })
}

fn parse_inset(s: &str) -> Option<InsetShape> {
    let parts: Vec<&str> = s.split(" round ").collect();

    let insets: Vec<LengthPercentage> = parts.first()?
        .split_whitespace()
        .filter_map(|p| parse_length_percentage(p))
        .collect();

    let (top, right, bottom, left) = match insets.len() {
        1 => (insets[0].clone(), insets[0].clone(), insets[0].clone(), insets[0].clone()),
        2 => (insets[0].clone(), insets[1].clone(), insets[0].clone(), insets[1].clone()),
        3 => (insets[0].clone(), insets[1].clone(), insets[2].clone(), insets[1].clone()),
        4 => (insets[0].clone(), insets[1].clone(), insets[2].clone(), insets[3].clone()),
        _ => return None,
    };

    Some(InsetShape {
        top, right, bottom, left,
        border_radius: None,
    })
}

fn parse_polygon(s: &str) -> Option<PolygonShape> {
    let s = s.trim();
    let (fill_rule, points_str) = if s.starts_with("evenodd") || s.starts_with("nonzero") {
        let (rule, rest) = s.split_once(',')?;
        let fill_rule = if rule.trim() == "evenodd" { FillRule::EvenOdd } else { FillRule::NonZero };
        (fill_rule, rest)
    } else {
        (FillRule::NonZero, s)
    };

    let points: Vec<(LengthPercentage, LengthPercentage)> = points_str
        .split(',')
        .filter_map(|p| {
            let coords: Vec<&str> = p.trim().split_whitespace().collect();
            if coords.len() >= 2 {
                Some((
                    parse_length_percentage(coords[0])?,
                    parse_length_percentage(coords[1])?,
                ))
            } else {
                None
            }
        })
        .collect();

    Some(PolygonShape { fill_rule, points })
}

fn parse_shape_radius(s: &str) -> ShapeRadius {
    match s.trim() {
        "closest-side" => ShapeRadius::ClosestSide,
        "farthest-side" => ShapeRadius::FarthestSide,
        _ => parse_length_percentage(s)
            .map(ShapeRadius::Length)
            .unwrap_or(ShapeRadius::ClosestSide),
    }
}

fn parse_shape_position(s: &str) -> ShapePosition {
    let parts: Vec<&str> = s.split_whitespace().collect();

    let x = parts.first()
        .and_then(|p| parse_position_keyword_or_length(p))
        .unwrap_or(LengthPercentage::Percentage(50.0));

    let y = parts.get(1)
        .and_then(|p| parse_position_keyword_or_length(p))
        .unwrap_or(LengthPercentage::Percentage(50.0));

    ShapePosition { x, y }
}

fn parse_position_keyword_or_length(s: &str) -> Option<LengthPercentage> {
    match s.trim() {
        "left" => Some(LengthPercentage::Percentage(0.0)),
        "center" => Some(LengthPercentage::Percentage(50.0)),
        "right" => Some(LengthPercentage::Percentage(100.0)),
        "top" => Some(LengthPercentage::Percentage(0.0)),
        "bottom" => Some(LengthPercentage::Percentage(100.0)),
        _ => parse_length_percentage(s),
    }
}

fn parse_length_percentage(s: &str) -> Option<LengthPercentage> {
    let s = s.trim();
    if let Some(pct) = s.strip_suffix('%') {
        pct.trim().parse().ok().map(LengthPercentage::Percentage)
    } else if let Some(px) = s.strip_suffix("px") {
        px.trim().parse().ok().map(LengthPercentage::Length)
    } else {
        s.parse().ok().map(LengthPercentage::Length)
    }
}

fn parse_geometry_box(s: &str) -> Option<GeometryBox> {
    match s.trim() {
        "border-box" => Some(GeometryBox::BorderBox),
        "padding-box" => Some(GeometryBox::PaddingBox),
        "content-box" => Some(GeometryBox::ContentBox),
        "margin-box" => Some(GeometryBox::MarginBox),
        "fill-box" => Some(GeometryBox::FillBox),
        "stroke-box" => Some(GeometryBox::StrokeBox),
        "view-box" => Some(GeometryBox::ViewBox),
        _ => None,
    }
}

// ============================================================================
// Motion Path
// ============================================================================

/// offset-path value
#[derive(Debug, Clone, PartialEq)]
pub enum OffsetPath {
    None,
    Url(String),
    Ray(RayPath),
    BasicShape(BasicShape),
    CoordBox(GeometryBox),
    Path(String),
}

impl Default for OffsetPath {
    fn default() -> Self {
        OffsetPath::None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RayPath {
    pub angle: f32,
    pub size: RaySize,
    pub contain: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RaySize {
    #[default]
    ClosestSide,
    ClosestCorner,
    FarthestSide,
    FarthestCorner,
    Sides,
}

/// offset-anchor value
#[derive(Debug, Clone, PartialEq)]
pub enum OffsetAnchor {
    Auto,
    Position(ShapePosition),
}

impl Default for OffsetAnchor {
    fn default() -> Self {
        OffsetAnchor::Auto
    }
}

/// offset-rotate value
#[derive(Debug, Clone, PartialEq)]
pub enum OffsetRotate {
    Auto,
    AutoReverse,
    Angle(f32),
    AutoAngle(f32),
}

impl Default for OffsetRotate {
    fn default() -> Self {
        OffsetRotate::Auto
    }
}

/// Full motion path properties
#[derive(Debug, Clone, Default)]
pub struct MotionPath {
    pub path: OffsetPath,
    pub distance: LengthPercentage,
    pub anchor: OffsetAnchor,
    pub rotate: OffsetRotate,
}

impl OffsetPath {
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();

        if value == "none" {
            return Some(OffsetPath::None);
        }

        // url()
        if let Some(inner) = value.strip_prefix("url(") {
            let inner = inner.strip_suffix(')')?.trim().trim_matches(|c| c == '"' || c == '\'');
            return Some(OffsetPath::Url(inner.to_string()));
        }

        // path()
        if let Some(inner) = value.strip_prefix("path(") {
            let inner = inner.strip_suffix(')')?.trim().trim_matches('"');
            return Some(OffsetPath::Path(inner.to_string()));
        }

        // ray()
        if let Some(inner) = value.strip_prefix("ray(") {
            let inner = inner.strip_suffix(')')?;
            return Some(OffsetPath::Ray(parse_ray(inner)?));
        }

        // Basic shapes
        if let Some(shape) = parse_basic_shape(value) {
            return Some(OffsetPath::BasicShape(shape));
        }

        // Coord box
        if let Some(box_type) = parse_geometry_box(value) {
            return Some(OffsetPath::CoordBox(box_type));
        }

        None
    }
}

fn parse_ray(s: &str) -> Option<RayPath> {
    let parts: Vec<&str> = s.split_whitespace().collect();

    let mut angle = 0.0;
    let mut size = RaySize::ClosestSide;
    let mut contain = false;

    for part in parts {
        if part.ends_with("deg") {
            if let Ok(a) = part.strip_suffix("deg").unwrap().parse() {
                angle = a;
            }
        } else if part.ends_with("rad") {
            if let Ok(a) = part.strip_suffix("rad").unwrap().parse::<f32>() {
                angle = a.to_degrees();
            }
        } else {
            match part {
                "closest-side" => size = RaySize::ClosestSide,
                "closest-corner" => size = RaySize::ClosestCorner,
                "farthest-side" => size = RaySize::FarthestSide,
                "farthest-corner" => size = RaySize::FarthestCorner,
                "sides" => size = RaySize::Sides,
                "contain" => contain = true,
                _ => {}
            }
        }
    }

    Some(RayPath { angle, size, contain })
}

// ============================================================================
// Style Caching
// ============================================================================

/// Cache for computed styles
#[derive(Debug, Default)]
pub struct StyleCache {
    /// Cache of selector -> specificity for quick lookup
    pub selector_cache: HashMap<String, Specificity>,
    /// Cache of computed styles by element signature
    pub computed_cache: HashMap<ElementSignature, ComputedStyle>,
    /// Hit/miss statistics
    pub hits: usize,
    pub misses: usize,
}

/// Signature for an element (for cache lookup)
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ElementSignature {
    pub tag: String,
    pub classes: Vec<String>,
    pub id: Option<String>,
    pub parent_tag: Option<String>,
    pub inline_style_hash: u64,
}

impl StyleCache {
    pub fn new() -> Self {
        StyleCache::default()
    }

    pub fn get_selector_specificity(&mut self, selector: &str) -> Specificity {
        if let Some(&spec) = self.selector_cache.get(selector) {
            self.hits += 1;
            return spec;
        }

        self.misses += 1;
        let spec = CssSelector::parse(selector)
            .map(|s| s.specificity)
            .unwrap_or_default();
        self.selector_cache.insert(selector.to_string(), spec);
        spec
    }

    pub fn get_computed(&mut self, sig: &ElementSignature) -> Option<&ComputedStyle> {
        if self.computed_cache.contains_key(sig) {
            self.hits += 1;
            self.computed_cache.get(sig)
        } else {
            self.misses += 1;
            None
        }
    }

    pub fn insert_computed(&mut self, sig: ElementSignature, style: ComputedStyle) {
        self.computed_cache.insert(sig, style);
    }

    pub fn clear(&mut self) {
        self.selector_cache.clear();
        self.computed_cache.clear();
    }

    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// Inherited style values for optimization
#[derive(Debug, Clone)]
pub struct InheritedStyle {
    pub color: Option<String>,
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub line_height: Option<f32>,
    pub text_align: TextAlign,
    pub visibility: Visibility,
    pub direction: Direction,
    pub writing_mode: WritingMode,
    pub quotes: Option<Vec<(String, String)>>,
    pub cursor: Cursor,
}

impl Default for InheritedStyle {
    fn default() -> Self {
        InheritedStyle {
            color: None,
            font_family: None,
            font_size: None,
            font_weight: FontWeight::Normal,
            font_style: FontStyle::Normal,
            line_height: None,
            text_align: TextAlign::Left,
            visibility: Visibility::Visible,
            direction: Direction::Ltr,
            writing_mode: WritingMode::HorizontalTb,
            quotes: None,
            cursor: Cursor::Auto,
        }
    }
}

impl InheritedStyle {
    pub fn from_computed(style: &ComputedStyle) -> Self {
        InheritedStyle {
            color: None, // Would need color field
            font_family: None,
            font_size: None,
            font_weight: style.font_weight,
            font_style: style.font_style,
            line_height: style.line_height,
            text_align: style.text_align,
            visibility: style.visibility,
            direction: style.direction,
            writing_mode: style.writing_mode,
            quotes: style.quotes.clone(),
            cursor: style.cursor,
        }
    }

    pub fn apply_to(&self, style: &mut ComputedStyle) {
        style.font_weight = self.font_weight;
        style.font_style = self.font_style;
        style.line_height = self.line_height;
        style.text_align = self.text_align;
        style.visibility = self.visibility;
        style.direction = self.direction;
        style.writing_mode = self.writing_mode;
        style.quotes = self.quotes.clone();
        style.cursor = self.cursor;
    }
}

// ============================================================================
// CSS Masking
// ============================================================================

/// CSS mask-image value
#[derive(Debug, Clone, PartialEq)]
pub enum MaskImage {
    None,
    Url(String),
    /// Linear gradient as raw CSS string
    LinearGradient(String),
    /// Radial gradient as raw CSS string
    RadialGradient(String),
    /// Conic gradient as raw CSS string
    ConicGradient(String),
    /// element() reference
    Element(String),
    /// Multiple mask layers
    Multiple(Vec<MaskImage>),
}

impl MaskImage {
    pub fn from_str(s: &str) -> Self {
        let s = s.trim();
        if s == "none" {
            return MaskImage::None;
        }
        if let Some(url) = s.strip_prefix("url(").and_then(|s| s.strip_suffix(')')) {
            let url = url.trim().trim_matches(|c| c == '"' || c == '\'');
            return MaskImage::Url(url.to_string());
        }
        if s.starts_with("linear-gradient(") {
            return MaskImage::LinearGradient(s.to_string());
        }
        if s.starts_with("radial-gradient(") {
            return MaskImage::RadialGradient(s.to_string());
        }
        if s.starts_with("conic-gradient(") {
            return MaskImage::ConicGradient(s.to_string());
        }
        if let Some(elem) = s.strip_prefix("element(").and_then(|s| s.strip_suffix(')')) {
            return MaskImage::Element(elem.trim().to_string());
        }
        // Handle multiple comma-separated values (but be careful with gradients containing commas)
        if s.contains(',') && !s.contains("gradient(") {
            let images: Vec<MaskImage> = s.split(',')
                .map(|part| MaskImage::from_str(part.trim()))
                .collect();
            if images.len() > 1 {
                return MaskImage::Multiple(images);
            }
        }
        MaskImage::None
    }

    pub fn is_none(&self) -> bool {
        matches!(self, MaskImage::None)
    }
}

/// CSS mask-mode value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskMode {
    /// Use alpha channel
    Alpha,
    /// Use luminance
    Luminance,
    /// Match source type (alpha for images, luminance for masks)
    MatchSource,
}

impl MaskMode {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "alpha" => MaskMode::Alpha,
            "luminance" => MaskMode::Luminance,
            "match-source" | "matchsource" => MaskMode::MatchSource,
            _ => MaskMode::MatchSource,
        }
    }
}

/// CSS mask-repeat value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskRepeat {
    Repeat,
    RepeatX,
    RepeatY,
    Space,
    Round,
    NoRepeat,
}

impl MaskRepeat {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "repeat" => MaskRepeat::Repeat,
            "repeat-x" => MaskRepeat::RepeatX,
            "repeat-y" => MaskRepeat::RepeatY,
            "space" => MaskRepeat::Space,
            "round" => MaskRepeat::Round,
            "no-repeat" => MaskRepeat::NoRepeat,
            _ => MaskRepeat::Repeat,
        }
    }
}

/// CSS mask-position value
#[derive(Debug, Clone, PartialEq)]
pub struct MaskPosition {
    pub x: PositionValue,
    pub y: PositionValue,
}

impl Default for MaskPosition {
    fn default() -> Self {
        MaskPosition {
            x: PositionValue::Percentage(0.0),
            y: PositionValue::Percentage(0.0),
        }
    }
}

impl MaskPosition {
    pub fn from_str(s: &str) -> Self {
        let s = s.trim().to_lowercase();
        let parts: Vec<&str> = s.split_whitespace().collect();

        match parts.as_slice() {
            ["center"] => MaskPosition {
                x: PositionValue::Percentage(50.0),
                y: PositionValue::Percentage(50.0),
            },
            ["top"] => MaskPosition {
                x: PositionValue::Percentage(50.0),
                y: PositionValue::Percentage(0.0),
            },
            ["bottom"] => MaskPosition {
                x: PositionValue::Percentage(50.0),
                y: PositionValue::Percentage(100.0),
            },
            ["left"] => MaskPosition {
                x: PositionValue::Percentage(0.0),
                y: PositionValue::Percentage(50.0),
            },
            ["right"] => MaskPosition {
                x: PositionValue::Percentage(100.0),
                y: PositionValue::Percentage(50.0),
            },
            [x, y] => MaskPosition {
                x: PositionValue::parse(x),
                y: PositionValue::parse(y),
            },
            _ => MaskPosition::default(),
        }
    }
}

/// Position value for mask-position
#[derive(Debug, Clone, PartialEq)]
pub enum PositionValue {
    Length(f32),
    Percentage(f32),
}

impl PositionValue {
    pub fn parse(s: &str) -> Self {
        let s = s.trim();
        if let Some(pct) = s.strip_suffix('%') {
            if let Ok(v) = pct.trim().parse::<f32>() {
                return PositionValue::Percentage(v);
            }
        }
        if let Some(px) = s.strip_suffix("px") {
            if let Ok(v) = px.trim().parse::<f32>() {
                return PositionValue::Length(v);
            }
        }
        match s {
            "left" | "top" => PositionValue::Percentage(0.0),
            "center" => PositionValue::Percentage(50.0),
            "right" | "bottom" => PositionValue::Percentage(100.0),
            _ => PositionValue::Percentage(0.0),
        }
    }
}

/// CSS mask-size value
#[derive(Debug, Clone, PartialEq)]
pub enum MaskSize {
    Auto,
    Cover,
    Contain,
    Length(f32, Option<f32>),
    Percentage(f32, Option<f32>),
}

impl MaskSize {
    pub fn from_str(s: &str) -> Self {
        let s = s.trim().to_lowercase();
        match s.as_str() {
            "auto" => MaskSize::Auto,
            "cover" => MaskSize::Cover,
            "contain" => MaskSize::Contain,
            _ => {
                let parts: Vec<&str> = s.split_whitespace().collect();
                if let Some(first) = parts.first() {
                    if let Some(pct) = first.strip_suffix('%') {
                        if let Ok(v) = pct.parse::<f32>() {
                            let second = parts.get(1).and_then(|s| {
                                s.strip_suffix('%').and_then(|p| p.parse().ok())
                            });
                            return MaskSize::Percentage(v, second);
                        }
                    }
                    if let Some(px) = first.strip_suffix("px") {
                        if let Ok(v) = px.parse::<f32>() {
                            let second = parts.get(1).and_then(|s| {
                                s.strip_suffix("px").and_then(|p| p.parse().ok())
                            });
                            return MaskSize::Length(v, second);
                        }
                    }
                }
                MaskSize::Auto
            }
        }
    }
}

/// CSS mask-composite value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskComposite {
    Add,
    Subtract,
    Intersect,
    Exclude,
}

impl MaskComposite {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "add" => MaskComposite::Add,
            "subtract" => MaskComposite::Subtract,
            "intersect" => MaskComposite::Intersect,
            "exclude" => MaskComposite::Exclude,
            _ => MaskComposite::Add,
        }
    }
}

/// CSS mask-clip value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskClip {
    BorderBox,
    PaddingBox,
    ContentBox,
    FillBox,
    StrokeBox,
    ViewBox,
    NoClip,
}

impl MaskClip {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "border-box" => MaskClip::BorderBox,
            "padding-box" => MaskClip::PaddingBox,
            "content-box" => MaskClip::ContentBox,
            "fill-box" => MaskClip::FillBox,
            "stroke-box" => MaskClip::StrokeBox,
            "view-box" => MaskClip::ViewBox,
            "no-clip" => MaskClip::NoClip,
            _ => MaskClip::BorderBox,
        }
    }
}

/// CSS mask-origin value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskOrigin {
    BorderBox,
    PaddingBox,
    ContentBox,
    FillBox,
    StrokeBox,
    ViewBox,
}

impl MaskOrigin {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "border-box" => MaskOrigin::BorderBox,
            "padding-box" => MaskOrigin::PaddingBox,
            "content-box" => MaskOrigin::ContentBox,
            "fill-box" => MaskOrigin::FillBox,
            "stroke-box" => MaskOrigin::StrokeBox,
            "view-box" => MaskOrigin::ViewBox,
            _ => MaskOrigin::BorderBox,
        }
    }
}

/// Shorthand mask property parser
pub struct MaskShorthand {
    pub image: MaskImage,
    pub mode: MaskMode,
    pub repeat: MaskRepeat,
    pub position: MaskPosition,
    pub size: MaskSize,
    pub composite: MaskComposite,
    pub clip: MaskClip,
    pub origin: MaskOrigin,
}

impl Default for MaskShorthand {
    fn default() -> Self {
        MaskShorthand {
            image: MaskImage::None,
            mode: MaskMode::MatchSource,
            repeat: MaskRepeat::Repeat,
            position: MaskPosition::default(),
            size: MaskSize::Auto,
            composite: MaskComposite::Add,
            clip: MaskClip::BorderBox,
            origin: MaskOrigin::BorderBox,
        }
    }
}

impl MaskShorthand {
    pub fn parse(value: &str) -> Self {
        let mut mask = MaskShorthand::default();
        let value = value.trim();

        // Simple parsing - look for url() or gradient first
        if value.starts_with("url(") || value.contains("gradient(") {
            mask.image = MaskImage::from_str(value);
        }

        // Parse other keywords
        for part in value.split_whitespace() {
            match part.to_lowercase().as_str() {
                "no-repeat" => mask.repeat = MaskRepeat::NoRepeat,
                "repeat" => mask.repeat = MaskRepeat::Repeat,
                "repeat-x" => mask.repeat = MaskRepeat::RepeatX,
                "repeat-y" => mask.repeat = MaskRepeat::RepeatY,
                "space" => mask.repeat = MaskRepeat::Space,
                "round" => mask.repeat = MaskRepeat::Round,
                "alpha" => mask.mode = MaskMode::Alpha,
                "luminance" => mask.mode = MaskMode::Luminance,
                "add" => mask.composite = MaskComposite::Add,
                "subtract" => mask.composite = MaskComposite::Subtract,
                "intersect" => mask.composite = MaskComposite::Intersect,
                "exclude" => mask.composite = MaskComposite::Exclude,
                "cover" => mask.size = MaskSize::Cover,
                "contain" => mask.size = MaskSize::Contain,
                "border-box" => {
                    mask.clip = MaskClip::BorderBox;
                    mask.origin = MaskOrigin::BorderBox;
                }
                "padding-box" => {
                    mask.clip = MaskClip::PaddingBox;
                    mask.origin = MaskOrigin::PaddingBox;
                }
                "content-box" => {
                    mask.clip = MaskClip::ContentBox;
                    mask.origin = MaskOrigin::ContentBox;
                }
                _ => {}
            }
        }

        mask
    }

    pub fn apply_to(&self, style: &mut ComputedStyle) {
        style.mask_image = self.image.clone();
        style.mask_mode = self.mode;
        style.mask_repeat = self.repeat;
        style.mask_position = self.position.clone();
        style.mask_size = self.size.clone();
        style.mask_composite = self.composite;
        style.mask_clip = self.clip;
        style.mask_origin = self.origin;
    }
}

// ============================================================================
// Backdrop Filter
// ============================================================================

/// CSS backdrop-filter value
#[derive(Debug, Clone, PartialEq)]
pub enum BackdropFilter {
    None,
    /// Single filter function
    Single(FilterFunction),
    /// Multiple filter functions
    Multiple(Vec<FilterFunction>),
}

impl BackdropFilter {
    pub fn from_str(s: &str) -> Self {
        let s = s.trim();
        if s == "none" {
            return BackdropFilter::None;
        }

        let mut functions = Vec::new();
        let mut remaining = s;

        while !remaining.is_empty() {
            remaining = remaining.trim_start();

            if let Some(func) = FilterFunction::parse_next(&mut remaining) {
                functions.push(func);
            } else {
                break;
            }
        }

        match functions.len() {
            0 => BackdropFilter::None,
            1 => BackdropFilter::Single(functions.remove(0)),
            _ => BackdropFilter::Multiple(functions),
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, BackdropFilter::None)
    }
}

/// Individual filter function
#[derive(Debug, Clone, PartialEq)]
pub enum FilterFunction {
    Blur(f32),
    Brightness(f32),
    Contrast(f32),
    Grayscale(f32),
    HueRotate(f32),
    Invert(f32),
    Opacity(f32),
    Saturate(f32),
    Sepia(f32),
    DropShadow {
        x: f32,
        y: f32,
        blur: f32,
        color: Option<String>,
    },
}

impl FilterFunction {
    pub fn parse_next(s: &mut &str) -> Option<Self> {
        let input = s.trim_start();

        // Find function name and opening paren
        let paren_pos = input.find('(')?;
        let name = input[..paren_pos].trim();

        // Find closing paren
        let close_pos = input.find(')')?;
        let args = input[paren_pos + 1..close_pos].trim();

        // Advance the input past this function
        *s = &input[close_pos + 1..];

        match name.to_lowercase().as_str() {
            "blur" => {
                let value = parse_filter_length(args)?;
                Some(FilterFunction::Blur(value))
            }
            "brightness" => {
                let value = parse_filter_percentage(args);
                Some(FilterFunction::Brightness(value))
            }
            "contrast" => {
                let value = parse_filter_percentage(args);
                Some(FilterFunction::Contrast(value))
            }
            "grayscale" => {
                let value = parse_filter_percentage(args);
                Some(FilterFunction::Grayscale(value))
            }
            "hue-rotate" => {
                let value = parse_angle(args);
                Some(FilterFunction::HueRotate(value))
            }
            "invert" => {
                let value = parse_filter_percentage(args);
                Some(FilterFunction::Invert(value))
            }
            "opacity" => {
                let value = parse_filter_percentage(args);
                Some(FilterFunction::Opacity(value))
            }
            "saturate" => {
                let value = parse_filter_percentage(args);
                Some(FilterFunction::Saturate(value))
            }
            "sepia" => {
                let value = parse_filter_percentage(args);
                Some(FilterFunction::Sepia(value))
            }
            "drop-shadow" => {
                let parts: Vec<&str> = args.split_whitespace().collect();
                let x = parts.first().and_then(|p| parse_filter_length(p)).unwrap_or(0.0);
                let y = parts.get(1).and_then(|p| parse_filter_length(p)).unwrap_or(0.0);
                let blur = parts.get(2).and_then(|p| parse_filter_length(p)).unwrap_or(0.0);
                let color = parts.get(3).map(|s| s.to_string());
                Some(FilterFunction::DropShadow { x, y, blur, color })
            }
            _ => None,
        }
    }
}

fn parse_filter_length(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(px) = s.strip_suffix("px") {
        return px.trim().parse().ok();
    }
    if let Some(em) = s.strip_suffix("em") {
        return em.trim().parse::<f32>().ok().map(|v| v * 16.0);
    }
    if let Some(rem) = s.strip_suffix("rem") {
        return rem.trim().parse::<f32>().ok().map(|v| v * 16.0);
    }
    if s == "0" {
        return Some(0.0);
    }
    None
}

fn parse_filter_percentage(s: &str) -> f32 {
    let s = s.trim();
    if let Some(pct) = s.strip_suffix('%') {
        return pct.trim().parse::<f32>().unwrap_or(100.0) / 100.0;
    }
    s.parse().unwrap_or(1.0)
}

fn parse_angle(s: &str) -> f32 {
    let s = s.trim();
    if let Some(deg) = s.strip_suffix("deg") {
        return deg.trim().parse().unwrap_or(0.0);
    }
    if let Some(rad) = s.strip_suffix("rad") {
        return rad.trim().parse::<f32>().unwrap_or(0.0) * 180.0 / std::f32::consts::PI;
    }
    if let Some(turn) = s.strip_suffix("turn") {
        return turn.trim().parse::<f32>().unwrap_or(0.0) * 360.0;
    }
    if let Some(grad) = s.strip_suffix("grad") {
        return grad.trim().parse::<f32>().unwrap_or(0.0) * 0.9;
    }
    0.0
}

// ============================================================================
// View Transitions API
// ============================================================================

/// CSS view-transition-name value
#[derive(Debug, Clone, PartialEq)]
pub enum ViewTransitionName {
    None,
    Auto,
    Custom(String),
}

impl ViewTransitionName {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "none" => ViewTransitionName::None,
            "auto" => ViewTransitionName::Auto,
            _ => ViewTransitionName::Custom(s.trim().to_string()),
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, ViewTransitionName::None)
    }
}

/// View transition pseudo-element types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewTransitionPseudo {
    /// ::view-transition
    Root,
    /// ::view-transition-group(name)
    Group,
    /// ::view-transition-image-pair(name)
    ImagePair,
    /// ::view-transition-old(name)
    Old,
    /// ::view-transition-new(name)
    New,
}

impl ViewTransitionPseudo {
    pub fn parse(s: &str) -> Option<(Self, Option<String>)> {
        let s = s.trim();

        if s == "view-transition" {
            return Some((ViewTransitionPseudo::Root, None));
        }

        // Parse view-transition-group(name), etc.
        if let Some(rest) = s.strip_prefix("view-transition-group(") {
            let name = rest.strip_suffix(')')?.trim();
            let name = if name == "*" { None } else { Some(name.to_string()) };
            return Some((ViewTransitionPseudo::Group, name));
        }

        if let Some(rest) = s.strip_prefix("view-transition-image-pair(") {
            let name = rest.strip_suffix(')')?.trim();
            let name = if name == "*" { None } else { Some(name.to_string()) };
            return Some((ViewTransitionPseudo::ImagePair, name));
        }

        if let Some(rest) = s.strip_prefix("view-transition-old(") {
            let name = rest.strip_suffix(')')?.trim();
            let name = if name == "*" { None } else { Some(name.to_string()) };
            return Some((ViewTransitionPseudo::Old, name));
        }

        if let Some(rest) = s.strip_prefix("view-transition-new(") {
            let name = rest.strip_suffix(')')?.trim();
            let name = if name == "*" { None } else { Some(name.to_string()) };
            return Some((ViewTransitionPseudo::New, name));
        }

        None
    }
}

/// View transition state
#[derive(Debug, Clone)]
pub struct ViewTransition {
    pub ready: bool,
    pub finished: bool,
    pub update_callback_done: bool,
    pub skip_transition: bool,
    pub named_elements: std::collections::HashMap<String, ViewTransitionElement>,
}

impl Default for ViewTransition {
    fn default() -> Self {
        ViewTransition {
            ready: false,
            finished: false,
            update_callback_done: false,
            skip_transition: false,
            named_elements: std::collections::HashMap::new(),
        }
    }
}

impl ViewTransition {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_element(&mut self, name: String, element: ViewTransitionElement) {
        self.named_elements.insert(name, element);
    }

    pub fn skip(&mut self) {
        self.skip_transition = true;
        self.finished = true;
    }
}

/// View transition element capture
#[derive(Debug, Clone)]
pub struct ViewTransitionElement {
    /// Captured old state
    pub old_bounds: Option<ElementBounds>,
    /// New state after DOM update
    pub new_bounds: Option<ElementBounds>,
    /// Transform origin
    pub transform_origin: (f32, f32),
    /// Whether this is the root element
    pub is_root: bool,
}

impl Default for ViewTransitionElement {
    fn default() -> Self {
        ViewTransitionElement {
            old_bounds: None,
            new_bounds: None,
            transform_origin: (50.0, 50.0),
            is_root: false,
        }
    }
}

/// Element bounds for view transitions
#[derive(Debug, Clone, Copy)]
pub struct ElementBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ElementBounds {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        ElementBounds { x, y, width, height }
    }
}
