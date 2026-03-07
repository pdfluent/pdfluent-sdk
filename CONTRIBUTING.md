# Contributing to XFA-Native-Rust

## Fix & Retest Workflow

When fixing bugs discovered by the corpus test runner, follow this process:

### 1. Pick a cluster

Use `xfa-test-runner clusters` to find the highest-impact error cluster. Prioritize panics first, then failures with the most affected PDFs.

### 2. Reproduce locally

```bash
xfa-test-runner download-examples \
  --test parse --category invalid_xref \
  --output tests/regression/fixtures/ --limit 5
```

Run the failing test against the smallest reproducer to confirm the issue.

### 3. Understand root cause

Read the relevant ISO 32000-2 or ISO 19005 spec section. Understand the full feature, not just the subset that triggers the bug.

### 4. Implement a generic fix

- Implement the WHOLE feature, not a special-case hack for one PDF
- No `if pdf_matches_this_specific_pattern { special_case }` workarounds
- No `unwrap()` or `todo!()` in library code
- No fixes without understanding the root cause

### 5. Add regression tests

Add 3-5 example PDFs as regression tests in the relevant crate's `tests/` directory:

```rust
#[test]
fn regression_cl0042_cmap_format6() {
    let pdf_data = include_bytes!("fixtures/cl0042_govdocs_023456.pdf");
    let text = pdf_extract::extract_text(pdf_data).expect("should not panic");
    assert!(!text.is_empty(), "should extract non-empty text");
}
```

Fixture rules:
- Keep PDFs small (< 100KB per fixture, max 5 per cluster)
- Naming: `cl{NNNN}_{source}_{hash_prefix}.pdf`
- Store in `tests/regression/fixtures/` per crate

### 6. Verify

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
```

### 7. Compare runs

After merging, run the corpus again and compare:

```bash
xfa-test-runner compare --run-a run-before --run-b run-after --db results.sqlite
```

The compare command will:
- Show per-test pass rate deltas
- List resolved and new clusters
- Exit non-zero if a regression is detected (more new clusters than resolved)

### Principles

- One fix per PR — don't mix unrelated changes
- Every fix must have regression tests
- A fix that introduces new failures is a regression
- False positives (we're stricter than the spec) are acceptable
- False negatives (we miss spec violations) are bugs
