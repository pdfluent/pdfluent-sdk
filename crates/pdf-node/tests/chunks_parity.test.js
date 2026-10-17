// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/**
 * Parity between the bindings: Python `iter_chunks` and Node `iterChunks`
 * give the same number of chunks and the same texts, in both modes and with
 * `hierarchical` on and off, on three fixtures -- `structured.pdf` (several
 * pages with a table and a heading), `outlined.pdf` (an outline), and
 * `sample.pdf` (one plain page).
 *
 * The Python side runs in a subprocess (`tests/chunks_parity_driver.py`) so
 * this suite needs both bindings built: the native addon here, and a Python
 * whose `pdfluent` is this tree's, as `PDFLUENT_PYTHON` (or `python3`). That
 * can be a `maturin develop` venv or a venv with this candidate's wheel
 * installed; `chunks_parity_guard.js` decides by content -- the importable
 * `chunks.py` byte-identical to this tree's, and the installed version equal
 * to this tree's -- not by path. Anything else is a loud skip, in the same
 * idiom as the addon check: the per-push run in ci.yml has neither, and a
 * stiff requirement would fail every push rather than test anything. In a
 * release lane (PDFLUENT_RELEASE_LANE=1, which node-bindings.yml sets on the
 * self-hosted pool) the same reason fails the suite instead.
 *
 * What is compared and what is not: counts and texts always, plus the
 * per-page `toc_items` and `tables` in markdown mode, whose shapes are
 * identical in both bindings. The metadata keys that differ by design --
 * `listItem` against `list_item`, `continuesPrevious`, `sectionPath`,
 * `tablePart` -- are not compared.
 *
 * Run:
 *   cd crates/pdf-node
 *   npm run build
 *   PDFLUENT_PYTHON=/path/to/venv/bin/python npx jest tests/chunks_parity.test.js
 */

const { spawnSync } = require('child_process')
const fs = require('fs')
const path = require('path')

const ROOT = path.join(__dirname, '..', '..', '..')
const FIXTURES = path.join(ROOT, 'fixtures')
const FIXTURE_PATHS = ['structured.pdf', 'outlined.pdf', 'sample.pdf'].map((n) =>
  path.join(FIXTURES, n),
)
const DRIVER = path.join(__dirname, 'chunks_parity_driver.py')
const PYTHON_CHUNKS = path.join(ROOT, 'crates', 'pdf-python', 'python', 'pdfluent', 'chunks.py')
const PYPROJECT = path.join(ROOT, 'crates', 'pdf-python', 'pyproject.toml')
const guard = require('./chunks_parity_guard')

function skipReason() {
  let iterChunks
  try {
    ;({ iterChunks } = require('../index'))
  } catch (e) {
    return 'native addon not built'
  }
  if (typeof iterChunks !== 'function') return 'iterChunks not exported (postbuild not applied?)'
  const python = process.env.PDFLUENT_PYTHON || 'python3'
  const tree = {
    chunksPath: PYTHON_CHUNKS,
    sha256: guard.sha256OfSource(PYTHON_CHUNKS),
    version: guard.projectVersion(fs.readFileSync(PYPROJECT, 'utf8')),
  }
  return guard.mismatchReason(python, guard.probePython(python), tree)
}

const reason = skipReason()
const releaseLane = guard.inReleaseLane(process.env)
if (reason && !releaseLane) {
  process.stderr.write(`SKIPPED (not a pass): ${reason} — binding parity not checked\n`)
}
if (reason && releaseLane) {
  // A release lane vouches for this parity: an unrun comparison is red here,
  // naming its cause, never a green suite with nothing in it.
  test('chunks parity prerequisites (release lane)', () => {
    throw new Error(`release lane cannot check binding parity: ${reason}`)
  })
}
const d = reason ? describe.skip : describe

d('Python iter_chunks and Node iterChunks agree', () => {
  const python = process.env.PDFLUENT_PYTHON || 'python3'
  let iterChunks
  let cases

  beforeAll(() => {
    // Required here, not at the top of this block: a skipped describe still
    // collects its body, and without the addon that require throws.
    ;({ iterChunks } = require('../index'))
    const run = spawnSync(python, [DRIVER, ...FIXTURE_PATHS], {
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
    })
    if (run.status !== 0) {
      throw new Error(`parity driver failed under ${python}:\n${run.stderr}`)
    }
    ;({ cases } = JSON.parse(run.stdout))
  }, 120000)

  for (const fixture of FIXTURE_PATHS) {
    for (const mode of ['blocks', 'markdown']) {
      for (const hierarchical of [false, true]) {
        test(`${path.basename(fixture)} ${mode} hierarchical=${hierarchical}: same counts and texts`, () => {
          const expected = cases.find(
            (c) => c.fixture === fixture && c.mode === mode && c.hierarchical === hierarchical,
          )
          const mine = iterChunks(fixture, { mode, hierarchical })
          expect(mine.length).toBe(expected.texts.length)
          expect(mine.map((c) => c.text)).toEqual(expected.texts)
          if (mode === 'markdown') {
            expect(mine.map((c) => c.metadata.toc_items)).toEqual(expected.toc_items)
            expect(mine.map((c) => c.metadata.tables)).toEqual(expected.tables)
          }
        })
      }
    }
  }
})
