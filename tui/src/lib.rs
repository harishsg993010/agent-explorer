//! Semantic Browser TUI - Interactive terminal-based web browser
//!
//! This crate provides a full interactive TUI for the semantic browser,
//! featuring vim-style navigation, form interaction, and mouse support.

pub mod app;
pub mod ast_renderer;
pub mod event;
pub mod render;
pub mod style;
pub mod widgets;

pub use app::{App, AppState};
pub use ast_renderer::render_pipeline_output;
pub use event::EventHandler;
pub use render::{RenderedPage, LineContent};
