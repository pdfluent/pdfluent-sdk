// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/**
 * Balanced-tier keywords on the Node extraction path (lane L7b).
 *
 * `extractMarkdown`/`extractBlocks` lost the balanced-tier keywords while the
 * CLI kept them (`--quality balanced --model-dir ...`): today `quality`,
 * `modelDir` and `keepFurniture` are silently ignored. The rebuilt binding
 * honours them again: blocks refuse the balanced tier outright (they have no
 * balanced answer), and Markdown runs it -- which, where no models are
 * installed, refuses naming the install. Either refusal is the proof the
 * keywords arrived.
 *
 * These run where no models are installed.
 *
 * Run:
 *   cd crates/pdf-node
 *   npm run build
 *   npx jest tests/balanced_keywords.test.js
 */

let addon
try {
  addon = require('../index')
} catch (e) {
  process.stderr.write('SKIPPED (not a pass): native addon not built — balanced keywords not checked\n')
}
const d = addon ? describe : describe.skip

const fs = require('fs')
const path = require('path')

const FIXTURES = path.join(__dirname, '..', '..', '..', 'fixtures')
const SAMPLE_PDF = path.join(FIXTURES, 'sample.pdf')

function open() {
  return addon.PdfDocument.open(fs.readFileSync(SAMPLE_PDF))
}

d('balanced-tier keywords (lane L7b)', () => {
  // Each fails on a binding that drops the options, where the keywords are
  // silently ignored and the fast tier answers instead (checked 14-10 by
  // wrapping both methods to drop their argument: 2 of 2 fail).
  test('extractBlocks refuses the balanced tier', () => {
    // Needs no models: the refusal fires before any weights are looked for.
    // The contract's refusal (balanced_bindings.test.js), not just any error.
    expect(() => open().extractBlocks({ quality: 'balanced' })).toThrow(/markdown-only/)
  })

  test('extractMarkdown runs the balanced tier', () => {
    // No models installed: the tier refuses naming the install. The refusal
    // -- not a silent fast-tier answer -- is the proof the keywords arrived.
    let err
    try {
      open().extractMarkdown({
        quality: 'balanced',
        modelDir: '/nonexistent-model-dir',
        keepFurniture: true,
      })
    } catch (e) {
      err = e
    }
    expect(err).toBeDefined()
    // The typed model refusal names the missing model and the install line.
    expect(String(err && err.message)).toMatch(/is not available/)
    expect(String(err && err.message)).toMatch(/pdfluent models install layout/)
  })
})
