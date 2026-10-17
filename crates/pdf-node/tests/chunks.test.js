// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/**
 * The chunker: `iterChunks`, the port of `pdfluent.chunks.iter_chunks`.
 *
 * Two halves. The static half feeds the chunker stub documents, so it runs
 * without the native addon (as it does on every push in ci.yml) and pins the
 * grouping rules without depending on what the heading and table models
 * decide about a PDF -- those decisions belong to other teams. The native
 * half reads the same fixtures the Python suite reads and asserts the same
 * chunk counts and per-page fields.
 *
 * Run:
 *   cd crates/pdf-node
 *   npm run build
 *   npx jest tests/chunks.test.js
 *
 * The addon check sits at the top, before the first suite: the skip lint
 * requires the announcement to run even when the native suite is skipped,
 * and anything after the first describe() never does.
 */

let addon
try {
  addon = require('../index')
} catch (e) {
  process.stderr.write('SKIPPED (not a pass): native addon not built — native half of the chunker not checked\n')
}
const d = addon ? describe : describe.skip

const fs = require('fs')
const path = require('path')

const { iterChunks } = require('../chunks')

const FIXTURES = path.join(__dirname, '..', '..', '..', 'fixtures')
const STRUCTURED_PDF = path.join(FIXTURES, 'structured.pdf')
const OUTLINED_PDF = path.join(FIXTURES, 'outlined.pdf')

function makeBlock(kind, text, page = 0, extra = {}) {
  return {
    kind,
    text,
    page,
    bbox: [0, 0, 10, 10],
    confidence: undefined,
    level: undefined,
    continuesPrevious: false,
    ...extra,
  }
}

function makeTable(rows, page = 0) {
  const cells = []
  rows.forEach((texts, row) => {
    texts.forEach((text, column) => {
      cells.push(makeBlock('tableCell', text, page, { row, column }))
    })
  })
  return cells
}

function stubDocument({ blocks = [], pages = 1, markdown = {}, bookmarks = [] } = {}) {
  return {
    pageCount: pages,
    extractBlocks: ({ firstPage = 0, pageCount = null } = {}) => {
      const last = pageCount === null || pageCount === undefined ? pages : firstPage + pageCount
      return blocks.filter((b) => b.page >= firstPage && b.page < last)
    },
    extractMarkdown: ({ firstPage = 0 } = {}) => markdown[firstPage] || '',
    bookmarks: () => bookmarks,
  }
}

describe('iterChunks over stub documents (no native addon)', () => {
  test('an unknown mode is refused', () => {
    expect(() => iterChunks(stubDocument(), { mode: 'pages' })).toThrow(
      "mode must be one of ('blocks', 'markdown'), not 'pages'",
    )
  })

  test('a non-positive budget is refused', () => {
    expect(() => iterChunks(stubDocument(), { hierarchical: true, maxChars: 0 })).toThrow(
      'maxChars must be at least 1, not 0',
    )
  })

  test('anything but a document, a path or a Buffer is refused', () => {
    expect(() => iterChunks(42)).toThrow(TypeError)
    expect(() => iterChunks(null)).toThrow(TypeError)
  })

  test('a long table is split by rows with the header repeated', () => {
    const header = ['Name', 'Role']
    const data = Array.from({ length: 12 }, (_, n) => [`member-${String(n).padStart(2, '0')}`, 'reviewer'])
    const chunks = iterChunks(stubDocument({ blocks: makeTable([header, ...data]) }), {
      hierarchical: true,
      maxChars: 80,
    })

    expect(chunks.length).toBeGreaterThan(1)
    const count = chunks.length
    expect(chunks.map((c) => c.metadata.tablePart)).toEqual(
      Array.from({ length: count }, (_, at) => `${at + 1} of ${count}`),
    )
    for (const piece of chunks) {
      expect(piece.metadata.kind).toBe('table')
      expect(piece.text.split('\n')[0]).toBe('Name | Role')
      expect(piece.text.length).toBeLessThanOrEqual(80)
    }
    expect(chunks.flatMap((c) => c.text.split('\n').slice(1))).toEqual(
      data.map(([name, role]) => `${name} | ${role}`),
    )
  })

  test('a table that fits stays one chunk without part numbers', () => {
    const chunks = iterChunks(
      stubDocument({ blocks: makeTable([['Band', 'Units'], ['Day', '500'], ['Night', '500']]) }),
      { hierarchical: true },
    )

    expect(chunks).toHaveLength(1)
    expect(chunks[0].text.split('\n')).toEqual(['Band | Units', 'Day | 500', 'Night | 500'])
    expect(chunks[0].metadata.tablePart).toBeUndefined()
    expect(chunks[0].metadata.sectionPath).toEqual([])
  })

  test('the default mode adds no hierarchical keys and never splits', () => {
    const long = makeTable([['Name', 'Role'], ...Array.from({ length: 12 }, (_, n) => [`m${n}`, 'r'])])
    const chunks = iterChunks(stubDocument({ blocks: long }), { maxChars: 80 })

    expect(chunks).toHaveLength(1)
    expect(chunks[0].metadata.tablePart).toBeUndefined()
    expect(chunks[0].metadata.sectionPath).toBeUndefined()
  })

  test('table rows are never split', () => {
    const huge = `target ${'y'.repeat(500)}`
    const rows = [['Key', 'Value'], ['a', '1'], ['big', huge], ['b', '2']]
    const chunks = iterChunks(stubDocument({ blocks: makeTable(rows) }), {
      hierarchical: true,
      maxChars: 100,
    })

    expect(chunks.length).toBeGreaterThan(1)
    const whole = `big | ${huge}`
    expect(chunks.filter((c) => c.text.split('\n').includes(whole))).toHaveLength(1)
    for (const piece of chunks) expect(piece.text.split('\n')[0]).toBe('Key | Value')
  })

  test('hierarchical chunks carry the enclosing headings', () => {
    const blocks = [
      makeBlock('heading', '2 Methods', 0, { level: 1 }),
      makeBlock('paragraph', 'How we measured.'),
      makeBlock('heading', '2.1 Data', 0, { level: 2 }),
      makeBlock('paragraph', 'Where it came from.'),
    ]
    const chunks = iterChunks(stubDocument({ blocks }), { hierarchical: true })

    expect(chunks.map((c) => c.metadata.sectionPath)).toEqual([
      [],
      ['2 Methods'],
      ['2 Methods'],
      ['2 Methods', '2.1 Data'],
    ])
    expect(chunks[3].metadata.section).toBe('2.1 Data')
  })

  test('markdown pages carry toc_items and tables', () => {
    const blocks = makeTable([['Band', 'Units'], ['Day', '500']], 1)
    const bookmarks = [
      { title: 'Chapter 1', page: 0, children: [] },
      {
        title: 'Chapter 2',
        page: 1,
        children: [{ title: 'Section 2.1', page: 1, children: [] }],
      },
    ]
    const chunks = iterChunks(
      stubDocument({ blocks, pages: 2, markdown: { 0: 'first', 1: 'second' }, bookmarks }),
      { mode: 'markdown' },
    )

    expect(chunks.map((c) => c.metadata.toc_items)).toEqual([
      [[1, 'Chapter 1', 0]],
      [
        [1, 'Chapter 2', 1],
        [2, 'Section 2.1', 1],
      ],
    ])
    expect(chunks[0].metadata.tables).toEqual([])
    expect(chunks[1].metadata.tables).toEqual([
      { bbox: [0, 0, 10, 10], rows: 2, columns: 2 },
    ])
  })

  test('entries without a page are left out but keep their children', () => {
    const bookmarks = [{ title: 'Part', page: null, children: [{ title: 'Chapter 1', page: 0, children: [] }] }]
    const chunks = iterChunks(stubDocument({ pages: 1, markdown: { 0: 'text' }, bookmarks }), {
      mode: 'markdown',
    })

    expect(chunks[0].metadata.toc_items).toEqual([[2, 'Chapter 1', 0]])
  })
})

d('iterChunks over real PDFs (native addon)', () => {
  const open = () => addon.PdfDocument.open(fs.readFileSync(STRUCTURED_PDF))

  test('a path, bytes and an open document give the same chunks', () => {
    const data = fs.readFileSync(STRUCTURED_PDF)
    const fromPath = iterChunks(STRUCTURED_PDF)
    const fromBytes = iterChunks(data)
    const fromDocument = iterChunks(open())

    expect(fromPath.map((c) => c.text)).toEqual(fromBytes.map((c) => c.text))
    expect(fromBytes.map((c) => c.text)).toEqual(fromDocument.map((c) => c.text))
    expect(fromPath.map((c) => c.metadata.kind)).toEqual([
      'heading',
      'listItem',
      'listItem',
      'listItem',
      'table',
    ])
    // Only a path names a source.
    expect(fromPath[0].metadata.source).toBe(STRUCTURED_PDF)
    expect(fromBytes[0].metadata.source).toBeUndefined()
  })

  test('a table is one chunk with the union of its cells boxes', () => {
    const table = iterChunks(STRUCTURED_PDF).at(-1)
    const cells = open().extractBlocks().filter((b) => b.kind === 'tableCell')

    expect(table.metadata.bbox).toEqual([
      Math.min(...cells.map((c) => c.bbox[0])),
      Math.min(...cells.map((c) => c.bbox[1])),
      Math.max(...cells.map((c) => c.bbox[2])),
      Math.max(...cells.map((c) => c.bbox[3])),
    ])
    expect(table.text.split('\n')).toHaveLength(new Set(cells.map((c) => c.row)).size)
  })

  test('tables holds one entry with the right row and column count', () => {
    const chunks = iterChunks(STRUCTURED_PDF, { mode: 'markdown' })

    expect(chunks.map((c) => c.metadata.page)).toEqual([0, 1])
    expect(chunks[0].metadata.tables).toEqual([])
    expect(chunks[1].metadata.tables).toHaveLength(1)
    expect(chunks[1].metadata.tables[0].rows).toBe(4)
    expect(chunks[1].metadata.tables[0].columns).toBe(3)
  })

  test('toc_items holds the entries for their page', () => {
    const chunks = iterChunks(OUTLINED_PDF, { mode: 'markdown' })

    expect(chunks.map((c) => c.metadata.toc_items)).toEqual([
      [[1, 'Chapter 1', 0]],
      [
        [1, 'Chapter 2', 1],
        [2, 'Section 2.1', 1],
      ],
      [[1, 'Appendix A', 2]],
    ])
  })

  test('blocks mode gains no page keys, and images and words stay absent', () => {
    for (const piece of iterChunks(OUTLINED_PDF)) {
      expect(piece.metadata.toc_items).toBeUndefined()
      expect(piece.metadata.tables).toBeUndefined()
    }
    for (const piece of iterChunks(OUTLINED_PDF, { mode: 'markdown' })) {
      expect(piece.metadata.images).toBeUndefined()
      expect(piece.metadata.words).toBeUndefined()
    }
  })
})
