# Word separation: solved, and the measurement mistake that hid it

**Status 2026-09-02: all six documents are fixed.** This note is kept because
its dead ends are instructive, but read the closing section first -- one of the
four things it tells you not to re-test is exactly what worked, and the reason
it looked dead is a measurement error worth more than the fix.

Everything below the line is the note as it stood on 2026-09-01.

---

# (2026-09-01) What is fixed, and what the last two documents are not

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


---

# 2026-09-02: solved, and why this note said it was not

`002_002193` and `002_002166` are fixed. Through the shipping pipeline,
`pdfa::convert_bytes`:

| | source | converted |
|---|---:|---:|
| `002_002193` | 3265 | 3377 |
| `002_002166` | 5458 | 5864 |

veraPDF 2b verdicts on all six are unchanged by the fix.

## The fix is the thing this note called dead

A code the font's own ToUnicode calls a space is never condemned. That is the
"exempting the space code" experiment above, which the note reports as changing
nothing and lists under "do not re-test". It works.

## Why it looked dead: the harness measured the wrong pipeline

The note says so itself, in the sentence nobody weighed: *"The harness is four
lines of `save_to` between the stage calls in `convert_pdfa`."*

`convert_pdfa` calls `fix_simple_font_out_of_range_codes`. That function's own
doc comment says it is superseded by `fix_simple_font_streams`. What ships is
`pdfa::convert_bytes`, and it calls the other one. So every measurement in this
note -- the stage-15 bisect, the five-site exemption, the paired-code lane, the
character counts -- describes a code path users do not run.

The first version of the fix repeated the error in the other direction: it
changed the shipped function and was measured with the superseded example, and
reported six documents fixed while the shipped pipeline was unchanged. Both
mistakes are the same one. A colleague reading the example's source found it,
which is why the entry point is now named in every measurement here.

## What the shipped pipeline actually does

Writing the document out after each of the forty font steps in
`pdfa::convert_bytes` and counting spaces:

    simple_range_notdef      3384   <- fix_simple_font_streams; holds
    subset_missing_glyphs     134   <- fix_type1_subset_missing_glyphs; loses them
    width_mismatches          127

Not stage 15 of 26, and not `fix_symbolic_font_notdef_streams`. The pass that
destroys the spaces is `fix_type1_subset_missing_glyphs`, which builds its own
condemned set through `collect_simple_invalid_codes` and never knew the rule.

## One null result worth keeping

Putting the rule at the tail of `collect_simple_invalid_codes` did nothing. That
helper returns early on both the TrueType and the Type 1 route, so its tail is
dead code for every real font: the shipped pipeline still fell from 3384 spaces
to 134 with the filter in place. The rule lives in a wrapper the routes cannot
bypass.

## What is still true above

The four dead ends are still dead **on the path they were measured on**, and
three of them are dead everywhere: the CID-0 analogy, the declared-width theory,
and the paired-code lane do not depend on which entry point runs them. Only the
space-code exemption was a false negative, and only because of the entry point.

The "what turns a space into a glyph" reframing was a good question asked about
the wrong pipeline. On the shipped path nothing turns a space into a glyph: the
byte is overwritten with `0x20`, in a font that has no glyph for `0x20`, and the
space is lost rather than transformed.

## The rule that comes out of this

Measure through the entry point the product uses. An example binary is not the
product, and a doc comment saying "superseded" is a load-bearing sentence.
