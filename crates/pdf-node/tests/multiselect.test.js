// Multi-select list box binding test (AcroForm closure).
// Self-contained: the fixture is embedded as base64 so the test needs no
// external file and runs in CI.
const fs = require('fs');
const os = require('os');
const path = require('path');

// A minimal AcroForm with one multi-select list box "languages" whose /Opt is
// ["EN","NL","DE","FR"] (from pdfluent-forms gen_acroform_corpus).
const MULTISELECT_PDF_B64 = 'JVBERi0xLjcKJbutwN4KMSAwIG9iago8PC9UeXBlL1BhZ2VzL0tpZHNbMyAwIFJdL0NvdW50IDE+PgplbmRvYmoKMiAwIG9iago8PC9MZW5ndGggMD4+c3RyZWFtCgplbmRzdHJlYW0gCmVuZG9iagozIDAgb2JqCjw8L1R5cGUvUGFnZS9QYXJlbnQgMSAwIFIvTWVkaWFCb3hbMCAwIDYxMiA3OTJdL0NvbnRlbnRzIDIgMCBSL1Jlc291cmNlczw8Pj4vQW5ub3RzWzQgMCBSXT4+CmVuZG9iago0IDAgb2JqCjw8L1R5cGUvQW5ub3QvU3VidHlwZS9XaWRnZXQvRlQvQ2gvVChsYW5ndWFnZXMpL0ZmIDIwOTcxNTIvUmVjdFsxMDAgNDAwIDMyMCA1MjBdL09wdFsoRU4pKE5MKShERSkoRlIpXT4+CmVuZG9iago1IDAgb2JqCjw8L0ZpZWxkc1s0IDAgUl0vREEoL0hlbHYgMCBUZiAwIGcpPj4KZW5kb2JqCjYgMCBvYmoKPDwvVHlwZS9DYXRhbG9nL1BhZ2VzIDEgMCBSL0Fjcm9Gb3JtIDUgMCBSPj4KZW5kb2JqCjcgMCBvYmoKPDwvUm9vdCA2IDAgUi9UeXBlL1hSZWYvU2l6ZSA4L1dbMSA0IDJdL0luZGV4WzEgN10vTGVuZ3RoIDQ5Pj5zdHJlYW0KAQAAAA8AAAEAAABCAAABAAAAcQAAAQAAAN0AAAEAAAFVAAABAAABigAAAQAAAcYAAAplbmRzdHJlYW0gCmVuZG9iagoKc3RhcnR4cmVmCjQ1NAolJUVPRg==';

let PdfDocument;
try {
  ({ PdfDocument } = require('../index'));
} catch (e) {
  PdfDocument = null;
}

const d = PdfDocument ? describe : describe.skip;

d('AcroForm multi-select list box', () => {
  const bytes = Buffer.from(MULTISELECT_PDF_B64, 'base64');
  const tmp = () => path.join(os.tmpdir(), `ms-${Date.now()}-${Math.random().toString(36).slice(2)}.pdf`);

  test('setMultiSelect persists across save/reopen', () => {
    const doc = PdfDocument.open(bytes);
    expect(() => doc.setMultiSelect('languages', ['FR', 'EN'])).not.toThrow();
    const out = tmp();
    doc.save(out);
    const reloaded = PdfDocument.open(fs.readFileSync(out));
    const field = reloaded.getFormFields().find(f => f.name === 'languages');
    expect(field).toBeDefined();
    // /V is an array → the flat read surface joins it; both options present.
    expect(field.value).toContain('FR');
    expect(field.value).toContain('EN');
    fs.unlinkSync(out);
  });

  test('setMultiSelect rejects an unknown option', () => {
    const doc = PdfDocument.open(bytes);
    expect(() => doc.setMultiSelect('languages', ['KL'])).toThrow();
  });

  test('setMultiSelect with empty array clears the selection', () => {
    const doc = PdfDocument.open(bytes);
    expect(() => doc.setMultiSelect('languages', [])).not.toThrow();
  });
});
