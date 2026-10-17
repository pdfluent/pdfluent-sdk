#!/usr/bin/env python3
"""What a block is, seen from the whole DOCUMENT rather than from its page (#599).

The block features of `features.py` describe a block against its own page: how
big it is against the body, how far down it sits, what stands beside it. The
heading decision is not on the page -- #583 measured two per-page rules and
both refused as many true headings as false ones -- and the trained classifier
stopped improving after 35 trees on 1.4 million rows, which is a model that has
run out of SIGNAL rather than out of data.

So these are the facts a reader uses and the model could not see: whether this
block's TYPE is one the document uses for a handful of lines or for half of
itself, how many distinct sizes stand above it, whether its face is the body's
face, how it is numbered, and how its measure compares with the column rather
than with the page.

Every one is computed from what the dump already carries, so no page is read
again; and every one is computable in the engine at the point `refine` runs,
where the whole document is in hand.
"""
from __future__ import annotations

import collections
import math
import re

# WHAT THE BLOCK ITSELF SAYS, once you know the page it is on: these vary
# block by block and a tree cannot use them to recognise a DOCUMENT.
LOCAL_FEATURES: list[str] = [
    "font_is_the_body_face", "title_case", "number_kind", "number_depth",
    "width_vs_column", "ends_comma",
]

# WHAT THE DOCUMENT SAYS ABOUT THE BLOCK. These take few values inside one
# document -- a report has a handful of type styles -- so a model split BY
# DOCUMENT can use them to recognise which document it is looking at rather
# than what the block is. Measured on the whole training split: adding them
# cost the heading axis 0.783 -> 0.658 while the accuracy over every class fell
# too, which is what memorising a document family looks like from outside.
DOCUMENT_ONLY_FEATURES: list[str] = [
    "sizes_above", "style_blocks", "style_share", "size_vs_document",
    "style_is_rare",
]

DOCUMENT_FEATURES: list[str] = LOCAL_FEATURES + DOCUMENT_ONLY_FEATURES

# `1.`, `1.2.3`, `IV.`, `A)` -- the four ways a document numbers a section, and
# the depth of the first. Kept apart because `A` and `1` mean different things
# at the same depth: a letter is usually a subsection and a roman numeral a
# front-matter part.
ARABIC = re.compile(r"^\s*(\d+(?:\.\d+)*)[.)]?\s")
ROMAN = re.compile(r"^\s*([IVXLCDM]+)[.)]\s")
LETTER = re.compile(r"^\s*([A-Za-z])[.)]\s")


def numbering(text: str) -> tuple[int, int]:
    """Which numbering a block opens with, and how deep it goes."""
    if match := ARABIC.match(text):
        return 1, match.group(1).count(".") + 1
    if ROMAN.match(text):
        return 2, 1
    if LETTER.match(text):
        return 3, 1
    return 0, 0


def title_case(text: str) -> bool:
    """Whether the words are capitalised the way a title is, and not shouted."""
    words = [word for word in text.split() if any(c.isalpha() for c in word)]
    if len(words) < 2 or text.isupper():
        return False
    capitalised = sum(1 for word in words if word[:1].isupper())
    return capitalised / len(words) >= 0.6


def style_of(block: dict) -> tuple:
    """The type a block is set in, as a document groups it."""
    return (round(float(block.get("size") or 0.0), 1),
            (block.get("bold") or 0.0) > 0.5,
            bool(block.get("all_caps")),
            block.get("font") or "")


def of_document(pages: list[dict]) -> dict[tuple[tuple, int], list[float]]:
    """One vector per block of one document, keyed by page and block index."""
    # KEYED BY THE PAGE'S PLACE AND NOT BY ITS ID. `feature_dump` gives every
    # page of one PDF the same `page_id` when the caller passes none -- the
    # digest of the file -- so a key of (page_id, block index) collides across
    # pages and the last page silently overwrites the first. The DocLayNet
    # driver passes a page hash per page and never showed it; the parity
    # fixture, built from a plain PDF, did.
    blocks = [((page.get("page_id"), at), block)
              for at, page in enumerate(pages)
              for block in (page.get("blocks") or [])]
    styles = collections.Counter(style_of(block) for _page, block in blocks)
    # THE FACE MOST OF THE DOCUMENT'S CHARACTERS ARE IN, counted by characters
    # and not by blocks: a document with a hundred short captions in one face
    # and twenty long paragraphs in another is set in the second.
    #
    # An earlier spelling of this multiplied the face's NAME by the character
    # count -- string repetition, not weighting -- and then asked whether a
    # block's face was a substring of that. It gave the right answer often
    # enough to look fine and the wrong one on `fixtures/structured.pdf`, where
    # the parity test caught it: Rust said 1 and the training vector said 0.
    faces: collections.Counter = collections.Counter()
    for _page, block in blocks:
        face = block.get("font")
        if face:
            faces[face] += max(1, int(block.get("n_chars") or 0))
    body_face = faces.most_common(1)[0][0] if faces else ""
    sizes = sorted({style[0] for style in styles}, reverse=True)
    widest = collections.defaultdict(list)
    for page_id, block in blocks:
        if (block.get("kind") or "") == "paragraph":
            widest[page_id].append(float(block.get("width_frac") or 0.0))
    column = {page: sorted(values)[len(values) // 2] if values else math.nan
              for page, values in widest.items()}
    total = max(1, len(blocks))
    out = {}
    for page_id, block in blocks:
        style = style_of(block)
        count = styles[style]
        width = float(block.get("width_frac") or 0.0)
        measure = column.get(page_id, math.nan)
        kind, depth = numbering(block.get("text") or "")
        out[(page_id, block.get("i"))] = [
            # LOCAL_FEATURES, in that order
            1.0 if (block.get("font") or "") == body_face else 0.0,
            1.0 if title_case(block.get("text") or "") else 0.0,
            float(kind),
            float(depth),
            width / measure if measure and not math.isnan(measure) and measure > 0 else math.nan,
            1.0 if (block.get("text") or "").rstrip().endswith(",") else 0.0,
            # DOCUMENT_ONLY_FEATURES, in that order
            float(sum(1 for size in sizes if size > style[0])),
            float(count),
            count / total,
            style[0] / max(sizes) if sizes and max(sizes) > 0 else math.nan,
            1.0 if count <= 3 else 0.0,
        ]
    return out
