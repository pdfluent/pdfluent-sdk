// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/**
 * An instance-method error is `instanceof` the PdfluentError of the entry
 * point the caller evaluated, also when an earlier evaluation in the same
 * process already guarded the shared native prototypes.
 *
 * That is every jest file after the first in a worker: jest gives each file
 * its own module registry and its own deep copy of `process`, so each file
 * evaluates `index.js` and `pdfluent-error.js` afresh and gets its own class,
 * while the `.node` binary and its prototypes are loaded once per process.
 * The guards on those prototypes used to stay the first file's, so the
 * second file's `doc.extractMarkdown(...)` threw the first file's class
 * (balanced_bindings.test.js on the 4-core Windows runner: "Expected
 * constructor: PdfluentError / Received constructor: PdfluentError").
 *
 * The test replays that inside one file: a fresh module registry
 * (`jest.isolateModules`) over a fresh copy of `process` without the
 * singleton, exactly the state a later jest file starts in.
 */

const fs = require('fs');
const path = require('path');

const ROOT = path.join(__dirname, '..', '..', '..');
const SAMPLE_PDF = path.join(ROOT, 'fixtures', 'sample.pdf');

let mod = null;
let loadError = null;
try {
  mod = require('../index');
} catch (e) {
  loadError = e;
}

if (!mod) {
  process.stderr.write(
    `SKIPPED (not a pass): error_identity_across_files.test.js -- native addon not built: ${loadError && loadError.message}\n`,
  );
  if (process.env.PDFLUENT_REQUIRE_ADDON === '1') {
    test('the native addon loads', () => {
      throw loadError;
    });
  }
}

/** Evaluate the entry point the way a later jest file in the worker does. */
function evaluateAsAnotherFile(processObject) {
  const own = globalThis.process;
  let entry;
  try {
    globalThis.process = processObject;
    jest.isolateModules(() => {
      entry = require('../index');
    });
  } finally {
    globalThis.process = own;
  }
  return entry;
}

/** A process copy as jest builds one per file: the real one, minus our singleton. */
function freshProcess() {
  const copy = Object.create(globalThis.process);
  copy.__pdfluentErrors__ = undefined;
  return copy;
}

function unknownQualityError(entry) {
  const doc = entry.PdfDocument.open(fs.readFileSync(SAMPLE_PDF));
  try {
    doc.extractMarkdown({ quality: 'ultra' });
  } catch (e) {
    return e;
  }
  throw new Error('expected extractMarkdown to throw');
}

if (mod) {
  describe('error class identity across entry-point evaluations', () => {
    let other = null;

    beforeAll(() => {
      other = evaluateAsAnotherFile(freshProcess());
    });

    afterAll(() => {
      // Hand the shared prototypes back to this file's own evaluation, so the
      // rest of the worker sees the state it would have without this file.
      evaluateAsAnotherFile(globalThis.process);
    });

    test('the replay is the jest situation: own class, shared native prototypes', () => {
      expect(other.PdfluentError).not.toBe(mod.PdfluentError);
      expect(Object.getPrototypeOf(other.PdfDocument)).toBe(
        Object.getPrototypeOf(mod.PdfDocument),
      );
    });

    test('an instance-method error is instanceof the later evaluation\'s class', () => {
      const err = unknownQualityError(other);
      expect(err).toBeInstanceOf(other.PdfluentError);
      expect(err.code).toBe('E-UNSUPPORTED');
    });

    test('an async instance-method rejection is instanceof the later evaluation\'s class', async () => {
      const doc = other.PdfDocument.open(fs.readFileSync(SAMPLE_PDF));
      let err = null;
      try {
        await doc.extractTextAsync(999);
      } catch (e) {
        err = e;
      }
      expect(err).toBeInstanceOf(other.PdfluentError);
      expect(err.code).toBe('E-PAGE-OUT-OF-RANGE');
    });

    test('evaluating again hands the prototypes back to the earlier class', () => {
      evaluateAsAnotherFile(globalThis.process);
      expect(unknownQualityError(mod)).toBeInstanceOf(mod.PdfluentError);
    });
  });
}
