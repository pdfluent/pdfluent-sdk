// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, test } from 'vitest'

import { PdfDocument } from '@pdfluent/node'
import { toChunks, toJson, toMarkdown } from '../src/index.js'

const here = dirname(fileURLToPath(import.meta.url))
const STRUCTURED_PDF = resolve(here, '..', '..', '..', 'fixtures', 'structured.pdf')

// The engine cases need @pdfluent/node@^1.1.0 (extractMarkdown, extractDocling,
// extractBlocks), which is neither published nor in the tree yet — the tree
// is 1.0.0. Until the installed engine provides the API they skip, visibly,
// like pytest.importorskip on the Python side; the refusal cases below need
// no engine and always run. The skip lifts itself the day 1.1.0 lands.
const engine = PdfDocument.prototype as unknown as Record<string, unknown>
const ENGINE_CASE = ['extractMarkdown', 'extractDocling', 'extractBlocks'].every(
  (m) => typeof engine[m] === 'function',
)
const engineTest = ENGINE_CASE ? test : test.skip

describe('toMarkdown', () => {
  engineTest('carries the fixture heading, list item and table', () => {
    const markdown = toMarkdown(STRUCTURED_PDF)
    expect(markdown).toContain('# Safety rules')
    expect(markdown).toContain('- Wear the issued boots on the floor.')
    expect(markdown).toContain('| Band | Units | Rate |')
  })

  engineTest('reads Buffer and Uint8Array like a path', () => {
    const data = readFileSync(STRUCTURED_PDF)
    expect(toMarkdown(data)).toBe(toMarkdown(STRUCTURED_PDF))
    expect(toMarkdown(new Uint8Array(data))).toBe(toMarkdown(STRUCTURED_PDF))
  })

  engineTest('selects pages', () => {
    const first = toMarkdown(STRUCTURED_PDF, { pages: [0] })
    expect(first).toContain('# Safety rules')
    expect(first).not.toContain('| Band | Units | Rate |')
  })

  test('refuses any quality but fast with a TypeError', () => {
    expect(() => toMarkdown(STRUCTURED_PDF, { quality: 'accurate' as 'fast' })).toThrowError(
      TypeError,
    )
    expect(() => toMarkdown(STRUCTURED_PDF, { quality: 'accurate' as 'fast' })).toThrowError(
      /"fast"/,
    )
  })

  test('refuses garbage that is no PDF', () => {
    expect(() => toMarkdown(Buffer.from('not a document at all'))).toThrowError(/not a PDF/)
  })
})

describe('toJson', () => {
  engineTest('is a DoclingDocument named for the file', () => {
    const doc = toJson(STRUCTURED_PDF) as { name: string; texts: Array<{ text?: string }> }
    expect(doc.name).toBe('structured')
    expect(doc.texts.map((t) => t.text ?? '').join(' ')).toContain('Safety rules')
  })

  engineTest('names Buffer sources document', () => {
    const doc = toJson(readFileSync(STRUCTURED_PDF)) as { name: string }
    expect(doc.name).toBe('document')
  })

  test('refuses any quality but fast with a TypeError', () => {
    expect(() => toJson(STRUCTURED_PDF, { quality: 'accurate' as 'fast' })).toThrowError(
      TypeError,
    )
  })
})

describe('toChunks', () => {
  engineTest('covers the fixture with provenance per chunk', () => {
    const chunks = toChunks(STRUCTURED_PDF)
    expect(chunks.map((c) => c.metadata.kind)).toEqual([
      'heading',
      'list_item',
      'list_item',
      'list_item',
      'table',
    ])
    const [heading, firstItem, , , table] = chunks
    expect(heading.text).toBe('Safety rules')
    expect(firstItem.metadata.sectionPath).toEqual(['Safety rules'])
    expect(firstItem.metadata.page).toBe(0)
    expect(table.metadata.page).toBe(1)
    const [x0, , x1] = table.metadata.bbox
    expect(x1).toBeGreaterThan(x0)
    expect(heading.metadata.source).toBe(STRUCTURED_PDF)
  })

  engineTest('selects pages in listed order', () => {
    const chunks = toChunks(STRUCTURED_PDF, { pages: [1, 0] })
    expect(chunks.map((c) => c.metadata.page)).toEqual([1, 0, 0, 0, 0])
  })

  engineTest('budgets by tokens: a tight budget splits the table by rows', () => {
    const roomy = toChunks(STRUCTURED_PDF, { maxTokens: 512 })
    const tight = toChunks(STRUCTURED_PDF, { maxTokens: 8 })
    expect(tight.length).toBeGreaterThan(roomy.length)
    expect(tight.some((c) => c.metadata.tablePart !== undefined)).toBe(true)
  })

  test('refuses a bad budget and a bad quality', () => {
    expect(() => toChunks(STRUCTURED_PDF, { maxTokens: 0 })).toThrowError(TypeError)
    expect(() => toChunks(STRUCTURED_PDF, { quality: 'accurate' as 'fast' })).toThrowError(
      TypeError,
    )
  })

  test('Office files are PDF-only in v1', () => {
    const zip = Buffer.from([0x50, 0x4b, 0x03, 0x04, 0, 0, 0, 0])
    expect(() => toChunks(zip)).toThrowError(/PDF.*v1/)
    expect(() => toMarkdown(zip)).toThrowError(/PDF.*v1/)
  })
})
