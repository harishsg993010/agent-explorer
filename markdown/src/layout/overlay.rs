//! Overlay layout and detection.
//!
//! Handles elements with position: fixed, absolute, or elements
//! that act as overlays (dialogs, modals, dropdowns, tooltips).
//!
//! Overlay strategies:
//! - InlineFallback: Render overlay content inline at detection point
//! - PinnedRegions: Collect overlays and render in dedicated sections
//! - ModalFocus: If a modal is open, render only modal content

use super::block::{layout_block, BlockLayoutContext};
use super::tree::LayoutBox;
use super::{Position, Visibility, Viewport};
use crate::ast::{Block, BlockKind, Document, InlineContent, Overlay, OverlayKind, OverlayPlan, OverlayRenderMode};
use crate::ids::{NodeId, OverlayId};

/// Overlay detection result
#[derive(Debug)]
pub struct DetectedOverlay {
    /// Unique identifier for this overlay
    pub id: OverlayId,
    /// The overlay's root layout box
    pub layout_box: LayoutBox,
    /// What kind of overlay this is
    pub kind: OverlayKind,
    /// Original DOM node ID
    pub node_id: NodeId,
    /// Z-index for stacking order
    pub z_index: i32,
}

/// Overlay layout context
pub struct OverlayLayoutContext<'a> {
    pub viewport: &'a Viewport,
    pub render_mode: OverlayRenderMode,
    /// Detected overlays during layout traversal
    pub overlays: Vec<DetectedOverlay>,
}

impl<'a> OverlayLayoutContext<'a> {
    pub fn new(viewport: &'a Viewport) -> Self {
        OverlayLayoutContext {
            viewport,
            render_mode: OverlayRenderMode::InlineFallback,
            overlays: Vec::new(),
        }
    }

    pub fn with_render_mode(mut self, mode: OverlayRenderMode) -> Self {
        self.render_mode = mode;
        self
    }
}

/// Detect if a layout box is an overlay
pub fn detect_overlay(layout_box: &LayoutBox) -> Option<OverlayKind> {
    // Check position
    let is_fixed = layout_box.style.position == Position::Fixed;
    let is_absolute = layout_box.style.position == Position::Absolute;

    // Check if it's a dialog element
    let is_dialog = layout_box.tag == "dialog";

    // Check for common overlay patterns via attributes or classes
    let role = layout_box.attrs.get("role").map(|s| s.as_str());
    let is_modal = role == Some("dialog") || role == Some("alertdialog");
    let is_menu = role == Some("menu") || role == Some("listbox");
    let is_tooltip = role == Some("tooltip");

    // Check aria-modal attribute
    let aria_modal = layout_box.attrs.get("aria-modal").map(|s| s.as_str()) == Some("true");

    // Detect overlay kind
    if is_dialog || (is_modal && aria_modal) {
        Some(OverlayKind::Modal)
    } else if is_menu {
        Some(OverlayKind::Dropdown)
    } else if is_tooltip {
        Some(OverlayKind::Tooltip)
    } else if is_fixed {
        // Fixed elements are often headers/footers or floating UI
        Some(OverlayKind::Fixed)
    } else if is_absolute {
        // Absolute positioned elements might be dropdowns or popups
        Some(OverlayKind::Popup)
    } else {
        None
    }
}

/// Check if an overlay is currently visible
pub fn is_overlay_visible(layout_box: &LayoutBox) -> bool {
    // Check visibility
    if layout_box.style.visibility == Visibility::Hidden {
        return false;
    }

    // Check display
    if layout_box.style.display == super::Display::None {
        return false;
    }

    // Check for hidden attribute
    if layout_box.attrs.contains_key("hidden") {
        return false;
    }

    // Check for aria-hidden
    if layout_box.attrs.get("aria-hidden").map(|s| s.as_str()) == Some("true") {
        return false;
    }

    // Check dialog open state
    if layout_box.tag == "dialog" {
        // Dialog is visible if it has the open attribute
        return layout_box.attrs.contains_key("open");
    }

    true
}

/// Extract overlays from a layout tree
pub fn extract_overlays(root: &LayoutBox) -> Vec<DetectedOverlay> {
    let mut overlays = Vec::new();
    extract_overlays_recursive(root, &mut overlays, 0);

    // Sort by z-index for proper stacking order
    overlays.sort_by(|a, b| a.z_index.cmp(&b.z_index));

    overlays
}

fn extract_overlays_recursive(
    layout_box: &LayoutBox,
    overlays: &mut Vec<DetectedOverlay>,
    depth: usize,
) {
    if let Some(kind) = detect_overlay(layout_box) {
        if is_overlay_visible(layout_box) {
            let z_index = layout_box.style.z_index.unwrap_or(depth as i32);

            overlays.push(DetectedOverlay {
                id: OverlayId::new(),
                layout_box: layout_box.clone(),
                kind,
                node_id: layout_box.node_id,
                z_index,
            });
        }
    }

    // Recurse into children
    for child in &layout_box.children {
        extract_overlays_recursive(child, overlays, depth + 1);
    }
}

/// Create an overlay plan from detected overlays
pub fn create_overlay_plan(overlays: Vec<DetectedOverlay>, mode: OverlayRenderMode) -> OverlayPlan {
    let overlays: Vec<Overlay> = overlays
        .into_iter()
        .map(|detected| Overlay {
            id: detected.id,
            kind: detected.kind,
            source_node: detected.node_id,
            content: Document::new(),
            z_index: detected.z_index,
            visible: true,
        })
        .collect();

    OverlayPlan {
        overlays,
        mode,
    }
}

/// Layout overlays and produce blocks
pub fn layout_overlays(
    detected: &[DetectedOverlay],
    viewport: &Viewport,
    mode: OverlayRenderMode,
) -> Vec<Block> {
    match mode {
        OverlayRenderMode::InlineFallback => {
            // Render each overlay as inline content at its detection point
            layout_overlays_inline(detected, viewport)
        }
        OverlayRenderMode::PinnedRegions => {
            // Render overlays in dedicated regions
            layout_overlays_pinned(detected, viewport)
        }
        OverlayRenderMode::ModalFocus => {
            // If there's a modal, render only that
            layout_overlays_modal_focus(detected, viewport)
        }
    }
}

/// Layout overlays inline (embedded in document flow)
fn layout_overlays_inline(detected: &[DetectedOverlay], viewport: &Viewport) -> Vec<Block> {
    let mut blocks = Vec::new();

    for overlay in detected {
        // Add a separator/label for the overlay
        let label = match overlay.kind {
            OverlayKind::Modal => "Modal Dialog",
            OverlayKind::Popup => "Popup",
            OverlayKind::Tooltip => "Tooltip",
            OverlayKind::Dropdown => "Menu",
            OverlayKind::Fixed => "Fixed Element",
            OverlayKind::Absolute => "Positioned Element",
            OverlayKind::Toast => "Notification",
        };

        // Add label as a heading
        blocks.push(Block {
            kind: BlockKind::Heading {
                level: 4,
                content: InlineContent::text(label),
            },
            source: Some(overlay.node_id),
        });

        // Layout the overlay content
        let mut ctx = BlockLayoutContext::new(viewport);
        let content_blocks = layout_block(&overlay.layout_box, &mut ctx);
        blocks.extend(content_blocks);

        // Add a separator after
        blocks.push(Block {
            kind: BlockKind::ThematicBreak,
            source: None,
        });
    }

    blocks
}

/// Layout overlays in pinned regions (header/footer sections)
fn layout_overlays_pinned(detected: &[DetectedOverlay], viewport: &Viewport) -> Vec<Block> {
    let mut header_blocks = Vec::new();
    let mut footer_blocks = Vec::new();
    let mut other_blocks = Vec::new();

    for overlay in detected {
        let mut ctx = BlockLayoutContext::new(viewport);
        let content_blocks = layout_block(&overlay.layout_box, &mut ctx);

        match overlay.kind {
            OverlayKind::Fixed => {
                // Fixed elements often go at top or bottom
                header_blocks.push(Block {
                    kind: BlockKind::ThematicBreak,
                    source: Some(overlay.node_id),
                });
                header_blocks.extend(content_blocks);
            }
            OverlayKind::Modal | OverlayKind::Popup => {
                // Modals and popups go at the end
                footer_blocks.push(Block {
                    kind: BlockKind::Heading {
                        level: 4,
                        content: InlineContent::text(if overlay.kind == OverlayKind::Modal {
                            "Modal"
                        } else {
                            "Popup"
                        }),
                    },
                    source: Some(overlay.node_id),
                });
                footer_blocks.extend(content_blocks);
            }
            _ => {
                other_blocks.extend(content_blocks);
            }
        }
    }

    let mut all_blocks = Vec::new();

    if !header_blocks.is_empty() {
        all_blocks.extend(header_blocks);
        all_blocks.push(Block {
            kind: BlockKind::ThematicBreak,
            source: None,
        });
    }

    all_blocks.extend(other_blocks);

    if !footer_blocks.is_empty() {
        all_blocks.push(Block {
            kind: BlockKind::ThematicBreak,
            source: None,
        });
        all_blocks.extend(footer_blocks);
    }

    all_blocks
}

/// Layout with modal focus - only render the topmost modal
fn layout_overlays_modal_focus(detected: &[DetectedOverlay], viewport: &Viewport) -> Vec<Block> {
    // Find the topmost modal (highest z-index)
    let modal = detected
        .iter()
        .filter(|o| o.kind == OverlayKind::Modal)
        .max_by_key(|o| o.z_index);

    if let Some(modal) = modal {
        let mut blocks = Vec::new();

        // Add modal header
        blocks.push(Block {
            kind: BlockKind::Heading {
                level: 2,
                content: InlineContent::text("Dialog"),
            },
            source: Some(modal.node_id),
        });

        // Layout modal content
        let mut ctx = BlockLayoutContext::new(viewport);
        let content_blocks = layout_block(&modal.layout_box, &mut ctx);
        blocks.extend(content_blocks);

        blocks
    } else {
        // No modal found, render nothing for overlays
        Vec::new()
    }
}

/// Check if we should suppress main content due to modal focus
pub fn has_blocking_modal(overlays: &[DetectedOverlay]) -> bool {
    overlays.iter().any(|o| {
        o.kind == OverlayKind::Modal && {
            // Check if it's truly blocking (aria-modal="true" or dialog element)
            o.layout_box.attrs.get("aria-modal").map(|s| s.as_str()) == Some("true")
                || o.layout_box.tag == "dialog"
        }
    })
}

/// Filter the main content tree to exclude overlay elements
pub fn filter_overlays_from_tree(layout_box: &LayoutBox) -> LayoutBox {
    let mut filtered = layout_box.clone();
    filter_overlays_recursive(&mut filtered);
    filtered
}

fn filter_overlays_recursive(layout_box: &mut LayoutBox) {
    // Remove children that are overlays
    layout_box.children.retain(|child| {
        detect_overlay(child).is_none()
    });

    // Recurse into remaining children
    for child in &mut layout_box.children {
        filter_overlays_recursive(child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::ComputedStyle;

    #[test]
    fn test_detect_dialog() {
        let node_id = NodeId::new(1);
        let mut layout_box = LayoutBox::new(node_id, "dialog", ComputedStyle::default());
        layout_box.attrs.insert("open".to_string(), "".to_string());

        let kind = detect_overlay(&layout_box);
        assert_eq!(kind, Some(OverlayKind::Modal));
        assert!(is_overlay_visible(&layout_box));
    }

    #[test]
    fn test_detect_fixed() {
        let node_id = NodeId::new(1);
        let mut style = ComputedStyle::default();
        style.position = Position::Fixed;
        let layout_box = LayoutBox::new(node_id, "div", style);

        let kind = detect_overlay(&layout_box);
        assert_eq!(kind, Some(OverlayKind::Fixed));
    }

    #[test]
    fn test_hidden_dialog() {
        let node_id = NodeId::new(1);
        let layout_box = LayoutBox::new(node_id, "dialog", ComputedStyle::default());
        // No "open" attribute

        assert!(!is_overlay_visible(&layout_box));
    }

    #[test]
    fn test_detect_menu() {
        let node_id = NodeId::new(1);
        let mut layout_box = LayoutBox::new(node_id, "div", ComputedStyle::default());
        layout_box.attrs.insert("role".to_string(), "menu".to_string());

        let kind = detect_overlay(&layout_box);
        assert_eq!(kind, Some(OverlayKind::Dropdown));
    }

    #[test]
    fn test_extract_overlays() {
        let node_id = NodeId::new(1);
        let child_id = NodeId::new(2);

        let mut root = LayoutBox::new(node_id, "div", ComputedStyle::default());

        let mut dialog = LayoutBox::new(child_id, "dialog", ComputedStyle::default());
        dialog.attrs.insert("open".to_string(), "".to_string());

        root.children.push(dialog);

        let overlays = extract_overlays(&root);
        assert_eq!(overlays.len(), 1);
        assert_eq!(overlays[0].kind, OverlayKind::Modal);
    }
}
