# PDFluent Cookbook

> 20+ recipes for common PDF tasks with PDFluent SDK.

The ten recipes at <https://pdfluent.com/cookbook/> are the ones that are
compiled: each is a `// site:<name>` block in
[`crates/pdfluent/examples/site_snippets.rs`](../../crates/pdfluent/examples/site_snippets.rs),
built by CI, and the page links back to it. The pages below are hand-written
and cover more ground; nothing builds them, so prefer the site's version of a
recipe when both carry one (#167, #247).

## Categories

### [Reading](./01-reading.md)
- Extract text from PDF
- Extract images from PDF
- Extract tables from PDF
- Get PDF metadata
- List all pages

### [Writing](./02-writing.md)
- Create a new PDF
- Add pages to PDF
- Remove pages from PDF
- Merge multiple PDFs
- Split a PDF

### [Forms](./03-forms.md)
- Fill a form field
- Flatten an XFA form
- Extract XFA form data
- Execute FormCalc expressions

### [Compliance](./04-compliance.md)
- Validate PDF/A compliance
- Convert to PDF/A
- Create ZUGFeRD invoice
- Verify signature

### [Security](./05-security.md)
- Sign a PDF with PAdES
- Verify a signature
- Encrypt a PDF
- Decrypt a PDF

---

*Last updated: May 2026*
