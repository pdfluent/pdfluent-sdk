// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
import { execFileSync, spawnSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, test } from 'vitest'

import { parsePages } from '../src/pages.js'

const here = dirname(fileURLToPath(import.meta.url))
const root = resolve(here, '..')
const CLI = resolve(root, 'dist', 'cjs', 'cli.js')
const STRUCTURED_PDF = resolve(here, '..', '..', '..', 'fixtures', 'structured.pdf')

// The bin runs from dist, so the suite needs `npm run build` first —
// run_extract_js_tests.sh builds before it tests. A missing bin fails with
// the command, not a bare ENOENT.
function bin(): string {
  if (!existsSync(CLI)) throw new Error(`bin missing at ${CLI}: run npm run build first`)
  return CLI
}

function run(args: string[], input?: Buffer): { status: number; stdout: string; stderr: string } {
  const proc = spawnSync('node', [bin(), ...args], {
    input,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  })
  return {
    status: proc.status ?? 1,
    stdout: proc.stdout ?? '',
    stderr: proc.stderr ?? '',
  }
}

// Usage errors need the engine installed but no 1.1.0 API (arg parsing
// fails before any engine call); extractions need the API. Both skip,
// visibly, when their floor is missing — like extract.test.ts.
const require = createRequire(import.meta.url)
let installed = false
let engineCase = false
try {
  const node = require('@pdfluent/node') as {
    PdfDocument: { prototype: Record<string, unknown> }
  }
  installed = true
  engineCase = ['extractMarkdown', 'extractDocling', 'extractBlocks'].every(
    (m) => typeof node.PdfDocument.prototype[m] === 'function',
  )
} catch {
  installed = false
  engineCase = false
}
const binTest = installed ? test : test.skip
const engineTest = engineCase ? test : test.skip

describe('parsePages', () => {
  test('counts from one, ranges inclusive', () => {
    expect(parsePages('1')).toEqual([0])
    expect(parsePages('1-3,5')).toEqual([0, 1, 2, 4])
  })

  test('sorts and deduplicates', () => {
    expect(parsePages('3,1-2,2')).toEqual([0, 1, 2])
  })

  test('refuses empties, zeros, reversals and garbage', () => {
    for (const spec of ['', '1,,2', '0', '1-0', '3-1', 'abc', '1-x']) {
      expect(() => parsePages(spec), spec).toThrowError(RangeError)
    }
  })
})

describe('bin', () => {
  test('carries a shebang', () => {
    const first = readFileSync(bin(), 'utf8').split('\n')[0]
    expect(first).toBe('#!/usr/bin/env node')
  })

  binTest('executes directly, --help exits zero', () => {
    execFileSync(bin(), ['--help'])
  })

  engineTest('extracts the fixture to markdown on stdout', () => {
    const { status, stdout } = run([STRUCTURED_PDF])
    expect(status).toBe(0)
    expect(stdout).toContain('# Safety rules')
    expect(stdout).toContain('- Wear the issued boots on the floor.')
  })

  binTest('a bad --format is a usage error', () => {
    const { status, stderr } = run([STRUCTURED_PDF, '--format', 'yaml'])
    expect(status).toBe(2)
    expect(stderr).toContain('--format')
  })

  binTest('a bad --pages is a usage error', () => {
    const { status, stderr } = run([STRUCTURED_PDF, '--pages', 'abc'])
    expect(status).toBe(2)
    expect(stderr).toContain('page')
  })

  engineTest('pages select counting from one', () => {
    const { status, stdout } = run([STRUCTURED_PDF, '--pages', '1'])
    expect(status).toBe(0)
    expect(stdout).toContain('# Safety rules')
    expect(stdout).not.toContain('| Band | Units | Rate |')
  })

  engineTest('reads stdin and writes the requested format', () => {
    const data = readFileSync(STRUCTURED_PDF)
    const { status, stdout } = run(['-', '--format', 'json'], data)
    expect(status).toBe(0)
    const doc = JSON.parse(stdout) as { name: string }
    expect(doc.name).toBe('document')
  })

  binTest('a missing file is an extraction error', () => {
    const { status, stderr } = run(['no-such-file.pdf'])
    expect(status).toBe(1)
    expect(stderr).toContain('no-such-file.pdf')
  })
})
