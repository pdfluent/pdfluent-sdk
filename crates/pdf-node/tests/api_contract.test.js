// Cross-language AcroForm API naming contract (Node surface).
//
// Canonical names (camelCase for JS, snake_case for Rust/Python):
//   - getFormFields()                 list fields
//   - setFormField(name, value)       set a single field
//   - setMultiSelect(name, values)    set a multi-select list box
//
// Deprecated aliases kept for backward compatibility (removed in 1.0.0):
//   - formFields()      -> getFormFields()
//   - setFieldValue()   -> setFormField()
//
// This test pins BOTH surfaces so a future rename cannot silently drop either.
const fs = require('fs');
const os = require('os');
const path = require('path');

// Minimal AcroForm: one multi-select list box "languages", /Opt
// ["EN","NL","DE","FR"] (same fixture as multiselect.test.js, self-contained).
const PDF_B64 = 'JVBERi0xLjcKJbutwN4KMSAwIG9iago8PC9UeXBlL1BhZ2VzL0tpZHNbMyAwIFJdL0NvdW50IDE+PgplbmRvYmoKMiAwIG9iago8PC9MZW5ndGggMD4+c3RyZWFtCgplbmRzdHJlYW0gCmVuZG9iagozIDAgb2JqCjw8L1R5cGUvUGFnZS9QYXJlbnQgMSAwIFIvTWVkaWFCb3hbMCAwIDYxMiA3OTJdL0NvbnRlbnRzIDIgMCBSL1Jlc291cmNlczw8Pj4vQW5ub3RzWzQgMCBSXT4+CmVuZG9iago0IDAgb2JqCjw8L1R5cGUvQW5ub3QvU3VidHlwZS9XaWRnZXQvRlQvQ2gvVChsYW5ndWFnZXMpL0ZmIDIwOTcxNTIvUmVjdFsxMDAgNDAwIDMyMCA1MjBdL09wdFsoRU4pKE5MKShERSkoRlIpXT4+CmVuZG9iago1IDAgb2JqCjw8L0ZpZWxkc1s0IDAgUl0vREEoL0hlbHYgMCBUZiAwIGcpPj4KZW5kb2JqCjYgMCBvYmoKPDwvVHlwZS9DYXRhbG9nL1BhZ2VzIDEgMCBSL0Fjcm9Gb3JtIDUgMCBSPj4KZW5kb2JqCjcgMCBvYmoKPDwvUm9vdCA2IDAgUi9UeXBlL1hSZWYvU2l6ZSA4L1dbMSA0IDJdL0luZGV4WzEgN10vTGVuZ3RoIDQ5Pj5zdHJlYW0KAQAAAA8AAAEAAABCAAABAAAAcQAAAQAAAN0AAAEAAAFVAAABAAABigAAAQAAAcYAAAplbmRzdHJlYW0gCmVuZG9iagoKc3RhcnR4cmVmCjQ1NAolJUVPRg==';

let PdfDocument;
try {
  ({ PdfDocument } = require('../index'));
} catch (e) {
  PdfDocument = null;
}

const d = PdfDocument ? describe : describe.skip;

d('AcroForm API naming contract', () => {
  const bytes = Buffer.from(PDF_B64, 'base64');
  const tmp = () => path.join(os.tmpdir(), `api-${Date.now()}-${Math.random().toString(36).slice(2)}.pdf`);

  test('canonical methods are exposed', () => {
    const doc = PdfDocument.open(bytes);
    expect(typeof doc.getFormFields).toBe('function');
    expect(typeof doc.setFormField).toBe('function');
    expect(typeof doc.setMultiSelect).toBe('function');
  });

  test('deprecated aliases are still exposed', () => {
    const doc = PdfDocument.open(bytes);
    expect(typeof doc.formFields).toBe('function');
    expect(typeof doc.setFieldValue).toBe('function');
  });

  test('formFields() alias returns the same fields as getFormFields()', () => {
    const doc = PdfDocument.open(bytes);
    const canonical = doc.getFormFields().map(f => f.name).sort();
    const aliased = doc.formFields().map(f => f.name).sort();
    expect(aliased).toEqual(canonical);
    expect(canonical).toContain('languages');
  });

  test('setFieldValue() alias forwards to setFormField()', () => {
    // "languages" is a choice field; a single valid option applies via the
    // string setter. The alias must behave identically to the canonical name.
    const doc = PdfDocument.open(bytes);
    expect(() => doc.setFieldValue('languages', 'FR')).not.toThrow();
    const out = tmp();
    doc.save(out);
    const reloaded = PdfDocument.open(fs.readFileSync(out));
    const field = reloaded.getFormFields().find(f => f.name === 'languages');
    expect(field).toBeDefined();
    expect(field.value).toContain('FR');
    fs.unlinkSync(out);
  });
});
