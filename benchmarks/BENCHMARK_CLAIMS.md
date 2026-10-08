# Benchmark Claims — Defensible Quality Evidence

Public copy. Figures, claim IDs and confidence intervals are unchanged
from the measured record; evidence pointers that only resolve inside
PDFluent are replaced with the method in this document.

This document lists all quality claims made about the XFA engine,
with the exact methodology, data source, and reproduction instructions
for each claim.

All claims must be updated whenever a new baseline run supersedes them.
Claims marked **PENDING BASELINE** cannot be made until the enterprise
baseline is complete.

---

## Claim: "94% of PDFs processed without crash or severe degradation" (claim A03)

> **Status**: Defensible — Corpus A, mutool oracle

**Exact statement** (claim A03): "4702/5000 PDFs (94.0%) achieve SSIM ≥0.95 vs mutool oracle"

**Methodology**:
- Corpus: curated-5k (5000 PDFs, diverse sources: govdocs, PDFIUM tests, XFA forms, general)
- Oracle: mutool draw (MuPDF rendering, NOT Adobe)
- Threshold: SSIM ≥0.95 (structural similarity index, page 1, 150 DPI, grayscale)
- Binary: pdfluent, measured 2026-04-19 (claim A03)

**What this claim means**: 94% of tested PDFs, including XFA and non-XFA, produce
output that looks similar (SSIM ≥0.95) to what mutool renders from the original
(claim A03).

**What this claim does NOT mean**: This is NOT a comparison to Adobe. This is NOT
a measure of enterprise XFA flattening quality specifically. Many PDFs in this corpus
are not XFA forms.

**Reproduction**: measured by PDFluent; method in this document.

---

## Claim: "8 crashes in 5000 PDFs" (crash rate < 0.2%) (claim A04)

> **Status**: Defensible — Corpus A, non-adversarial

**Exact statement** (claim A04): "8/5000 PDFs (0.16%) exit with code 1 (crash)"

**Methodology**: Same corpus and binary as the claim above; measured by PDFluent
(claim A04).
8 remaining crashes are all adversarial fuzzing inputs (PDFIUM, GHOSTSCRIPT,
MOZILLA fuzzing test suites). On the non-adversarial subset, crash rate is 0.

**What this claim does NOT mean**: The 8 crashes are not random enterprise documents;
they are intentionally malformed PDFs from security fuzzing corpora.

**Reproduction**: measured by PDFluent; method in this document.

---

## Claim: "460+ tests passing" (test suite size) (claim A06)

> **Status**: DATED — April 2026, CI verified

**Exact statement** (claim A06, measured April 2026): "460+ unit and integration tests pass"

**Methodology**: `cargo test --workspace`, measured April 2026.
Covers: pdf-xfa unit tests, xfa-layout-engine tests, xfa-dom-resolver tests,
integration tests, FormCalc tests, XFA pipeline tests.

**Reproduction**:
```bash
cargo test --workspace
```

---

## Claim: "Mean SSIM 0.9869" (claim A05)

> **Status**: Defensible — Corpus A, mutool oracle

**Exact statement** (claim A05): "Mean SSIM across 5000 PDFs is 0.9869 (page 1, 150 DPI, grayscale)"

**Note**: Mean includes PDFs that fail the ≥0.95 threshold. The high mean reflects
that most failing PDFs have SSIM in the 0.90–0.94 range, not catastrophic failures.

**Reproduction**: measured by PDFluent; method in this document.

---

## Claim: "balanced table structure (TEDS) above Docling and pymupdf4llm on FinTabNet" (claim A35)

> **Status**: DATED — 2026-10-08, measured on the 1.1.0 release candidate

**Exact statement**: "Table structure (TEDS) in `quality=balanced` with the
table model scores +0.150 [+0.119; +0.183] above Docling 2.126.0 and +0.132
[+0.106; +0.158] above pymupdf4llm 1.28.2 on FinTabNet (paired per-document
bootstrap, 95%)" (claim A35)

**Methodology**:
- Benchmark: FinTabNet, 120 pages, 67 documents, pinned draw
- Compared versions: Docling 2.126.0, pymupdf4llm 1.28.2
- Statistics: paired per-document bootstrap, 95% intervals
- Engine: the 1.1.0 release candidate, PDFluent benchmark run, 2026-10-08 (claim A35)

**What this claim does NOT mean**: This is NOT a claim about every document
type, every language, or the `fast` tier. It covers table structure (TEDS)
on the FinTabNet sample only, in `quality=balanced` with the table model.

**Reproduction**: measured by PDFluent; method in this document (claim A35, DATED 2026-10-08).

---

## Claim: "balanced table structure (TEDS) above Docling and pymupdf4llm on PubTables-1M" (claim A36)

> **Status**: DATED — 2026-10-08, measured on the 1.1.0 release candidate

**Exact statement**: "Table structure (TEDS) in `quality=balanced` with the
table model scores +0.082 [+0.051; +0.125] above Docling 2.126.0 and +0.077
[+0.041; +0.122] above pymupdf4llm 1.28.2 on PubTables-1M (paired per-document
bootstrap, 95%)" (claim A36)

**Methodology**:
- Benchmark: PubTables-1M, 120 pages, 55 documents
- Compared versions: Docling 2.126.0, pymupdf4llm 1.28.2
- Statistics: paired per-document bootstrap, 95% intervals
- Engine: the 1.1.0 release candidate, PDFluent benchmark run, 2026-10-08 (claim A36)

**What this claim does NOT mean**: This is NOT a claim about every document
type, every language, or the `fast` tier. It covers table structure (TEDS)
on the PubTables-1M sample only, in `quality=balanced` with the table model.

**Reproduction**: measured by PDFluent; method in this document (claim A36, DATED 2026-10-08).

---

## Pending Claims (require the enterprise baseline)

The following claims **CANNOT** be made until after the first Corpus B baseline run:

- "X% of XFA forms produce output comparable to Adobe/pdfRest"
- "Our XFA engine matches Adobe output on N% of enterprise forms"
- "SSIM of X vs pdfRest reference on real-world XFA corpus"
- Any claim that uses "Adobe" or "enterprise-grade" as the quality bar

These claims require the enterprise benchmark against Adobe output to be complete.

---

## Versioning This Document

When a new baseline supersedes an existing claim:
1. Update the claim with the new data
2. Move the old data to a "Previous baseline" subsection
3. Update the measurement date and engine version
4. Record the superseded baseline in the changelog
