// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/**
 * A document as retrieval chunks, each carrying where it came from.
 *
 * A port of `pdfluent.chunks.iter_chunks` (Python) with the same semantics:
 * text to embed, and the metadata a retriever needs to cite it -- the page,
 * the box on that page, the kind of block and, where a learned model decided
 * the kind, how sure it was.
 *
 * Two modes:
 *
 * - `"blocks"` (the default): one chunk per paragraph, heading and list item
 *   of `extractBlocks`, and ONE chunk per table rather than one per cell -- a
 *   cell on its own is a number without its column. Running heads and page
 *   numbers are left out unless `includeFurniture` is set, because a line
 *   every page repeats is the chunk that wins every similarity search.
 * - `"markdown"`: one chunk per page, the page's `extractMarkdown`, with the
 *   per-page fields a vector-store pipeline ingests from pymupdf4llm's
 *   `page_chunks`: `toc_items` and `tables`.
 *
 * Option and metadata names are camelCase where Python is snake_case
 * (`includeFurniture`, `firstPage`, `pageCount`, `maxChars`,
 * `continuesPrevious`, `sectionPath`, `tablePart`); the block kinds stay the
 * binding's own (`listItem`, as `extractBlocks` reports it). `toc_items`
 * keeps its pymupdf4llm spelling in both bindings.
 */
'use strict'

const MODES = ['blocks', 'markdown']

function union(a, b) {
  return [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.max(a[2], b[2]), Math.max(a[3], b[3])]
}

/** A table's cells as rows of `a | b | c`, one row per line. */
function tableText(cells) {
  const rows = new Map()
  for (const cell of cells) {
    if (!rows.has(cell.row)) rows.set(cell.row, [])
    rows.get(cell.row).push(cell)
  }
  return [...rows.keys()]
    .sort((a, b) => a - b)
    .map((r) =>
      rows
        .get(r)
        .sort((a, b) => a.column - b.column)
        .map((c) => c.text.trim())
        .join(' | '),
    )
    .join('\n')
}

/**
 * The section stack below a heading: cut to its level, then pushed.
 *
 * A heading of unknown level is a top-level one: cutting nothing would nest
 * the rest of the document under a guess.
 */
function enterHeading(stack, text, level) {
  let depth = level || 1
  if (depth < 1) depth = 1
  return stack.slice(0, depth - 1).concat([text])
}

/**
 * A table run as `{ text, cells, tablePart }` pieces, or null to keep one.
 *
 * Null means the table fits the budget, has no data rows to split by, or is
 * a single row: splitting further would mean splitting inside a row, which
 * is refused whatever the budget says.
 */
function splitTable(run, maxChars) {
  const rows = new Map()
  for (const cell of run) {
    if (!rows.has(cell.row)) rows.set(cell.row, [])
    rows.get(cell.row).push(cell)
  }
  const ordered = [...rows.keys()]
    .sort((a, b) => a - b)
    .map((r) => rows.get(r).sort((a, b) => a.column - b.column))
  const lines = ordered.map((row) => row.map((c) => c.text.trim()).join(' | '))
  if (lines.join('\n').length <= maxChars) return null
  const header = lines[0]
  const headerCells = ordered[0]
  const pieces = []
  let current = []
  let size = header.length
  for (let i = 1; i < lines.length; i++) {
    if (current.length > 0 && size + 1 + lines[i].length > maxChars) {
      pieces.push(current)
      current = []
      size = header.length
    }
    current.push([lines[i], ordered[i]])
    size += 1 + lines[i].length
  }
  if (current.length > 0) pieces.push(current)
  if (pieces.length < 2) return null
  return pieces.map((piece, at) => ({
    text: [header].concat(piece.map(([line]) => line)).join('\n'),
    cells: headerCells.concat(piece.flatMap(([, rowCells]) => rowCells)),
    tablePart: `${at + 1} of ${pieces.length}`,
  }))
}

function blockChunks(blocks, includeFurniture, hierarchical, maxChars) {
  const chunks = []
  let section = null
  let path = []
  let at = 0
  while (at < blocks.length) {
    const block = blocks[at]
    if (block.kind === 'tableCell') {
      // One table: a run of cells on one page whose (row, column) climbs,
      // which is how the engine's own serializers read a table.
      const run = [block]
      at += 1
      while (
        at < blocks.length &&
        blocks[at].kind === 'tableCell' &&
        blocks[at].page === block.page &&
        (blocks[at].row > run[run.length - 1].row ||
          (blocks[at].row === run[run.length - 1].row &&
            blocks[at].column > run[run.length - 1].column))
      ) {
        run.push(blocks[at])
        at += 1
      }
      const split = hierarchical ? splitTable(run, maxChars) : null
      if (split === null) {
        let bbox = [...run[0].bbox]
        for (const cell of run.slice(1)) bbox = union(bbox, cell.bbox)
        const metadata = { page: block.page, kind: 'table', bbox }
        if (section !== null) metadata.section = section
        if (hierarchical) metadata.sectionPath = [...path]
        chunks.push({ text: tableText(run), metadata })
      } else {
        for (const { text, cells, tablePart } of split) {
          let bbox = [...cells[0].bbox]
          for (const cell of cells.slice(1)) bbox = union(bbox, cell.bbox)
          const metadata = { page: block.page, kind: 'table', bbox, tablePart }
          if (section !== null) metadata.section = section
          metadata.sectionPath = [...path]
          chunks.push({ text, metadata })
        }
      }
      continue
    }
    at += 1
    if ((block.kind === 'header' || block.kind === 'footer') && !includeFurniture) continue
    const text = block.text.trim()
    if (!text) continue
    let ancestors = null
    if (block.kind === 'heading') {
      section = text
      const stack = enterHeading(path, text, block.level)
      ancestors = stack.slice(0, -1)
      path = stack
    }
    const metadata = { page: block.page, kind: block.kind, bbox: [...block.bbox] }
    if (block.confidence !== undefined && block.confidence !== null) {
      metadata.confidence = block.confidence
    }
    if (block.kind === 'heading' && block.level !== undefined && block.level !== null) {
      metadata.level = block.level
    }
    if (block.continuesPrevious) metadata.continuesPrevious = true
    if (section !== null && block.kind !== 'heading') metadata.section = section
    if (hierarchical) metadata.sectionPath = ancestors !== null ? [...ancestors] : [...path]
    chunks.push({ text, metadata })
  }
  return chunks
}

/** The section stack at the first block of each page. */
function pagePaths(blocks) {
  const paths = new Map()
  let stack = []
  for (const block of blocks) {
    if (!paths.has(block.page)) paths.set(block.page, [...stack])
    if (block.kind === 'heading') {
      const text = block.text.trim()
      if (text) stack = enterHeading(stack, text, block.level)
    }
  }
  return paths
}

/**
 * The outline as `[level, title, page]` triples, in document order.
 *
 * Levels count from 1, pymupdf4llm's numbering; pages stay 0-based, this
 * binding's. An entry without a page -- a named destination the engine does
 * not resolve yet -- points nowhere and is left out, but its children still
 * count: the level is the depth in the tree, not in what survived.
 */
function flattenToc(bookmarks, level = 1) {
  const flat = []
  for (const bookmark of bookmarks || []) {
    if (bookmark.page !== undefined && bookmark.page !== null) {
      flat.push([level, bookmark.title, bookmark.page])
    }
    flat.push(...flattenToc(bookmark.children || [], level + 1))
  }
  return flat
}

/**
 * One `{ bbox, rows, columns }` per table on each page.
 *
 * A table is the same run `blockChunks` reads as one chunk: cells on one
 * page whose `(row, column)` climbs. `rows` and `columns` are the grid's
 * extent, so a 4-row by 3-column table reads 4 and 3.
 */
function pageTables(blocks) {
  const tables = new Map()
  let at = 0
  while (at < blocks.length) {
    const block = blocks[at]
    if (block.kind !== 'tableCell') {
      at += 1
      continue
    }
    const run = [block]
    at += 1
    while (
      at < blocks.length &&
      blocks[at].kind === 'tableCell' &&
      blocks[at].page === block.page &&
      (blocks[at].row > run[run.length - 1].row ||
        (blocks[at].row === run[run.length - 1].row &&
          blocks[at].column > run[run.length - 1].column))
    ) {
      run.push(blocks[at])
      at += 1
    }
    let bbox = [...run[0].bbox]
    for (const cell of run.slice(1)) bbox = union(bbox, cell.bbox)
    if (!tables.has(block.page)) tables.set(block.page, [])
    tables.get(block.page).push({
      bbox,
      rows: Math.max(...run.map((c) => c.row)) + 1,
      columns: Math.max(...run.map((c) => c.column)) + 1,
    })
  }
  return tables
}

function repr(value) {
  return typeof value === 'string' ? `'${value}'` : String(value)
}

/**
 * Open whatever `iterChunks` accepts: an open `PdfDocument`, a path, or a
 * `Buffer`. The binding is required lazily -- this module is itself required
 * from the generated entry point, so requiring it back at load time would
 * close a circle.
 */
function openInput(input) {
  if (typeof input === 'string') {
    const { openPdf } = require('./index.js')
    return { doc: openPdf(input), source: input }
  }
  if (Buffer.isBuffer(input)) {
    const { PdfDocument } = require('./index.js')
    return { doc: PdfDocument.open(input), source: null }
  }
  if (input !== null && typeof input === 'object' && typeof input.extractBlocks === 'function') {
    return { doc: input, source: null }
  }
  throw new TypeError(
    `document must be a PdfDocument, a path string, or a Buffer, not ${repr(input)}`,
  )
}

/**
 * The document as chunks, in reading order.
 *
 * Preview: output quality is still improving and the API may change in a minor release.
 *
 * `document` is an open `PdfDocument`, or a path or `Buffer` to open.
 * `firstPage` and `pageCount` choose a window of pages, as they do on
 * `extractBlocks`.
 *
 * `hierarchical` adds `sectionPath` -- the enclosing headings -- to every
 * chunk and splits tables longer than `maxChars` by rows, repeating the
 * header row in each piece. It leaves the default output untouched: with
 * `hierarchical: false` a list item, a paragraph and a table are one chunk
 * each however long they are.
 *
 * @experimental
 */
function iterChunks(document, options = {}) {
  const {
    mode = 'blocks',
    includeFurniture = false,
    firstPage = 0,
    pageCount = null,
    hierarchical = false,
    maxChars = 2000,
  } = options || {}
  if (!MODES.includes(mode)) {
    throw new Error(`mode must be one of ('blocks', 'markdown'), not ${repr(mode)}`)
  }
  if (maxChars < 1) {
    throw new Error(`maxChars must be at least 1, not ${repr(maxChars)}`)
  }
  const { doc, source } = openInput(document)
  const windowOpts =
    pageCount === null || pageCount === undefined ? { firstPage } : { firstPage, pageCount }
  if (mode === 'markdown') {
    const last = pageCount === null || pageCount === undefined ? doc.pageCount : firstPage + pageCount
    const window = doc.extractBlocks(windowOpts)
    const paths = hierarchical ? pagePaths(window) : new Map()
    const perPage = pageTables(window)
    const toc = flattenToc(typeof doc.bookmarks === 'function' ? doc.bookmarks() : [])
    const chunks = []
    let current = []
    const end = Math.min(last, doc.pageCount)
    for (let page = firstPage; page < end; page++) {
      if (paths.has(page)) current = paths.get(page)
      const text = doc.extractMarkdown({ firstPage: page, pageCount: 1 }).trim()
      if (!text) continue
      const metadata = source === null ? {} : { source }
      Object.assign(metadata, {
        page,
        kind: 'page',
        toc_items: toc.filter((entry) => entry[2] === page),
        tables: perPage.get(page) || [],
      })
      if (hierarchical) metadata.sectionPath = [...current]
      chunks.push({ text, metadata })
    }
    return chunks
  }
  const blocks = doc.extractBlocks(windowOpts)
  return blockChunks(blocks, includeFurniture, hierarchical, maxChars).map((chunk) =>
    source === null ? chunk : { text: chunk.text, metadata: { source, ...chunk.metadata } },
  )
}

module.exports = { iterChunks }
