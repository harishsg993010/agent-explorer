//! Full table layout implementation.
//!
//! Handles:
//! - Column width calculation (content-based and explicit)
//! - Cell content wrapping
//! - Header/body/footer separation
//! - Cell alignment
//! - Colspan/rowspan (simplified)
//! - Fallback to stacked rows for narrow viewports

use super::block::{collect_inline_content, BlockLayoutContext};
use super::inline::{display_width, layout_inline, InlineLayoutContext};
use super::tree::LayoutBox;
use super::Viewport;
use crate::ast::{Alignment, Block, BlockKind, InlineContent, TableCell};
use crate::ids::NodeId;

/// Table layout mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableMode {
    /// Automatically choose based on available width
    Auto,
    /// Force Markdown table format
    ForceMarkdown,
    /// Force stacked row format (for narrow terminals)
    ForceStacked,
}

/// Table layout result
#[derive(Debug)]
pub struct TableLayoutResult {
    pub block: Block,
    /// Whether we used stacked layout due to width constraints
    pub used_stacked: bool,
}

/// A parsed table structure
#[derive(Debug)]
pub struct ParsedTable {
    pub caption: Option<InlineContent>,
    pub headers: Vec<TableCellData>,
    pub rows: Vec<Vec<TableCellData>>,
    pub column_count: usize,
    pub alignments: Vec<Alignment>,
    pub source_node: NodeId,
}

/// Data for a single cell
#[derive(Debug, Clone)]
pub struct TableCellData {
    pub content: InlineContent,
    pub source: Option<NodeId>,
    pub colspan: usize,
    pub rowspan: usize,
    pub alignment: Alignment,
    pub vertical_align: super::VerticalAlign,
    pub is_header: bool,
}

/// Layout a table element
pub fn layout_table(
    table_box: &LayoutBox,
    ctx: &BlockLayoutContext,
    mode: TableMode,
) -> TableLayoutResult {
    // Parse the table structure
    let parsed = parse_table(table_box);

    if parsed.column_count == 0 {
        return TableLayoutResult {
            block: Block {
                kind: BlockKind::Paragraph {
                    content: InlineContent::new(),
                },
                source: Some(table_box.node_id),
            },
            used_stacked: false,
        };
    }

    // Calculate column widths
    let column_widths = calculate_column_widths(&parsed, ctx.available_width);
    let total_width = column_widths.iter().sum::<usize>()
        + (parsed.column_count + 1) * 3; // borders and padding

    // Decide layout mode
    let use_stacked = match mode {
        TableMode::ForceMarkdown => false,
        TableMode::ForceStacked => true,
        TableMode::Auto => total_width > ctx.available_width,
    };

    if use_stacked {
        layout_stacked_table(&parsed, ctx)
    } else {
        layout_markdown_table(&parsed, &column_widths, ctx)
    }
}

/// Parse a table box into structured data
fn parse_table(table_box: &LayoutBox) -> ParsedTable {
    let mut caption = None;
    let mut headers = Vec::new();
    let mut rows = Vec::new();
    let mut alignments = Vec::new();
    let mut max_columns = 0usize;

    for child in &table_box.children {
        match child.tag.as_str() {
            "caption" => {
                // Parse table caption
                caption = Some(collect_inline_content(child));
            }
            "thead" => {
                for row in &child.children {
                    if row.tag == "tr" {
                        let (cells, aligns) = parse_row(row, true);
                        max_columns = max_columns.max(count_effective_columns(&cells));
                        if alignments.is_empty() {
                            alignments = aligns;
                        }
                        headers.extend(cells);
                    }
                }
            }
            "tbody" | "tfoot" => {
                for row in &child.children {
                    if row.tag == "tr" {
                        let (cells, aligns) = parse_row(row, false);
                        max_columns = max_columns.max(count_effective_columns(&cells));
                        if alignments.is_empty() && !aligns.is_empty() {
                            alignments = aligns;
                        }
                        if !cells.is_empty() {
                            rows.push(cells);
                        }
                    }
                }
            }
            "tr" => {
                // Direct tr child (no thead/tbody)
                let is_header = child
                    .children
                    .first()
                    .map(|c| c.tag == "th")
                    .unwrap_or(false);
                let (cells, aligns) = parse_row(child, is_header);
                max_columns = max_columns.max(count_effective_columns(&cells));
                if alignments.is_empty() && !aligns.is_empty() {
                    alignments = aligns;
                }
                if is_header && headers.is_empty() {
                    headers = cells;
                } else if !cells.is_empty() {
                    rows.push(cells);
                }
            }
            "colgroup" => {
                // Parse column alignments from colgroup
                for col in &child.children {
                    if col.tag == "col" {
                        alignments.push(get_alignment_from_style(&col.style));
                    }
                }
            }
            _ => {}
        }
    }

    // Ensure alignments vector matches column count
    while alignments.len() < max_columns {
        alignments.push(Alignment::Default);
    }

    ParsedTable {
        caption,
        headers,
        rows,
        column_count: max_columns,
        alignments,
        source_node: table_box.node_id,
    }
}

/// Count effective columns considering colspan
fn count_effective_columns(cells: &[TableCellData]) -> usize {
    cells.iter().map(|c| c.colspan).sum()
}

/// Parse a table row into cells
fn parse_row(row: &LayoutBox, is_header_row: bool) -> (Vec<TableCellData>, Vec<Alignment>) {
    let mut cells = Vec::new();
    let mut alignments = Vec::new();

    for cell in &row.children {
        if cell.tag == "td" || cell.tag == "th" {
            let is_header = cell.tag == "th" || is_header_row;
            let content = collect_inline_content(cell);

            // Get colspan/rowspan from HTML attributes or computed style
            let colspan = cell
                .attrs
                .get("colspan")
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| cell.style.colspan.max(1));
            let rowspan = cell
                .attrs
                .get("rowspan")
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| cell.style.rowspan.max(1));

            let alignment = get_alignment_from_style(&cell.style);
            alignments.push(alignment);

            cells.push(TableCellData {
                content,
                source: Some(cell.node_id),
                colspan,
                rowspan,
                alignment,
                vertical_align: cell.style.vertical_align,
                is_header,
            });
        }
    }

    (cells, alignments)
}

/// Get alignment from computed style
fn get_alignment_from_style(style: &super::ComputedStyle) -> Alignment {
    match style.text_align {
        super::TextAlign::Left | super::TextAlign::Start => Alignment::Left,
        super::TextAlign::Right | super::TextAlign::End => Alignment::Right,
        super::TextAlign::Center => Alignment::Center,
        super::TextAlign::Justify => Alignment::Left,
    }
}

/// Calculate optimal column widths
fn calculate_column_widths(table: &ParsedTable, available_width: usize) -> Vec<usize> {
    if table.column_count == 0 {
        return Vec::new();
    }

    // Calculate content widths for each column
    let mut min_widths = vec![3usize; table.column_count]; // Minimum 3 chars per column
    let mut max_widths = vec![0usize; table.column_count];

    // Measure header cells
    for (i, cell) in table.headers.iter().enumerate() {
        if i < table.column_count {
            let width = measure_cell_width(&cell.content);
            min_widths[i] = min_widths[i].max(min_cell_width(&cell.content));
            max_widths[i] = max_widths[i].max(width);
        }
    }

    // Measure body cells
    for row in &table.rows {
        for (i, cell) in row.iter().enumerate() {
            if i < table.column_count {
                let width = measure_cell_width(&cell.content);
                min_widths[i] = min_widths[i].max(min_cell_width(&cell.content));
                max_widths[i] = max_widths[i].max(width);
            }
        }
    }

    // Calculate total desired width
    let border_overhead = (table.column_count + 1) * 3; // | and padding
    let available_for_content = available_width.saturating_sub(border_overhead);

    let total_max: usize = max_widths.iter().sum();
    let total_min: usize = min_widths.iter().sum();

    if total_max <= available_for_content {
        // All columns fit at max width
        max_widths
    } else if total_min >= available_for_content {
        // Use minimum widths
        min_widths
    } else {
        // Proportionally distribute available space
        let extra_space = available_for_content - total_min;
        let extra_needed = total_max - total_min;

        let mut widths = min_widths.clone();
        for i in 0..table.column_count {
            let column_extra = max_widths[i] - min_widths[i];
            let share = if extra_needed > 0 {
                (column_extra as f64 / extra_needed as f64 * extra_space as f64) as usize
            } else {
                0
            };
            widths[i] += share;
        }
        widths
    }
}

/// Measure the width of cell content
fn measure_cell_width(content: &InlineContent) -> usize {
    content
        .spans
        .iter()
        .map(|s| display_width(&s.content))
        .sum::<usize>()
        .max(3)
}

/// Get minimum width of cell (longest word)
fn min_cell_width(content: &InlineContent) -> usize {
    content
        .spans
        .iter()
        .flat_map(|s| s.content.split_whitespace())
        .map(|w| display_width(w))
        .max()
        .unwrap_or(3)
        .max(3)
}

/// Layout table in Markdown format
fn layout_markdown_table(
    table: &ParsedTable,
    column_widths: &[usize],
    _ctx: &BlockLayoutContext,
) -> TableLayoutResult {
    // Convert to AST table format
    let headers: Vec<TableCell> = if !table.headers.is_empty() {
        table
            .headers
            .iter()
            .map(|c| TableCell {
                content: c.content.clone(),
                source: c.source,
            })
            .collect()
    } else if let Some(first_row) = table.rows.first() {
        // Use first row as header if no explicit headers
        first_row
            .iter()
            .map(|c| TableCell {
                content: c.content.clone(),
                source: c.source,
            })
            .collect()
    } else {
        Vec::new()
    };

    let rows: Vec<Vec<TableCell>> = table
        .rows
        .iter()
        .skip(if table.headers.is_empty() { 1 } else { 0 })
        .map(|row| {
            row.iter()
                .map(|c| TableCell {
                    content: c.content.clone(),
                    source: c.source,
                })
                .collect()
        })
        .collect();

    TableLayoutResult {
        block: Block {
            kind: BlockKind::Table {
                headers,
                rows,
                alignments: table.alignments.clone(),
            },
            source: Some(table.source_node),
        },
        used_stacked: false,
    }
}

/// Layout table in stacked format (for narrow terminals)
fn layout_stacked_table(table: &ParsedTable, ctx: &BlockLayoutContext) -> TableLayoutResult {
    let mut blocks = Vec::new();

    // Get header labels
    let header_labels: Vec<String> = table
        .headers
        .iter()
        .map(|c| c.content.plain_text())
        .collect();

    // Render each row as a stacked block
    for (row_idx, row) in table.rows.iter().enumerate() {
        // Row separator
        if row_idx > 0 {
            blocks.push(Block {
                kind: BlockKind::ThematicBreak,
                source: None,
            });
        }

        // Each cell as label: value
        for (col_idx, cell) in row.iter().enumerate() {
            let label = header_labels.get(col_idx).cloned().unwrap_or_default();
            let value = cell.content.plain_text();

            let mut content = InlineContent::new();
            if !label.is_empty() {
                content.push(crate::ast::Span {
                    kind: crate::ast::SpanKind::Strong,
                    content: format!("{}: ", label),
                    source: None,
                });
            }
            content.push(crate::ast::Span {
                kind: crate::ast::SpanKind::Text,
                content: value,
                source: cell.source,
            });

            blocks.push(Block {
                kind: BlockKind::Paragraph { content },
                source: cell.source,
            });
        }
    }

    TableLayoutResult {
        block: Block {
            kind: BlockKind::Container {
                blocks,
                indent: ctx.indent,
            },
            source: Some(table.source_node),
        },
        used_stacked: true,
    }
}

/// Render a table to Markdown string
pub fn render_table_markdown(
    headers: &[TableCell],
    rows: &[Vec<TableCell>],
    alignments: &[Alignment],
    max_width: usize,
) -> String {
    if headers.is_empty() && rows.is_empty() {
        return String::new();
    }

    // Calculate column widths
    let column_count = headers.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
    let mut widths = vec![3usize; column_count];

    for (i, cell) in headers.iter().enumerate() {
        widths[i] = widths[i].max(measure_cell_width(&cell.content));
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < column_count {
                widths[i] = widths[i].max(measure_cell_width(&cell.content));
            }
        }
    }

    let mut output = String::new();

    // Header row
    output.push('|');
    for (i, cell) in headers.iter().enumerate() {
        let text = cell.content.plain_text();
        let width = widths.get(i).copied().unwrap_or(3);
        output.push_str(&format!(" {:width$} |", text, width = width));
    }
    output.push('\n');

    // Separator row
    output.push('|');
    for (i, alignment) in alignments.iter().take(column_count).enumerate() {
        let width = widths.get(i).copied().unwrap_or(3);
        let sep = match alignment {
            Alignment::Left | Alignment::Default => format!(":{}", "-".repeat(width)),
            Alignment::Right => format!("{}:", "-".repeat(width)),
            Alignment::Center => format!(":{}:", "-".repeat(width - 1)),
        };
        output.push_str(&format!(" {} |", sep));
    }
    output.push('\n');

    // Data rows
    for row in rows {
        output.push('|');
        for (i, cell) in row.iter().enumerate() {
            let text = cell.content.plain_text();
            let width = widths.get(i).copied().unwrap_or(3);
            output.push_str(&format!(" {:width$} |", text, width = width));
        }
        output.push('\n');
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::ComputedStyle;

    fn make_table_cell(text: &str) -> LayoutBox {
        let mut cell = LayoutBox::new(NodeId::new(1), "td", ComputedStyle::default());
        cell.children
            .push(LayoutBox::text_node(NodeId::new(2), text));
        cell
    }

    fn make_table() -> LayoutBox {
        let mut table = LayoutBox::new(NodeId::new(0), "table", ComputedStyle::default());

        // Header row
        let mut thead = LayoutBox::new(NodeId::new(1), "thead", ComputedStyle::default());
        let mut header_row = LayoutBox::new(NodeId::new(2), "tr", ComputedStyle::default());

        let mut th1 = LayoutBox::new(NodeId::new(3), "th", ComputedStyle::default());
        th1.children
            .push(LayoutBox::text_node(NodeId::new(4), "Name"));
        let mut th2 = LayoutBox::new(NodeId::new(5), "th", ComputedStyle::default());
        th2.children
            .push(LayoutBox::text_node(NodeId::new(6), "Value"));

        header_row.children.push(th1);
        header_row.children.push(th2);
        thead.children.push(header_row);
        table.children.push(thead);

        // Body rows
        let mut tbody = LayoutBox::new(NodeId::new(7), "tbody", ComputedStyle::default());
        let mut row1 = LayoutBox::new(NodeId::new(8), "tr", ComputedStyle::default());
        let mut td1 = make_table_cell("Item 1");
        let mut td2 = make_table_cell("100");
        td1.tag = "td".to_string();
        td2.tag = "td".to_string();
        row1.children.push(td1);
        row1.children.push(td2);
        tbody.children.push(row1);
        table.children.push(tbody);

        table
    }

    #[test]
    fn test_parse_table() {
        let table = make_table();
        let parsed = parse_table(&table);

        assert_eq!(parsed.column_count, 2);
        assert!(!parsed.headers.is_empty());
        assert!(!parsed.rows.is_empty());
    }

    #[test]
    fn test_calculate_widths() {
        let table = make_table();
        let parsed = parse_table(&table);
        let widths = calculate_column_widths(&parsed, 80);

        assert_eq!(widths.len(), 2);
        assert!(widths.iter().all(|&w| w >= 3));
    }

    #[test]
    fn test_layout_markdown_table() {
        let table = make_table();
        let viewport = Viewport::new(80);
        let ctx = BlockLayoutContext::new(&viewport);

        let result = layout_table(&table, &ctx, TableMode::ForceMarkdown);
        assert!(!result.used_stacked);
    }

    #[test]
    fn test_layout_stacked_table() {
        let table = make_table();
        let viewport = Viewport::new(80);
        let ctx = BlockLayoutContext::new(&viewport);

        let result = layout_table(&table, &ctx, TableMode::ForceStacked);
        assert!(result.used_stacked);
    }
}
