//! Semantic Browser TUI - Interactive terminal-based web browser
//!
//! Usage:
//!   semantic-tui [URL]
//!
//! Examples:
//!   semantic-tui                     # Start with welcome screen
//!   semantic-tui https://example.com # Open URL directly

use std::io::{self, stdout};
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    Terminal,
};

use tui::{
    app::{App, AppState},
    ast_renderer::render_pipeline_output,
    event::{Action, EventHandler},
    render::render_markdown,
    style::Theme,
    widgets::{ConsolePanel, ContentArea, HelpOverlay, StatusBar, UrlBar, ConsoleEntry},
};
use markdown::{PipelineOutput, Viewport, WidgetMap};

/// Browser command sent to the loading thread
enum BrowserCommand {
    Navigate(String),
    Refresh,
    Stop,
}

/// Browser event received from the loading thread
enum BrowserEvent {
    Loading { progress: f32 },
    Loaded {
        pipeline_output: PipelineOutput,
        widget_map: WidgetMap,
        url: String,
        console: Vec<ConsoleEntry>,
    },
    Error { message: String },
}

fn main() -> anyhow::Result<()> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let initial_url = args.get(1).cloned();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new();
    let theme = Theme::dark();

    // Get terminal size
    let size = terminal.size()?;
    app.terminal_size = (size.width, size.height);

    // Create channels for browser communication
    let (cmd_tx, cmd_rx) = mpsc::channel::<BrowserCommand>();
    let (event_tx, event_rx) = mpsc::channel::<BrowserEvent>();

    // Spawn browser loading thread
    let browser_thread = thread::spawn(move || {
        browser_worker(cmd_rx, event_tx);
    });

    // Navigate to initial URL if provided
    if let Some(url) = initial_url {
        let url = if !url.contains("://") {
            format!("https://{}", url)
        } else {
            url
        };
        app.navigate(&url);
        let _ = cmd_tx.send(BrowserCommand::Navigate(url));
    }

    // Main event loop
    let result = run_app(&mut terminal, &mut app, &theme, &cmd_tx, &event_rx);

    // Stop browser thread
    let _ = cmd_tx.send(BrowserCommand::Stop);
    let _ = browser_thread.join();

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

/// Main application loop
fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    theme: &Theme,
    cmd_tx: &mpsc::Sender<BrowserCommand>,
    event_rx: &mpsc::Receiver<BrowserEvent>,
) -> anyhow::Result<()> {
    loop {
        // Check for browser events (non-blocking)
        while let Ok(event) = event_rx.try_recv() {
            match event {
                BrowserEvent::Loading { progress } => {
                    app.loading_progress = progress;
                }
                BrowserEvent::Loaded { pipeline_output, widget_map, url, console } => {
                    let page = render_pipeline_output(&pipeline_output, &widget_map, &url);
                    app.set_page(page);
                    for entry in console {
                        app.add_console_entry(entry);
                    }
                }
                BrowserEvent::Error { message } => {
                    app.set_error(message);
                }
            }
        }

        // Draw UI
        terminal.draw(|frame| {
            let size = frame.area();
            app.terminal_size = (size.width, size.height);

            // Calculate layout
            let console_height = if app.console_visible { 8 } else { 0 };

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2),                    // URL bar
                    Constraint::Min(1),                       // Content
                    Constraint::Length(console_height),       // Console
                    Constraint::Length(2),                    // Status bar
                ])
                .split(size);

            // Render widgets
            frame.render_widget(UrlBar::new(app, theme), chunks[0]);
            frame.render_widget(ContentArea::new(app, theme), chunks[1]);

            if app.console_visible {
                frame.render_widget(ConsolePanel::new(app, theme), chunks[2]);
            }

            frame.render_widget(StatusBar::new(app, theme), chunks[3]);

            // Render overlays
            if matches!(app.state, AppState::Help) {
                frame.render_widget(HelpOverlay::new(theme), size);
            }

            if matches!(app.state, AppState::Error) {
                render_error_overlay(frame, app, theme, size);
            }
        })?;

        // Handle input events
        if let Some(event) = EventHandler::poll(Duration::from_millis(50))? {
            let action = EventHandler::handle(app, event);

            match action {
                Action::Navigate(url) => {
                    // Resolve relative URLs against current page
                    let resolved_url = resolve_url(&url, app.current_url());
                    app.navigate(&resolved_url);
                    let _ = cmd_tx.send(BrowserCommand::Navigate(resolved_url));
                }
                Action::Load(url) => {
                    // Load URL without modifying history (for back/forward)
                    app.loading = true;
                    app.loading_progress = 0.0;
                    app.state = AppState::Loading;
                    app.focus_index = None;
                    app.form_values.clear();
                    app.form_cursors.clear();
                    let _ = cmd_tx.send(BrowserCommand::Navigate(url));
                }
                Action::Refresh => {
                    if let Some(url) = app.current_url() {
                        let url = url.to_string();
                        app.loading = true;
                        app.loading_progress = 0.0;
                        app.state = AppState::Loading;
                        let _ = cmd_tx.send(BrowserCommand::Navigate(url));
                    }
                }
                Action::ClickButton(_id) => {
                    // Handle button clicks - submit form with collected values
                    if let Some(url) = build_form_submit_url(app) {
                        app.add_console_entry(ConsoleEntry::log(format!("Submitting form to: {}", url)));
                        app.navigate(&url);
                        let _ = cmd_tx.send(BrowserCommand::Navigate(url));
                    }
                }
                Action::SubmitForm(_id) => {
                    // Handle form submission (same as button click)
                    if let Some(url) = build_form_submit_url(app) {
                        app.navigate(&url);
                        let _ = cmd_tx.send(BrowserCommand::Navigate(url));
                    }
                }
                Action::Quit => {
                    break;
                }
                Action::None => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Build a form submission URL from current form values
fn build_form_submit_url(app: &App) -> Option<String> {
    let page = app.page.as_ref()?;
    let base_url = app.current_url()?;

    // Parse base URL
    let base = url::Url::parse(base_url).ok()?;

    // Get form action - could be absolute, relative, or empty
    let action = page.form_action.as_deref().unwrap_or("");

    // Resolve form action against base URL
    let mut url = if action.is_empty() {
        // Empty action means submit to current URL
        base.clone()
    } else if action.starts_with("http://") || action.starts_with("https://") {
        // Absolute URL
        url::Url::parse(action).ok()?
    } else if action.starts_with("//") {
        // Protocol-relative
        url::Url::parse(&format!("{}:{}", base.scheme(), action)).ok()?
    } else if action.starts_with('/') {
        // Absolute path
        let mut new_url = base.clone();
        new_url.set_path(action);
        new_url.set_query(None);
        new_url
    } else {
        // Relative path
        base.join(action).ok()?
    };

    // Collect all form field values
    let mut query_params = Vec::new();

    for (element_id, field_name) in &page.form_fields {
        if let Some(value) = app.form_values.get(element_id) {
            if !value.is_empty() {
                query_params.push((field_name.clone(), value.clone()));
            }
        }
    }

    // If no form values, return None
    if query_params.is_empty() {
        return None;
    }

    // Set query parameters (for GET method)
    if page.form_method == "get" {
        url.query_pairs_mut().clear();
        for (name, value) in query_params {
            url.query_pairs_mut().append_pair(&name, &value);
        }
    }
    // TODO: Handle POST method differently

    Some(url.to_string())
}

/// Resolve a URL against a base URL
fn resolve_url(url: &str, base: Option<&str>) -> String {
    // Already absolute
    if url.starts_with("http://") || url.starts_with("https://") {
        return url.to_string();
    }

    // Protocol-relative
    if url.starts_with("//") {
        return format!("https:{}", url);
    }

    // Need a base URL for relative resolution
    let base = match base {
        Some(b) => b,
        None => {
            // No base, assume https if it looks like a domain
            if url.contains('.') && !url.contains(' ') {
                return format!("https://{}", url);
            }
            return url.to_string();
        }
    };

    // Parse base URL
    let base_url = match url::Url::parse(base) {
        Ok(u) => u,
        Err(_) => return url.to_string(),
    };

    // Absolute path
    if url.starts_with('/') {
        return format!(
            "{}://{}{}",
            base_url.scheme(),
            base_url.host_str().unwrap_or(""),
            url
        );
    }

    // Relative path - resolve against base
    match base_url.join(url) {
        Ok(resolved) => resolved.to_string(),
        Err(_) => url.to_string(),
    }
}

/// Render error overlay
fn render_error_overlay(
    frame: &mut ratatui::Frame,
    app: &App,
    theme: &Theme,
    area: Rect,
) {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Clear, Paragraph};
    use ratatui::layout::Alignment;

    let message = app.error_message.as_deref().unwrap_or("Unknown error");

    // Calculate overlay size
    let width = 50.min(area.width - 4);
    let height = 6;
    let x = (area.width - width) / 2;
    let y = (area.height - height) / 2;

    let overlay_area = Rect::new(x, y, width, height);

    // Clear background
    frame.render_widget(Clear, overlay_area);

    let block = Block::default()
        .title(" Error ")
        .title_alignment(Alignment::Center)
        .title_style(
            Style::default()
                .fg(theme.error)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.error))
        .style(Style::default().bg(Color::Rgb(40, 20, 20)));

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            message,
            Style::default().fg(theme.fg),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press Esc or Enter to dismiss",
            Style::default().fg(theme.muted),
        )),
    ];

    let paragraph = Paragraph::new(content)
        .block(block)
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, overlay_area);
}

/// Browser worker thread - handles page loading
fn browser_worker(
    cmd_rx: mpsc::Receiver<BrowserCommand>,
    event_tx: mpsc::Sender<BrowserEvent>,
) {
    loop {
        match cmd_rx.recv() {
            Ok(BrowserCommand::Navigate(url)) => {
                // Send loading progress
                let _ = event_tx.send(BrowserEvent::Loading { progress: 0.1 });

                // Load the page using the browser engine
                match load_page(&url, |progress| {
                    let _ = event_tx.send(BrowserEvent::Loading { progress });
                }) {
                    Ok((pipeline_output, widget_map, console)) => {
                        let _ = event_tx.send(BrowserEvent::Loaded {
                            pipeline_output,
                            widget_map,
                            url,
                            console,
                        });
                    }
                    Err(e) => {
                        let _ = event_tx.send(BrowserEvent::Error {
                            message: e.to_string(),
                        });
                    }
                }
            }
            Ok(BrowserCommand::Refresh) => {
                // Refresh is handled by sending Navigate again
            }
            Ok(BrowserCommand::Stop) | Err(_) => {
                break;
            }
        }
    }
}

/// Load a page using the semantic browser engine
fn load_page(
    url: &str,
    progress_callback: impl Fn(f32),
) -> anyhow::Result<(PipelineOutput, WidgetMap, Vec<ConsoleEntry>)> {
    progress_callback(0.2);

    // Fetch the URL
    let response = net::fetch(url)?;
    progress_callback(0.4);

    // Parse HTML into DOM
    let dom = Rc::new(dom::Dom::parse(&response.body_text)?);
    progress_callback(0.5);

    // Initialize JS runtime
    let mut runtime = js::JsRuntime::new_with_url(dom.clone(), &response.url)?;
    progress_callback(0.6);

    // Sync cookies from response
    if let Some(domain) = url::Url::parse(&response.url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
    {
        js::add_cookies_from_headers(&response.headers, &domain);
    }

    // Fire DOMContentLoaded
    runtime.fire_dom_content_loaded();
    progress_callback(0.65);

    // Execute scripts
    let scripts = dom.get_scripts();
    let total_scripts = scripts.len().min(100);
    let mut console_entries = Vec::new();

    for (i, script) in scripts.into_iter().take(100).enumerate() {
        let script_progress = 0.65 + (0.25 * (i as f32 / total_scripts as f32));
        progress_callback(script_progress);

        // Get script content
        let script_content = if let Some(src) = &script.src {
            // Resolve relative URLs
            let script_url = if src.starts_with("http://") || src.starts_with("https://") {
                src.clone()
            } else if src.starts_with("//") {
                format!("https:{}", src)
            } else if src.starts_with('/') {
                if let Ok(base) = url::Url::parse(&response.url) {
                    format!("{}://{}{}", base.scheme(), base.host_str().unwrap_or(""), src)
                } else {
                    continue;
                }
            } else {
                continue;
            };

            // Skip known problematic scripts
            if script_url.contains("polyfill") {
                continue;
            }

            match net::fetch(&script_url) {
                Ok(resp) => resp.body_text,
                Err(e) => {
                    console_entries.push(ConsoleEntry::network_error(
                        &script_url,
                        0,
                        e.to_string(),
                    ));
                    continue;
                }
            }
        } else {
            script.content.clone()
        };

        // Skip large scripts
        if script_content.len() > 1024 * 1024 {
            continue;
        }

        // Execute script
        if let Some(error) = runtime.execute_safe(&script_content) {
            console_entries.push(ConsoleEntry::js_error(
                "JavaScript Error",
                &error,
                script.src.clone(),
                None,
                None,
            ));
        }

        // Run pending scripts
        runtime.execute_pending_scripts();
    }

    // Fire load event
    runtime.fire_load();
    progress_callback(0.9);

    // Use layout pipeline for rendering
    let viewport = Viewport::new(80); // Default 80 columns
    let (pipeline_output, widget_map) = markdown::layout_dom(&dom, &viewport);
    progress_callback(1.0);

    // Collect console output
    let console_output = runtime.console_output();
    for line in console_output {
        if line.starts_with("[ERROR]") || line.starts_with("Error:") {
            console_entries.push(ConsoleEntry::error(&line));
        } else if line.starts_with("[WARN]") || line.starts_with("Warning:") {
            console_entries.push(ConsoleEntry::warn(&line));
        } else {
            console_entries.push(ConsoleEntry::log(&line));
        }
    }

    Ok((pipeline_output, widget_map, console_entries))
}
