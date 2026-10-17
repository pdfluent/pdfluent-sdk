// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/**
 * The guard that decides whether the parity suite may compare against a
 * Python interpreter (`chunks_parity_guard.js`). No addon and no engine: this
 * runs on every push in ci.yml, where the parity suite itself skips.
 *
 * The probe half runs a real interpreter over a fake `pdfluent` in a temp
 * directory -- a package with a `chunks.py` and a dist-info carrying a
 * version -- because that is the half a wheel install goes through.
 */

const fs = require('fs')
const os = require('os')
const path = require('path')
const { spawnSync } = require('child_process')
const guard = require('./chunks_parity_guard')

const TREE_CHUNKS = path.join(__dirname, '..', '..', 'pdf-python', 'python', 'pdfluent', 'chunks.py')

function findPython() {
  for (const p of ['python3', 'python']) {
    if (spawnSync(p, ['-c', 'import sys; sys.exit(0 if sys.version_info >= (3, 8) else 1)']).status === 0) return p
  }
  return null
}

const PYTHON = findPython()
if (!PYTHON) {
  process.stderr.write('SKIPPED (not a pass): no python3 or python >= 3.8 on PATH — the probe half of the parity guard not checked\n')
}
describe('projectVersion', () => {
  test('reads the [project] version', () => {
    expect(guard.projectVersion('[build-system]\nrequires = []\n\n[project]\nname = "pdfluent"\nversion = "1.1.0"\n')).toBe('1.1.0')
  })
  test('ignores a version in another table', () => {
    expect(guard.projectVersion('[tool.x]\nversion = "9.9.9"\n[project]\nversion = "1.1.0"\n')).toBe('1.1.0')
    expect(guard.projectVersion('[project]\nname = "p"\n[tool.x]\nversion = "9.9.9"\n')).toBeNull()
  })
  test('reads CRLF files', () => {
    expect(guard.projectVersion('[project]\r\nversion = "2.0.0"\r\n')).toBe('2.0.0')
  })
  test("reads this tree's pyproject", () => {
    const text = fs.readFileSync(path.join(__dirname, '..', '..', 'pdf-python', 'pyproject.toml'), 'utf8')
    expect(guard.projectVersion(text)).toMatch(/^\d+\.\d+\.\d+/)
  })
})

describe('sha256OfSource', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'parity-guard-sha-'))
  afterAll(() => fs.rmSync(dir, { recursive: true, force: true }))
  test('one byte more is another hash', () => {
    fs.writeFileSync(path.join(dir, 'a.py'), 'x = 1\n')
    fs.writeFileSync(path.join(dir, 'b.py'), 'x = 2\n')
    expect(guard.sha256OfSource(path.join(dir, 'a.py'))).not.toBe(guard.sha256OfSource(path.join(dir, 'b.py')))
  })
  test('CRLF and LF are the same source', () => {
    fs.writeFileSync(path.join(dir, 'lf.py'), 'x = 1\ny = 2\n')
    fs.writeFileSync(path.join(dir, 'crlf.py'), 'x = 1\r\ny = 2\r\n')
    expect(guard.sha256OfSource(path.join(dir, 'crlf.py'))).toBe(guard.sha256OfSource(path.join(dir, 'lf.py')))
  })
})

describe('mismatchReason', () => {
  const tree = { chunksPath: '/tree/chunks.py', sha256: 'aaa', version: '1.1.0' }
  test('same content and version: no reason', () => {
    expect(guard.mismatchReason('py', { file: '/venv/chunks.py', sha256: 'aaa', version: '1.1.0' }, tree)).toBeNull()
  })
  test('no import: names the interpreter and the error', () => {
    const r = guard.mismatchReason('py', { error: 'ModuleNotFoundError: x' }, tree)
    expect(r).toContain('no importable pdfluent.chunks under py')
    expect(r).toContain('ModuleNotFoundError: x')
  })
  test('other content: names both files and both hashes', () => {
    const r = guard.mismatchReason('py', { file: '/venv/chunks.py', sha256: 'bbb', version: '1.1.0' }, tree)
    expect(r).toContain('/venv/chunks.py (sha256 bbb)')
    expect(r).toContain('/tree/chunks.py (sha256 aaa)')
  })
  test('same content, other version: names both versions', () => {
    const r = guard.mismatchReason('py', { file: '/venv/chunks.py', sha256: 'aaa', version: '1.0.0' }, tree)
    expect(r).toContain('version 1.0.0, this tree is 1.1.0')
  })
  test('no installed version is a mismatch, not a match', () => {
    expect(guard.mismatchReason('py', { file: '/venv/chunks.py', sha256: 'aaa', version: null }, tree)).not.toBeNull()
  })
  test('no tree version is a mismatch, not a match', () => {
    expect(guard.mismatchReason('py', { file: '/v', sha256: 'aaa', version: null }, { ...tree, version: null })).not.toBeNull()
  })
})

describe('inReleaseLane', () => {
  test('only PDFLUENT_RELEASE_LANE=1', () => {
    expect(guard.inReleaseLane({ PDFLUENT_RELEASE_LANE: '1' })).toBe(true)
    expect(guard.inReleaseLane({ PDFLUENT_RELEASE_LANE: '0' })).toBe(false)
    expect(guard.inReleaseLane({})).toBe(false)
  })
})

const d = PYTHON ? describe : describe.skip

d('probePython over a fake installed pdfluent', () => {
  let site
  beforeAll(() => {
    site = fs.mkdtempSync(path.join(os.tmpdir(), 'parity-guard-site-'))
    fs.mkdirSync(path.join(site, 'pdfluent'))
    fs.writeFileSync(path.join(site, 'pdfluent', '__init__.py'), '')
    fs.copyFileSync(TREE_CHUNKS, path.join(site, 'pdfluent', 'chunks.py'))
    // chunks.py imports the native module; the fake stands in for it.
    fs.writeFileSync(path.join(site, 'pdfluent', '_native.py'), 'class Document: pass\n')
    fs.mkdirSync(path.join(site, 'pdfluent-1.1.0.dist-info'))
    fs.writeFileSync(path.join(site, 'pdfluent-1.1.0.dist-info', 'METADATA'), 'Metadata-Version: 2.1\nName: pdfluent\nVersion: 1.1.0\n')
  })
  afterAll(() => fs.rmSync(site, { recursive: true, force: true }))
  const env = () => ({ ...process.env, PYTHONPATH: site })

  test("reports the file, this tree's hash and the dist-info version", () => {
    const probe = guard.probePython(PYTHON, env())
    expect(probe.error).toBeUndefined()
    expect(fs.realpathSync(probe.file)).toBe(fs.realpathSync(path.join(site, 'pdfluent', 'chunks.py')))
    expect(probe.sha256).toBe(guard.sha256OfSource(TREE_CHUNKS))
    expect(probe.version).toBe('1.1.0')
    const tree = { chunksPath: TREE_CHUNKS, sha256: guard.sha256OfSource(TREE_CHUNKS), version: '1.1.0' }
    expect(guard.mismatchReason(PYTHON, probe, tree)).toBeNull()
  })

  test('a tampered chunks.py is caught', () => {
    fs.appendFileSync(path.join(site, 'pdfluent', 'chunks.py'), '# not this tree\n')
    const probe = guard.probePython(PYTHON, env())
    const tree = { chunksPath: TREE_CHUNKS, sha256: guard.sha256OfSource(TREE_CHUNKS), version: '1.1.0' }
    expect(guard.mismatchReason(PYTHON, probe, tree)).toContain('not this tree')
  })

  test('a pdfluent without chunks is an error, not a crash', () => {
    // A pdfluent package first on PYTHONPATH shadows any installed one, so
    // this branch is reached even on a machine with pdfluent in site-packages.
    const bare = fs.mkdtempSync(path.join(os.tmpdir(), 'parity-guard-bare-'))
    try {
      fs.mkdirSync(path.join(bare, 'pdfluent'))
      fs.writeFileSync(path.join(bare, 'pdfluent', '__init__.py'), '')
      const probe = guard.probePython(PYTHON, { ...process.env, PYTHONPATH: bare })
      expect(probe.error).toContain('pdfluent.chunks')
    } finally {
      fs.rmSync(bare, { recursive: true, force: true })
    }
  })

  test('an interpreter that does not exist is an error naming it', () => {
    const probe = guard.probePython(path.join(site, 'no-such-python'), env())
    expect(probe.error).toContain('no-such-python')
  })
})
