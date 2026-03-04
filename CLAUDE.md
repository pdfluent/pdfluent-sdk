# XFA-Native-Rust — Project Conventions

## Project Overview
High-performance XFA (XML Forms Architecture) engine in Rust. Goal: full Adobe Reader parity for XFA forms, including dynamic reflow and FormCalc scripting.

## Source of Truth
- **XFA 3.3 Spec:** https://pdfa.org/norm-refs/XFA-3_3.pdf
- **FormCalc Reference:** https://helpx.adobe.com/pdf/aem-forms/6-2/formcalc-reference.pdf
- **Backlog:** See BACKLOG.md for structured epics and implementation order

## Architecture
Cargo workspace with 6 crates:
- `xfa-dom-resolver` — SOM path resolution, Template/Data DOM (XFA §3)
- `formcalc-interpreter` — FormCalc lexer, parser, interpreter (XFA §25)
- `xfa-layout-engine` — Box Model, pagination, reflow (XFA §4, §8)
- `pdfium-ffi-bridge` — PDFium FFI, rendering, UI events
- `xfa-golden-tests` — Visual regression testing pipeline
- `xfa-cli` — CLI entry point

## Tech Stack
- **Rust 2021 edition**, stable toolchain
- `roxmltree` for XML parsing (read-only DOM)
- `pdfium-render` for PDF rendering via PDFium
- `thiserror` for error types

## Coding Conventions
- Use `cargo fmt` before every commit
- Use `cargo clippy -- -D warnings` — no warnings allowed
- All public APIs must have doc comments
- Error handling: use `thiserror` + `Result<T, Error>`, no `.unwrap()` in library code
- Tests: `#[cfg(test)]` modules in each file + integration tests in `tests/`

## Autonomy Principle
Claude must be fully self-sufficient:
- Run all tests via `cargo test`
- Render PDFs to PNG and inspect visually (vision) for layout debugging
- Consult the XFA spec PDF directly for architectural decisions
- Never require human intervention for verification

## Git Workflow
- `master` branch for stable code
- Feature branches: `epic-N/description` (e.g., `epic-1/som-path-resolver`)
- Conventional commit messages in English
- Use `commit-commands` plugin for commits
