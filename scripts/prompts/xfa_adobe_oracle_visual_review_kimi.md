# Kimi CLI — XFA Adobe/pdfRest Oracle Visual Review Prompt

Offline multimodal triage of PDFluent-vs-oracle flatten renders, **oracle-covered
pages only**. Invoke per doc with the covered PNG pairs attached.

## Invocation

```
kimi --image pdfrest_page-<N>.png --image pf-<N>.png \
     --prompt "$(cat scripts/prompts/xfa_adobe_oracle_visual_review_kimi.md)"
```

## Prompt body

You are comparing two renders of the SAME flattened PDF page:
- IMAGE 1 = Adobe/pdfRest oracle flatten (reference / ground truth).
- IMAGE 2 = PDFluent flatten output (under test).

Compare ONLY this page. Do not assume anything about other pages.

Report strictly as JSON:

```json
{
  "page": <int>,
  "match": "full" | "minor" | "major",
  "defect_bucket": "OK" | "MISSING_STATIC_DRAW_TEXT" | "MISSING_CAPTIONS"
    | "CAPTION_OVERLAP" | "TABLE_REFLOW_DIFF" | "FONT_SUBSTITUTION"
    | "WHITESPACE_DIFF" | "OTHER",
  "note": "<one concise sentence, no PII, describe the visual difference>"
}
```

Rules:
- "full" = visually equivalent (ignore antialiasing/whitespace/font-hinting noise).
- "minor" = cosmetic only (spacing, color, compression artifacts).
- "major" = content fidelity loss (missing text/captions, overlap, wrong reflow).
- Never invent content not visible in the images.
- Output ONLY the JSON object. No prose before or after.
