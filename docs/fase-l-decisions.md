# Fase L — Node.js Bindings — Autonomous Decisions

## Architecture

- **napi-rs v2** chosen over N-API raw bindings for ergonomic Rust → JS class mapping
- `pdf-node` crate produces a `cdylib` (shared library) loadable by Node.js
- `PdfDocument` wraps `pdf_engine::PdfDocument` in `Arc<>` for safe sharing across async operations
- All async methods use `tokio::task::spawn_blocking` to offload CPU-heavy work to the tokio blocking pool, keeping the Node.js event loop free

## L1 — napi-rs Bindings + Async API

### Classes exposed
- **`PdfDocument`** — Main document handle with factory methods `open()`, `openAsync()`, `openWithPassword()`
- **`PdfPage`** — Per-page handle with getters (`width`, `height`, `index`) and methods (`render`, `text`, `thumbnail`)

### Sync + Async pattern
Every heavy operation has both sync and async variants:
- `renderPage()` / `renderPageAsync()`
- `extractText()` / `extractTextAsync()`
- `open()` / `openAsync()`

Async variants return Promises and run on worker threads via `spawn_blocking`.

### Design decisions
- **`Arc<PdfDocument>`** for thread safety — napi-rs classes must be `Send`, so the inner Rust document is wrapped in `Arc`
- **Buffer output** for rendered pages — RGBA pixel data returned as `Buffer` for direct compatibility with Sharp, Jimp, and other image libraries
- **0-based page indices** — Matches JS convention (unlike PDF's 1-based convention)
- **`RenderOpts` as plain JS object** — Uses `#[napi(object)]` for ergonomic JS interop instead of a class
- `form_error` helper prepared but `#[allow(dead_code)]` since form field manipulation through Node bindings is deferred until the form engine is fully wired up

## L2 — npm Package + TypeScript + Documentation

### Package structure
- `package.json` with `@xfa-engine/pdf-node` scope, napi platform targets
- `index.js` — Platform-specific native binary loader (CommonJS)
- `index.d.ts` — Full TypeScript type definitions for all classes, interfaces, and methods
- `README.md` — Quick start, async API, Sharp integration, Express server example
- `.npmignore` — Excludes Rust source from npm package

### Platform targets
- macOS x64 + arm64 (Apple Silicon)
- Linux x64 glibc + musl (Alpine) + arm64
- Windows x64

### TypeScript typings
Hand-written `.d.ts` file matching the napi-rs generated API exactly. Covers all public interfaces: `DocumentInfo`, `BookmarkItem`, `RenderOpts`, `RenderResult`, `PageGeometry`, `PdfPage`, `PdfDocument`.

### Deferred items
- **Prebuild CI workflow**: Platform binary builds require CI runners for each target; workflow structure defined in package.json scripts but actual GitHub Actions workflow deferred to when CI is set up
- **npm publish**: Requires npm account and token; package.json is publish-ready
- **Benchmarks vs pdfjs-dist/pdf-lib**: Deferred to when real PDF test fixtures are available
