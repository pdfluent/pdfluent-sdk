// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, test } from 'vitest'

const here = dirname(fileURLToPath(import.meta.url))
const root = resolve(here, '..')
const manifest = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8')) as Record<
  string,
  any
>
const readme = readFileSync(resolve(root, 'README.md'), 'utf8')

const PRICING = 'https://pdfluent.com/sdk/pricing/'

describe('published manifest', () => {
  test('names the package with a short summary', () => {
    expect(manifest.name).toBe('@pdfluent/extract')
    expect(manifest.version).toBe('0.1.0')
    expect(manifest.description).toBe(
      'PDF to Markdown, JSON and RAG chunks — fast, local, no system dependencies',
    )
    expect(manifest.description.length).toBeLessThanOrEqual(90)
  })

  test('ships ESM, CJS and types', () => {
    expect(manifest.main).toBe('./dist/cjs/index.js')
    expect(manifest.module).toBe('./dist/esm/index.js')
    expect(manifest.types).toBe('./dist/cjs/index.d.ts')
    const subpath = manifest.exports['.']
    expect(subpath.import.default).toBe('./dist/esm/index.js')
    expect(subpath.require.default).toBe('./dist/cjs/index.js')
  })

  test('packs code, types and both licence texts', () => {
    for (const entry of ['dist/', 'LICENSE', 'LICENSE-COMMERCIAL', 'LICENSE-OFFER', 'README.md']) {
      expect(manifest.files).toContain(entry)
    }
  })

  test('declares the same dual licence as the other npm packages', () => {
    // npm rejects a LicenseRef expression outright, so the field points at
    // the file — the same pointer @pdfluent/node publishes.
    expect(manifest.license).toBe('SEE LICENSE IN LICENSE-OFFER')
    const offer = readFileSync(resolve(root, 'LICENSE-OFFER'), 'utf8')
    expect(offer).toContain('AGPL-3.0-only OR LicenseRef-PDFluent-Commercial')
  })

  test('depends on the same major of @pdfluent/node', () => {
    expect(manifest.dependencies['@pdfluent/node']).toBe('^1.1.0')
  })

  test('exposes the pdfluent-extract console script', () => {
    expect(manifest.bin).toEqual({ 'pdfluent-extract': './dist/cjs/cli.js' })
  })

  test('carries homepage, repository, bugs and keywords, and no funding', () => {
    expect(manifest.homepage).toBe('https://pdfluent.com')
    expect(manifest.repository.url).toBe('https://github.com/pdfluent/pdfluent-sdk')
    expect(manifest.bugs.url).toBe('https://github.com/pdfluent/pdfluent-sdk/issues')
    for (const keyword of ['pdf', 'markdown', 'rag', 'chunking', 'docling']) {
      expect(manifest.keywords).toContain(keyword)
    }
    expect(manifest.funding).toBeUndefined()
  })
})

describe('published README', () => {
  test('opens with install and the three-line use', () => {
    expect(readme).toContain('npm install @pdfluent/extract')
    expect(readme).toContain("import { toMarkdown } from '@pdfluent/extract'")
    expect(readme).toContain("console.log(toMarkdown('report.pdf'))")
  })

  test('covers chunks, platforms, quality and licence', () => {
    expect(readme).toContain('## Chunks for RAG')
    expect(readme).toContain('toChunks')
    expect(readme).toContain('## Supported platforms')
    expect(readme).toContain('musl')
    expect(readme).toContain("quality: 'fast'")
    expect(readme).toContain('## Licence')
    expect(readme).toContain(PRICING)
  })

  test('links are absolute', () => {
    for (const match of readme.matchAll(/\]\(([^)]+)\)/g)) {
      const target = match[1]
      expect(target.startsWith('https://') || target.startsWith('#')).toBe(true)
    }
  })

  test('names no internal tracker', () => {
    // Written as two halves so this file itself never carries the term.
    expect(readme).not.toContain('pdfluent-' + 'internal')
    expect(readme.match(/(?<![\w/])#\d{3,4}\b/)).toBeNull()
  })
})
