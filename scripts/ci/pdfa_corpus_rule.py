#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The selection rule for the PDF/A measurement corpus, in one importable place.

Three things live here, and nothing else: the seed, the order it induces, and
the classifier that decides which stratum a document belongs to. The builder
(`build_pdfa_corpus_manifest.py`), the guard (`a_manifest_pins_every_file.py`)
and the materialiser (`fetch_pdfa_corpus.py`) all import this file, so there is
exactly one definition of "which ten thousand documents" and it is the one under
review.

Why a seeded order rather than "take the first ten thousand": a corpus that is
the head of an upstream listing is a corpus whose composition is decided by
whoever numbered the files. GovDocs1's numbering follows the crawl; SAFEDOCS's
follows the SHA-256 of the content. Neither is ours to lean on, and neither can
be argued with afterwards. A published seed can: anybody can recompute the order
and check that the set we measured on is the set the rule produces.

Why the halves are drawn by hash rather than by slicing the order: the holdout
has to be independent of the tuning set for the whole life of the corpus. If it
were the tail of one order, growing the tuning set would eat into it. Hashing
the key with a different label gives two interleaved halves that stay put when
the pool grows, and a document can never migrate from one to the other.

Why the walk is content-unique on top of that: a key is a name, and upstream
holds the same bytes under more than one name -- GovDocs1 stores several
thousand documents twice, and a document can also sit in GovDocs1 and in
SAFEDOCS at once. Two names hash to two halves, so without a content rule the
sealed holdout ends up holding documents whose bytes the tuning rounds have
already seen. That is a leak, not a duplicate row, and no check on names can
find it. `content_unique` therefore walks both sources in one seeded order and
keeps the first occurrence of each distinct SHA-256.
"""

from __future__ import annotations

import hashlib
import re
import zlib

# The seed is part of the published rule. Changing it draws a different corpus
# and burns the holdout (benchmarks/HOLDOUTS.md); it is not a knob.
SEED = "pdfluent-pdfa-corpus-2026-09-07"
RULE_VERSION = "v1"

# What each half is worth. Both halves are the same size on purpose: a holdout
# smaller than the tuning set buys a weaker claim for the same work.
PER_HALF = 10_000

# How the two sources divide a half. GovDocs1 is the archival body -- government
# documents from a 2008 crawl, which is the population a PDF/A archiving claim is
# actually about. SAFEDOCS is the modern tail: it is where transparency, PDF 1.7,
# tagged structure and this decade's producers live, and GovDocs1 has almost none
# of them. Four in ten is enough to fill those strata without turning an
# archiving corpus into a web crawl.
SOURCE_SHARE = {"govdocs1": 6_000, "safedocs": 4_000}

# Every stratum cell carries this floor, in each half separately. A cell that
# cannot reach it is recorded with the number actually found, never rounded up.
STRATUM_FLOOR = 100

SIZE_BANDS = (
    ("lt_100k", 0, 100_000),
    ("100k_1m", 100_000, 1_000_000),
    ("1m_10m", 1_000_000, 10_000_000),
    ("ge_10m", 10_000_000, 1 << 62),
)

# Feature classes that carry a floor. `encrypted` and `tagged` are recorded per
# document but carry none: encryption sits at roughly five in ten thousand
# upstream (SAFEDOCS pdfinfo, 4,295 of 7.9 million), so a floor of a hundred
# would force the corpus to over-represent it by two orders of magnitude to
# satisfy a guard.
FLOORED_FEATURES = (
    "fonts_embedded",
    "fonts_not_embedded",
    "images",
    "transparency",
    "non_rgb_colour",
    "forms",
    "scanned_image_only",
)
RECORDED_FEATURES = FLOORED_FEATURES + ("encrypted", "tagged")

PDF_VERSIONS = ("1.0", "1.1", "1.2", "1.3", "1.4", "1.5", "1.6", "1.7", "2.0")

PRODUCER_FAMILIES = (
    "acrobat_distiller",
    "adobe_other",
    "ghostscript",
    "microsoft_office",
    "libreoffice_openoffice",
    "tex",
    "quartz_mac",
    "apache_fop",
    "scanner_ocr",
    "web_toolkit",
    "other",
    "unknown",
)

# Order matters: the first pattern that matches wins, so the specific vendors
# come before the families that would otherwise swallow them.
_PRODUCER_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    ("acrobat_distiller", re.compile(r"distiller", re.I)),
    ("scanner_ocr", re.compile(
        r"xerox|abbyy|scansoft|kofax|canon|ricoh|epson|omnipage|"
        r"finereader|scanner|toshiba e-?studio|konica", re.I)),
    ("adobe_other", re.compile(
        r"acrobat|adobe|pdfmaker|indesign|illustrator|framemaker|livecycle",
        re.I)),
    ("ghostscript", re.compile(r"ghostscript|gpl ghostscript|artifex", re.I)),
    ("microsoft_office", re.compile(
        r"microsoft|word|powerpoint|excel|visio|publisher|sharepoint", re.I)),
    ("libreoffice_openoffice", re.compile(
        r"libreoffice|openoffice|staroffice|writer", re.I)),
    ("tex", re.compile(r"pdftex|xetex|luatex|dvips|dvipdf|latex|tex live|miktex",
                       re.I)),
    ("quartz_mac", re.compile(r"quartz|mac os x|coregraphics|cairo", re.I)),
    ("apache_fop", re.compile(r"fop|apache", re.I)),
    ("web_toolkit", re.compile(
        r"wkhtmltopdf|chrome|chromium|skia|headless|itext|reportlab|tcpdf|"
        r"fpdf|dompdf|prince|weasyprint|jasper|crystal reports|pdflib|"
        r"pdf-?xchange|nitro|foxit|bullzip|pdfcreator|novapdf|amyuni", re.I)),
)


def rank(source: str, key: str) -> str:
    """The position of a document in the seeded walk. Lower sorts first."""
    return hashlib.sha256(
        f"{SEED}|rank|{source}|{key}".encode()).hexdigest()


def shard(source: str, key: str, shards: int) -> int:
    """Which entry file a document is written to.

    Its own hash, and not the leading bytes of `rank`, because the corpus is a
    PREFIX of the rank order -- the first sixteen thousand of two hundred
    thousand -- so every selected document's rank begins in the same narrow
    range. Sharding on those bytes put 44% of one half in one file and 11% in
    another, which is how a four-way split ends up producing a file over the
    repository's blob limit.
    """
    digest = hashlib.sha256(f"{SEED}|shard|{source}|{key}".encode()).hexdigest()
    return int(digest[:8], 16) % shards


def content_unique(rows: list[dict]) -> tuple[list[dict], int]:
    """Drop every document whose bytes an earlier-ranked document already has.

    One order over both sources, so which copy of a pair survives does not
    depend on which source happened to be walked first, and the survivor keeps
    whatever half its own key gives it. Returns the kept rows in seeded order
    and the number dropped, because a draw that silently shrank its pool is a
    draw whose strata counts nobody can check.
    """
    seen: set[str] = set()
    kept: list[dict] = []
    for row in sorted(rows, key=lambda r: rank(r["source"], r["key"])):
        if row["sha256"] in seen:
            continue
        seen.add(row["sha256"])
        kept.append(row)
    return kept, len(rows) - len(kept)


def half(source: str, key: str) -> str:
    """Which half a document belongs to, for the life of the corpus."""
    digest = hashlib.sha256(f"{SEED}|half|{source}|{key}".encode()).hexdigest()
    return "tuning" if int(digest[:16], 16) % 2 == 0 else "holdout"


def size_band(n: int) -> str:
    for name, low, high in SIZE_BANDS:
        if low <= n < high:
            return name
    return SIZE_BANDS[-1][0]


def producer_family(producer: str | None) -> str:
    if not producer or not producer.strip():
        return "unknown"
    for name, pattern in _PRODUCER_PATTERNS:
        if pattern.search(producer):
            return name
    return "other"


def _literal_string(body: bytes, start: int) -> bytes:
    """Read a PDF literal string starting at the opening parenthesis."""
    out = bytearray()
    depth = 0
    i = start
    while i < len(body) and i - start < 4096:
        c = body[i]
        if c == 0x5C:  # backslash
            out.extend(body[i:i + 2])
            i += 2
            continue
        if c == 0x28:  # (
            depth += 1
            if depth > 1:
                out.append(c)
        elif c == 0x29:  # )
            depth -= 1
            if depth == 0:
                return bytes(out)
            out.append(c)
        else:
            out.append(c)
        i += 1
    return bytes(out)


def _decode_pdf_string(raw: bytes) -> str:
    if raw.startswith(b"\xfe\xff"):
        return raw[2:].decode("utf-16-be", "replace")
    return raw.decode("latin-1", "replace")


_PRODUCER_KEYS = (
    re.compile(rb"/Producer\s*(\(|<)"),
    re.compile(rb"/Creator\s*(\(|<)"),
)

_UNESCAPE = re.compile(r"\\([()\\])")


def _find_producer(blobs: list[bytes]) -> str | None:
    """/Producer if the file has one, /Creator only if it does not.

    Both are searched over the whole file rather than resolved through the
    trailer, so preference has to be explicit: a Word document carries
    `/Creator (Microsoft Word)` and `/Producer (Acrobat Distiller)`, and taking
    whichever appears first in the byte stream would sort the same document
    into a different family depending on how its writer laid out the objects.
    """
    for key in _PRODUCER_KEYS:
        found = _find_with(blobs, key)
        if found:
            return found
    return None


def _find_with(blobs: list[bytes], key: re.Pattern[bytes]) -> str | None:
    for blob in blobs:
        for m in key.finditer(blob):
            if m.group(1) == b"(":
                value = _decode_pdf_string(_literal_string(blob, m.end() - 1))
            else:
                end = blob.find(b">", m.end())
                if end < 0 or end - m.end() > 4096:
                    continue
                hexed = re.sub(rb"[^0-9a-fA-F]", b"", blob[m.end():end])
                if len(hexed) % 2:
                    hexed += b"0"
                try:
                    value = _decode_pdf_string(bytes.fromhex(hexed.decode()))
                except ValueError:
                    continue
            value = _UNESCAPE.sub(r"\1",
                                  value.replace("\x00", "")).strip()
            if value:
                return value[:200]
    return None


# A cap, not a budget: a document whose streams inflate past this is classified
# on what was read. Without it one bomb in twenty thousand stalls the build.
_INFLATE_BUDGET = 24 << 20
_STREAM_CAP = 4000


def _inflated(body: bytes) -> list[bytes]:
    """Every Flate stream in the file that still inflates, up to the budget.

    A PDF written after 2005 keeps its object definitions -- and therefore its
    Producer, its font descriptors and its colour spaces -- inside compressed
    object streams. Classifying on the raw bytes alone reads those files as
    featureless, which is the failure mode this exists to avoid: it would show
    up as a corpus with a suspiciously modern-free tail.
    """
    out: list[bytes] = []
    spent = 0
    for m in re.finditer(rb"stream\r?\n", body):
        if len(out) >= _STREAM_CAP or spent >= _INFLATE_BUDGET:
            break
        chunk = body[m.end():m.end() + (2 << 20)]
        try:
            data = zlib.decompressobj().decompress(
                chunk, _INFLATE_BUDGET - spent)
        except zlib.error:
            continue
        if data:
            out.append(data)
            spent += len(data)
    return out


_MARKERS: tuple[tuple[str, re.Pattern[bytes]], ...] = (
    ("fontfile", re.compile(rb"/FontFile[23]?\b")),
    ("font", re.compile(rb"/(?:Type\s*/Font|BaseFont)\b")),
    ("images", re.compile(rb"/Subtype\s*/Image\b")),
    ("transparency", re.compile(
        rb"/SMask\b|/S\s*/Transparency\b|/BM\s*/(?!Normal|Compatible)[A-Z]")),
    ("non_rgb_colour", re.compile(
        rb"/DeviceCMYK\b|/Separation\b|/ICCBased\b|/DeviceN\b|/Indexed\b|"
        rb"/CalCMYK\b|/Lab\b")),
    ("forms", re.compile(rb"/AcroForm\b|/XFA\b")),
    ("tagged", re.compile(rb"/StructTreeRoot\b|/Marked\s+true\b")),
    ("encrypted", re.compile(rb"/Encrypt\b")),
)

_HEADER = re.compile(rb"%PDF-(\d\.\d)")


def classify(body: bytes) -> dict | None:
    """Read one document's bytes into the strata it belongs to.

    Returns None when the bytes are not a PDF at all -- an upstream corpus of
    crawled files contains HTML error pages and truncated downloads under a
    `.pdf` name, and admitting those would make the conformance denominator a
    measure of the crawl rather than of the converter.
    """
    head = body[:1024]
    m = _HEADER.search(head)
    if m is None:
        return None
    version = m.group(1).decode()
    if version not in PDF_VERSIONS:
        version = "unknown"

    blobs = [body, *_inflated(body)]
    hits = {name: any(p.search(b) for b in blobs) for name, p in _MARKERS}

    features = []
    if hits["fontfile"]:
        features.append("fonts_embedded")
    elif hits["font"]:
        features.append("fonts_not_embedded")
    for name in ("images", "transparency", "non_rgb_colour", "forms",
                 "tagged", "encrypted"):
        if hits[name]:
            features.append(name)
    if hits["images"] and not hits["fontfile"] and not hits["font"]:
        features.append("scanned_image_only")

    return {
        "version": version,
        "producer": _find_producer(blobs),
        "features": sorted(features),
    }


def floored_cells() -> list[str]:
    """Every cell that carries the floor, whether or not the corpus holds one.

    Enumerated from the rule rather than counted from the corpus, because a
    census only names cells with at least one document in them: a stratum the
    corpus holds nothing of would otherwise be invisible, which is the one way
    a gap in coverage could report as coverage.
    """
    return ([f"producer:{p}" for p in PRODUCER_FAMILIES]
            # `version:unknown` is deliberately absent. It is the residual
            # bucket -- a header naming a version this rule does not list --
            # and a corpus holding none of those is a corpus that read every
            # header, not a corpus with a gap. Flooring it would demand a
            # hundred malformed headers per half.
            + [f"version:{v}" for v in PDF_VERSIONS]
            + [f"size:{b}" for b, _low, _high in SIZE_BANDS]
            + [f"feature:{f}" for f in FLOORED_FEATURES])


def strata_of(entry: dict) -> list[str]:
    """Every cell a document counts towards, as the guard reads them."""
    cells = [
        f"producer:{entry['producer_family']}",
        f"version:{entry['version']}",
        f"size:{entry['size_band']}",
    ]
    cells += [f"feature:{f}" for f in entry["features"]]
    return cells
