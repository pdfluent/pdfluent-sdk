// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/**
 * Does a Python interpreter carry this tree's `pdfluent.chunks`?
 *
 * The parity suite (`chunks_parity.test.js`) compares Node `iterChunks`
 * against Python `iter_chunks`. That comparison only means something when the
 * Python side is this tree's chunker. Path equality with the checkout used to
 * be the test, which only a `maturin develop` editable install passes; an
 * installed wheel of the same candidate never could. The test here is content:
 *
 *   1. `pdfluent.chunks` imports under the interpreter;
 *   2. its source file is byte-identical to this tree's `chunks.py`
 *      (sha256, line endings normalised: a Windows checkout may carry CRLF);
 *   3. the installed `pdfluent` distribution reports this tree's version
 *      (`crates/pdf-python/pyproject.toml`).
 *
 * What it cannot see is the native module inside the wheel, which a version
 * string does not pin to a commit. The workflow that installs the wheel pins
 * that instead, by taking the wheel from a build-wheels run on the same commit.
 *
 * Kept apart from the suite so `chunks_parity_guard.test.js` can drive every
 * branch with no addon and no engine.
 */

const crypto = require('crypto')
const fs = require('fs')
const { spawnSync } = require('child_process')

// Prints one JSON line: {error} when the import fails, else {file, sha256,
// version}. Exit 0 either way, so a non-zero status means the interpreter
// itself did not run.
const PROBE = [
  'import hashlib, json, sys',
  'try:',
  '    import pdfluent.chunks as c',
  'except Exception as e:',
  '    print(json.dumps({"error": "%s: %s" % (type(e).__name__, e)}))',
  '    sys.exit(0)',
  'try:',
  '    from importlib.metadata import version',
  '    v = version("pdfluent")',
  'except Exception:',
  '    v = None',
  'with open(c.__file__, "rb") as f:',
  '    data = f.read().replace(b"\\r\\n", b"\\n")',
  'print(json.dumps({"file": c.__file__, "sha256": hashlib.sha256(data).hexdigest(), "version": v}))',
].join('\n')

function sha256OfSource(file) {
  const data = fs.readFileSync(file).toString('latin1').replace(/\r\n/g, '\n')
  return crypto.createHash('sha256').update(Buffer.from(data, 'latin1')).digest('hex')
}

/** The `version` of the `[project]` table, or null when there is none. */
function projectVersion(pyprojectText) {
  let inProject = false
  for (const raw of pyprojectText.split(/\r?\n/)) {
    const line = raw.trim()
    if (line.startsWith('[')) {
      inProject = line === '[project]'
      continue
    }
    if (!inProject) continue
    const m = line.match(/^version\s*=\s*"([^"]+)"/)
    if (m) return m[1]
  }
  return null
}

/** Run the probe under `python`; returns the parsed answer or {error}. */
function probePython(python, env = process.env) {
  const run = spawnSync(python, ['-c', PROBE], { encoding: 'utf8', env })
  if (run.error) return { error: `cannot run ${python}: ${run.error.message}` }
  if (run.status !== 0) return { error: `${python} exited ${run.status}: ${(run.stderr || '').trim()}` }
  try {
    return JSON.parse(run.stdout.trim().split(/\r?\n/).pop())
  } catch (e) {
    return { error: `unreadable probe output from ${python}: ${run.stdout.trim()}` }
  }
}

/**
 * Why the parity suite cannot vouch for this interpreter, or null when it can.
 *
 * `probe` is what `probePython` returned; `tree` is {chunksPath, sha256,
 * version} for this checkout.
 */
function mismatchReason(python, probe, tree) {
  if (probe.error) {
    return `no importable pdfluent.chunks under ${python} (${probe.error}); set PDFLUENT_PYTHON to a venv with this candidate's wheel or a maturin develop install`
  }
  if (tree.version === null) {
    return `no [project] version in this tree's pyproject.toml, so nothing to match ${python}'s pdfluent ${probe.version} against`
  }
  if (probe.sha256 !== tree.sha256) {
    return `pdfluent.chunks under ${python} is ${probe.file} (sha256 ${probe.sha256}), not this tree's ${tree.chunksPath} (sha256 ${tree.sha256})`
  }
  if (probe.version !== tree.version) {
    return `pdfluent under ${python} is version ${probe.version}, this tree is ${tree.version}`
  }
  return null
}

/** Skip becomes fail in a release lane, as everywhere else in this repo. */
function inReleaseLane(env) {
  return env.PDFLUENT_RELEASE_LANE === '1'
}

module.exports = { PROBE, sha256OfSource, projectVersion, probePython, mismatchReason, inReleaseLane }
