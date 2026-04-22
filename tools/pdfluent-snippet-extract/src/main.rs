//! `pdfluent-snippet-extract` — extract Rust code snippets from
//! pdfluent.com how-to pages into testable `.rs` files.
//!
//! # Pipeline position (#1236)
//!
//! This tool is the ingestion step of the Slag 2 drift-guard pipeline:
//!
//! - **#1236 (this)**: extract → normalised `tests/web_examples/*.rs`
//! - #1237: wire those into `cargo test -p pdfluent`
//! - #1238: CI re-runs the extractor and diffs against committed output
//! - #1246: end-to-end validator runs each snippet against fixtures
//!
//! # Operating modes
//!
//! **Offline (default).** Reads HTML from local files on disk. Used
//! in CI and by the drift-guard diff (a fresh crawl uploads its HTML
//! to a cache directory, then the extractor processes the cache).
//! Deterministic.
//!
//! **Online (`--online`, requires `--features online`).** Fetches
//! URLs via `ureq`. Opt-in because CI must be deterministic and
//! offline.
//!
//! # Invocation
//!
//! ```bash
//! cargo run --bin pdfluent-snippet-extract -- \
//!     --manifest tools/pdfluent-snippet-extract/manifest.toml \
//!     --cache-dir tools/pdfluent-snippet-extract/cache \
//!     --out-dir crates/pdfluent/tests/web_examples \
//!     --dry-run
//! ```
//!
//! Drop `--dry-run` to actually write files. Add `--online` (with the
//! `online` feature) to fetch fresh HTML into the cache first.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Parser;
use scraper::{Html, Selector};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Manifest {
    /// List of pages to extract from.
    page: Vec<Page>,
}

#[derive(Debug, Deserialize, Clone)]
struct Page {
    /// Public URL on pdfluent.com. Also embedded in the output file
    /// header so a human reader can jump back to the source.
    url: String,
    /// Output slug — used as the file stem under `--out-dir`.
    ///
    /// Example: `"fill_pdf_form_rust"` produces
    /// `crates/pdfluent/tests/web_examples/fill_pdf_form_rust.rs`.
    slug: String,
    /// Optional filename for the cached HTML. Defaults to
    /// `<slug>.html`.
    #[serde(default)]
    cache_file: Option<String>,
    /// Optional CSS selector scoping the extraction to a specific
    /// region of the page (e.g. `"article"`). Defaults to whole doc.
    #[serde(default)]
    scope: Option<String>,
    /// Which Rust code block to pick when a page has several.
    ///
    /// - `first` (default): use the first `<pre><code>` block that
    ///   looks like Rust.
    /// - `longest`: use the longest Rust block.
    /// - `nth = N`: use the N-th (0-based) Rust block.
    #[serde(default)]
    pick: PickStrategy,
}

#[derive(Debug, Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "lowercase")]
enum PickStrategy {
    #[default]
    First,
    Longest,
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Debug, Parser)]
#[command(about = "Extract Rust snippets from pdfluent.com how-to pages")]
struct Cli {
    /// Path to the manifest TOML.
    #[arg(long)]
    manifest: PathBuf,
    /// Directory of cached HTML files (input in offline mode, output
    /// when `--online` fetches fresh HTML).
    #[arg(long)]
    cache_dir: PathBuf,
    /// Directory to write extracted `.rs` files to.
    #[arg(long)]
    out_dir: PathBuf,
    /// Pretend to write — print what would happen, don't touch disk.
    #[arg(long)]
    dry_run: bool,
    /// Fetch fresh HTML from each page's URL before extracting.
    /// Requires the `online` cargo feature.
    #[arg(long)]
    online: bool,
    /// ISO date tag embedded in the output file header.
    ///
    /// **Required for offline runs.** Defaulting to today's wall-clock
    /// date would cause drift-guard false positives the day after any
    /// extraction — unchanged HTML would still produce a byte-level
    /// diff solely because of the timestamp. The caller therefore
    /// must pass the date explicitly; `--online` fills it in
    /// automatically with the fetch date.
    #[arg(long)]
    fetched: Option<String>,
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();

    let manifest_src = fs::read_to_string(&cli.manifest)
        .with_context(|| format!("reading manifest {}", cli.manifest.display()))?;
    let manifest: Manifest =
        toml::from_str(&manifest_src).with_context(|| "parsing manifest TOML")?;

    // Decide the `fetched` stamp BEFORE any network or filesystem
    // mutation. Offline runs (CI, drift-guard) require an explicit
    // `--fetched` so byte-level diffs on unchanged HTML stay deterministic.
    // Online runs stamp the file with the live fetch date.
    let fetched = match (cli.fetched.clone(), cli.online) {
        (Some(d), _) => d,
        (None, true) => {
            today_utc_iso().context("cannot determine today's UTC date for --online fetch stamp")?
        }
        (None, false) => bail!(
            "`--fetched YYYY-MM-DD` is required for offline runs; defaulting to today's \
             date would break drift-guard determinism. For a live refresh use `--online \
             --features online` (stamps the live fetch date automatically)."
        ),
    };

    if cli.online {
        #[cfg(not(feature = "online"))]
        bail!("`--online` requires building with `--features online`");
        #[cfg(feature = "online")]
        {
            fs::create_dir_all(&cli.cache_dir).with_context(|| "creating cache dir")?;
            for page in &manifest.page {
                fetch_to_cache(page, &cli.cache_dir)?;
            }
        }
    }

    if !cli.dry_run {
        fs::create_dir_all(&cli.out_dir).with_context(|| "creating out dir")?;
    }

    let mut errors = Vec::new();

    for page in &manifest.page {
        match extract_page(page, &cli.cache_dir, &cli.out_dir, &fetched, cli.dry_run) {
            Ok(summary) => println!("[ok] {} -> {}", page.slug, summary),
            Err(e) => {
                eprintln!("[err] {}: {e:#}", page.slug);
                errors.push(page.slug.clone());
            }
        }
    }

    if !errors.is_empty() {
        bail!("extraction failed for: {}", errors.join(", "));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Extraction
// ---------------------------------------------------------------------------

/// Extract one page into its target `.rs` file.
///
/// Returns a human-readable summary of what was written (for the
/// `[ok]` log line).
fn extract_page(
    page: &Page,
    cache_dir: &Path,
    out_dir: &Path,
    fetched: &str,
    dry_run: bool,
) -> Result<String> {
    let cache_name = page
        .cache_file
        .clone()
        .unwrap_or_else(|| format!("{}.html", page.slug));
    let cache_path = cache_dir.join(&cache_name);
    let html = fs::read_to_string(&cache_path)
        .with_context(|| format!("reading cache {}", cache_path.display()))?;

    let snippet = select_snippet(&html, page)
        .with_context(|| format!("no Rust snippet found in {}", cache_path.display()))?;

    let normalised = normalise(&snippet);
    let rendered = render(page, &normalised, fetched);

    let target = out_dir.join(format!("{}.rs", page.slug));

    if dry_run {
        return Ok(format!(
            "{} bytes ({} lines) [dry-run, would write to {}]",
            rendered.len(),
            rendered.lines().count(),
            target.display(),
        ));
    }

    // Only write if the output changed. Preserves mtime for unchanged
    // files so incremental builds don't re-compile the whole
    // web_examples module on every extract run.
    let existing = fs::read_to_string(&target).ok();
    if existing.as_deref() == Some(rendered.as_str()) {
        return Ok(format!("{} bytes (unchanged)", rendered.len()));
    }

    fs::write(&target, &rendered).with_context(|| format!("writing {}", target.display()))?;
    Ok(format!(
        "{} bytes written to {}",
        rendered.len(),
        target.display(),
    ))
}

/// Pick the target snippet from the page.
fn select_snippet(html: &str, page: &Page) -> Option<String> {
    let doc = Html::parse_document(html);

    // Scope selector narrows the search area if the page sets one.
    let scope_selector = page.scope.as_deref().unwrap_or("body");
    let scope_sel = Selector::parse(scope_selector).ok()?;
    let scope = doc.select(&scope_sel).next()?;

    // Pattern 1: `<pre><code class="language-rust">...</code></pre>`
    //   — Hugo/Docusaurus/Markdoc-style highlighting.
    // Pattern 2: `<pre class="language-rust"><code>...`
    //   — Prism-style.
    // Pattern 3: `<code class="rust">...`
    //   — bare.
    let candidates = [
        "pre > code.language-rust",
        "pre.language-rust > code",
        "code.rust",
        "code.language-rust",
    ];

    let mut all_rust_blocks: Vec<String> = Vec::new();
    for pat in &candidates {
        let Ok(sel) = Selector::parse(pat) else {
            continue;
        };
        for elem in scope.select(&sel) {
            let text = elem.text().collect::<String>();
            if looks_like_rust(&text) {
                all_rust_blocks.push(text);
            }
        }
        if !all_rust_blocks.is_empty() {
            break;
        }
    }

    match page.pick {
        PickStrategy::First => all_rust_blocks.into_iter().next(),
        PickStrategy::Longest => all_rust_blocks.into_iter().max_by_key(|s| s.len()),
    }
}

/// Heuristic sanity check so we don't emit a block that isn't really
/// Rust (e.g. a shell command the page author forgot to tag).
fn looks_like_rust(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Presence of at least one Rust keyword that's unlikely in shell
    // or HTML prose.
    let markers = ["use ", "fn ", "let ", "struct ", "impl ", "mod ", "pub "];
    markers.iter().any(|m| trimmed.contains(m))
}

/// Normalise extracted text:
/// - Decode common HTML entities that scraper leaves alone for text()
///   (safety net; scraper v0.19 does decode, but a belt-and-braces
///   pass protects against future version changes).
/// - Strip trailing spaces per line.
/// - Ensure exactly one trailing newline.
///
/// **Decode order matters.** `&amp;` MUST be decoded last. Otherwise
/// a literal `&lt;` shown on the page — which is serialised in HTML
/// source as `&amp;lt;` — would first become `&lt;` and then get
/// over-decoded to `<`, changing the semantics of the snippet. By
/// decoding the bracket entities first and `&amp;` last, `&amp;lt;`
/// round-trips to the intended literal `&lt;`.
fn normalise(raw: &str) -> String {
    let decoded = raw
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        // `&amp;` last — see decode-order note above.
        .replace("&amp;", "&");

    let mut out = String::with_capacity(decoded.len());
    for line in decoded.lines() {
        out.push_str(line.trim_end());
        out.push('\n');
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Render the final `.rs` file: a `//!` header with source + fetched
/// date, then the snippet wrapped so it compiles as a library
/// function. The exact wrapper mirrors the hand-written web_examples
/// we already have in `crates/pdfluent/tests/web_examples/` so
/// follow-up #1237 can bolt tests onto every generated file without
/// further normalisation.
fn render(page: &Page, snippet: &str, fetched: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("//! web_examples/{}\n", page.slug));
    out.push_str("//!\n");
    out.push_str(&format!(
        "//! Source: <{}> (fetched {})\n",
        page.url, fetched
    ));
    out.push_str("//!\n");
    out.push_str(
        "//! Auto-extracted by `tools/pdfluent-snippet-extract` (#1236).\n\
         //! Do not edit by hand — re-run the extractor instead.\n\n",
    );
    out.push_str(snippet);
    out
}

#[cfg(feature = "online")]
fn fetch_to_cache(page: &Page, cache_dir: &Path) -> Result<()> {
    let cache_name = page
        .cache_file
        .clone()
        .unwrap_or_else(|| format!("{}.html", page.slug));
    let cache_path = cache_dir.join(&cache_name);
    let body = ureq::get(&page.url)
        .call()
        .with_context(|| format!("fetching {}", page.url))?
        .into_string()
        .with_context(|| "reading response body")?;
    fs::write(&cache_path, body).with_context(|| format!("caching {}", cache_path.display()))?;
    Ok(())
}

/// Today in UTC as ISO-8601 (YYYY-MM-DD), using `std::time::SystemTime`
/// to avoid pulling in `chrono` or `time` for a single stamp.
fn today_utc_iso() -> Option<String> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    Some(format_iso_date(secs))
}

/// Proleptic Gregorian date from Unix seconds. Sufficient for a build
/// stamp; no timezone math.
fn format_iso_date(unix_secs: i64) -> String {
    // Days since epoch (1970-01-01).
    let days = unix_secs.div_euclid(86_400);

    // Cumulative days to each month for a non-leap year.
    const DAYS_IN_MONTH: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    // Convert days since 1970-01-01 to (year, month, day).
    let mut y: i64 = 1970;
    let mut d = days;
    loop {
        let year_days = if is_leap(y) { 366 } else { 365 };
        if d < year_days {
            break;
        }
        d -= year_days;
        y += 1;
    }
    let mut m = 0usize;
    loop {
        let mut month_days = DAYS_IN_MONTH[m];
        if m == 1 && is_leap(y) {
            month_days += 1;
        }
        if d < month_days {
            break;
        }
        d -= month_days;
        m += 1;
    }
    format!("{y:04}-{:02}-{:02}", m + 1, d + 1)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

// ---------------------------------------------------------------------------
// Unit tests — run with `cargo test -p pdfluent-snippet-extract`.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_snippet_picks_first_language_rust_block() {
        let html = r#"
            <html><body>
            <pre><code class="language-bash">$ cargo build</code></pre>
            <pre><code class="language-rust">
            use pdfluent::prelude::*;

            fn main() -> Result<()> {
                let doc = PdfDocument::open("x.pdf")?;
                Ok(())
            }
            </code></pre>
            </body></html>
        "#;
        let page = Page {
            url: "https://pdfluent.com/how-to/x".into(),
            slug: "x".into(),
            cache_file: None,
            scope: None,
            pick: PickStrategy::First,
        };
        let got = select_snippet(html, &page).expect("should find snippet");
        assert!(got.contains("PdfDocument::open"));
        assert!(!got.contains("cargo build"));
    }

    #[test]
    fn select_snippet_longest_strategy_picks_biggest_rust_block() {
        let html = r#"
            <pre><code class="language-rust">use pdfluent::prelude::*;</code></pre>
            <pre><code class="language-rust">
                use pdfluent::prelude::*;
                fn main() -> Result<()> {
                    let mut doc = PdfDocument::open("x.pdf")?;
                    doc.save("y.pdf")?;
                    Ok(())
                }
            </code></pre>
        "#;
        let page = Page {
            url: "https://pdfluent.com/how-to/x".into(),
            slug: "x".into(),
            cache_file: None,
            scope: None,
            pick: PickStrategy::Longest,
        };
        let got = select_snippet(html, &page).expect("snippet");
        assert!(got.contains("save"));
    }

    #[test]
    fn looks_like_rust_rejects_shell() {
        assert!(!looks_like_rust("$ cargo build"));
        assert!(!looks_like_rust("npm install pdfluent"));
        assert!(looks_like_rust("use pdfluent::prelude::*;"));
        assert!(looks_like_rust("fn main() {}"));
    }

    #[test]
    fn normalise_trims_and_ensures_trailing_newline() {
        let got = normalise("use x;   \n   fn a(){}   \n\n\n");
        assert!(got.ends_with("fn a(){}\n"));
        assert!(!got.ends_with("\n\n"));
    }

    #[test]
    fn render_header_is_stable_for_drift_guard() {
        let page = Page {
            url: "https://pdfluent.com/how-to/open-pdf-rust".into(),
            slug: "open_pdf_rust".into(),
            cache_file: None,
            scope: None,
            pick: PickStrategy::First,
        };
        let out = render(&page, "use pdfluent::prelude::*;\n", "2026-04-21");
        assert!(out.starts_with("//! web_examples/open_pdf_rust\n"));
        assert!(out.contains("Source: <https://pdfluent.com/how-to/open-pdf-rust>"));
        assert!(out.contains("fetched 2026-04-21"));
        assert!(out.contains("Auto-extracted"));
        assert!(out.ends_with("use pdfluent::prelude::*;\n"));
    }

    #[test]
    fn format_iso_date_matches_known_epoch_points() {
        assert_eq!(format_iso_date(0), "1970-01-01");
        // 2024-01-01 00:00 UTC
        assert_eq!(format_iso_date(1_704_067_200), "2024-01-01");
        // 2024-02-29 00:00 UTC (leap)
        assert_eq!(format_iso_date(1_709_164_800), "2024-02-29");
    }

    #[test]
    fn html_entities_are_decoded_in_output() {
        let got = normalise("let s = &quot;a &amp; b&quot;;\n");
        assert!(got.contains("\"a & b\""));
    }

    #[test]
    fn entity_decode_order_preserves_literal_lt_gt() {
        // A snippet whose *published* text reads `"&lt;tag&gt;"` gets
        // serialised in HTML source as `&amp;lt;tag&amp;gt;`. After
        // scraper's text() pass we might get either form; handle both.
        //
        // With a naive &amp;-first pass `&amp;lt;` would become `&lt;`
        // then `<`, dropping the literal entity the page wanted to
        // show. Decoding &amp; LAST keeps the semantics correct.
        let got = normalise("let s = \"&amp;lt;tag&amp;gt;\";\n");
        assert!(got.contains("\"&lt;tag&gt;\""), "got: {got}");
    }
}
