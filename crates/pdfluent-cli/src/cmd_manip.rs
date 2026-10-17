//! `merge`, `split`, and `pages`: page-level document operations.
//!
//! Thin over [`PdfMerger`](pdfluent::PdfMerger),
//! [`extract_pages`](pdfluent::PdfDocument::extract_pages), and
//! [`rotate_page`](pdfluent::PdfDocument::rotate_page). The facade exposes no
//! delete or reorder entry point, so `--delete` and `--reorder` compose the
//! two it does expose — extract plus merge — with no PDF logic of their own.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::PathBuf;

use clap::Args;
use serde_json::json;

use crate::input;
use crate::output::{self, Ctx};
use crate::pagespec::{self, SpecError};

// ---------------------------------------------------------------------------
// merge
// ---------------------------------------------------------------------------

/// Bookmark handling when merging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Bookmarks {
    /// Each source keeps its bookmarks under a top-level entry.
    Concat,
    /// All bookmarks flattened into one top-level sequence.
    /// Refused until the facade supports it (it renders concat-like today).
    Flatten,
    /// Bookmarks dropped. Refused until the facade supports it.
    Discard,
}

impl From<Bookmarks> for pdfluent::BookmarkMergeStrategy {
    fn from(value: Bookmarks) -> Self {
        match value {
            Bookmarks::Concat => pdfluent::BookmarkMergeStrategy::Concat,
            Bookmarks::Flatten => pdfluent::BookmarkMergeStrategy::FlattenAll,
            Bookmarks::Discard => pdfluent::BookmarkMergeStrategy::Discard,
        }
    }
}

#[derive(Debug, Args)]
pub struct MergeArgs {
    /// Input PDFs in merge order (`-` reads one input from stdin).
    #[arg(required = true)]
    pub inputs: Vec<String>,

    /// Output PDF, or `-` for stdout.
    #[arg(long, short = 'o', required = true)]
    pub out: String,

    /// How to combine bookmarks.
    #[arg(long, value_enum, default_value = "concat")]
    pub bookmarks: Bookmarks,

    /// Overwrite the existing output file.
    #[arg(long)]
    pub force: bool,
}

pub(crate) fn run_merge(ctx: &Ctx, args: &MergeArgs) -> u8 {
    let unsupported = match args.bookmarks {
        Bookmarks::Concat => None,
        Bookmarks::Flatten => Some("flatten"),
        Bookmarks::Discard => Some("discard"),
    };
    if let Some(which) = unsupported {
        return output::error(
            "merge",
            ctx,
            "BOOKMARKS_NOT_SUPPORTED",
            &format!(
                "merge --bookmarks {which} is not supported by the `pdfluent` facade (facade gap, reported rather than re-implemented): the facade renders every strategy concat-like. Only --bookmarks concat is supported today"
            ),
        );
    }
    if args.inputs.iter().filter(|i| i.as_str() == "-").count() > 1 {
        return output::error(
            "merge",
            ctx,
            "BAD_USAGE",
            "only one input can be `-` (stdin holds a single document)",
        );
    }
    let mut merger = pdfluent::PdfMerger::new().with_bookmarks(args.bookmarks.into());
    let mut names = Vec::with_capacity(args.inputs.len());
    let mut input_paths = Vec::with_capacity(args.inputs.len());
    for raw in &args.inputs {
        let loaded = match input::load("merge", ctx, raw) {
            Ok(loaded) => loaded,
            Err(code) => return code,
        };
        let doc = match input::open("merge", ctx, &loaded) {
            Ok(doc) => doc,
            Err(code) => return code,
        };
        output::verbose(
            ctx,
            &format!("merge: {} ({} pages)", loaded.display, doc.page_count()),
        );
        names.push(loaded.display);
        if let Some(path) = loaded.path {
            input_paths.push(path);
        }
        merger = merger.add(doc);
    }
    let merged = match merger.build() {
        Ok(doc) => doc,
        Err(e) => return output::facade_error("merge", ctx, &e),
    };
    let page_count = merged.page_count();
    let saved = match input::save("merge", ctx, &merged, &args.out, &input_paths, args.force) {
        Ok(saved) => saved,
        Err(code) => return code,
    };
    let path = saved.path;
    output::success(
        "merge",
        ctx,
        json!({
            "inputs": names,
            "page_count": page_count,
            "output_path": path.as_ref().map(|p| p.display().to_string()),
        }),
        vec![],
        |ctx| {
            if let Some(path) = path {
                output::note(
                    ctx,
                    &format!("Wrote {} ({page_count} pages)", path.display()),
                );
            }
        },
    )
}

// ---------------------------------------------------------------------------
// split
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
pub struct SplitArgs {
    /// Input PDF, or `-` for stdin.
    #[arg(required = true)]
    pub input: String,

    /// What to split by (exactly one).
    #[command(flatten)]
    pub mode: SplitMode,

    /// Output directory (created when missing).
    #[arg(long, short = 'o', required = true)]
    pub out: String,

    /// Overwrite existing part files.
    #[arg(long)]
    pub force: bool,
}

/// Split mode: `--pages` or `--every`, exactly one (clap-enforced).
#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub struct SplitMode {
    /// Pages to split out, e.g. `1-3,5`: one file per item.
    #[arg(long)]
    pub pages: Option<String>,

    /// Split into chunks of N pages.
    #[arg(long, value_name = "N")]
    pub every: Option<usize>,
}

pub(crate) fn run_split(ctx: &Ctx, args: &SplitArgs) -> u8 {
    let every = match args.mode.every {
        Some(0) => {
            return output::error("split", ctx, "BAD_USAGE", "--every must be at least 1");
        }
        every => every,
    };
    let loaded = match input::load("split", ctx, &args.input) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let doc = match input::open("split", ctx, &loaded) {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    let total = doc.page_count();
    let ranges: Vec<(usize, usize)> = match (&args.mode.pages, every) {
        (Some(spec), _) => match pagespec::parse_ranges(spec) {
            Ok(ranges) => ranges,
            Err(SpecError::Syntax(msg)) => return output::error("split", ctx, "BAD_USAGE", &msg),
            Err(SpecError::OutOfRange(msg)) => {
                return output::error("split", ctx, "PAGE_RANGE", &msg);
            }
        },
        (None, Some(n)) => pagespec::chunks(total, n),
        // Unreachable: clap requires exactly one of --pages/--every.
        (None, None) => {
            return output::error("split", ctx, "BAD_USAGE", "pass --pages or --every");
        }
    };
    match pagespec::check_top(&ranges, total) {
        Err(SpecError::OutOfRange(msg)) | Err(SpecError::Syntax(msg)) => {
            return output::error("split", ctx, "PAGE_RANGE", &msg);
        }
        Ok(()) => {}
    }
    let dir = PathBuf::from(&args.out);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return output::error(
            "split",
            ctx,
            "IO",
            &format!("could not create directory {}: {e}", dir.display()),
        );
    }
    // Every part path is known before anything is written, so a refusal
    // names the file and leaves no half-split directory behind.
    let targets: Vec<PathBuf> = ranges
        .iter()
        .map(|(lo, hi)| {
            dir.join(if lo == hi {
                format!("page-{lo}.pdf")
            } else {
                format!("pages-{lo}-{hi}.pdf")
            })
        })
        .collect();
    for target in &targets {
        if let Err(code) =
            input::guard_output("split", ctx, loaded.path.as_slice(), target, args.force)
        {
            return code;
        }
    }
    let mut parts = Vec::with_capacity(ranges.len());
    for ((lo, hi), target) in ranges.iter().zip(&targets) {
        let part = match doc.extract_pages(*lo..=*hi) {
            Ok(part) => part,
            Err(e) => return output::facade_error("split", ctx, &e),
        };
        if let Err(code) = input::save_doc("split", ctx, &part, target) {
            return code;
        }
        output::verbose(
            ctx,
            &format!("split: pages {lo}-{hi} -> {}", target.display()),
        );
        parts.push(json!({
            "pages": pagespec::render_range((*lo, *hi)),
            "page_count": part.page_count(),
            "output_path": target.display().to_string(),
        }));
    }
    let written: Vec<String> = parts
        .iter()
        .filter_map(|p| p["output_path"].as_str().map(str::to_string))
        .collect();
    output::success(
        "split",
        ctx,
        json!({ "input": loaded.display, "parts": parts }),
        vec![],
        |ctx| {
            for path in &written {
                output::note(ctx, &format!("Wrote {path}"));
            }
        },
    )
}

// ---------------------------------------------------------------------------
// pages
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
pub struct PagesArgs {
    /// Input PDF, or `-` for stdin.
    #[arg(required = true)]
    pub input: String,

    /// The operation (exactly one).
    #[command(flatten)]
    pub mode: PagesMode,

    /// Pages `--rotate` applies to. Default: every page.
    #[arg(long, value_name = "SPEC")]
    pub pages: Option<String>,

    /// Output PDF, or `-` for stdout.
    #[arg(long, short = 'o', required = true)]
    pub out: String,

    /// Overwrite the existing output file.
    #[arg(long)]
    pub force: bool,
}

/// Page operation: exactly one of `--rotate/--delete/--reorder/--extract`.
#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub struct PagesMode {
    /// Rotate pages by degrees: 90, 180, 270 (negatives allowed).
    /// Applies to `--pages`, or every page without it.
    #[arg(long, value_name = "DEG")]
    pub rotate: Option<i32>,

    /// Delete pages, e.g. `2,5-7`.
    #[arg(long, value_name = "SPEC")]
    pub delete: Option<String>,

    /// New page order, e.g. `3,1,2`: every page exactly once.
    #[arg(long, value_name = "SPEC")]
    pub reorder: Option<String>,

    /// Keep only these pages, e.g. `1-3,5`.
    #[arg(long, value_name = "SPEC")]
    pub extract: Option<String>,
}

pub(crate) fn run_pages(ctx: &Ctx, args: &PagesArgs) -> u8 {
    if args.pages.is_some() && args.mode.rotate.is_none() {
        // `--pages` without `--rotate` names nothing: the other operations
        // take their own spec. Refused before any work, rather than silently
        // ignored.
        return output::error(
            "pages",
            ctx,
            "BAD_USAGE",
            "--pages only applies to --rotate; --delete/--reorder/--extract take their spec directly",
        );
    }
    let loaded = match input::load("pages", ctx, &args.input) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let mut doc = match input::open("pages", ctx, &loaded) {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    let total = doc.page_count();

    let op: &str;
    if let Some(degrees) = args.mode.rotate {
        op = "rotate";
        let rotation = match degrees {
            90 | -270 => pdfluent::Rotation::Clockwise90,
            180 | -180 => pdfluent::Rotation::Clockwise180,
            270 | -90 => pdfluent::Rotation::Clockwise270,
            _ => {
                return output::error(
                    "pages",
                    ctx,
                    "BAD_USAGE",
                    &format!("--rotate takes 90, 180, or 270 (negatives allowed), not {degrees}"),
                );
            }
        };
        let targets: Vec<usize> = match args.pages.as_deref() {
            Some(spec) => match pagespec::parse_list(spec, total) {
                Ok(list) => list,
                Err(SpecError::Syntax(msg)) => {
                    return output::error("pages", ctx, "BAD_USAGE", &msg);
                }
                Err(SpecError::OutOfRange(msg)) => {
                    return output::error("pages", ctx, "PAGE_RANGE", &msg);
                }
            },
            None => (1..=total).collect(),
        };
        for page in targets {
            if let Err(e) = doc.rotate_page(page, rotation) {
                return output::facade_error("pages", ctx, &e);
            }
        }
    } else if let Some(spec) = args.mode.delete.as_deref() {
        op = "delete";
        let doomed = match pagespec::parse_list(spec, total) {
            Ok(list) => list,
            Err(SpecError::Syntax(msg)) => return output::error("pages", ctx, "BAD_USAGE", &msg),
            Err(SpecError::OutOfRange(msg)) => {
                return output::error("pages", ctx, "PAGE_RANGE", &msg);
            }
        };
        let keep = pagespec::complement(total, &doomed);
        if keep.is_empty() {
            return output::error(
                "pages",
                ctx,
                "EMPTY_RESULT",
                "deleting every page leaves an empty document; nothing was written",
            );
        }
        doc = match keep_as_document("pages", ctx, &doc, &keep) {
            Ok(doc) => doc,
            Err(code) => return code,
        };
    } else if let Some(spec) = args.mode.reorder.as_deref() {
        op = "reorder";
        let flat = match pagespec::parse_ranges(spec) {
            Ok(ranges) => ranges
                .into_iter()
                .flat_map(|(lo, hi)| lo..=hi)
                .collect::<Vec<_>>(),
            Err(SpecError::Syntax(msg)) => return output::error("pages", ctx, "BAD_USAGE", &msg),
            Err(SpecError::OutOfRange(_)) => {
                return output::error("pages", ctx, "BAD_USAGE", "unreachable");
            }
        };
        let order = match pagespec::check_permutation(&flat, total) {
            Ok(order) => order,
            Err(SpecError::Syntax(msg)) => return output::error("pages", ctx, "BAD_USAGE", &msg),
            Err(SpecError::OutOfRange(msg)) => {
                return output::error("pages", ctx, "PAGE_RANGE", &msg);
            }
        };
        let mut merger = pdfluent::PdfMerger::new();
        for page in &order {
            match doc.extract_pages(*page..=*page) {
                Ok(part) => merger = merger.add(part),
                Err(e) => return output::facade_error("pages", ctx, &e),
            }
        }
        match merger.build() {
            Ok(rebuilt) => doc = rebuilt,
            Err(e) => return output::facade_error("pages", ctx, &e),
        }
    } else if let Some(spec) = args.mode.extract.as_deref() {
        op = "extract";
        let ranges = match pagespec::parse_ranges(spec) {
            Ok(ranges) => ranges,
            Err(SpecError::Syntax(msg)) => return output::error("pages", ctx, "BAD_USAGE", &msg),
            Err(SpecError::OutOfRange(_)) => {
                return output::error("pages", ctx, "BAD_USAGE", "unreachable");
            }
        };
        if let Err(e) = pagespec::check_top(&ranges, total) {
            let msg = match e {
                SpecError::Syntax(msg) | SpecError::OutOfRange(msg) => msg,
            };
            return output::error("pages", ctx, "PAGE_RANGE", &msg);
        }
        doc = match keep_as_document("pages", ctx, &doc, &ranges) {
            Ok(doc) => doc,
            Err(code) => return code,
        };
    } else {
        // Unreachable: clap requires exactly one operation.
        return output::error(
            "pages",
            ctx,
            "BAD_USAGE",
            "pass one of --rotate, --delete, --reorder, or --extract",
        );
    }

    let page_count = doc.page_count();
    let saved = match input::save(
        "pages",
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
        "pages",
        ctx,
        json!({
            "input": loaded.display,
            "op": op,
            "page_count": page_count,
            "output_path": path.as_ref().map(|p| p.display().to_string()),
        }),
        vec![],
        |ctx| {
            if let Some(path) = path {
                output::note(
                    ctx,
                    &format!("Wrote {} ({page_count} pages)", path.display()),
                );
            }
        },
    )
}

/// One range extracts directly; several extract-and-merge through the facade.
fn keep_as_document(
    command: &str,
    ctx: &Ctx,
    doc: &pdfluent::PdfDocument,
    ranges: &[(usize, usize)],
) -> Result<pdfluent::PdfDocument, u8> {
    // Called after validation; ranges is never empty here.
    if ranges.len() == 1 {
        let (lo, hi) = ranges[0];
        return doc
            .extract_pages(lo..=hi)
            .map_err(|e| output::facade_error(command, ctx, &e));
    }
    let mut merger = pdfluent::PdfMerger::new();
    for (lo, hi) in ranges {
        let part = doc
            .extract_pages(*lo..=*hi)
            .map_err(|e| output::facade_error(command, ctx, &e))?;
        merger = merger.add(part);
    }
    merger
        .build()
        .map_err(|e| output::facade_error(command, ctx, &e))
}
