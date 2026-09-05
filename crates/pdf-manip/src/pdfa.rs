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

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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

    /// No real page tree could be recovered; no placeholder output was made.
    #[error("no pages could be recovered from the input")]
    NoPages,

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
    /// Conservative font subsetting result, when the feature is enabled.
    #[cfg(feature = "font-subset")]
    pub subsets: crate::pdfa_subset::SubsetReport,
    /// Best-effort failures that require caller attention and external validation.
    pub warnings: Vec<String>,
}

/// Convert raw PDF bytes to PDF/A.
///
/// Handles loading, repair of damaged xref/page trees, empty-password
/// decryption, the conversion pipeline, and serialization.
///
/// This does not validate that the result conforms to PDF/A. Use
/// [`convert_bytes_with_report`] for repair diagnostics and validate the output
/// separately. Repeated Free Tier conversions can add another watermark.
pub fn convert_bytes(
    data: &[u8],
    opts: &PdfAConvertOptions<'_>,
) -> Result<Vec<u8>, PdfAConvertError> {
    convert_bytes_with_report(data, opts).map(|(bytes, _)| bytes)
}

/// Convert through the same shipping pipeline and retain its repair report.
/// A report describes attempted repairs; it is not a conformance verdict.
pub fn convert_bytes_with_report(
    data: &[u8],
    opts: &PdfAConvertOptions<'_>,
) -> Result<(Vec<u8>, PdfAConvertReport), PdfAConvertError> {
    let mut doc = load_for_conversion(data, opts)?;
    let report = convert_document(&mut doc, opts)?;

    opts.step("save");
    // Normalize before offsets are computed. Adding a binary-comment line
    // after serialization shifts every xref offset when the source marker
    // has fewer than four high bytes.
    doc.binary_mark = vec![0xe2, 0xe3, 0xcf, 0xd3];
    // A loaded xref-stream dictionary also serves as the trailer. Its old
    // stream filters/offsets do not describe the newly written xref stream;
    // an indirect DecodeParms can even require resolving objects before the
    // reader has bootstrapped its object table.
    for key in [
        b"Filter".as_slice(),
        b"DecodeParms",
        b"F",
        b"FFilter",
        b"FDecodeParms",
        b"Length",
        b"DL",
        b"Type",
        b"W",
        b"Index",
        b"Prev",
        b"XRefStm",
    ] {
        doc.trailer.remove(key);
    }
    crate::optimize::remove_unused_objects(&mut doc);
    let mut saved = Vec::new();
    if opts.conformance.part() == 1 {
        doc.version = "1.4".into();
        doc.reference_table.cross_reference_type = lopdf::xref::XrefType::CrossReferenceTable;
        doc.save_to(&mut saved)
    } else {
        doc.version = "1.7".into();
        doc.save_modern(&mut saved)
    }
    .map_err(|e| PdfAConvertError::Save(e.to_string()))?;

    // The writer already supplies the correct offset. The legacy byte repair
    // searches for literal "xref" inside streams, so must not run on a valid
    // modern serialization (an embedded file may itself contain that token).
    Ok((saved, report))
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

    if doc.get_pages().is_empty() && !doc.trailer.has(b"Encrypt") {
        return Err(PdfAConvertError::NoPages);
    }

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
/// from cheap normalization to rebuilding real page objects from the input.
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
    let is_pdfa1 = opts.conformance.part() == 1;

    opts.step("join_content_fragments");
    report.warnings.extend(join_content_fragments(doc));

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
    if doc.get_pages().is_empty() {
        return Err(PdfAConvertError::NoPages);
    }

    best_effort(opts, "preserve_single_cff_zero", &mut report, || {
        crate::pdfa_fonts::preserve_single_cff_zero(doc)
    });

    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::inline_image::editing_warnings(doc)
    })) {
        Ok(warnings) => report.warnings.extend(warnings),
        Err(_) => report.warnings.push(
            "inline image inspection failed; safe content editing could not be established".into(),
        ),
    }

    run_font_steps(doc, opts, &mut report);

    opts.step("colorspace");
    repair::fix_wrong_root(doc);
    let cs = required(opts, "normalize_colorspaces", || {
        crate::pdfa_colorspace::normalize_colorspaces(doc)
    })?;
    report.output_intent_added = cs.output_intent_added;

    opts.step("fixups");
    best_effort(opts, "run_fixups", &mut report, || {
        crate::pdfa_fixups::run_fixups(doc)
    });

    // Appends a blank glyph carrying the width the font dictionary declares, so
    // it has to run after every width pass — reading that width earlier bakes in
    // a stale value and creates the 6.2.11.5 mismatch it was meant to avoid.
    opts.step("cff_missing_space");
    best_effort(opts, "fix_cff_subset_missing_space", &mut report, || {
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

    best_effort(opts, "blank_cid_zero", &mut report, || {
        crate::pdfa_fonts::fix_blank_cid_zero(doc)
    });

    // Repairs can replace streams or leave newly embedded programs raw. Do
    // this last: later font passes must never mutate a program we just shared.
    opts.step("compact_storage");
    compact_storage(doc);
    #[cfg(feature = "font-subset")]
    {
        opts.step("subset_fonts");
        report.subsets = crate::pdfa_subset::subset_fonts(doc);
        // Different source programs may now contain the same retained glyphs.
        compact_storage(doc);
    }
    if !is_pdfa1 {
        for obj in doc.objects.values_mut() {
            if let lopdf::Object::Stream(stream) = obj {
                if stream
                    .dict
                    .get(b"Type")
                    .and_then(lopdf::Object::as_name)
                    .ok()
                    == Some(b"Metadata")
                    && stream.allows_compression
                    && !stream.dict.has(b"Filter")
                    && !stream.dict.has(b"DecodeParms")
                    && !stream.dict.has(b"F")
                {
                    let _ = stream.compress();
                }
            }
        }
    }

    report.page_count = doc.get_pages().len();
    Ok(report)
}

/// A page's Contents array is one logical program. A producer may split an
/// array, dictionary or operand sequence between streams; parsing each fragment
/// independently can drop delimiters and text. Join only arrays containing a
/// fragment that is not a complete standalone content program.
fn join_content_fragments(doc: &mut Document) -> Vec<String> {
    use lopdf::{Object, Stream};
    let mut warnings = Vec::new();
    let pages: Vec<_> = doc.get_pages().values().copied().collect();
    let mut cache = std::collections::BTreeMap::new();
    for page in pages {
        let ids = crate::content_editor::get_content_stream_ids(doc, page);
        if ids.len() < 2 {
            continue;
        }
        let complete_array = doc
            .get_object(page)
            .ok()
            .and_then(|o| o.as_dict().ok())
            .and_then(|d| d.get(b"Contents").ok())
            .and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| o.as_array().ok())
            .is_some_and(|a| {
                a.len() == ids.len() && a.iter().all(|o| matches!(o, Object::Reference(_)))
            });
        if !complete_array {
            warnings.push(format!(
                "page {} {} has unsupported content-array entries; fragments left intact",
                page.0, page.1
            ));
            continue;
        }
        if let Some(&joined) = cache.get(&ids) {
            if let Ok(dictionary) = doc.get_object_mut(page).and_then(Object::as_dict_mut) {
                dictionary.set("Contents", joined);
            }
            continue;
        }
        let mut data = Vec::new();
        let mut incomplete = false;
        let mut allow_compression = true;
        let mut unavailable = false;
        for id in &ids {
            let Ok(stream) = doc.get_object(*id).and_then(Object::as_stream) else {
                unavailable = true;
                break;
            };
            let Ok(bytes) = stream.get_plain_content() else {
                unavailable = true;
                break;
            };
            if data.len().saturating_add(bytes.len()).saturating_add(1)
                > crate::flate_decode::MAX_DEFLATE_BYTES as usize
            {
                unavailable = true;
                break;
            }
            incomplete |= crate::content_editor::content_stream_too_deeply_nested(&bytes)
                || lopdf::content::Content::decode_strict(&bytes).is_err();
            if !data.is_empty() {
                data.push(b'\n');
            }
            data.extend_from_slice(&bytes);
            allow_compression &= stream.allows_compression;
        }
        if unavailable {
            warnings.push(format!(
                "page {} {} content fragments could not be joined safely",
                page.0, page.1
            ));
            continue;
        }
        if !incomplete {
            continue;
        }
        let joined = doc.add_object(
            Stream::new(lopdf::Dictionary::new(), data).with_compression(allow_compression),
        );
        cache.insert(ids, joined);
        if let Ok(dictionary) = doc.get_object_mut(page).and_then(Object::as_dict_mut) {
            dictionary.set("Contents", joined);
        }
    }
    warnings
}

/// Lossless storage cleanup, without changing glyphs, pixels or content tokens.
pub(crate) fn compact_storage(doc: &mut Document) {
    use lopdf::{Dictionary, Object, ObjectId};
    use sha2::{Digest, Sha256};
    use std::collections::{BTreeMap, BTreeSet};

    // Include nested direct dictionaries (not just standalone descriptors).
    fn visit_dicts(obj: &mut Object, visit: &mut impl FnMut(&mut Dictionary)) {
        match obj {
            Object::Dictionary(d) => {
                visit(d);
                for (_, value) in d.iter_mut() {
                    visit_dicts(value, visit);
                }
            }
            Object::Stream(s) => {
                visit(&mut s.dict);
                for (_, value) in s.dict.iter_mut() {
                    visit_dicts(value, visit);
                }
            }
            Object::Array(a) => a.iter_mut().for_each(|o| visit_dicts(o, visit)),
            _ => {}
        }
    }

    crate::optimize::remove_unused_objects(doc);
    let keys: [&[u8]; 5] = [
        b"FontFile",
        b"FontFile2",
        b"FontFile3",
        b"ToUnicode",
        b"DestOutputProfile",
    ];
    let mut programs = BTreeSet::new();
    let mut metadata = BTreeSet::new();
    for obj in doc.objects.values_mut() {
        visit_dicts(obj, &mut |d| {
            for key in keys {
                if let Ok(id) = d.get(key).and_then(Object::as_reference) {
                    programs.insert(id);
                }
            }
            if let Ok(id) = d.get(b"Metadata").and_then(Object::as_reference) {
                metadata.insert(id);
            }
        });
    }
    for (id, obj) in &mut doc.objects {
        let Object::Stream(s) = obj else { continue };
        // PDF/A-1 metadata must remain unfiltered. Keep it so for every level.
        // Do not add a filter to external-file streams or activate previously
        // inactive DecodeParms. Existing image codecs/filters stay untouched.
        // A caller of the public `convert_document` can mark a stream as one
        // that compression would corrupt; honour that flag as lopdf's own
        // `Document::compress` does.
        if !s.allows_compression
            || metadata.contains(id)
            || s.dict.get(b"Type").and_then(Object::as_name).ok() == Some(b"Metadata")
            || [
                b"Filter".as_slice(),
                b"DecodeParms",
                b"F",
                b"FFilter",
                b"FDecodeParms",
            ]
            .iter()
            .any(|key| s.dict.has(key))
        {
            continue;
        }
        // compress() changes the stream only after successful encoding and
        // only if the payload saving exceeds the added filter overhead.
        let _ = s.compress();
    }

    // Scope sharing to immutable font programs, ToUnicode and output profiles.
    // Content/Form/Image streams can carry context-dependent semantics or be
    // edited independently on subsequent conversion. Equal bytes alone are not
    // sufficient: FontFile3 Subtype and Type1 Length1/2/3 also interpret them.
    // Keep every FontDescriptor, Encoding, Widths and ToUnicode independent.
    let mut candidates: BTreeMap<[u8; 32], Vec<ObjectId>> = BTreeMap::new();
    let mut replacements = BTreeMap::new();
    for id in programs {
        let Ok(stream) = doc.get_object(id).and_then(Object::as_stream) else {
            continue;
        };
        let hash: [u8; 32] = Sha256::digest(&stream.content).into();
        let bucket = candidates.entry(hash).or_default();
        if let Some(canonical) = bucket.iter().copied().find(|other| {
            doc.get_object(*other)
                .and_then(Object::as_stream)
                .is_ok_and(|s| s.dict == stream.dict && s.content == stream.content)
        }) {
            replacements.insert(id, canonical);
        } else {
            bucket.push(id);
        }
    }
    for obj in doc.objects.values_mut() {
        visit_dicts(obj, &mut |d| {
            for key in keys {
                if let Ok(old) = d.get(key).and_then(Object::as_reference) {
                    if let Some(&canonical) = replacements.get(&old) {
                        d.set(key, Object::Reference(canonical));
                    }
                }
            }
        });
    }
    // A duplicate could have a non-font reference too; reachability, rather
    // than blindly deleting the duplicate IDs, preserves such references.
    crate::optimize::remove_unused_objects(doc);
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
            best_effort(opts, $label, report, || $call);
        };
    }

    // Inline font dicts must become standalone objects before anything tries to
    // reference or rewrite them.
    font_step!(
        "promote_inline_fonts",
        crate::pdfa_fonts::promote_inline_font_dicts(doc)
    );

    font_step!(
        "preserve_custom_cff_encoding",
        crate::pdfa_fonts::preserve_custom_cff_encoding(doc)
    );

    font_step!(
        "preserve_symbolic_text",
        crate::pdfa_fonts::preserve_symbolic_text_mapping(doc)
    );

    opts.step("font_embed");
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::pdfa_fonts::embed_fonts(doc)
    })) {
        Ok(Ok(r)) => {
            if r.non_embedded_found > 0 {
                report.warnings.push(format!(
                    "{} fonts required embedding during conversion (including generated resources); resolved or fallback fonts may change glyph shapes and text geometry",
                    r.non_embedded_found
                ));
            }
            for (font, reason) in &r.failed {
                report
                    .warnings
                    .push(format!("font embedding failed for {font}: {reason}"));
            }
            report.fonts = Some(r);
        }
        Ok(Err(e)) => report.warnings.push(format!("font embedding failed: {e}")),
        Err(_) => report.warnings.push("font embedding panicked".into()),
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

    // A font's own CFF encoding is authoritative when the PDF has no
    // explicit encoding. Resolve it before the generic ASCII fallback.
    font_step!(
        "cff_tounicode",
        crate::pdfa_fonts::fix_type1_tounicode_from_cff(doc)
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

    // Widths, once more, now that every encoding rewrite has happened.
    //
    // §6.2.11.5 compares the declared width against the glyph the *final*
    // encoding resolves to, and several passes above rename a code after the
    // first width reconciliation has run — `fix_notdef_glyph_refs` points a
    // code at `space` to avoid a §6.2.11.8:1 violation, for instance. The
    // first pass then leaves a width describing the glyph the code used to
    // mean (govdocs holdout 074_074896: code 94 renamed from `mu1` to
    // `space`, width left at mu's 576 against space's 250). The pass is
    // idempotent, so re-running it costs nothing where nothing moved.
    font_step!(
        "width_mismatches_final",
        crate::pdfa_fonts::fix_font_width_mismatches(doc)
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
fn best_effort<T>(
    opts: &PdfAConvertOptions<'_>,
    step: &'static str,
    report: &mut PdfAConvertReport,
    f: impl FnOnce() -> T,
) {
    let _ = opts;
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err() {
        report
            .warnings
            .push(format!("{step} panicked; repair may be incomplete"));
    }
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
    fn split_content_arrays_and_dictionaries_are_joined_before_individual_stream_repairs() {
        let mut doc = lopdf::Document::with_version("1.7");
        let first = b"/Span << /MCID ".to_vec();
        let second = b"7 >> BDC BT /F 12 Tf [".to_vec();
        let third = b"(kept) 20 ( text)] TJ ET EMC".to_vec();
        let ids: Vec<_> = [first.clone(), second.clone(), third.clone()]
            .into_iter()
            .map(|data| {
                doc.add_object(lopdf::Stream::new(dictionary! {}, data).with_compression(false))
            })
            .collect();
        let array = doc.add_object(lopdf::Object::Array(
            ids.iter().copied().map(lopdf::Object::Reference).collect(),
        ));
        let pages = doc.new_object_id();
        let page = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,"Contents"=>array});
        let other = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,"Contents"=>array});
        doc.objects.insert(pages,lopdf::Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>vec![lopdf::Object::Reference(page),lopdf::Object::Reference(other)],"Count"=>2}));
        let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
        doc.trailer.set("Root", root);
        assert!(join_content_fragments(&mut doc).is_empty());
        let joined = doc
            .get_object(page)
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Contents")
            .unwrap()
            .as_reference()
            .unwrap();
        assert_eq!(
            doc.get_object(other)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Contents")
                .unwrap()
                .as_reference()
                .unwrap(),
            joined
        );
        let stream = doc.get_object(joined).unwrap().as_stream().unwrap();
        assert!(!stream.allows_compression);
        let expected = [first.clone(), second, third].join(&b'\n');
        assert_eq!(stream.content, expected);
        let parsed = lopdf::content::Content::decode_strict(&stream.content).unwrap();
        assert_eq!(
            parsed.operations[0].operands[1]
                .as_dict()
                .unwrap()
                .get(b"MCID")
                .unwrap()
                .as_i64()
                .unwrap(),
            7
        );
        assert!(parsed
            .operations
            .iter()
            .any(|o| o.operator == "TJ" && o.operands[0].as_array().unwrap().len() == 3));
        assert_eq!(
            doc.get_object(ids[0]).unwrap().as_stream().unwrap().content,
            first
        );
        assert!(join_content_fragments(&mut doc).is_empty());
    }

    #[test]
    fn modern_writer_offsets_survive_short_markers_and_old_xref_parameters() {
        let mut input = Document::load_mem(&minimal_pdf()).unwrap();
        input.binary_mark = vec![0xb5, 0xb6];
        let parameters = input.add_object(dictionary! {"Predictor" => 12, "Columns" => 4});
        input.trailer.set("DecodeParms", parameters);
        let mut raw = Vec::new();
        input.save_to(&mut raw).unwrap();
        let output = convert_bytes(&raw, &PdfAConvertOptions::default()).unwrap();
        assert!(output.starts_with(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n"));
        let parsed = Document::load_mem(&output).unwrap();
        assert!(!parsed.trailer.has(b"DecodeParms"));
        assert!(!parsed.trailer.has(b"Prev"));
        assert_eq!(parsed.get_pages().len(), 1);
        for (id, entry) in &parsed.reference_table.entries {
            if let lopdf::xref::XrefEntry::Normal { offset, generation } = entry {
                assert!(
                    output[*offset as usize..]
                        .starts_with(format!("{id} {generation} obj").as_bytes()),
                    "incorrect offset for object {id}"
                );
            }
        }
    }

    #[test]
    fn rejects_non_pdf_input() {
        let err = convert_bytes(b"not a pdf at all", &PdfAConvertOptions::default()).unwrap_err();
        assert!(matches!(err, PdfAConvertError::NotAPdf));
    }

    #[test]
    fn converted_content_is_compressed_without_changing_tokens() {
        let mut doc = Document::load_mem(&minimal_pdf()).unwrap();
        let page = *doc.get_pages().values().next().unwrap();
        let content = doc
            .get_dictionary(page)
            .unwrap()
            .get(b"Contents")
            .unwrap()
            .as_reference()
            .unwrap();
        let tokens = b"q 0 0 10 10 re f Q\n".repeat(4096);
        doc.objects.insert(
            content,
            lopdf::Stream::new(dictionary! {}, tokens.clone()).into(),
        );
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        let converted = convert_bytes(&bytes, &PdfAConvertOptions::default()).unwrap();
        let output = Document::load_mem(&converted).unwrap();
        let stream = output.get_object(content).unwrap().as_stream().unwrap();
        assert_eq!(stream.decompressed_content().unwrap(), tokens);
        assert!(stream.content.len() < tokens.len() / 10);
    }

    #[test]
    fn converted_icc_and_pdfa2_metadata_are_compressed() {
        let converted = convert_bytes(&minimal_pdf(), &PdfAConvertOptions::default()).unwrap();
        let output = Document::load_mem(&converted).unwrap();
        let intent = output
            .catalog()
            .unwrap()
            .get(b"OutputIntents")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .as_reference()
            .unwrap();
        let profile = output
            .get_dictionary(intent)
            .unwrap()
            .get(b"DestOutputProfile")
            .unwrap()
            .as_reference()
            .unwrap();
        let stream = output.get_object(profile).unwrap().as_stream().unwrap();
        assert!(
            stream.dict.has(b"Filter"),
            "new output profiles must be compressed"
        );
        assert!(stream.content.len() < stream.decompressed_content().unwrap().len());
        let metadata = output
            .catalog()
            .unwrap()
            .get(b"Metadata")
            .unwrap()
            .as_reference()
            .unwrap();
        assert!(output
            .get_object(metadata)
            .unwrap()
            .as_stream()
            .unwrap()
            .dict
            .has(b"Filter"));
    }

    #[test]
    fn pdfa1_storage_uses_classic_xref_and_unfiltered_metadata() {
        for conformance in [PdfAConformance::A1a, PdfAConformance::A1b] {
            let bytes = convert_bytes(
                &minimal_pdf(),
                &PdfAConvertOptions {
                    conformance,
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(bytes.starts_with(b"%PDF-1.4"));
            let output = Document::load_mem(&bytes).unwrap();
            assert!(matches!(
                output.reference_table.cross_reference_type,
                lopdf::xref::XrefType::CrossReferenceTable
            ));
            for o in output.objects.values() {
                if let lopdf::Object::Stream(s) = o {
                    let kind = s.dict.get(b"Type").and_then(lopdf::Object::as_name).ok();
                    assert_ne!(kind, Some(b"ObjStm".as_slice()));
                    if kind == Some(b"Metadata") {
                        assert!(!s.dict.has(b"Filter"));
                    }
                }
            }
        }
    }

    #[test]
    fn no_page_recovery_does_not_claim_success_with_a_blank_placeholder() {
        let mut doc = Document::with_version("1.7");
        let pages = doc.add_object(
            dictionary! {"Type" => "Pages", "Kids" => Vec::<lopdf::Object>::new(), "Count" => 0},
        );
        let root = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages});
        doc.trailer.set("Root", root);
        assert!(matches!(
            convert_document(&mut doc, &PdfAConvertOptions::default()),
            Err(PdfAConvertError::NoPages)
        ));
    }

    #[test]
    fn best_effort_panics_are_reported() {
        let mut report = PdfAConvertReport::default();
        best_effort(
            &PdfAConvertOptions::default(),
            "owned_test",
            &mut report,
            || panic!("synthetic malformed input"),
        );
        assert_eq!(
            report.warnings,
            ["owned_test panicked; repair may be incomplete"]
        );
    }

    #[test]
    fn conversion_drops_unreachable_cycles_and_old_metadata() {
        let mut doc = Document::load_mem(&minimal_pdf()).unwrap();
        let a = doc.new_object_id();
        let b = doc.add_object(dictionary! { "Next" => a });
        doc.objects.insert(
            a,
            lopdf::Stream::new(dictionary! { "Next" => b }, vec![42; 65536]).into(),
        );
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        let once = convert_bytes(&bytes, &PdfAConvertOptions::default()).unwrap();
        let twice = convert_bytes(&once, &PdfAConvertOptions::default()).unwrap();
        let output = Document::load_mem(&once).unwrap();
        assert!(!output.objects.contains_key(&a));
        assert!(!output.objects.contains_key(&b));
        assert!(
            // Free-tier conversion adds another watermark on every call. That
            // pre-existing behaviour is separate from unreachable storage.
            twice.len() <= once.len() + 1024,
            "second conversion accumulated storage: {} -> {}",
            once.len(),
            twice.len()
        );
    }

    #[test]
    fn caller_metadata_compression_opt_out_survives_conversion() {
        let mut doc = Document::load_mem(&minimal_pdf()).unwrap();
        let metadata = doc.add_object(
            lopdf::Stream::new(
                dictionary! {"Type"=>"Metadata", "Subtype"=>"XML"},
                b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"/>".to_vec(),
            )
            .with_compression(false),
        );
        doc.catalog_mut().unwrap().set("Metadata", metadata);
        convert_document(&mut doc, &PdfAConvertOptions::default()).unwrap();
        let stream = doc.get_object(metadata).unwrap().as_stream().unwrap();
        assert!(!stream.allows_compression);
        assert!(!stream.dict.has(b"Filter"));
    }

    #[test]
    fn storage_compaction_leaves_streams_that_refuse_compression_alone() {
        use lopdf::{Object, Stream};
        let mut doc = Document::load_mem(&minimal_pdf()).unwrap();
        let raw = Stream::new(dictionary! {}, vec![42; 4096]).with_compression(false);
        let id = doc.add_object(raw.clone());
        doc.catalog_mut()
            .unwrap()
            .set("Preserve", Object::Reference(id));
        compact_storage(&mut doc);
        assert_eq!(doc.get_object(id).unwrap().as_stream().unwrap(), &raw);
    }

    #[test]
    fn storage_compaction_preserves_existing_codecs_and_decode_parameters() {
        use lopdf::{Object, Stream};
        let mut doc = Document::load_mem(&minimal_pdf()).unwrap();
        let streams = [
            Stream::new(dictionary! { "Filter" => "DCTDecode" }, vec![42; 4096]),
            Stream::new(
                dictionary! { "DecodeParms" => dictionary! { "Predictor" => 12 } },
                vec![42; 4096],
            ),
            Stream::new(dictionary! { "Type" => "Metadata" }, vec![42; 4096]),
        ];
        let ids: Vec<_> = streams.iter().map(|s| doc.add_object(s.clone())).collect();
        doc.catalog_mut().unwrap().set(
            "Preserve",
            Object::Array(ids.iter().map(|id| Object::Reference(*id)).collect()),
        );
        compact_storage(&mut doc);
        for (id, expected) in ids.iter().zip(streams) {
            assert_eq!(doc.get_object(*id).unwrap().as_stream().unwrap(), &expected);
        }
    }

    #[test]
    fn converts_a_minimal_pdf_and_adds_output_intent() {
        let out = convert_bytes(&minimal_pdf(), &PdfAConvertOptions::default())
            .expect("minimal PDF should convert");
        assert!(out.starts_with(b"%PDF-"));
        // XMP metadata and an OutputIntent are the two things every PDF/A file
        // must carry; both are absent from the input.
        let doc = Document::load_mem(&out).unwrap();
        let catalog = doc.catalog().unwrap();
        assert!(!catalog
            .get(b"OutputIntents")
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
        let id = catalog.get(b"Metadata").unwrap().as_reference().unwrap();
        let xml = doc
            .get_object(id)
            .unwrap()
            .as_stream()
            .unwrap()
            .decompressed_content()
            .unwrap();
        assert!(String::from_utf8_lossy(&xml).contains("pdfaid"));
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
            let doc = Document::load_mem(&out).unwrap();
            let metadata = doc
                .catalog()
                .unwrap()
                .get(b"Metadata")
                .unwrap()
                .as_reference()
                .unwrap();
            let stream = doc.get_object(metadata).unwrap().as_stream().unwrap();
            let xml = if stream.dict.has(b"Filter") {
                stream.decompressed_content().unwrap()
            } else {
                stream.content.clone()
            };
            let text = String::from_utf8_lossy(&xml);
            assert!(text.contains(marker), "{level:?} did not produce {marker}");
        }
    }
}
