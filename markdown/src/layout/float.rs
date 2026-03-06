//! CSS Float layout implementation.
//!
//! This module handles CSS floats (float: left, float: right) which allows
//! elements to be taken out of normal flow and positioned to the left or right
//! of their container, with text and inline content wrapping around them.
//!
//! Float layout is essential for rendering legacy websites that rely on
//! float-based layouts for sidebars, image wrapping, and multi-column designs.

use crate::layout::{Clear, Float};

/// Represents a floated element and its position
#[derive(Debug, Clone)]
pub struct FloatedBox {
    /// Left position in characters
    pub x: usize,
    /// Top position in lines
    pub y: usize,
    /// Width in characters
    pub width: usize,
    /// Height in lines
    pub height: usize,
    /// Float direction
    pub float: Float,
}

impl FloatedBox {
    /// Get the right edge of the float
    pub fn right(&self) -> usize {
        self.x + self.width
    }

    /// Get the bottom edge of the float
    pub fn bottom(&self) -> usize {
        self.y + self.height
    }
}

/// Tracks active floats in a block formatting context.
///
/// This struct maintains the positions of left and right floats,
/// allowing content to flow around them properly.
#[derive(Debug, Clone, Default)]
pub struct FloatContext {
    /// Left floats (sorted by y position)
    left_floats: Vec<FloatedBox>,
    /// Right floats (sorted by y position)
    right_floats: Vec<FloatedBox>,
    /// Container width
    container_width: usize,
}

impl FloatContext {
    /// Create a new float context for a container
    pub fn new(container_width: usize) -> Self {
        FloatContext {
            left_floats: Vec::new(),
            right_floats: Vec::new(),
            container_width,
        }
    }

    /// Add a float to the context
    pub fn add_float(&mut self, float_box: FloatedBox) {
        match float_box.float {
            Float::Left | Float::InlineStart => {
                self.left_floats.push(float_box);
                self.left_floats.sort_by_key(|f| f.y);
            }
            Float::Right | Float::InlineEnd => {
                self.right_floats.push(float_box);
                self.right_floats.sort_by_key(|f| f.y);
            }
            Float::None => {}
        }
    }

    /// Place a float and return its position.
    ///
    /// The float is placed at the current y position, avoiding overlaps with
    /// existing floats. Returns the (x, y) position for the float.
    pub fn place_float(
        &mut self,
        float: Float,
        width: usize,
        height: usize,
        current_y: usize,
    ) -> (usize, usize) {
        match float {
            Float::Left | Float::InlineStart => {
                let (x, y) = self.find_left_position(width, height, current_y);
                self.add_float(FloatedBox {
                    x,
                    y,
                    width,
                    height,
                    float,
                });
                (x, y)
            }
            Float::Right | Float::InlineEnd => {
                let (x, y) = self.find_right_position(width, height, current_y);
                self.add_float(FloatedBox {
                    x,
                    y,
                    width,
                    height,
                    float,
                });
                (x, y)
            }
            Float::None => (0, current_y),
        }
    }

    /// Find position for a left float
    fn find_left_position(&self, width: usize, height: usize, min_y: usize) -> (usize, usize) {
        let mut y = min_y;

        loop {
            // Calculate left edge position (after any existing left floats at this y)
            let left_edge = self.left_edge_at(y, height);

            // Check if there's room (considering right floats)
            let right_edge = self.right_edge_at(y, height);
            let available_width = right_edge.saturating_sub(left_edge);

            if available_width >= width {
                return (left_edge, y);
            }

            // Move down past the lowest float affecting this line
            let next_y = self.next_clear_y(y, Clear::Both);
            if next_y == y {
                // No more floats, place at left edge
                return (left_edge, y);
            }
            y = next_y;
        }
    }

    /// Find position for a right float
    fn find_right_position(&self, width: usize, height: usize, min_y: usize) -> (usize, usize) {
        let mut y = min_y;

        loop {
            // Calculate right edge position (before any existing right floats at this y)
            let right_edge = self.right_edge_at(y, height);

            // Check if there's room (considering left floats)
            let left_edge = self.left_edge_at(y, height);
            let available_width = right_edge.saturating_sub(left_edge);

            if available_width >= width {
                return (right_edge.saturating_sub(width), y);
            }

            // Move down past the lowest float affecting this line
            let next_y = self.next_clear_y(y, Clear::Both);
            if next_y == y {
                return (self.container_width.saturating_sub(width), y);
            }
            y = next_y;
        }
    }

    /// Get the left edge of available content area at a given y position.
    ///
    /// This is the right edge of any left floats overlapping the given y range.
    pub fn left_edge_at(&self, y: usize, height: usize) -> usize {
        self.left_floats
            .iter()
            .filter(|f| self.overlaps_y_range(f, y, height))
            .map(|f| f.right())
            .max()
            .unwrap_or(0)
    }

    /// Get the right edge of available content area at a given y position.
    ///
    /// This is the left edge of any right floats overlapping the given y range.
    pub fn right_edge_at(&self, y: usize, height: usize) -> usize {
        self.right_floats
            .iter()
            .filter(|f| self.overlaps_y_range(f, y, height))
            .map(|f| f.x)
            .min()
            .unwrap_or(self.container_width)
    }

    /// Get the available width at a given y position
    pub fn available_width_at(&self, y: usize, height: usize) -> usize {
        let left = self.left_edge_at(y, height);
        let right = self.right_edge_at(y, height);
        right.saturating_sub(left)
    }

    /// Check if a float overlaps a given y range
    fn overlaps_y_range(&self, float_box: &FloatedBox, y: usize, height: usize) -> bool {
        let range_bottom = y + height;
        // Ranges overlap if neither is entirely before the other
        float_box.y < range_bottom && float_box.bottom() > y
    }

    /// Calculate the y position to clear floats based on clear value.
    ///
    /// Returns the y position that would be below all cleared floats.
    pub fn clear_y(&self, clear: Clear, current_y: usize) -> usize {
        match clear {
            Clear::None => current_y,
            Clear::Left | Clear::InlineStart => {
                self.left_floats
                    .iter()
                    .filter(|f| f.bottom() > current_y)
                    .map(|f| f.bottom())
                    .max()
                    .unwrap_or(current_y)
            }
            Clear::Right | Clear::InlineEnd => {
                self.right_floats
                    .iter()
                    .filter(|f| f.bottom() > current_y)
                    .map(|f| f.bottom())
                    .max()
                    .unwrap_or(current_y)
            }
            Clear::Both => {
                let left_max = self
                    .left_floats
                    .iter()
                    .filter(|f| f.bottom() > current_y)
                    .map(|f| f.bottom())
                    .max()
                    .unwrap_or(current_y);
                let right_max = self
                    .right_floats
                    .iter()
                    .filter(|f| f.bottom() > current_y)
                    .map(|f| f.bottom())
                    .max()
                    .unwrap_or(current_y);
                left_max.max(right_max)
            }
        }
    }

    /// Get the next y position where the float situation changes.
    ///
    /// This is used when there's not enough room at the current y position.
    fn next_clear_y(&self, current_y: usize, clear: Clear) -> usize {
        let mut candidates: Vec<usize> = Vec::new();

        if matches!(clear, Clear::Left | Clear::Both | Clear::InlineStart) {
            for f in &self.left_floats {
                if f.bottom() > current_y {
                    candidates.push(f.bottom());
                }
            }
        }

        if matches!(clear, Clear::Right | Clear::Both | Clear::InlineEnd) {
            for f in &self.right_floats {
                if f.bottom() > current_y {
                    candidates.push(f.bottom());
                }
            }
        }

        candidates
            .into_iter()
            .filter(|&y| y > current_y)
            .min()
            .unwrap_or(current_y)
    }

    /// Check if there are any active floats
    pub fn has_floats(&self) -> bool {
        !self.left_floats.is_empty() || !self.right_floats.is_empty()
    }

    /// Get all floats that extend past a given y position
    pub fn floats_at(&self, y: usize) -> impl Iterator<Item = &FloatedBox> {
        self.left_floats
            .iter()
            .chain(self.right_floats.iter())
            .filter(move |f| f.y <= y && f.bottom() > y)
    }

    /// Clear all floats (used when starting a new block formatting context)
    pub fn clear_all(&mut self) {
        self.left_floats.clear();
        self.right_floats.clear();
    }

    /// Get the bottom of all floats
    pub fn floats_bottom(&self) -> usize {
        let left_bottom = self.left_floats.iter().map(|f| f.bottom()).max().unwrap_or(0);
        let right_bottom = self.right_floats.iter().map(|f| f.bottom()).max().unwrap_or(0);
        left_bottom.max(right_bottom)
    }
}

/// Helper struct for laying out content alongside floats.
///
/// This provides line-by-line information about available space
/// when content needs to wrap around floats.
#[derive(Debug, Clone)]
pub struct FloatAwareLineBox {
    /// Starting x position for the line
    pub x: usize,
    /// Available width for the line
    pub width: usize,
    /// Y position of this line
    pub y: usize,
}

impl FloatContext {
    /// Get line boxes for a range of lines, accounting for floats.
    ///
    /// Returns a vector of line boxes, one per line in the range,
    /// each describing the available space on that line.
    pub fn get_line_boxes(&self, start_y: usize, num_lines: usize) -> Vec<FloatAwareLineBox> {
        (0..num_lines)
            .map(|i| {
                let y = start_y + i;
                let x = self.left_edge_at(y, 1);
                let right = self.right_edge_at(y, 1);
                FloatAwareLineBox {
                    x,
                    width: right.saturating_sub(x),
                    y,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_float_context() {
        let ctx = FloatContext::new(80);
        assert_eq!(ctx.left_edge_at(0, 1), 0);
        assert_eq!(ctx.right_edge_at(0, 1), 80);
        assert_eq!(ctx.available_width_at(0, 1), 80);
    }

    #[test]
    fn test_left_float() {
        let mut ctx = FloatContext::new(80);
        ctx.place_float(Float::Left, 20, 5, 0);

        // Lines 0-4 should have reduced width
        assert_eq!(ctx.left_edge_at(0, 1), 20);
        assert_eq!(ctx.available_width_at(0, 1), 60);

        // Line 5 should be clear
        assert_eq!(ctx.left_edge_at(5, 1), 0);
        assert_eq!(ctx.available_width_at(5, 1), 80);
    }

    #[test]
    fn test_right_float() {
        let mut ctx = FloatContext::new(80);
        ctx.place_float(Float::Right, 20, 5, 0);

        // Lines 0-4 should have reduced width on the right
        assert_eq!(ctx.right_edge_at(0, 1), 60);
        assert_eq!(ctx.available_width_at(0, 1), 60);

        // Line 5 should be clear
        assert_eq!(ctx.right_edge_at(5, 1), 80);
    }

    #[test]
    fn test_both_floats() {
        let mut ctx = FloatContext::new(80);
        ctx.place_float(Float::Left, 20, 5, 0);
        ctx.place_float(Float::Right, 20, 3, 0);

        // Lines 0-2: both floats active
        assert_eq!(ctx.left_edge_at(0, 1), 20);
        assert_eq!(ctx.right_edge_at(0, 1), 60);
        assert_eq!(ctx.available_width_at(0, 1), 40);

        // Lines 3-4: only left float active
        assert_eq!(ctx.left_edge_at(3, 1), 20);
        assert_eq!(ctx.right_edge_at(3, 1), 80);
        assert_eq!(ctx.available_width_at(3, 1), 60);
    }

    #[test]
    fn test_clear_left() {
        let mut ctx = FloatContext::new(80);
        ctx.place_float(Float::Left, 20, 5, 0);

        assert_eq!(ctx.clear_y(Clear::Left, 0), 5);
        assert_eq!(ctx.clear_y(Clear::Right, 0), 0);
        assert_eq!(ctx.clear_y(Clear::Both, 0), 5);
    }

    #[test]
    fn test_stacked_left_floats() {
        let mut ctx = FloatContext::new(80);

        // First float: 20 chars wide, 3 lines tall
        ctx.place_float(Float::Left, 20, 3, 0);

        // Second float: should stack after the first
        let (x, y) = ctx.place_float(Float::Left, 15, 2, 0);
        assert_eq!(x, 20); // Placed to the right of first float
        assert_eq!(y, 0); // At the same y position if there's room
    }
}
