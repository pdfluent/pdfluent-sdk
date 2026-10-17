//! `redact`: permanent removal, thin over
//! [`redact`](pdfluent::PdfDocument::redact) and
//! [`redact_region`](pdfluent::PdfDocument::redact_region).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use clap::Args;
use serde_json::json;

use crate::input;
use crate::output::{self, Ctx};

#[derive(Debug, Args)]
pub struct RedactArgs {
    /// Input PDF, or `-` for stdin.
    #[arg(required = true)]
    pub input: String,

    /// Output PDF, or `-` for stdout.
    #[arg(long, short = 'o', required = true)]
    pub out: String,

    /// What to redact (exactly one).
    #[command(flatten)]
    pub mode: RedactMode,

    /// Case-sensitive text matching.
    #[arg(long)]
    pub case_sensitive: bool,

    /// Limit text search to pages, e.g. `1-3,5`.
    #[arg(long)]
    pub pages: Option<String>,

    /// Overwrite the existing output file.
    #[arg(long)]
    pub force: bool,
}

/// Redaction target: exactly one of `--search/--regex/--boxes`.
#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub struct RedactMode {
    /// Literal text to redact everywhere it appears.
    #[arg(long, value_name = "TEXT")]
    pub search: Option<String>,

    /// Regular expression to redact.
    #[arg(long, value_name = "RE")]
    pub regex: Option<String>,

    /// JSON boxes file: `[{"page": 1, "rect": [x0, y0, x1, y1]}]`
    /// (points, 1-based pages) or `{"boxes": [...]}`.
    #[arg(long, value_name = "FILE")]
    pub boxes: Option<String>,
}

pub(crate) fn run(ctx: &Ctx, args: &RedactArgs) -> u8 {
    if args.case_sensitive && args.mode.boxes.is_some() {
        return output::error(
            "redact",
            ctx,
            "BAD_USAGE",
            "--case-sensitive only applies to --search/--regex",
        );
    }
    if args.pages.is_some() && args.mode.boxes.is_some() {
        return output::error(
            "redact",
            ctx,
            "BAD_USAGE",
            "--pages only applies to --search/--regex (boxes name their own pages)",
        );
    }
    let loaded = match input::load("redact", ctx, &args.input) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let mut doc = match input::open("redact", ctx, &loaded) {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    let total = doc.page_count();
    let summary: serde_json::Value;

    if let Some(query) = args.mode.search.as_deref() {
        if query.is_empty() {
            return output::error("redact", ctx, "BAD_USAGE", "--search needs text");
        }
        let opts = match search_opts(args, total) {
            Ok(opts) => opts,
            Err((code, msg)) => return output::error("redact", ctx, &code, &msg),
        };
        if let Err(e) = doc.redact(query, opts) {
            return output::facade_error("redact", ctx, &e);
        }
        summary = json!({ "query": query, "regex": false });
    } else if let Some(pattern) = args.mode.regex.as_deref() {
        if pattern.is_empty() {
            return output::error("redact", ctx, "BAD_USAGE", "--regex needs a pattern");
        }
        let opts = match search_opts(args, total) {
            Ok(opts) => opts,
            Err((code, msg)) => return output::error("redact", ctx, &code, &msg),
        };
        let opts = opts.regex(true);
        if let Err(e) = doc.redact(pattern, opts) {
            return output::facade_error("redact", ctx, &e);
        }
        summary = json!({ "query": pattern, "regex": true });
    } else if let Some(file) = args.mode.boxes.as_deref() {
        let boxes = match read_boxes(file) {
            Ok(boxes) => boxes,
            Err(msg) => return output::error("redact", ctx, "UNREADABLE_INPUT", &msg),
        };
        if boxes.is_empty() {
            return output::error("redact", ctx, "BAD_USAGE", "boxes file names no boxes");
        }
        for (page, rect) in &boxes {
            if *page == 0 || *page > total {
                return output::error(
                    "redact",
                    ctx,
                    "PAGE_RANGE",
                    &format!("box page {page} is outside this document (1–{total})"),
                );
            }
            if let Err(e) = doc.redact_region(*page, *rect) {
                return output::facade_error("redact", ctx, &e);
            }
        }
        summary = json!({ "boxes": boxes.len() });
    } else {
        // Unreachable: clap requires exactly one mode.
        return output::error(
            "redact",
            ctx,
            "BAD_USAGE",
            "pass one of --search, --regex, or --boxes",
        );
    }

    let saved = match input::save(
        "redact",
        ctx,
        &doc,
        &args.out,
        loaded.path.as_slice(),
        args.force,
    ) {
        Ok(saved) => saved,
        Err(code) => return code,
    };
    let path = saved.path;
    output::success(
        "redact",
        ctx,
        json!({
            "input": loaded.display,
            "summary": summary,
            "output_path": path.as_ref().map(|p| p.display().to_string()),
        }),
        vec![],
        |ctx| {
            if let Some(path) = path {
                output::note(ctx, &format!("Wrote {}", path.display()));
            }
        },
    )
}

/// Builds the facade options; page-spec errors map to CLI codes.
fn search_opts(
    args: &RedactArgs,
    total: usize,
) -> Result<pdfluent::RedactOptions, (String, String)> {
    let mut opts = pdfluent::RedactOptions::new().case_sensitive(args.case_sensitive);
    if let Some(spec) = args.pages.as_deref() {
        let list = crate::pagespec::parse_list(spec, total).map_err(|e| match e {
            crate::pagespec::SpecError::Syntax(msg) => ("BAD_USAGE".to_string(), msg),
            crate::pagespec::SpecError::OutOfRange(msg) => ("PAGE_RANGE".to_string(), msg),
        })?;
        opts = opts.on_pages(&list);
    }
    Ok(opts)
}

/// Parse a boxes file: a bare array or `{"boxes": [...]}`. Rectangles are
/// `[x0, y0, x1, y1]` in PDF points; pages count from 1.
fn read_boxes(file: &str) -> Result<Vec<(usize, [f64; 4])>, String> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| format!("could not read boxes file {file}: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("boxes file {file} is not JSON: {e}"))?;
    let items = match &value {
        serde_json::Value::Array(items) => items.clone(),
        serde_json::Value::Object(map) => match map.get("boxes") {
            Some(serde_json::Value::Array(items)) => items.clone(),
            _ => {
                return Err(format!(
                    "boxes file {file}: want an array or {{\"boxes\": [...]}}"
                ))
            }
        },
        _ => {
            return Err(format!(
                "boxes file {file}: want an array or {{\"boxes\": [...]}}"
            ))
        }
    };
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let at = format!("boxes file {file}, box {i}");
        let page = item
            .get("page")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| format!("{at}: `page` must be a positive integer"))?
            as usize;
        let rect = item
            .get("rect")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("{at}: `rect` must be [x0, y0, x1, y1]"))?;
        if rect.len() != 4 {
            return Err(format!("{at}: `rect` must be [x0, y0, x1, y1]"));
        }
        let mut nums = [0.0; 4];
        for (j, num) in rect.iter().enumerate() {
            nums[j] = num
                .as_f64()
                .filter(|f| f.is_finite())
                .ok_or_else(|| format!("{at}: `rect` must be four finite numbers"))?;
        }
        if nums[0] >= nums[2] || nums[1] >= nums[3] {
            return Err(format!("{at}: `rect` needs x0 < x1 and y0 < y1"));
        }
        out.push((page, nums));
    }
    Ok(out)
}
