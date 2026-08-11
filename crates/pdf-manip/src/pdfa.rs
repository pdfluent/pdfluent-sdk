//! PDF/A conversion: the whole pipeline, in one place.
//!
//! Converting an arbitrary PDF to PDF/A is not one operation but roughly forty,
//! most of them font repairs, in an order that matters. This module owns that
//! order. Everything that ships — the WASM build, the C ABI, the `pdfluent`
//! facade — and the test harness that measures conformance all call
//! [`convert_bytes`] or [`convert_document`], so a pass-rate measured in the
//! harness describes what a customer actually gets.
//!
//! Before this module existed the orchestration lived only in the test runner,
//! and the shipped WASM entry point ran six of the forty steps. Any figure
//! measured then described code no customer could invoke.
//!
//! # Which entry point
//!
//! [`convert_bytes`] takes raw PDF bytes and returns converted bytes. It adds
//! load, repair, decryption and save around [`convert_document`], and is what
//! you want unless you already hold a parsed [`Document`].
//!
//! [`convert_document`] runs the conversion steps against an already-loaded
//! document, leaving loading and saving to the caller.
//!
//! ```no_run
//! use pdf_manip::pdfa::{convert_bytes, PdfAConvertOptions};
//!
//! let input = std::fs::read("scan.pdf").unwrap();
//! let pdfa = convert_bytes(&input, &PdfAConvertOptions::default()).unwrap();
//! std::fs::write("scan_pdfa.pdf", pdfa).unwrap();
//! ```
//!
//! # Best-effort steps
//!
//! Four steps are load-bearing: cleanup, colour-space normalization, XMP
//! repair, and save. If one of those fails the conversion fails, because its
//! absence guarantees a non-conformant result.
//!
//! Every font repair is best-effort. Each one targets a specific defect class
//! and does nothing when the defect is absent, so a failure means one rule may
//! still be violated — not that the output is unusable. They are individually
//! wrapped so a panic inside a malformed font program cannot take down a batch
//! conversion.

use lopdf::Document;
use thiserror::Error;

use crate::pdfa_repair as repair;
pub use crate::pdfa_xmp::PdfAConformance;

/// Why a PDF/A conversion could not be completed.
///
/// The distinction that matters to a batch caller is between input the
/// converter cannot accept at all ([`NotAPdf`](Self::NotAPdf),
/// [`LoadFailed`](Self::LoadFailed), [`Encrypted`](Self::Encrypted)) and a
/// required step that failed on input it did accept ([`Step`](Self::Step),
/// [`Panicked`](Self::Panicked), [`Save`](Self::Save)). The first group is a
/// property of the document; the second is worth investigating.
#[derive(Debug, Error)]
pub enum PdfAConvertError {
    /// The bytes contain no `%PDF-` header anywhere.
    #[error("not a PDF file (missing %PDF header)")]
    NotAPdf,

    /// The document could not be loaded, and every repair strategy failed.
    #[error("load failed (all repair strategies exhausted)")]
    LoadFailed,

    /// The document is encrypted and the empty user password did not open it.
    #[error("encrypted PDF (decryption failed)")]
    Encrypted,

    /// A required step returned an error. The step name matches the label
    /// passed to [`PdfAConvertOptions::on_step`].
    #[error("{step} failed: {source}")]
    Step {
        /// Name of the failing step.
        step: &'static str,
        /// The underlying error.
        source: crate::ManipError,
    },

    /// A required step panicked. Best-effort steps never produce this.
    #[error("panic in {0}")]
    Panicked(&'static str),

    /// Serializing the converted document failed.
    #[error("save failed: {0}")]
    Save(String),
}

/// An external repair pass: takes the original bytes, returns repaired bytes,
/// or `None` when it cannot help. See [`PdfAConvertOptions::external_repair`].
pub type ExternalRepair<'a> = &'a (dyn Fn(&[u8]) -> Option<Vec<u8>> + 'a);

/// Progress callback: receives each step's name as it starts.
pub type StepCallback<'a> = &'a (dyn Fn(&str) + 'a);

/// How to convert.
///
/// The defaults convert to PDF/A-2b with no external repair tool and no
/// progress reporting, which is what most callers want.
pub struct PdfAConvertOptions<'a> {
    /// Target conformance level. Defaults to [`PdfAConformance::A2b`].
    pub conformance: PdfAConformance,

    /// An external repair pass tried before the internal byte-level repairs
    /// when lopdf cannot load the input, and again when decryption fails.
    ///
    /// The test harness passes a `qpdf --decrypt` shell-out here. Shelling out
    /// to a tool that may not be installed does not belong in a library, so the
    /// library exposes the seam instead of making the choice.
    pub external_repair: Option<ExternalRepair<'a>>,

    /// Called with each step's name as it starts. Used by the harness for
    /// progress display and for pinpointing which step hangs on a given file.
    pub on_step: Option<StepCallback<'a>>,
}

impl Default for PdfAConvertOptions<'_> {
    fn default() -> Self {
        Self {
            conformance: PdfAConformance::A2b,
            external_repair: None,
            on_step: None,
        }
    }
}

impl PdfAConvertOptions<'_> {
    fn step(&self, name: &'static str) {
        if let Some(cb) = self.on_step {
            cb(name);
        }
    }
}

/// What the conversion did.
///
/// Carries the sub-reports rather than a flattened summary, because the useful
/// question after a batch run is which *kind* of repair a document needed.
#[derive(Debug, Clone, Default)]
pub struct PdfAConvertReport {
    /// Pages in the converted document.
    pub page_count: usize,
    /// What cleanup removed or normalized.
    pub cleanup: crate::pdfa_cleanup::PdfACleanupReport,
    /// What font embedding did. `None` when the step failed outright.
    pub fonts: Option<crate::pdfa_fonts::FontEmbedReport>,
    /// Whether an OutputIntent was added.
    pub output_intent_added: bool,
    /// Whether a page tree had to be repaired or synthesized before conversion.
    pub page_tree_repaired: bool,
}

/// Convert raw PDF bytes to PDF/A.
///
/// Handles loading, repair of damaged xref/page trees, empty-password
/// decryption, the conversion pipeline, and serialization.
///
/// This does not check whether the input is already PDF/A. Conversion is
/// idempotent, so running it on a conformant file is wasted work but not
/// harmful; callers that want to skip should test with `pdf_compliance`.
pub fn convert_bytes(
    data: &[u8],
    opts: &PdfAConvertOptions<'_>,
) -> Result<Vec<u8>, PdfAConvertError> {
    let mut doc = load_for_conversion(data, opts)?;
    convert_document(&mut doc, opts)?;

    opts.step("save");
    let mut saved = Vec::new();
    doc.save_to(&mut saved)
        .map_err(|e| PdfAConvertError::Save(e.to_string()))?;

    crate::pdfa_cleanup::fix_pdf_header(&mut saved);
    crate::pdfa_cleanup::fix_startxref(&mut saved);
    Ok(saved)
}

/// Load a document for conversion, repairing and decrypting as needed.
///
/// Exposed separately because the harness reports "could not load" and "could
/// not convert" as different outcomes.
pub fn load_for_conversion(
    data: &[u8],
    opts: &PdfAConvertOptions<'_>,
) -> Result<Document, PdfAConvertError> {
    if !data.windows(5).any(|w| w == b"%PDF-") {
        return Err(PdfAConvertError::NotAPdf);
    }

    opts.step("load");
    let mut doc = match Document::load_mem(data) {
        Ok(d) if !d.objects.is_empty() => d,
        Ok(_) | Err(_) => {
            opts.step("repair");
            external_repair_doc(data, opts)
                .or_else(|| repair::try_repair_for_lopdf(data))
                .ok_or(PdfAConvertError::LoadFailed)?
        }
    };

    // lopdf drops objects containing `#XX`-escaped names. Only retried when the
    // normal load produced no pages at all, because replacing `#` breaks valid
    // PDFs that use legitimate hex escaping.
    if doc.get_pages().is_empty() && repair::raw_has_hash_names(data) {
        let sanitized = repair::sanitize_hash_names_raw(data);
        if let Ok(d2) = Document::load_mem(&sanitized) {
            if !d2.get_pages().is_empty() {
                doc = d2;
            }
        }
    }

    recover_page_tree(&mut doc, data);

    if doc.trailer.get(b"Encrypt").is_ok() {
        match doc.decrypt("") {
            Ok(()) => {
                doc.trailer.remove(b"Encrypt");
            }
            Err(_) => {
                let mut repaired =
                    external_repair_doc(data, opts).ok_or(PdfAConvertError::Encrypted)?;
                repair::fix_wrong_root(&mut repaired);
                doc = repaired;
                if doc.trailer.get(b"Encrypt").is_ok() {
                    if doc.decrypt("").is_err() {
                        return Err(PdfAConvertError::Encrypted);
                    }
                    doc.trailer.remove(b"Encrypt");
                }
            }
        }
    }

    Ok(doc)
}

fn external_repair_doc(data: &[u8], opts: &PdfAConvertOptions<'_>) -> Option<Document> {
    let repaired = (opts.external_repair?)(data)?;
    repair::try_load(&repaired).or_else(|| repair::try_repair_for_lopdf(&repaired))
}

/// Bring a damaged page tree back to something with pages in it, escalating
/// from cheap normalization to synthesizing a placeholder page.
fn recover_page_tree(doc: &mut Document, original: &[u8]) -> bool {
    repair::fix_wrong_root(doc);
    let _ = repair::normalize_page_tree_types(doc);
    repair::strip_null_page_kids(doc);

    if !doc.get_pages().is_empty() {
        return false;
    }

    repair::try_fix_missing_page_types(doc);

    if doc.get_pages().is_empty() {
        if let Some(mut rebuilt) = repair::try_rebuild_xref_from_objects(original) {
            repair::fix_wrong_root(&mut rebuilt);
            let _ = repair::normalize_page_tree_types(&mut rebuilt);
            repair::strip_null_page_kids(&mut rebuilt);
            let _ = repair::try_fix_missing_page_types(&mut rebuilt);
            *doc = rebuilt;
        }
    }

    if doc.get_pages().is_empty() {
        // A document with no page tree at all cannot be validated by anything
        // downstream. One empty page is a worse document than the original but
        // a parseable one, which is the only state a conversion can report on.
        let _ = repair::ensure_placeholder_page_tree(doc);
    }

    true
}

/// Run every conversion step against an already-loaded document.
///
/// See the [module docs](self) for which steps are required and which are
/// best-effort.
pub fn convert_document(
    doc: &mut Document,
    opts: &PdfAConvertOptions<'_>,
) -> Result<PdfAConvertReport, PdfAConvertError> {
    let mut report = PdfAConvertReport::default();
    let is_pdfa1 = matches!(opts.conformance, PdfAConformance::A1b);

    opts.step("cleanup");
    let cleanup = required(opts, "cleanup_for_pdfa", || {
        crate::pdfa_cleanup::cleanup_for_pdfa(doc, is_pdfa1)
    })?;
    report.cleanup = cleanup;

    // Cleanup strips encryption, which can be what kept lopdf from seeing the
    // page tree. Worth one more recovery attempt now that it is gone.
    if doc.get_pages().is_empty() {
        report.page_tree_repaired = recover_page_tree(doc, &[]);
    }

    run_font_steps(doc, opts, &mut report);

    opts.step("colorspace");
    repair::fix_wrong_root(doc);
    let cs = required(opts, "normalize_colorspaces", || {
        crate::pdfa_colorspace::normalize_colorspaces(doc)
    })?;
    report.output_intent_added = cs.output_intent_added;

    opts.step("fixups");
    best_effort(opts, "run_fixups", || crate::pdfa_fixups::run_fixups(doc));

    // Appends a blank glyph carrying the width the font dictionary declares, so
    // it has to run after every width pass — reading that width earlier bakes in
    // a stale value and creates the 6.2.11.5 mismatch it was meant to avoid.
    opts.step("cff_missing_space");
    best_effort(opts, "fix_cff_subset_missing_space", || {
        crate::pdfa_fonts::fix_cff_subset_missing_space(doc)
    });

    // Fixups can introduce or rewrite DeviceN/Separation structures, so the
    // §6.2.4.4 consistency checks get one more pass over the result.
    opts.step("colorspace_post_fixups");
    repair::fix_wrong_root(doc);
    required(opts, "normalize_colorspaces (post-fixups)", || {
        crate::pdfa_colorspace::normalize_colorspaces(doc)
    })?;

    opts.step("xmp_repair");
    required(opts, "repair_xmp_metadata", || {
        crate::pdfa_xmp::repair_xmp_metadata(doc, opts.conformance, None)
    })?;

    report.page_count = doc.get_pages().len();
    Ok(report)
}

/// The font repairs, in the order that measured best on the govdocs corpus.
///
/// Order is not arbitrary and several steps document why they must follow
/// another. Reordering without measuring is how conformance regressions get in.
fn run_font_steps(
    doc: &mut Document,
    opts: &PdfAConvertOptions<'_>,
    report: &mut PdfAConvertReport,
) {
    macro_rules! font_step {
        ($label:literal, $call:expr) => {
            opts.step($label);
            best_effort(opts, $label, || $call);
        };
    }

    // Inline font dicts must become standalone objects before anything tries to
    // reference or rewrite them.
    font_step!(
        "promote_inline_fonts",
        crate::pdfa_fonts::promote_inline_font_dicts(doc)
    );

    opts.step("font_embed");
    if let Ok(Ok(r)) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::pdfa_fonts::embed_fonts(doc)
    })) {
        report.fonts = Some(r);
    }

    font_step!("pfb_streams", crate::pdfa_fonts::fix_pfb_font_streams(doc));
    font_step!(
        "type1_stub_fonts",
        crate::pdfa_fonts::fix_type1_stub_font_files(doc)
    );

    // Must precede the width passes so they compare against the right font type.
    font_step!(
        "mislabeled_truetype",
        crate::pdfa_fonts::fix_mislabeled_truetype_as_cff(doc)
    );
    font_step!(
        "truetype_with_cff_program",
        crate::pdfa_fonts::fix_truetype_with_cff_program(doc)
    );

    font_step!(
        "cff_invalid_bcd",
        crate::pdfa_fonts::fix_cff_invalid_bcd(doc)
    );
    font_step!(
        "type1_charstrings",
        crate::pdfa_fonts::fix_type1_nonstandard_charstrings(doc)
    );
    font_step!(
        "type1_eexec_space",
        crate::pdfa_fonts::fix_type1_eexec_space_prefix(doc)
    );

    font_step!("cff_widths", crate::pdfa_fonts::fix_cff_widths(doc));
    font_step!(
        "tt_cid_widths",
        crate::pdfa_fonts::fix_truetype_cid_widths(doc)
    );
    font_step!("charset", crate::pdfa_fonts::fix_type1_charset(doc));

    font_step!(
        "font_encoding",
        crate::pdfa_fonts::fix_truetype_encoding(doc)
    );
    font_step!(
        "symbolic_cmap",
        crate::pdfa_fonts::fix_existing_symbolic_truetype_cmaps(doc)
    );
    font_step!(
        "unicode_cmap",
        crate::pdfa_fonts::fix_truetype_unicode_cmap(doc)
    );

    font_step!(
        "type1_tounicode",
        crate::pdfa_fonts::fix_type1_tounicode_from_encoding(doc)
    );
    font_step!(
        "type0_tounicode",
        crate::pdfa_fonts::fix_type0_tounicode(doc)
    );
    font_step!(
        "cff_tounicode",
        crate::pdfa_fonts::fix_type1_tounicode_from_cff(doc)
    );
    font_step!(
        "tounicode_forbidden",
        crate::pdfa_fonts::fix_tounicode_forbidden_values(doc)
    );

    font_step!("notdef_refs", crate::pdfa_fonts::fix_notdef_glyph_refs(doc));
    font_step!(
        "type3_notdef",
        crate::pdfa_fonts::fix_type3_notdef_charprocs(doc)
    );
    font_step!("cid_notdef", crate::pdfa_fonts::fix_cid_font_notdef(doc));
    font_step!(
        "symbolic_notdef",
        crate::pdfa_fonts::fix_symbolic_font_notdef_streams(doc)
    );
    font_step!(
        "simple_range_notdef",
        crate::pdfa_fonts::fix_simple_font_streams(doc)
    );
    font_step!(
        "subset_missing_glyphs",
        crate::pdfa_fonts::fix_type1_subset_missing_glyphs(doc)
    );

    font_step!(
        "undef_encoding",
        crate::pdfa_fonts::fix_undefined_encoding_codes(doc)
    );
    font_step!("symbolic_flags", crate::pdfa_fonts::fix_symbolic_flags(doc));
    font_step!(
        "classic_symbolic_encoding",
        crate::pdfa_fonts::fix_classic_symbolic_base14_encoding(doc)
    );

    font_step!(
        "missing_widths",
        crate::pdfa_fonts::fix_missing_simple_font_widths(doc)
    );
    font_step!(
        "type3_widths",
        crate::pdfa_fonts::fix_type3_font_widths(doc)
    );
    font_step!(
        "width_mismatches",
        crate::pdfa_fonts::fix_font_width_mismatches(doc)
    );
    font_step!(
        "symbolic_widths",
        crate::pdfa_fonts::fix_symbolic_font_widths(doc)
    );
    font_step!(
        "remaining_tt_widths",
        crate::pdfa_fonts::fix_remaining_tt_width_mismatches(doc)
    );

    font_step!("cidset", crate::pdfa_fonts::fix_cidset(doc));
    font_step!(
        "cidtogidmap",
        crate::pdfa_fonts::fix_missing_cidtogidmap(doc)
    );

    // Runs last among the font steps: which code of a duplicated
    // supplement/main pair is actually used is only stable once every
    // content-rewriting pass has run.
    font_step!(
        "cff_enc_supplements",
        crate::pdfa_fonts::fix_cff_encoding_supplements(doc)
    );
    // Must follow the supplement pass: it validates against the CFF encoding
    // the supplements were just folded into.
    font_step!(
        "custom_cff_enc_widths",
        crate::pdfa_fonts::fix_custom_cff_encoding_widths(doc)
    );
}

/// Run a step whose failure means the output cannot be conformant.
fn required<T>(
    opts: &PdfAConvertOptions<'_>,
    step: &'static str,
    f: impl FnOnce() -> crate::Result<T>,
) -> Result<T, PdfAConvertError> {
    let _ = opts;
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(source)) => Err(PdfAConvertError::Step { step, source }),
        Err(_) => Err(PdfAConvertError::Panicked(step)),
    }
}

/// Run a step that repairs one defect class and does nothing when that defect
/// is absent. A panic here is contained: a malformed font program cannot take
/// down a batch conversion.
fn best_effort<T>(opts: &PdfAConvertOptions<'_>, step: &'static str, f: impl FnOnce() -> T) {
    let _ = (opts, step);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    /// A minimal one-page PDF, built by hand so the test does not depend on
    /// any fixture file.
    fn minimal_pdf() -> Vec<u8> {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(lopdf::Stream::new(lopdf::Dictionary::new(), Vec::new()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => lopdf::Dictionary::new(),
            "Contents" => content_id,
        });
        doc.objects.insert(
            pages_id,
            lopdf::Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn rejects_non_pdf_input() {
        let err = convert_bytes(b"not a pdf at all", &PdfAConvertOptions::default()).unwrap_err();
        assert!(matches!(err, PdfAConvertError::NotAPdf));
    }

    #[test]
    fn converts_a_minimal_pdf_and_adds_output_intent() {
        let out = convert_bytes(&minimal_pdf(), &PdfAConvertOptions::default())
            .expect("minimal PDF should convert");
        assert!(out.starts_with(b"%PDF-"));
        // XMP metadata and an OutputIntent are the two things every PDF/A file
        // must carry; both are absent from the input.
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("OutputIntent"), "no OutputIntent in output");
        assert!(text.contains("pdfaid"), "no PDF/A XMP identifier in output");
    }

    #[test]
    fn repeated_conversion_keeps_one_output_profile() {
        let once = convert_bytes(&minimal_pdf(), &PdfAConvertOptions::default()).unwrap();
        let twice = convert_bytes(&once, &PdfAConvertOptions::default()).unwrap();
        assert!(twice.starts_with(b"%PDF-"));

        // ISO 19005-2 §6.2.3:2: every OutputIntent carrying a DestOutputProfile
        // must reference the *same* indirect ICC object. Converting an already
        // converted file is the case where a second profile would slip in.
        let doc = Document::load_mem(&twice).unwrap();
        let intents = match doc.catalog().unwrap().get(b"OutputIntents") {
            Ok(lopdf::Object::Array(a)) => a.clone(),
            other => panic!("no OutputIntents array after conversion: {other:?}"),
        };
        let profiles: std::collections::HashSet<_> = intents
            .iter()
            .filter_map(|i| i.as_reference().ok())
            .filter_map(|id| doc.objects.get(&id))
            .filter_map(|o| o.as_dict().ok())
            .filter_map(|d| d.get(b"DestOutputProfile").ok())
            .filter_map(|o| o.as_reference().ok())
            .collect();
        assert_eq!(
            profiles.len(),
            1,
            "OutputIntents reference {} distinct ICC profiles, §6.2.3:2 allows 1",
            profiles.len()
        );
    }

    #[test]
    fn step_callback_reports_progress() {
        let seen = std::cell::RefCell::new(Vec::new());
        let opts = PdfAConvertOptions {
            on_step: Some(&|s: &str| seen.borrow_mut().push(s.to_string())),
            ..Default::default()
        };
        convert_bytes(&minimal_pdf(), &opts).unwrap();
        let seen = seen.into_inner();
        assert!(seen.contains(&"cleanup".to_string()));
        assert!(seen.contains(&"xmp_repair".to_string()));
        assert!(seen.contains(&"save".to_string()));
    }

    #[test]
    fn conformance_level_reaches_the_xmp() {
        for (level, marker) in [
            (PdfAConformance::A1b, "<pdfaid:part>1"),
            (PdfAConformance::A2b, "<pdfaid:part>2"),
            (PdfAConformance::A3b, "<pdfaid:part>3"),
        ] {
            let opts = PdfAConvertOptions {
                conformance: level,
                ..Default::default()
            };
            let out = convert_bytes(&minimal_pdf(), &opts).unwrap();
            let text = String::from_utf8_lossy(&out);
            assert!(text.contains(marker), "{level:?} did not produce {marker}");
        }
    }
}
