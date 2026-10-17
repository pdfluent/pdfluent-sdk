/**
 * The Node error contract: a real failure throws a PdfluentError whose `.code`
 * is the catalogue code and whose `.operation` names the call (#587).
 *
 * Until #587 no operation could produce one. Every call-site built a bare
 * napi error from a string, `.code` read `GenericFailure`, and the class the
 * typings promised was constructed nowhere -- while the docs told consumers to
 * branch on `.code` and never on the message. Each case below provokes one
 * code on a real document; route a call-site back to a plain string error and
 * the case named after that code and call goes red.
 *
 * The last test ties the file to docs/error_catalogue.md: every code the Node
 * column says this binding raises is provoked here, or excused below with the
 * reason it cannot be provoked from a test.
 */

const fs = require('fs');
const os = require('os');
const path = require('path');

const ROOT = path.join(__dirname, '..', '..', '..');
const SAMPLE_PDF = path.join(ROOT, 'fixtures', 'sample.pdf');
const ACROFORM_PDF = path.join(ROOT, 'fixtures', 'acroform.pdf');
const CATALOGUE = path.join(ROOT, 'docs', 'error_catalogue.md');

let mod = null;
let loadError = null;
try {
  mod = require('../index');
} catch (e) {
  loadError = e;
}

if (!mod) {
  // jest does not print console output from a suite that is skipped whole;
  // a raw stream write is the one thing that reaches the log.
  process.stderr.write(
    `SKIPPED (not a pass): error_contract.test.js -- native addon not built: ${loadError && loadError.message}\n`
  );
  // The landing-lane gate built the addon on purpose; a skip there would be a
  // pass that tested nothing (scripts/ci/the_node_error_contract_holds.sh).
  if (process.env.PDFLUENT_REQUIRE_ADDON === '1') {
    test('the native addon loads', () => {
      throw loadError;
    });
  }
}

const d = mod ? describe : describe.skip;

// Codes the Node column claims but no test can provoke on purpose.
const NOT_PROVOKED = {
  'E-INTERNAL': 'raised when a worker thread dies or a serialiser fails; no input reaches it',
  'E-BUDGET-RESOURCE-LIMIT': 'needs a document past a default engine processing limit',
};

function tmpPath(name) {
  return path.join(os.tmpdir(), `pdf-node-error-contract-${name}-${process.pid}-${Date.now()}.pdf`);
}

function capture(fn) {
  try {
    fn();
  } catch (e) {
    return e;
  }
  throw new Error('expected the call to throw');
}

async function captureAsync(fn) {
  try {
    await fn();
  } catch (e) {
    return e;
  }
  throw new Error('expected the call to reject');
}

d('the Node error contract', () => {
  const sample = () => fs.readFileSync(SAMPLE_PDF);
  let encrypted;

  beforeAll(() => {
    const out = tmpPath('encrypted');
    mod.PdfDocument.open(sample()).encrypt(out, 'right-password');
    encrypted = fs.readFileSync(out);
    fs.unlinkSync(out);
  });

  // [code, operation, provoke]
  const SYNC_CASES = [
    ['E-PARSE-INVALID-PDF', 'open', () => mod.PdfDocument.open(Buffer.from('not a pdf'))],
    ['E-PARSE-INVALID-PDF', 'open', () => mod.TextEditor.open(Buffer.from('not a pdf'))],
    ['E-IO-FILE-NOT-FOUND', 'openPdf', () => mod.openPdf(path.join(os.tmpdir(), 'no-such-file-587.pdf'))],
    ['E-IO-FILE-NOT-FOUND', 'mergePdfs', () => mod.mergePdfs([path.join(os.tmpdir(), 'no-such-file-587.pdf')], tmpPath('merged'))],
    ['E-IO-GENERIC', 'openPdf', () => mod.openPdf(os.tmpdir())],
    ['E-IO-GENERIC', 'validatePdfa', () => mod.validatePdfa(os.tmpdir(), '2b')],
    ['E-SECURITY-DECRYPTION-FAILED', 'open', () => mod.PdfDocument.open(encrypted)],
    ['E-SECURITY-DECRYPTION-FAILED', 'openWithPassword', () => mod.PdfDocument.openWithPassword(encrypted, 'wrong-password')],
    ['E-PAGE-OUT-OF-RANGE', 'page', () => mod.PdfDocument.open(sample()).page(999)],
    ['E-PAGE-OUT-OF-RANGE', 'renderPage', () => mod.PdfDocument.open(sample()).renderPage(999)],
    ['E-PAGE-OUT-OF-RANGE', 'extractText', () => mod.PdfDocument.open(sample()).extractText(999)],
    ['E-PAGE-OUT-OF-RANGE', 'pageGeometry', () => mod.PdfDocument.open(sample()).pageGeometry(999)],
    ['E-PAGE-OUT-OF-RANGE', 'annotations', () => mod.PdfDocument.open(sample()).annotations(999)],
    ['E-UNSUPPORTED', 'addAnnotation', () => mod.PdfDocument.open(sample()).addAnnotation(0, 'no-such-type', [0, 0, 10, 10], null)],
    ['E-UNSUPPORTED', 'validatePdfa', () => mod.PdfDocument.open(sample()).validatePdfa('9z')],
    ['E-UNSUPPORTED', 'mergePdfs', () => mod.mergePdfs([], tmpPath('merged'))],
    ['E-UNSUPPORTED', 'setFormField', () => mod.PdfDocument.open(fs.readFileSync(ACROFORM_PDF)).setFormField('no.such.field', 'x')],
    ['E-UNSUPPORTED', 'setFieldValue', () => mod.PdfDocument.open(fs.readFileSync(ACROFORM_PDF)).setFieldValue('no.such.field', 'x')],
    ['E-UNSUPPORTED', 'findText', () => mod.TextEditor.open(sample()).findText('{"txet":"typo"}')],
    ['E-EDIT-TEXT-FAILED', 'replaceMatches', () => mod.TextEditor.open(sample()).replaceMatches('[{"id":"stale","text":"x"}]', '{}')],
  ];

  test.each(SYNC_CASES)('%s from %s', (code, operation, provoke) => {
    const err = capture(provoke);
    expect(err).toBeInstanceOf(mod.PdfluentError);
    expect(err).toBeInstanceOf(Error);
    expect({ code: err.code, operation: err.operation }).toEqual({ code, operation });
    expect(err.name).toBe('PdfluentError');
    // The payload is unpacked, not passed through: a message that still
    // starts with the JSON brace means the layer never ran.
    expect(err.message).not.toMatch(/^\{/);
    expect(err.message.length).toBeGreaterThan(0);
  });

  const ASYNC_CASES = [
    ['E-PARSE-INVALID-PDF', 'openAsync', () => mod.PdfDocument.openAsync(Buffer.from('not a pdf'))],
    ['E-PAGE-OUT-OF-RANGE', 'renderPageAsync', () => mod.PdfDocument.open(sample()).renderPageAsync(999)],
    ['E-PAGE-OUT-OF-RANGE', 'extractTextAsync', () => mod.PdfDocument.open(sample()).extractTextAsync(999)],
    ['E-PAGE-OUT-OF-RANGE', 'thumbnail', () => mod.PdfDocument.open(sample()).thumbnail(999)],
  ];

  test.each(ASYNC_CASES)('%s from %s (rejected promise)', async (code, operation, provoke) => {
    const err = await captureAsync(provoke);
    expect(err).toBeInstanceOf(mod.PdfluentError);
    expect({ code: err.code, operation: err.operation }).toEqual({ code, operation });
  });

  test('a PdfluentError built without metadata still has string code and operation', () => {
    const e = new mod.PdfluentError('by hand');
    expect(typeof e.code).toBe('string');
    expect(typeof e.operation).toBe('string');
  });

  test('a PdfPage the native side created is covered too', () => {
    const doc = mod.PdfDocument.open(sample());
    const page = doc.page(0);
    expect(page).toBeInstanceOf(mod.PdfPage);
    expect(doc).toBeInstanceOf(mod.PdfDocument);
    expect(mod.PdfDocument.name).toBe('PdfDocument');
  });

  test('every code the Node column says is raised is provoked here', () => {
    const provoked = new Set([...SYNC_CASES, ...ASYNC_CASES].map(([code]) => code));
    const lines = fs.readFileSync(CATALOGUE, 'utf8').split('\n');
    const header = lines.find((l) => l.startsWith('| Code |')).split(' | ').map((c) => c.trim());
    const column = header.indexOf('Node');
    const raised = lines
      .filter((l) => l.startsWith('| `E-'))
      .map((l) => l.split(' | '))
      .filter((cells) => cells[column].trim() === 'PdfluentError')
      .map((cells) => cells[0].replace(/^\| `|`$/g, ''));
    expect(raised.length).toBeGreaterThan(0);
    const unaccounted = raised.filter((code) => !provoked.has(code) && !(code in NOT_PROVOKED));
    expect(unaccounted).toEqual([]);
  });
});
