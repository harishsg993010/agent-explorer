# Semantic Browser - Architecture Documentation

A headless browser engine that renders HTML/CSS/JavaScript into Markdown output. Built in Rust with the Boa JavaScript engine.

## Overview

Semantic Browser fetches web pages, executes JavaScript, and converts the rendered DOM to Markdown. It's designed for content extraction from modern SPAs and dynamic websites.

```
URL → Fetch → Parse HTML → Execute JS → Render Markdown → Output
```

## Project Structure

```
semantic-browser/
├── browser/          # CLI entry point & main pipeline
├── js/               # JavaScript runtime (Boa) & Web APIs
├── dom/              # DOM parsing & manipulation
├── net/              # HTTP fetching with cookie support
├── markdown/         # HTML to Markdown conversion
└── Cargo.toml        # Workspace configuration
```

## Crate Architecture

### 1. browser (Entry Point)

**File**: `browser/src/main.rs`

The main pipeline orchestrator:

```rust
fn run(url: &str) -> Result<String> {
    // 1. Fetch URL
    let response = net::fetch(url)?;

    // 2. Sync cookies from response
    js::add_cookies_from_headers(&response.headers, &domain);

    // 3. Parse HTML into DOM
    let dom = Rc::new(dom::Dom::parse(&response.body_text)?);

    // 4. Initialize JS runtime
    let mut runtime = js::JsRuntime::new_with_url(dom.clone(), &response.url)?;

    // 5. Fire DOMContentLoaded
    runtime.fire_dom_content_loaded();

    // 6. Execute scripts (max 100)
    for script in scripts {
        runtime.execute_safe(&script_content);
        runtime.execute_pending_scripts();
    }

    // 7. Fire load event
    runtime.fire_load();

    // 8. Render to Markdown
    markdown::render_dom_query(&dom)
}
```

**Execution Limits**:
| Limit | Value | Purpose |
|-------|-------|---------|
| MAX_SCRIPTS | 100 | Prevent runaway execution |
| MAX_DYNAMIC_SCRIPTS | 50 | Limit DOM-injected scripts |
| MAX_TOTAL_SCRIPT_BYTES | 10MB | Prevent OOM |
| MAX_FINAL_PASSES | 10 | Event loop iterations |

### 2. net (HTTP Client)

**File**: `net/src/lib.rs`

HTTP fetching with browser emulation:

```rust
pub struct HttpResponse {
    pub url: String,                    // Final URL after redirects
    pub status: u16,                    // HTTP status code
    pub headers: HashMap<String, String>,
    pub body_text: String,              // Response body
}

pub fn fetch(url: &str) -> Result<HttpResponse>
```

**Features**:
- User-Agent: Chrome 120 emulation
- Automatic redirect following (max 10)
- Cookie persistence via global jar
- Compression: gzip, brotli, deflate
- 30-second timeout
- Full browser headers (Sec-Fetch-*, etc.)

### 3. dom (DOM Implementation)

**File**: `dom/src/lib.rs`

Live DOM with JavaScript bindings:

```rust
pub struct Dom {
    document: RcDom,
    id_cache: RefCell<HashMap<String, Handle>>,
}

pub struct InlineScript {
    pub content: String,
    pub src: Option<String>,
    pub is_module: bool,
    pub is_async: bool,
    pub is_defer: bool,
}
```

**DOM API Coverage**:
- `getElementById()`, `getElementsByTagName()`, `getElementsByClassName()`
- `querySelector()`, `querySelectorAll()`
- `createElement()`, `createTextNode()`, `createDocumentFragment()`
- `appendChild()`, `insertBefore()`, `removeChild()`, `replaceChild()`
- Attribute manipulation: `getAttribute()`, `setAttribute()`, `removeAttribute()`
- Class list: `add()`, `remove()`, `toggle()`, `contains()`
- Tree traversal: parent, children, siblings

### 4. js (JavaScript Runtime)

**File**: `js/src/lib.rs`

Boa-based JavaScript engine with 150+ Web APIs:

```rust
pub struct JsRuntime {
    context: Context,
    dom: Rc<Dom>,
    console_output: Rc<RefCell<Vec<String>>>,
}
```

**Core Methods**:
```rust
impl JsRuntime {
    pub fn new_with_url(dom: Rc<Dom>, url: &str) -> Result<Self>
    pub fn execute(&mut self, script: &str) -> JsResult<JsValue>
    pub fn execute_safe(&mut self, script: &str) -> Option<String>
    pub fn run_event_loop_tick(&mut self)
    pub fn fire_dom_content_loaded(&mut self)
    pub fn fire_load(&mut self)
}
```

### 5. markdown (Rendering)

**File**: `markdown/src/lib.rs`

Converts live DOM to Markdown:

```rust
pub fn render_dom_query(dom: &Dom) -> String
```

**Conversion Rules**:
- Headings: `<h1>` → `# `, `<h2>` → `## `, etc.
- Bold/Italic: `<strong>` → `**`, `<em>` → `*`
- Links: `<a href="...">` → `[text](url)`
- Images: `<img>` → `![alt](src)`
- Lists: `<ul>/<ol>` → `- ` / `1. `
- Code: `<code>` → backticks, `<pre>` → fenced blocks
- Skips: `<script>`, `<style>`, `<meta>`, `<head>`

---

## JavaScript API Implementation

### Module Overview

| Module | APIs |
|--------|------|
| `document.rs` | Document properties, methods, collections |
| `element.rs` | Element API, attributes, styles |
| `events.rs` | Event, CustomEvent, MouseEvent, KeyboardEvent, etc. |
| `event_system.rs` | addEventListener, dispatchEvent, propagation |
| `timers.rs` | setTimeout, setInterval, requestAnimationFrame, queueMicrotask |
| `fetch.rs` | fetch(), Request, Response, Headers |
| `cookies.rs` | document.cookie with domain/path scoping |
| `storage.rs` | localStorage, sessionStorage, IndexedDB |
| `network.rs` | WebSocket, EventSource, MessageChannel |
| `observers.rs` | MutationObserver, IntersectionObserver, ResizeObserver |
| `crypto.rs` | SubtleCrypto (hash, encrypt, sign, etc.) |
| `encoding.rs` | TextEncoder, TextDecoder, btoa, atob |
| `streams.rs` | ReadableStream, WritableStream |
| `intl.rs` | Intl.DateTimeFormat, NumberFormat, etc. |
| `canvas.rs` | CanvasRenderingContext2D (stub) |
| `animation.rs` | Web Animations API |
| `web_components.rs` | Custom Elements, Shadow DOM |
| `workers.rs` | Worker, SharedWorker |
| `modern.rs` | Clipboard, History, Geolocation, etc. |

### Event System

**File**: `js/src/event_system.rs`

Full DOM event propagation:

```
Capture Phase (1) → Target Phase (2) → Bubble Phase (3)
```

**Supported Options**:
- `capture`: Listen during capture phase
- `once`: Remove after first invocation
- `passive`: Cannot call preventDefault()

### Timers & Event Loop

**File**: `js/src/timers.rs`

```javascript
// All supported
setTimeout(callback, delay)
setInterval(callback, interval)
requestAnimationFrame(callback)
requestIdleCallback(callback, options)
queueMicrotask(callback)
scheduler.postTask(callback, { priority: "user-blocking" })
```

**Task Priorities**:
1. Microtasks (queueMicrotask, Promise.then)
2. Animation frames (requestAnimationFrame)
3. User-blocking tasks
4. User-visible tasks (default)
5. Background tasks
6. Idle callbacks (requestIdleCallback)

### Fetch API

**File**: `js/src/fetch.rs`

Real HTTP requests (not mocked):

```javascript
const response = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data)
});
const json = await response.json();
```

### Storage

**File**: `js/src/storage.rs`

SQLite-backed persistence:

```javascript
// localStorage & sessionStorage
localStorage.setItem('key', 'value');
const value = localStorage.getItem('key');

// IndexedDB
const request = indexedDB.open('myDB', 1);
request.onupgradeneeded = (e) => {
    const db = e.target.result;
    db.createObjectStore('store', { keyPath: 'id' });
};
```

### Cookies

**File**: `js/src/cookies.rs`

Full cookie specification support:

```javascript
document.cookie = "name=value; path=/; secure; SameSite=Lax";
```

**Features**:
- Domain/path matching
- HttpOnly respect (not exposed to JS)
- Secure flag handling
- SameSite enforcement
- Expires/Max-Age support

---

## Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        browser/main.rs                          │
│                                                                 │
│  1. CLI Argument Parsing                                        │
│  2. Initialize Logging                                          │
│  3. Execute Pipeline                                            │
└─────────────────────────┬───────────────────────────────────────┘
                          │
          ┌───────────────┼───────────────┐
          │               │               │
          ▼               ▼               ▼
    ┌───────────┐   ┌───────────┐   ┌───────────┐
    │    net    │   │    dom    │   │    js     │
    │           │   │           │   │           │
    │ • fetch() │   │ • parse() │   │ • Boa     │
    │ • cookies │   │ • query() │   │ • APIs    │
    │ • headers │   │ • modify()│   │ • events  │
    └─────┬─────┘   └─────┬─────┘   └─────┬─────┘
          │               │               │
          └───────────────┼───────────────┘
                          │
                          ▼
                    ┌───────────┐
                    │ markdown  │
                    │           │
                    │ • render  │
                    │ • format  │
                    │ • links   │
                    └─────┬─────┘
                          │
                          ▼
                   Markdown Output
```

---

## Configuration

### Environment Variables

```bash
RUST_LOG=debug          # Enable debug logging
RUST_LOG=warn           # Default: warnings only
RUST_LOG=info           # Info level
```

### Script Execution Limits

| Setting | Value | Configurable |
|---------|-------|--------------|
| Max scripts | 100 | `MAX_SCRIPTS` in main.rs |
| Max dynamic scripts | 50 | `MAX_DYNAMIC_SCRIPTS` |
| Max total bytes | 10MB | `MAX_TOTAL_SCRIPT_BYTES` |
| Max single script | 2MB | `MAX_SCRIPT_SIZE` in script_loader.rs |
| Event loop passes | 10 | `MAX_FINAL_PASSES` |

### HTTP Settings

| Setting | Value |
|---------|-------|
| User-Agent | Chrome 120 |
| Timeout | 30 seconds |
| Max redirects | 10 |
| Compression | gzip, brotli, deflate |

---

## Known Limitations

### Boa JavaScript Engine

1. **Parser OOM**: Certain minified polyfills trigger 32GB allocation bug
   - Workaround: Skip scripts with "polyfill" in URL or exactly 112,594 bytes

2. **Strict Mode**: Some `/=` patterns in minified code cause syntax errors

3. **Missing Features**:
   - No WebAssembly
   - No ES Modules (import/export)
   - Limited Proxy support

### DOM Limitations

1. **CSS Selectors**: Only simple selectors supported
   - Supported: `tag`, `.class`, `#id`, `[attr]`, `[attr=value]`
   - Not supported: Complex combinators, pseudo-classes

2. **Layout**: No CSS layout engine (no computed styles)

3. **Media**: No image/video/audio processing

### API Stubs

Some APIs return stub implementations for compatibility:
- `canvas.getContext('2d')` - Returns mock context
- `navigator.geolocation` - Returns default position
- `Notification` - Always grants permission
- `WebGL` - Not implemented

---

## Error Handling

### Non-Fatal Script Errors

Scripts fail individually without aborting the pipeline:

```rust
if let Some(error) = runtime.execute_safe(&script_content) {
    log::warn!("Script #{} failed: {}", i + 1, error);
    // Continue with next script
}
```

### Common Errors

| Error | Cause | Impact |
|-------|-------|--------|
| `ReferenceError: X is not defined` | Missing global/module | Script skipped |
| `TypeError: not a constructor` | API registered incorrectly | Fixed in events.rs |
| `SyntaxError: unexpected token` | Boa parser limitation | Script skipped |
| Memory allocation failed | Boa OOM bug | Pipeline continues |

---

## Usage

### Basic Usage

```bash
semantic-browser https://example.com
```

### With Debug Logging

```bash
RUST_LOG=debug semantic-browser https://example.com
```

### Capture to File

```bash
semantic-browser https://example.com > output.md 2>/dev/null
```

---

## Building

### Requirements

- Rust 1.70+
- OpenSSL development headers (Linux)

### Build Commands

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test
```

### Binary Location

```
target/release/semantic-browser.exe  # Windows
target/release/semantic-browser      # Linux/macOS
```

---

## Testing

### Run All Tests

```bash
cargo test
```

### Test Against Website

```bash
./target/release/semantic-browser "https://example.com"
```

### Tested Sites

| Site | Status | Notes |
|------|--------|-------|
| example.com | ✅ | Simple HTML |
| github.com | ✅ | Static + some JS |
| openai.com | ✅ | Next.js SPA (polyfill skipped) |
| vercel.com | ✅ | Next.js SPA |
| amazon.com | ✅ | Complex JS |
| cisco.com | ✅ | Enterprise site |
| facebook.com | ✅ | Custom module system |

---

## Architecture Decisions

### Why Boa?

- Pure Rust (no FFI)
- Good ES2020+ support
- Active development
- Stable ABI for embedding

### Why Real HTTP?

- Accurate cookie handling
- Real redirects
- Actual content (not mocked)
- Works with authenticated sessions

### Why SQLite for Storage?

- Persistent across runs
- Full IndexedDB support
- Atomic operations
- No external dependencies

### Why Not Headless Chrome?

- Smaller binary size
- No browser installation required
- More control over execution
- Predictable resource usage

---

## Contributing

### Key Files to Understand

1. **Pipeline**: `browser/src/main.rs`
2. **DOM Core**: `dom/src/lib.rs`
3. **JS Runtime**: `js/src/lib.rs`
4. **Document API**: `js/src/document.rs`
5. **Events**: `js/src/events.rs`, `js/src/event_system.rs`

### Adding a New Web API

1. Create module in `js/src/` (e.g., `my_api.rs`)
2. Implement using `NativeFunction::from_copy_closure`
3. Use `FunctionObjectBuilder` with `.constructor(true)` for constructors
4. Register in `js/src/lib.rs`

### Example: Adding an API

```rust
// js/src/my_api.rs
use boa_engine::{
    Context, JsResult, JsValue, NativeFunction,
    object::FunctionObjectBuilder, property::Attribute, js_string,
};

pub fn register_my_api(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        // Implementation
        Ok(JsValue::undefined())
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("MyAPI"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(
        js_string!("MyAPI"),
        ctor,
        Attribute::all()
    )?;

    Ok(())
}
```

---

## License

See LICENSE file in repository root.
