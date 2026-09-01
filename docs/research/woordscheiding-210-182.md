# Word separation: what is fixed, and what the last two documents are not

Status on 2026-09-01. Four of the six documents in #210 are fixed; two are not,
and this records what they are **not**, so the next attempt does not spend a day
re-walking the same three hypotheses.

## The fixed four

`000_000338`, `001_001696`, `000_000956`, `002_002195`. One cause, written down
four times in `fix_cid_font_notdef`: that CID 0 is `.notdef`. Some subsetted CID
fonts put the space there and say so in their own ToUnicode, `<0000>` to
`<0020>`. See the commit for the four sites and the measurements.

## The other two are a different fault

`002_002193` and `002_002166`. Everything below is measured, not reasoned.

**They never reach `fix_cid_font_notdef`.** A probe printing every font that
function repairs prints nothing for either document. Their fonts are simple
TrueType and Type1, not CID:

    002_002193: TrueType 5, Type1 6
    002_002166: TrueType 8, Type1 6

**The loss is at stage 15 of 26, `fix_symbolic_font_notdef_streams`.** Bisected
by saving after each stage and counting spaces from outside:
`002_002193` holds 3265 spaces through stage 14 and has 134 after stage 15.

**That function does condemn the space code.** For several fonts in both
documents it puts `0x20` in `invalid_codes`. The reason is visible: with no
`/Encoding` and no `/Differences`, `glyph_name` is `None`, so the decision falls
to the font's cmap — and for the condemning fonts the cmap has **no entry for
`0x20` at all**.

## What that is not, and this is the part worth keeping

**It is not the CID-0 fault in another dress.** The tempting move is to port the
fix by analogy: CID 0 is a blank the font declares in its ToUnicode, so look for
the same here. These fonts have no such declaration, and the glyph is genuinely
absent rather than blank.

**It is not "the space carries an advance".** The next hypothesis was that
`0x20` has a declared width and removing the code removes the advance. Two of
the fonts do declare one — 277, 278, 250 — but the fonts that actually condemn
the space have `/Widths` arrays of **2 and 8 entries** with `FirstChar 0`, so
they declare no width for `0x20` at all.

**And condemning `0x20` is not the cause.** This is the finding that cost the
most and is worth the most. Keeping `0x20` unconditionally — `code == b' '`
exempted from `invalid_codes` with no other condition — changes nothing:

    002_002193   3265 -> 127   (unchanged)
    002_002166   5458 -> 1046  (unchanged)

So stage 15 destroys the word separation, and it is *not* doing it through the
space code. Something else in that function does it, and the three hypotheses
above are all excluded by measurement.

## Where to start

Inside `fix_symbolic_font_notdef_streams`, after `invalid_codes` is built:
what does the stream rewriting do to codes other than `0x20`, and how does that
remove spacing? The bisect harness that found the stage is four lines of
`save_to` between stage calls; it is worth rebuilding rather than reasoning.

Do not re-test: the CID-0 analogy, the declared-width theory, or exempting the
space code. All three are dead by measurement, above.
