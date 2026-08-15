//! Layout-aware text editing: find → stage → commit (Phase 1B engine).
//!
//! This is the match-based replacement engine designed in
//! `docs/TEXT_REPLACE_ENGINE_DESIGN.md`. Unlike [`crate::text_replace`] it
//! never skips silently, preserves the page's `/Contents` stream structure,
//! detects text in Form XObjects, refuses signed documents by default and
//! reports every staged edit.
//!
//! ```no_run
//! use lopdf::Document;
//! use pdf_manip::text_edit::{begin_text_edit, DocumentRevision, ReplaceOptions, TextQuery};
//!
//! let bytes = std::fs::read("in.pdf").unwrap();
//! let mut doc = Document::load_mem(&bytes).unwrap();
//! let revision = DocumentRevision::from_source_bytes(&bytes);
//!
//! let mut session = begin_text_edit(&mut doc, revision).unwrap();
//! let matches = session.find_text(TextQuery::exact("Acme B.V.")).unwrap();
//! session
//!     .stage_replace(&matches[0].id, "Example B.V.", ReplaceOptions::default())
//!     .unwrap();
//! let report = session.commit().unwrap();
//! assert_eq!(report.replacements_applied, 1);
//! ```

mod apply;
mod scan;
mod signatures;
mod token;

use std::collections::HashMap;
use std::fmt;
use std::ops::{Bound, RangeBounds};

use lopdf::{Document, Object};

use crate::content_editor::multiply_matrix;
use crate::error::ManipError;
use crate::text_replace::inject_fallback_font;

pub use token::DocumentRevision;

use apply::{EditRequest, PreparedPage};
use scan::{ContainerScan, PageScan};
use token::TokenPayload;

/// Geometry tolerance for region matching (design §10.3).
const REGION_EPSILON: f64 = 1e-6;
/// Text-context window (bytes on each side) hashed into a MatchId.
const CONTEXT_WINDOW: usize = 32;

// ===========================================================================
// Identifiers
// ===========================================================================

/// Opaque, serializable locator for one text match.
///
/// Wire format: `pdfluent-match-v1.<base64url(payload)>` (design §10.2).
/// Valid only against the exact [`DocumentRevision`] that produced it; every
/// use fully revalidates the locator against the live document.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct MatchId(String);

impl MatchId {
    /// Wrap a previously serialized token (validated on first use).
    pub fn from_token(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    /// The serialized token.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MatchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ===========================================================================
// Errors
// ===========================================================================

/// Why a [`MatchId`] no longer resolves (design §3). No fuzzy relocation is
/// ever attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum StaleReason {
    /// The document revision differs from the one that produced the id.
    RevisionChanged,
    /// The matched source bytes changed under the locator.
    SourceBytesChanged,
    /// The surrounding context changed under the locator.
    ContextChanged,
    /// The container (page/stream/XObject) no longer resolves.
    ContainerMissing,
}

/// Container kinds the Phase 1B engine detects but cannot edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum UnsupportedContainer {
    /// Text lives in a Form XObject (editing lands with clone-on-write).
    FormXObject,
    /// Operators straddle stream boundaries; the page was scanned fused.
    FusedPageStreams,
    /// A content stream of this page is shared with another page.
    SharedPageStream,
}

/// Typed errors of the text-edit engine.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TextEditError {
    /// The locator does not resolve against the current document revision.
    #[error("stale match id ({reason:?})")]
    StaleMatch {
        /// The stale locator.
        match_id: MatchId,
        /// Why it is stale.
        reason: StaleReason,
    },
    /// The token could not be decoded or failed strict validation.
    #[error("invalid match id: {reason}")]
    InvalidMatchId {
        /// Decoder diagnostics.
        reason: String,
    },
    /// The query is malformed (e.g. empty needle).
    #[error("invalid query: {reason}")]
    InvalidQuery {
        /// What was wrong.
        reason: String,
    },
    /// Two staged edits overlap in the same container.
    #[error("staged edits overlap")]
    OverlappingEdits {
        /// First edit.
        a: MatchId,
        /// Second edit.
        b: MatchId,
    },
    /// The same match was staged twice.
    #[error("match already staged")]
    DuplicateStage {
        /// The duplicated locator.
        match_id: MatchId,
    },
    /// The match lives in a container the engine cannot edit yet.
    #[error("unsupported container: {kind:?}")]
    UnsupportedContainer {
        /// The match.
        match_id: MatchId,
        /// Container kind.
        kind: UnsupportedContainer,
    },
    /// The match spans text with differing font/size/style.
    #[error("match spans multiple styles: {detail}")]
    UnsupportedStyleSpan {
        /// The match.
        match_id: MatchId,
        /// Which styles differ.
        detail: String,
    },
    /// Replacement (or retained) text could not be encoded.
    #[error("encoding failed in font '{font}': {detail}")]
    EncodingFailed {
        /// The edit, when attributable.
        match_id: Option<MatchId>,
        /// Font that could not encode the text.
        font: String,
        /// Encoder diagnostics.
        detail: String,
    },
    /// The original font cannot encode the replacement and the policy is
    /// [`FontFallback::Deny`].
    #[error("font fallback denied for font '{font}': {detail}")]
    FontFallbackDenied {
        /// The edit, when attributable.
        match_id: Option<MatchId>,
        /// The original font.
        font: String,
        /// Why the original font failed.
        detail: String,
    },
    /// The match is covered by `/ActualText` (design §7).
    #[error("match is covered by /ActualText")]
    TaggedTextConflict {
        /// The match.
        match_id: MatchId,
        /// Decoded glyph text.
        visual_text: String,
        /// The enclosing /ActualText value.
        actual_text: String,
    },
    /// The document carries digital signatures and the policy is
    /// [`SignaturePolicy::RejectSignedDocuments`].
    #[error("document is digitally signed ({} signature(s))", signatures.len())]
    SignedDocumentRejected {
        /// The signatures found.
        signatures: Vec<SignatureSummary>,
    },
    /// Document permissions forbid content modification.
    #[error("document permissions forbid content modification")]
    PermissionsDenied,
    /// The requested fit policy is not implemented in this phase.
    #[error("unsupported fit policy: {policy:?}")]
    UnsupportedFitPolicy {
        /// The requested policy.
        policy: FitPolicy,
    },
    /// Underlying document error.
    #[error(transparent)]
    Document(#[from] ManipError),
    /// Invariant violation — please report.
    #[error("internal error: {detail}")]
    Internal {
        /// Diagnostics.
        detail: String,
    },
}

impl TextEditError {
    fn with_match_id(self, id: &MatchId) -> Self {
        match self {
            TextEditError::EncodingFailed {
                match_id: None,
                font,
                detail,
            } => TextEditError::EncodingFailed {
                match_id: Some(id.clone()),
                font,
                detail,
            },
            TextEditError::FontFallbackDenied {
                match_id: None,
                font,
                detail,
            } => TextEditError::FontFallbackDenied {
                match_id: Some(id.clone()),
                font,
                detail,
            },
            other => other,
        }
    }
}

/// A failed commit: the first fatal cause plus the validation outcome of
/// every staged edit. The document was not modified.
#[derive(Debug, thiserror::Error)]
#[error("commit failed: {error}")]
pub struct CommitError {
    /// First fatal cause.
    #[source]
    pub error: TextEditError,
    /// Per-edit outcomes for all staged edits.
    pub results: Vec<TextReplacementResult>,
}

// ===========================================================================
// Policies & options
// ===========================================================================

/// What the engine may do when replacement text does not fit (design §4.3).
///
/// Implemented: [`FitPolicy::Exact`] and [`FitPolicy::ShrinkToFit`]. The rest
/// are reserved and rejected with [`TextEditError::UnsupportedFitPolicy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum FitPolicy {
    /// Same position, font size and spacing; no measurement-based fitting.
    Exact,
    /// Adjust character/word spacing within limits (Phase 2).
    AdjustSpacing,
    /// Measure the replacement against the space the original occupied and
    /// scale the font size down until it fits, to a floor of 50%.
    ///
    /// Reports what it did: a `shrunk-to-fit` diagnostic with the applied
    /// percentage, or `shrink-floor-reached` when the text still overruns at
    /// the floor — shrinking further would trade one defect for an
    /// unreadable one.
    ShrinkToFit,
    /// Re-break the replacement onto more lines inside the width the
    /// original occupied, keeping the font size — Acrobat's behaviour when
    /// you edit inside a text box, and the natural choice for translations.
    ///
    /// Added lines are not pushed away from content below; like Acrobat, no
    /// other object on the page moves. Reported as a `reflowed` diagnostic.
    ReflowInBounds,
    /// Expand the text box within caller-supplied bounds (Phase 2).
    ExpandBounds,
}

/// Font fallback policy. Fallback use is never silent: it is always visible
/// in the per-edit result (`font_substituted`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum FontFallback {
    /// Fail with [`TextEditError::FontFallbackDenied`] (default).
    Deny,
    /// Use the named font (must exist in the page resources).
    Explicit(String),
    /// Inject a Helvetica/WinAnsiEncoding resource and use it.
    ///
    /// Bounded by WinAnsi: nothing above U+00FF can be written this way.
    InjectStandard,
    /// Embed the caller-supplied font as a Type0/`Identity-H` composite font
    /// and write the replacement through it.
    ///
    /// This is the only policy that can write scripts the document never
    /// contained — Polish, Greek, Cyrillic, CJK and so on — because the
    /// reachable characters are those of the supplied font rather than of a
    /// 256-entry encoding. See [`crate::unicode_font`].
    ///
    /// The font is embedded (subsetted to the glyphs actually used), so the
    /// caller must hold a licence permitting embedding.
    #[cfg(feature = "font-subset")]
    EmbedUnicode(crate::unicode_font::UnicodeFont),
}

/// Transaction policy (design §10.4). Strictest-wins across the staged
/// edits: the commit runs `BestEffort` only when **every** staged edit
/// requested it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum CommitPolicy {
    /// Validate everything, then apply everything — or nothing (default).
    AllOrNothing,
    /// Apply the valid subset deterministically (earlier-staged edits win
    /// conflicts) and report every failure per edit. Never silent.
    BestEffort,
}

/// Digital-signature policy (design §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum SignaturePolicy {
    /// Refuse to commit into a signed document (default).
    RejectSignedDocuments,
    /// Proceed; existing signatures will fail validation afterwards and the
    /// report sets `signatures_invalidated`.
    AllowPostSignatureChange,
}

/// Tagged-text policy (design §7). Phase 1: reject `/ActualText` conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum TaggedTextPolicy {
    /// Fail with [`TextEditError::TaggedTextConflict`].
    Reject,
}

/// How a region rectangle selects matches (design §10.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum RegionRelation {
    /// Positive-area bounding-box intersection (default).
    Intersects,
    /// The complete match bounding box inside the region.
    Contained,
}

/// Options for one staged replacement.
#[derive(Debug, Clone)]
pub struct ReplaceOptions {
    /// Fit policy. [`FitPolicy::Exact`] (default) or
    /// [`FitPolicy::ShrinkToFit`].
    pub fit: FitPolicy,
    /// Font fallback policy.
    pub font_fallback: FontFallback,
    /// Transaction policy (Phase 1B: [`CommitPolicy::AllOrNothing`] only).
    pub commit_policy: CommitPolicy,
    /// Signature policy; the strictest policy among staged edits wins.
    pub signature_policy: SignaturePolicy,
    /// Tagged-text policy.
    pub tagged_text_policy: TaggedTextPolicy,
}

impl Default for ReplaceOptions {
    fn default() -> Self {
        Self {
            fit: FitPolicy::Exact,
            font_fallback: FontFallback::Deny,
            commit_policy: CommitPolicy::AllOrNothing,
            signature_policy: SignaturePolicy::RejectSignedDocuments,
            tagged_text_policy: TaggedTextPolicy::Reject,
        }
    }
}

impl ReplaceOptions {
    /// Set the fit policy.
    #[must_use]
    pub fn fit(mut self, fit: FitPolicy) -> Self {
        self.fit = fit;
        self
    }

    /// Set the font fallback policy.
    #[must_use]
    pub fn font_fallback(mut self, fallback: FontFallback) -> Self {
        self.font_fallback = fallback;
        self
    }

    /// Set the signature policy.
    #[must_use]
    pub fn signature_policy(mut self, policy: SignaturePolicy) -> Self {
        self.signature_policy = policy;
        self
    }

    /// Set the transaction policy.
    #[must_use]
    pub fn commit_policy(mut self, policy: CommitPolicy) -> Self {
        self.commit_policy = policy;
        self
    }
}

// ===========================================================================
// Query
// ===========================================================================

/// A text search query. Matching operates on the decoded visual text of the
/// logical reading sequence per container (design §10 / Phase 1B narrowing).
#[derive(Debug, Clone)]
pub struct TextQuery {
    needle: String,
    case_insensitive: bool,
    pages: Option<(u32, u32)>,
    region: Option<(u32, [f64; 4], RegionRelation)>,
    limit: Option<usize>,
}

impl TextQuery {
    /// Search for this exact text.
    pub fn exact(text: impl Into<String>) -> Self {
        Self {
            needle: text.into(),
            case_insensitive: false,
            pages: None,
            region: None,
            limit: None,
        }
    }

    /// Unicode-simple case-insensitive matching.
    #[must_use]
    pub fn case_insensitive(mut self, yes: bool) -> Self {
        self.case_insensitive = yes;
        self
    }

    /// Restrict to a 1-based page range.
    #[must_use]
    pub fn pages(mut self, range: impl RangeBounds<u32>) -> Self {
        let start = match range.start_bound() {
            Bound::Included(&s) => s,
            Bound::Excluded(&s) => s + 1,
            Bound::Unbounded => 1,
        };
        let end = match range.end_bound() {
            Bound::Included(&e) => e,
            Bound::Excluded(&e) => e.saturating_sub(1),
            Bound::Unbounded => u32::MAX,
        };
        self.pages = Some((start.max(1), end));
        self
    }

    /// Restrict to matches whose bbox intersects `rect` on `page`
    /// (positive-area intersection; see [`RegionRelation`]).
    #[must_use]
    pub fn region(self, page: u32, rect: [f64; 4]) -> Self {
        self.region_with(page, rect, RegionRelation::Intersects)
    }

    /// Region restriction with an explicit relation.
    #[must_use]
    pub fn region_with(mut self, page: u32, rect: [f64; 4], relation: RegionRelation) -> Self {
        self.region = Some((page, rect, relation));
        self
    }

    /// Return at most `n` matches (document order).
    #[must_use]
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }
}

// ===========================================================================
// Matches
// ===========================================================================

/// Writing direction of matched text. Phase 1 supports LTR only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum WritingDirection {
    /// Left to right.
    Ltr,
}

/// Where a match lives.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum ContainerKind {
    /// One stream of the page's `/Contents` (index within the array).
    PageStream {
        /// 0-based index in `/Contents` order.
        index: u32,
    },
    /// A Form XObject reached from the page.
    FormXObject {
        /// Resource-name path from the page (e.g. `["Fm0"]`).
        path: Vec<String>,
        /// Number of pages referencing this XObject.
        shared_by: u32,
    },
    /// The page had to be scanned fused (see [`UnsupportedContainer`]).
    FusedPageStreams,
}

/// Container identity of a match or a modified stream.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ContainerInfo {
    /// 1-based page number.
    pub page: u32,
    /// lopdf object id of the stream, `(number, generation)`.
    pub stream_obj: (u32, u16),
    /// Container kind.
    pub kind: ContainerKind,
}

/// One text-showing operator touched by a match.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MatchSpan {
    /// Operator index in the container's logical operator sequence.
    pub op_index: usize,
    /// Byte range of the match within the operator's decoded text.
    pub char_start: usize,
    /// End of the byte range.
    pub char_end: usize,
}

/// Visual style at the start of a match.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MatchStyle {
    /// Font resource name.
    pub font_name: String,
    /// Font size.
    pub font_size: f64,
    /// Fill color (RGB).
    pub fill_color: [f64; 3],
    /// Character spacing (Tc).
    pub char_spacing: f64,
    /// Word spacing (Tw).
    pub word_spacing: f64,
    /// Horizontal scaling (Tz, percent).
    pub horiz_scaling: f64,
    /// Text rise (Ts).
    pub text_rise: f64,
}

/// One found occurrence.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TextMatch {
    /// Opaque serializable locator.
    pub id: MatchId,
    /// The matched visual text.
    pub text: String,
    /// 1-based page number.
    pub page: u32,
    /// Approximate device-space bounding box `[x0, y0, x1, y1]`.
    pub bbox: [f64; 4],
    /// Touched text-showing operators.
    pub spans: Vec<MatchSpan>,
    /// Style at the start of the match.
    pub style: MatchStyle,
    /// Text matrix × CTM at the start of the match.
    pub transform: [f64; 6],
    /// Writing direction (Phase 1: LTR).
    pub writing_direction: WritingDirection,
    /// Container the match lives in.
    pub container: ContainerInfo,
    /// Enclosing `/ActualText`, when present.
    pub actual_text: Option<String>,
    /// Whether this match can be edited by the current engine phase.
    pub editable: bool,
    /// Why the match is not editable, when it is not.
    pub unsupported: Option<UnsupportedReason>,
    /// Non-fatal observations (approximate bbox, style notes, …).
    pub warnings: Vec<Diagnostic>,
}

/// Why a found match cannot be edited in this phase.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum UnsupportedReason {
    /// Unsupported container kind.
    Container(UnsupportedContainer),
    /// The match spans differing fonts/sizes.
    StyleSpan {
        /// Which styles differ.
        detail: String,
    },
    /// The match is covered by `/ActualText`.
    TaggedText,
}

/// A coded, human-readable observation attached to matches and results.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Diagnostic {
    /// Stable machine-readable code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
}

/// A digital signature found in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SignatureSummary {
    /// Signature field name (`/T`).
    pub field_name: String,
    /// DocMDP certification level (`/P`), when this is a certification
    /// signature: 1 = no changes, 2 = form fill, 3 = annotations too.
    pub docmdp_permission: Option<u32>,
}

// ===========================================================================
// Report
// ===========================================================================

/// Outcome of one staged edit.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum ReplacementStatus {
    /// The edit was applied.
    Applied,
    /// The edit failed validation or encoding.
    Failed {
        /// Human-readable failure description.
        reason: String,
    },
}

/// Per-edit result (design §4.4).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TextReplacementResult {
    /// The staged match.
    pub match_id: MatchId,
    /// What happened.
    pub status: ReplacementStatus,
    /// Bounding box of the original text.
    pub old_bbox: [f64; 4],
    /// Bounding box of the new text (Phase 2: measured; Phase 1B: `None`).
    pub new_bbox: Option<[f64; 4]>,
    /// Font used for the replacement text.
    pub font_used: String,
    /// Whether a fallback font was used (never silent).
    pub font_substituted: bool,
    /// Original font size.
    pub old_font_size: f64,
    /// New font size (Phase 1B: unchanged).
    pub new_font_size: f64,
    /// Fit policy that was applied.
    pub fit_applied: FitPolicy,
    /// Number of text lines after the edit (Phase 1B: touched operators).
    pub new_line_count: u32,
    /// Overflow information (Phase 2).
    pub overflow: Option<String>,
    /// Whether `/ActualText` was updated (`None` = none present).
    pub actual_text_updated: Option<bool>,
    /// Whether the page participates in a structure tree.
    pub tags_affected: bool,
    /// Diagnostics with remediation hints.
    pub diagnostics: Vec<Diagnostic>,
}

/// Exhaustive commit report: `matches_found == replacements_applied +
/// replacements_failed` always holds (no silent skips).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TextReplacementReport {
    /// Number of staged edits in this transaction.
    pub matches_found: usize,
    /// Edits applied.
    pub replacements_applied: usize,
    /// Edits failed (0 for a successful AllOrNothing commit).
    pub replacements_failed: usize,
    /// Pages whose content changed.
    pub pages_modified: Vec<u32>,
    /// Streams that were rewritten (only touched containers).
    pub containers_modified: Vec<ContainerInfo>,
    /// Containers that had to be fused during scanning.
    pub containers_fused: Vec<ContainerInfo>,
    /// Whether the document carries digital signatures.
    pub signatures_present: bool,
    /// Whether this commit invalidated them (only under
    /// [`SignaturePolicy::AllowPostSignatureChange`]).
    pub signatures_invalidated: bool,
    /// Per-edit results, one per staged edit.
    pub results: Vec<TextReplacementResult>,
    /// Revision to pass into the next [`begin_text_edit`] on this document.
    pub next_revision: DocumentRevision,
}

// ===========================================================================
// Session
// ===========================================================================

/// Start a text-edit session on a document.
///
/// `revision` must be [`DocumentRevision::from_source_bytes`] of the exact
/// bytes the document was loaded from, or the `next_revision` of the previous
/// commit's report. Fails with [`TextEditError::PermissionsDenied`] when the
/// document's encryption permissions forbid content modification.
pub fn begin_text_edit(
    doc: &mut Document,
    revision: DocumentRevision,
) -> Result<TextEditSession<'_>, TextEditError> {
    if signatures::modification_forbidden(doc) {
        return Err(TextEditError::PermissionsDenied);
    }
    Ok(TextEditSession {
        doc,
        revision,
        scans: HashMap::new(),
        staged: Vec::new(),
    })
}

/// Convenience: find + stage + commit in one call (design §4).
///
/// Every found occurrence is accounted for in the report: editable matches
/// are staged and committed under `options`; matches the engine cannot edit
/// (Form XObjects, style spans, `/ActualText`, …) appear as `Failed` results
/// with their typed reason — never as silent skips. The editable set commits
/// under the transaction policy in `options` (default `AllOrNothing`).
pub fn replace_text(
    doc: &mut Document,
    revision: DocumentRevision,
    query: TextQuery,
    replacement: &str,
    options: ReplaceOptions,
) -> Result<TextReplacementReport, CommitError> {
    let wrap = |error: TextEditError| CommitError {
        error,
        results: Vec::new(),
    };
    let mut session = begin_text_edit(doc, revision).map_err(wrap)?;
    let matches = session.find_text(query).map_err(wrap)?;

    let mut skipped: Vec<TextReplacementResult> = Vec::new();
    for m in &matches {
        if m.editable {
            if let Err(e) = session.stage_replace(&m.id, replacement, options.clone()) {
                skipped.push(unstaged_result(m, &options, e.to_string()));
            }
        } else {
            let reason = match &m.unsupported {
                Some(UnsupportedReason::Container(kind)) => {
                    format!("unsupported container: {kind:?}")
                }
                Some(UnsupportedReason::StyleSpan { detail }) => {
                    format!("match spans multiple styles: {detail}")
                }
                Some(UnsupportedReason::TaggedText) => {
                    "match is covered by /ActualText".to_string()
                }
                None => "not editable".to_string(),
            };
            skipped.push(unstaged_result(m, &options, reason));
        }
    }

    let mut report = match session.commit() {
        Ok(r) => r,
        Err(mut e) => {
            e.results.extend(skipped);
            return Err(e);
        }
    };
    report.matches_found += skipped.len();
    report.replacements_failed += skipped.len();
    report.results.extend(skipped);
    Ok(report)
}

/// Failed-result row for a match that was never staged.
fn unstaged_result(
    m: &TextMatch,
    options: &ReplaceOptions,
    reason: String,
) -> TextReplacementResult {
    TextReplacementResult {
        match_id: m.id.clone(),
        status: ReplacementStatus::Failed { reason },
        old_bbox: m.bbox,
        new_bbox: None,
        font_used: m.style.font_name.clone(),
        font_substituted: false,
        old_font_size: m.style.font_size,
        new_font_size: m.style.font_size,
        fit_applied: options.fit,
        new_line_count: m.spans.len() as u32,
        overflow: None,
        actual_text_updated: m.actual_text.as_ref().map(|_| false),
        tags_affected: false,
        diagnostics: m.warnings.clone(),
    }
}

struct StagedEdit {
    id: MatchId,
    payload: TokenPayload,
    snapshot: TextMatch,
    replacement: String,
    options: ReplaceOptions,
}

/// An in-progress text-edit transaction (design §2).
///
/// Edits are staged without touching the document; [`TextEditSession::commit`]
/// validates every staged edit, rebuilds only the touched streams, and swaps
/// them in atomically (`AllOrNothing`).
pub struct TextEditSession<'d> {
    doc: &'d mut Document,
    revision: DocumentRevision,
    scans: HashMap<u32, PageScan>,
    staged: Vec<StagedEdit>,
}

impl TextEditSession<'_> {
    /// Find matches for `query` across the requested pages, including inside
    /// Form XObjects (reported non-editable in Phase 1B).
    pub fn find_text(&mut self, query: TextQuery) -> Result<Vec<TextMatch>, TextEditError> {
        if query.needle.is_empty() {
            return Err(TextEditError::InvalidQuery {
                reason: "empty search text".to_string(),
            });
        }
        let page_count = self.doc.get_pages().len() as u32;
        let (lo, hi) = query.pages.unwrap_or((1, u32::MAX));
        let hi = hi.min(page_count);

        let mut matches = Vec::new();
        for page in lo..=hi {
            self.ensure_scan(page)?;
            let scan = &self.scans[&page];

            for (s, e) in find_all(
                &scan.content.combined,
                &query.needle,
                query.case_insensitive,
            ) {
                matches.push(build_match(&self.revision, scan, ScanTarget::Page, (s, e)));
            }
            for (xi, xobj) in scan.xobjects.iter().enumerate() {
                for (s, e) in find_all(&xobj.scan.combined, &query.needle, query.case_insensitive) {
                    matches.push(build_match(
                        &self.revision,
                        scan,
                        ScanTarget::Xobject(xi),
                        (s, e),
                    ));
                }
            }
        }

        if let Some((page, rect, relation)) = query.region {
            matches.retain(|m| m.page == page && region_matches(&m.bbox, &rect, relation));
        }
        if let Some(n) = query.limit {
            matches.truncate(n);
        }
        Ok(matches)
    }

    /// Re-hydrate a serialized [`MatchId`] (asynchronous workflows). The
    /// locator is fully revalidated: revision, container, range and content
    /// hashes must all still match.
    pub fn resolve(&mut self, id: &MatchId) -> Result<TextMatch, TextEditError> {
        let payload = token::decode_token(id.as_str())?;
        token::check_revision(&payload, &self.revision, id)?;
        self.ensure_scan(payload.page)?;
        let scan = &self.scans[&payload.page];

        let target = if payload.ck == "p" {
            ScanTarget::Page
        } else {
            let path = payload.ck.trim_start_matches("x:");
            let found = scan
                .xobjects
                .iter()
                .position(|x| x.name_path.join("/") == path);
            match found {
                Some(xi) => ScanTarget::Xobject(xi),
                None => {
                    return Err(TextEditError::StaleMatch {
                        match_id: id.clone(),
                        reason: StaleReason::ContainerMissing,
                    })
                }
            }
        };
        let container = target.container(scan);
        let (s, e) = (payload.chr[0] as usize, payload.chr[1] as usize);
        let combined = &container.combined;
        if e > combined.len() || !combined.is_char_boundary(s) || !combined.is_char_boundary(e) {
            return Err(TextEditError::StaleMatch {
                match_id: id.clone(),
                reason: StaleReason::SourceBytesChanged,
            });
        }
        if token::hash64_hex(&combined.as_bytes()[s..e]) != payload.sh {
            return Err(TextEditError::StaleMatch {
                match_id: id.clone(),
                reason: StaleReason::SourceBytesChanged,
            });
        }
        if context_hash(combined, (s, e)) != payload.ch {
            return Err(TextEditError::StaleMatch {
                match_id: id.clone(),
                reason: StaleReason::ContextChanged,
            });
        }
        Ok(build_match(&self.revision, scan, target, (s, e)))
    }

    /// Stage a replacement for one match. The document is not modified until
    /// [`TextEditSession::commit`].
    pub fn stage_replace(
        &mut self,
        target: &MatchId,
        replacement: &str,
        options: ReplaceOptions,
    ) -> Result<(), TextEditError> {
        if !matches!(
            options.fit,
            FitPolicy::Exact | FitPolicy::ShrinkToFit | FitPolicy::ReflowInBounds
        ) {
            return Err(TextEditError::UnsupportedFitPolicy {
                policy: options.fit,
            });
        }
        if self.staged.iter().any(|s| &s.id == target) {
            return Err(TextEditError::DuplicateStage {
                match_id: target.clone(),
            });
        }

        let snapshot = self.resolve(target)?;
        if let Some(reason) = &snapshot.unsupported {
            return Err(match reason {
                UnsupportedReason::Container(kind) => TextEditError::UnsupportedContainer {
                    match_id: target.clone(),
                    kind: *kind,
                },
                UnsupportedReason::StyleSpan { detail } => TextEditError::UnsupportedStyleSpan {
                    match_id: target.clone(),
                    detail: detail.clone(),
                },
                UnsupportedReason::TaggedText => TextEditError::TaggedTextConflict {
                    match_id: target.clone(),
                    visual_text: snapshot.text.clone(),
                    actual_text: snapshot.actual_text.clone().unwrap_or_default(),
                },
            });
        }

        let payload = token::decode_token(target.as_str())?;
        self.staged.push(StagedEdit {
            id: target.clone(),
            payload,
            snapshot,
            replacement: replacement.to_string(),
            options,
        });
        Ok(())
    }

    /// The currently staged match ids, in staging order.
    pub fn staged(&self) -> Vec<&MatchId> {
        self.staged.iter().map(|s| &s.id).collect()
    }

    /// Remove a staged edit. Returns whether it was present.
    pub fn unstage(&mut self, target: &MatchId) -> bool {
        let before = self.staged.len();
        self.staged.retain(|s| &s.id != target);
        self.staged.len() != before
    }

    /// Drop all staged edits without touching the document.
    pub fn abort(self) {}

    /// Validate every staged edit, then apply the transaction.
    ///
    /// Under `AllOrNothing` (the default, strictest-wins: it applies unless
    /// **every** staged edit requested `BestEffort`) any failure aborts
    /// before the first byte changes and returns [`CommitError`] with all
    /// per-edit results.
    ///
    /// Under `BestEffort` (design §10.4, Phase 1C) the valid subset is
    /// applied and every failure is reported in the returned report — never
    /// silently: `matches_found == replacements_applied +
    /// replacements_failed` always holds. Subset selection is deterministic:
    /// on overlap the earlier-staged edit wins; encoding/validation failures
    /// remove only the failing edit and planning is repeated with the rest.
    /// Document-level rejections (signatures, permissions) still fail the
    /// whole commit.
    ///
    /// In both modes all touched containers are rebuilt into temporary
    /// buffers first; the document is only mutated after the surviving edit
    /// set and every rebuilt container validated (prepare-then-swap).
    pub fn commit(mut self) -> Result<TextReplacementReport, CommitError> {
        let staged_count = self.staged.len();
        if staged_count == 0 {
            return Ok(self.empty_report());
        }
        let best_effort = self
            .staged
            .iter()
            .all(|s| s.options.commit_policy == CommitPolicy::BestEffort);

        // Signature policy (document-level, before any mutation).
        let found_signatures = signatures::detect_signatures(self.doc);
        let signatures_present = !found_signatures.is_empty();
        let any_reject = self
            .staged
            .iter()
            .any(|s| s.options.signature_policy == SignaturePolicy::RejectSignedDocuments);
        if signatures_present && any_reject {
            let error = TextEditError::SignedDocumentRejected {
                signatures: found_signatures,
            };
            let results = self.all_failed_results(&format!("{error}"));
            return Err(CommitError { error, results });
        }

        // Overlap detection (design §5). AllOrNothing: first conflict aborts.
        // BestEffort: the earlier-staged edit wins deterministically.
        let mut failed: HashMap<usize, TextEditError> = HashMap::new();
        for i in 0..self.staged.len() {
            if failed.contains_key(&i) {
                continue;
            }
            for j in (i + 1)..self.staged.len() {
                if failed.contains_key(&j) {
                    continue;
                }
                let (a, b) = (&self.staged[i], &self.staged[j]);
                if a.payload.page == b.payload.page
                    && a.payload.ck == b.payload.ck
                    && ranges_overlap(a.payload.chr, b.payload.chr)
                {
                    let error = TextEditError::OverlappingEdits {
                        a: a.id.clone(),
                        b: b.id.clone(),
                    };
                    if best_effort {
                        failed.insert(j, error);
                    } else {
                        let results = self.all_failed_results(&format!("{error}"));
                        return Err(CommitError { error, results });
                    }
                }
            }
        }

        // Prepare loop: plan the active subset; under BestEffort remove
        // failing edits and re-plan until the subset is stable.
        let mut active: Vec<usize> = (0..staged_count)
            .filter(|i| !failed.contains_key(i))
            .collect();
        let prepared: Vec<(u32, PreparedPage)>;
        loop {
            match self.prepare_active(&active) {
                Ok((p, new_failures)) => {
                    if new_failures.is_empty() {
                        prepared = p;
                        break;
                    }
                    if !best_effort {
                        return Err(self.all_or_nothing_failure(new_failures, failed));
                    }
                    for (i, e) in new_failures {
                        failed.insert(i, e);
                    }
                    active.retain(|i| !failed.contains_key(i));
                    if active.is_empty() {
                        prepared = Vec::new();
                        break;
                    }
                }
                Err(e) => {
                    let results = self.all_failed_results(&format!("{e}"));
                    return Err(CommitError { error: e, results });
                }
            }
        }

        // Swap phase: inject fallback fonts first, then rewrite the touched
        // streams. Only stream objects are mutated; /Contents structure and
        // untouched streams keep their exact bytes.
        let mut prepared = prepared;
        for (page, p) in &mut prepared {
            // An embedded Unicode font takes precedence: when one is present
            // the rebuilt streams already reference it by name, and the
            // Helvetica resource would be dead weight.
            #[cfg(feature = "font-subset")]
            if let Some(encoder) = p.unicode_encoder.take() {
                if let Err(e) = encoder.embed(self.doc, *page) {
                    let error = TextEditError::Internal {
                        detail: format!("embedding the Unicode font on page {page} failed: {e}"),
                    };
                    let results = self.all_failed_results(&format!("{error}"));
                    return Err(CommitError { error, results });
                }
                continue;
            }
            if p.inject_fallback && inject_fallback_font(self.doc, *page).is_none() {
                let error = TextEditError::Internal {
                    detail: format!("fallback font injection failed on page {page}"),
                };
                let results = self.all_failed_results(&format!("{error}"));
                return Err(CommitError { error, results });
            }
        }
        let prepared = prepared;
        let mut containers_modified = Vec::new();
        for (page, p) in &prepared {
            for (idx, (stream_id, bytes)) in p.touched_streams.iter().enumerate() {
                write_stream_bytes(self.doc, *stream_id, bytes);
                containers_modified.push(ContainerInfo {
                    page: *page,
                    stream_obj: *stream_id,
                    kind: ContainerKind::PageStream {
                        index: p.touched_stream_indices[idx] as u32,
                    },
                });
            }
        }

        // Report.
        let applied_count = active.len();
        let mut results = Vec::with_capacity(staged_count);
        for (i, staged) in self.staged.iter().enumerate() {
            if let Some(e) = failed.get(&i) {
                results.push(base_result(
                    staged,
                    ReplacementStatus::Failed {
                        reason: e.to_string(),
                    },
                    false,
                    None,
                ));
            } else {
                let info = prepared
                    .iter()
                    .flat_map(|(_, p)| p.outcomes.iter())
                    .find(|o| o.staged_index == i)
                    .and_then(|o| o.result.as_ref().ok());
                results.push(base_result(
                    staged,
                    ReplacementStatus::Applied,
                    info.map(|i| i.font_substituted).unwrap_or(false),
                    info,
                ));
            }
        }
        let mut pages_modified: Vec<u32> = prepared
            .iter()
            .filter(|(_, p)| !p.touched_streams.is_empty())
            .map(|(page, _)| *page)
            .collect();
        pages_modified.sort_unstable();
        pages_modified.dedup();
        let containers_fused = self
            .scans
            .values()
            .filter(|s| s.fused)
            .flat_map(|s| {
                s.stream_ids.iter().map(|&id| ContainerInfo {
                    page: s.page,
                    stream_obj: id,
                    kind: ContainerKind::FusedPageStreams,
                })
            })
            .collect();

        Ok(TextReplacementReport {
            matches_found: staged_count,
            replacements_applied: applied_count,
            replacements_failed: staged_count - applied_count,
            pages_modified,
            containers_modified,
            containers_fused,
            signatures_present,
            signatures_invalidated: signatures_present && applied_count > 0,
            results,
            next_revision: if applied_count > 0 {
                self.revision.next()
            } else {
                self.revision
            },
        })
    }

    /// Plan the given staged-edit subset. Returns the prepared pages plus
    /// per-edit failures found during planning. The document is not mutated.
    #[allow(clippy::type_complexity)]
    fn prepare_active(
        &mut self,
        active: &[usize],
    ) -> Result<(Vec<(u32, PreparedPage)>, HashMap<usize, TextEditError>), TextEditError> {
        let mut pages: Vec<u32> = active
            .iter()
            .map(|&i| self.staged[i].payload.page)
            .collect();
        pages.sort_unstable();
        pages.dedup();

        let mut prepared = Vec::new();
        let mut failures: HashMap<usize, TextEditError> = HashMap::new();
        for &page in &pages {
            self.ensure_scan(page)?;
            let scan = &self.scans[&page];
            let requests: Vec<EditRequest> = active
                .iter()
                .map(|&i| (i, &self.staged[i]))
                .filter(|(_, s)| s.payload.page == page)
                .map(|(i, s)| EditRequest {
                    staged_index: i,
                    chr: (s.payload.chr[0] as usize, s.payload.chr[1] as usize),
                    replacement: s.replacement.clone(),
                    fallback: s.options.font_fallback.clone(),
                    fit: s.options.fit,
                })
                .collect();
            let p = apply::prepare_page(scan, &requests)?;
            for outcome in &p.outcomes {
                if let Err(e) = &outcome.result {
                    let id = &self.staged[outcome.staged_index].id;
                    failures.insert(outcome.staged_index, clone_error(e).with_match_id(id));
                }
            }
            prepared.push((page, p));
        }
        Ok((prepared, failures))
    }

    /// Build the AllOrNothing abort error from the first planning failure.
    fn all_or_nothing_failure(
        &self,
        new_failures: HashMap<usize, TextEditError>,
        mut failed: HashMap<usize, TextEditError>,
    ) -> CommitError {
        for (i, e) in new_failures {
            failed.insert(i, e);
        }
        let mut results = Vec::with_capacity(self.staged.len());
        for (i, staged) in self.staged.iter().enumerate() {
            let status = match failed.get(&i) {
                Some(e) => ReplacementStatus::Failed {
                    reason: e.to_string(),
                },
                None => ReplacementStatus::Failed {
                    reason: "aborted: transaction is AllOrNothing and another edit failed"
                        .to_string(),
                },
            };
            results.push(base_result(staged, status, false, None));
        }
        let first = failed
            .into_iter()
            .min_by_key(|(i, _)| *i)
            .map(|(_, e)| e)
            .expect("non-empty");
        CommitError {
            error: first,
            results,
        }
    }

    fn ensure_scan(&mut self, page: u32) -> Result<(), TextEditError> {
        if !self.scans.contains_key(&page) {
            let scan = scan::scan_page(self.doc, page)?;
            self.scans.insert(page, scan);
        }
        Ok(())
    }

    fn empty_report(&self) -> TextReplacementReport {
        TextReplacementReport {
            matches_found: 0,
            replacements_applied: 0,
            replacements_failed: 0,
            pages_modified: Vec::new(),
            containers_modified: Vec::new(),
            containers_fused: Vec::new(),
            signatures_present: false,
            signatures_invalidated: false,
            results: Vec::new(),
            next_revision: self.revision,
        }
    }

    fn all_failed_results(&self, reason: &str) -> Vec<TextReplacementResult> {
        self.staged
            .iter()
            .map(|s| {
                base_result(
                    s,
                    ReplacementStatus::Failed {
                        reason: reason.to_string(),
                    },
                    false,
                    None,
                )
            })
            .collect()
    }
}

fn base_result(
    staged: &StagedEdit,
    status: ReplacementStatus,
    font_substituted: bool,
    info: Option<&apply::AppliedInfo>,
) -> TextReplacementResult {
    TextReplacementResult {
        match_id: staged.id.clone(),
        status,
        old_bbox: staged.snapshot.bbox,
        new_bbox: None,
        font_used: info
            .map(|i| i.font_used.clone())
            .unwrap_or_else(|| staged.snapshot.style.font_name.clone()),
        font_substituted,
        old_font_size: staged.snapshot.style.font_size,
        new_font_size: staged.snapshot.style.font_size,
        fit_applied: staged.options.fit,
        new_line_count: staged.snapshot.spans.len() as u32,
        overflow: None,
        actual_text_updated: staged.snapshot.actual_text.as_ref().map(|_| false),
        tags_affected: false,
        diagnostics: info.map(|i| i.diagnostics.clone()).unwrap_or_default(),
    }
}

/// Clone a TextEditError for per-edit attribution (errors are not `Clone`
/// because of the `ManipError` source; degrade that case to a message).
fn clone_error(e: &TextEditError) -> TextEditError {
    match e {
        TextEditError::EncodingFailed {
            match_id,
            font,
            detail,
        } => TextEditError::EncodingFailed {
            match_id: match_id.clone(),
            font: font.clone(),
            detail: detail.clone(),
        },
        TextEditError::FontFallbackDenied {
            match_id,
            font,
            detail,
        } => TextEditError::FontFallbackDenied {
            match_id: match_id.clone(),
            font: font.clone(),
            detail: detail.clone(),
        },
        other => TextEditError::Internal {
            detail: other.to_string(),
        },
    }
}

// ===========================================================================
// Match building
// ===========================================================================

enum ScanTarget {
    Page,
    Xobject(usize),
}

impl ScanTarget {
    fn container<'a>(&self, scan: &'a PageScan) -> &'a ContainerScan {
        match self {
            ScanTarget::Page => &scan.content,
            ScanTarget::Xobject(i) => &scan.xobjects[*i].scan,
        }
    }
}

fn build_match(
    revision: &DocumentRevision,
    scan: &PageScan,
    target: ScanTarget,
    range: (usize, usize),
) -> TextMatch {
    let container = target.container(scan);
    let (s, e) = range;
    let text = container.combined[s..e].to_string();

    // Involved runs.
    let bounds = &container.run_bounds;
    let ri0 = run_index(bounds, s);
    let ri1 = run_index(bounds, e.saturating_sub(1).max(s));
    let runs = &container.runs[ri0..=ri1];

    let mut spans = Vec::with_capacity(runs.len());
    for (k, run) in runs.iter().enumerate() {
        let run_start = bounds[ri0 + k];
        spans.push(MatchSpan {
            op_index: run.ops_range.start,
            char_start: s.max(run_start) - run_start,
            char_end: (e.min(bounds[ri0 + k + 1])) - run_start,
        });
    }

    // Style + transform from the graphics state before the first op.
    let first = &runs[0];
    let snapshot = container.tracker.state_at(first.ops_range.start);
    let (style, transform) = match snapshot {
        Some(gs) => (
            MatchStyle {
                font_name: first.font_name.clone(),
                font_size: first.font_size,
                fill_color: gs.fill_color,
                char_spacing: gs.char_spacing,
                word_spacing: gs.word_spacing,
                horiz_scaling: gs.horiz_scaling,
                text_rise: gs.text_rise,
            },
            multiply_matrix(&gs.text_matrix, &gs.ctm),
        ),
        None => (
            MatchStyle {
                font_name: first.font_name.clone(),
                font_size: first.font_size,
                fill_color: [0.0; 3],
                char_spacing: 0.0,
                word_spacing: 0.0,
                horiz_scaling: 100.0,
                text_rise: 0.0,
            },
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        ),
    };

    // Approximate bbox: union of the involved runs' boxes.
    let mut bbox = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for run in runs {
        bbox[0] = bbox[0].min(run.x);
        bbox[1] = bbox[1].min(run.y);
        bbox[2] = bbox[2].max(run.x + run.width);
        bbox[3] = bbox[3].max(run.y + run.font_size);
    }
    let mut warnings = vec![Diagnostic {
        code: "approximate-bbox".to_string(),
        message: "bbox derived from estimated run metrics (exact metrics land in Phase 2)"
            .to_string(),
    }];

    // Container info + editability.
    let (container_info, mut unsupported) = match &target {
        ScanTarget::Page => {
            let (stream_idx, _) = scan.source_of(first.ops_range.start).unwrap_or((0, 0));
            let stream_obj = scan.stream_ids.get(stream_idx).copied().unwrap_or((0, 0));
            if scan.fused {
                (
                    ContainerInfo {
                        page: scan.page,
                        stream_obj,
                        kind: ContainerKind::FusedPageStreams,
                    },
                    Some(UnsupportedReason::Container(
                        UnsupportedContainer::FusedPageStreams,
                    )),
                )
            } else if scan.shared_stream {
                (
                    ContainerInfo {
                        page: scan.page,
                        stream_obj,
                        kind: ContainerKind::PageStream {
                            index: stream_idx as u32,
                        },
                    },
                    Some(UnsupportedReason::Container(
                        UnsupportedContainer::SharedPageStream,
                    )),
                )
            } else {
                (
                    ContainerInfo {
                        page: scan.page,
                        stream_obj,
                        kind: ContainerKind::PageStream {
                            index: stream_idx as u32,
                        },
                    },
                    None,
                )
            }
        }
        ScanTarget::Xobject(xi) => {
            let x = &scan.xobjects[*xi];
            (
                ContainerInfo {
                    page: scan.page,
                    stream_obj: x.stream_id,
                    kind: ContainerKind::FormXObject {
                        path: x.name_path.clone(),
                        shared_by: x.shared_by,
                    },
                },
                Some(UnsupportedReason::Container(
                    UnsupportedContainer::FormXObject,
                )),
            )
        }
    };

    // Style-span homogeneity (design §8).
    if unsupported.is_none() {
        let mixed = runs.iter().any(|r| {
            r.font_name != first.font_name || (r.font_size - first.font_size).abs() > 1e-9
        });
        if mixed {
            unsupported = Some(UnsupportedReason::StyleSpan {
                detail: "match spans runs with differing font or size".to_string(),
            });
        }
    }

    // /ActualText coverage (design §7).
    let actual_text = runs
        .iter()
        .find_map(|r| container.actual.get(r.ops_range.start).cloned().flatten());
    if unsupported.is_none() && actual_text.is_some() {
        unsupported = Some(UnsupportedReason::TaggedText);
        warnings.push(Diagnostic {
            code: "actual-text-present".to_string(),
            message: "match is covered by /ActualText; replacement is rejected in Phase 1"
                .to_string(),
        });
    }

    let ck = match &target {
        ScanTarget::Page => "p".to_string(),
        ScanTarget::Xobject(xi) => format!("x:{}", scan.xobjects[*xi].name_path.join("/")),
    };
    let payload = TokenPayload {
        v: 1,
        fp: revision.digest_hex(),
        ctr: revision.counter(),
        page: scan.page,
        ck,
        chr: [s as u64, e as u64],
        sh: token::hash64_hex(text.as_bytes()),
        ch: context_hash(&container.combined, (s, e)),
    };
    let id = MatchId(token::encode_token(&payload));

    TextMatch {
        editable: unsupported.is_none(),
        id,
        text,
        page: scan.page,
        bbox,
        spans,
        style,
        transform,
        writing_direction: WritingDirection::Ltr,
        container: container_info,
        actual_text,
        unsupported,
        warnings,
    }
}

fn run_index(bounds: &[usize], offset: usize) -> usize {
    match bounds.binary_search(&offset) {
        Ok(i) => i.min(bounds.len().saturating_sub(2)),
        Err(i) => i - 1,
    }
}

/// Hash of the text surrounding a match (±32 bytes, clamped to char
/// boundaries), used for cheap stale detection.
fn context_hash(combined: &str, range: (usize, usize)) -> String {
    let mut lo = range.0.saturating_sub(CONTEXT_WINDOW);
    while lo > 0 && !combined.is_char_boundary(lo) {
        lo -= 1;
    }
    let mut hi = (range.1 + CONTEXT_WINDOW).min(combined.len());
    while hi < combined.len() && !combined.is_char_boundary(hi) {
        hi += 1;
    }
    let mut data = Vec::new();
    data.extend_from_slice(&combined.as_bytes()[lo..range.0]);
    data.push(0);
    data.extend_from_slice(&combined.as_bytes()[range.1..hi]);
    token::hash64_hex(&data)
}

fn ranges_overlap(a: [u64; 2], b: [u64; 2]) -> bool {
    a[0] < b[1] && b[0] < a[1]
}

fn region_matches(bbox: &[f64; 4], rect: &[f64; 4], relation: RegionRelation) -> bool {
    match relation {
        RegionRelation::Intersects => {
            let w = bbox[2].min(rect[2]) - bbox[0].max(rect[0]);
            let h = bbox[3].min(rect[3]) - bbox[1].max(rect[1]);
            w > REGION_EPSILON && h > REGION_EPSILON
        }
        RegionRelation::Contained => {
            bbox[0] >= rect[0] - REGION_EPSILON
                && bbox[1] >= rect[1] - REGION_EPSILON
                && bbox[2] <= rect[2] + REGION_EPSILON
                && bbox[3] <= rect[3] + REGION_EPSILON
        }
    }
}

/// Non-overlapping occurrences of `needle` in `haystack` as byte ranges.
fn find_all(haystack: &str, needle: &str, case_insensitive: bool) -> Vec<(usize, usize)> {
    if !case_insensitive {
        return haystack
            .match_indices(needle)
            .map(|(s, m)| (s, s + m.len()))
            .collect();
    }
    let mut out = Vec::new();
    let needle_chars: Vec<char> = needle.chars().collect();
    let mut iter = haystack.char_indices().peekable();
    while let Some(&(start, _)) = iter.peek() {
        let mut probe = haystack[start..].chars();
        let mut end = start;
        let mut ok = true;
        for &nc in &needle_chars {
            match probe.next() {
                Some(hc) if chars_eq_fold(hc, nc) => end += hc.len_utf8(),
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            out.push((start, end));
            // Skip past the match (non-overlapping).
            while let Some(&(pos, _)) = iter.peek() {
                if pos < end {
                    iter.next();
                } else {
                    break;
                }
            }
        } else {
            iter.next();
        }
    }
    out
}

fn chars_eq_fold(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

// ===========================================================================
// Stream writing (swap phase)
// ===========================================================================

/// Rewrite one content stream object in place (same object id, `/Contents`
/// untouched). Mirrors the compression behaviour of the legacy write path.
fn write_stream_bytes(doc: &mut Document, stream_id: (u32, u16), bytes: &[u8]) {
    use std::io::Write;
    let compressed = {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        if encoder.write_all(bytes).is_ok() {
            encoder.finish().unwrap_or_else(|_| bytes.to_vec())
        } else {
            bytes.to_vec()
        }
    };
    let (content, use_flate) = if compressed.len() < bytes.len() {
        (compressed, true)
    } else {
        (bytes.to_vec(), false)
    };
    if let Ok(Object::Stream(ref mut s)) = doc.get_object_mut(stream_id) {
        s.content = content;
        if use_flate {
            s.dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
        } else {
            s.dict.remove(b"Filter");
        }
        s.dict
            .set("Length", Object::Integer(s.content.len() as i64));
    }
}
