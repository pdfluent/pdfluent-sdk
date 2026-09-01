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

**And condemning `0x20` is not the cause.** Exempting the space code changes
nothing:

    002_002193   3265 -> 127   (unchanged)
    002_002166   5458 -> 1046  (unchanged)

**Correction to the first version of this note, 2026-09-01 later the same day.**
That claim was right and the experiment behind it was weaker than described. It
exempted `0x20` at three insertion sites; there are **five** in
`fix_symbolic_font_notdef_streams`, and the two it missed are the ones that
matter for these fonts. `F4`, `F6` and `F7` declare `FirstChar 0` with
`LastChar` of 7, 1 and 2, so `0x20` is outside their declared range and is
condemned by the range loop, not by the glyph check.

Redone with all five guarded and a build that actually compiled -- the first
attempt at the five-site version failed to build and I nearly read the stale
binary's output as a result -- the numbers above are unchanged. The exclusion
stands, on better evidence than it first had.

**Not the paired-code lane either.** `fix_simple_text_string` has a heuristic
that treats a string as two-byte pairs and rewrites every other byte;
misdetecting the lane would corrupt text and spacing together. Forcing the
single-byte path changes neither document.

## What the numbers say is actually happening

Invalid codes are **replaced with `0x20`, not removed** -- read from
`fix_simple_text_string`, which assigns `bytes[idx] = 0x20`. So this function
adds spaces. And yet:

| | non-space characters | spaces |
|---|---:|---:|
| `002_002193` source | 10783 | 3265 |
| after conversion | 15036 | 127 |

**The output has 4253 more visible characters and 3138 fewer spaces.** Spaces are
not being deleted; they are becoming something that renders. That is the same
signature as the CID fault -- character retention above 100% because a space now
counts as a character -- in a document whose fonts are simple, not CID.

So the next hypothesis is not "what removes the spaces" but "what turns a space
into a glyph", and it is somewhere that runs at or before stage 15 rather than
in the code that condemns codes.

## Where to start

Not "what removes the spaces" -- nothing does. Ask what turns a space into a
visible glyph, and confirm the stage first: the bisect that put the loss at 15
was run before the CID fix landed, so re-run it on current master before
trusting it. The harness is four lines of `save_to` between the stage calls in
`convert_pdfa`, counting spaces from outside with `mutool draw -F txt`.

Two threads worth pulling, in this order because the first is cheaper:

1. **Count the characters, not just the spaces.** The 4253 extra visible
   characters are the strongest signal available and nobody has yet asked which
   glyphs they are. Diffing the extracted text of source and output line by line
   should name them in minutes.
2. **Check whether the replacement is the culprit rather than the condemnation.**
   `fix_simple_text_string` writes `0x20` over every invalid code. For a font
   whose `LastChar` is 1, nearly every code is invalid, so its text becomes a run
   of spaces -- and if some *other* pass then treats a long run of spaces as
   removable, the two together would produce exactly what is measured.

Do not re-test: the CID-0 analogy, the declared-width theory, exempting the
space code (all five sites), or the paired-code lane. All four are dead by
measurement, above.
