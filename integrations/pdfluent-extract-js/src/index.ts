// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
/**
 * PDF files to Markdown, JSON and RAG chunks, in three calls.
 *
 * This package is a thin wrapper: the engine underneath is `@pdfluent/node`,
 * which npm pulls in automatically. There is nothing to compile and no system
 * dependency to install.
 *
 * ```ts
 * import { toMarkdown } from '@pdfluent/extract'
 * console.log(toMarkdown('report.pdf'))
 * ```
 *
 * PDF only in v1. `quality: 'fast'` is the only level; anything else is a
 * `TypeError` — there is no hidden fallback to another pipeline.
 */

import { PdfDocument, type BlockInfo } from '@pdfluent/node'
import { readSource, kindOf, type Source } from './source.js'

export type Quality = 'fast'
export type { Source }

const QUALITIES: readonly Quality[] = ['fast']

/** Tokens to characters for the chunk budget: ~4 characters per token. */
const CHARS_PER_TOKEN = 4

export interface MarkdownOptions {
  quality?: Quality
  /** 0-based PDF page indices; absent for the whole file. */
  pages?: number[]
}

export interface JsonOptions extends MarkdownOptions {}

export interface ChunksOptions extends MarkdownOptions {
  /** Per-chunk budget, about four characters to the token. Default 512. */
  maxTokens?: number
}

export interface ChunkMetadata {
  page: number
  kind: string
  bbox: [number, number, number, number]
  sectionPath: string[]
  source?: string
  section?: string
  level?: number
  confidence?: number
  continuesPrevious?: boolean
  tablePart?: string
}

export interface Chunk {
  text: string
  metadata: ChunkMetadata
}

function checkQuality(quality: Quality | string | undefined): asserts quality is Quality {
  if (quality !== undefined && !QUALITIES.includes(quality as Quality)) {
    throw new TypeError(
      `quality must be one of ${QUALITIES.map((q) => JSON.stringify(q)).join(', ')}, not ${JSON.stringify(quality)}`,
    )
  }
}

function checkMaxTokens(maxTokens: number | undefined): number {
  const value = maxTokens ?? 512
  if (!Number.isInteger(value) || value < 1) {
    throw new TypeError(`maxTokens must be a positive int, not ${JSON.stringify(maxTokens) ?? 'undefined'}`)
  }
  return value
}

function openPdf(data: Buffer): PdfDocument {
  const kind = kindOf(data)
  if (kind === 'office') {
    throw new Error('toMarkdown/toJson/toChunks read PDF files in v1; Office files are not scheduled yet')
  }
  if (kind !== 'pdf') {
    throw new Error('not a PDF file: no %PDF- in the first 1024 bytes')
  }
  return PdfDocument.open(data)
}

/**
 * The PDF as Markdown, in reading order.
 *
 * `source` is a path, a `Buffer` or a `Uint8Array`. `pages` lists 0-based
 * page indices (`undefined` for the whole file).
 */
export function toMarkdown(source: Source, options: MarkdownOptions = {}): string {
  checkQuality(options.quality)
  const { data } = readSource(source)
  const doc = openPdf(data)
  return doc.extractMarkdown(options.pages !== undefined ? { pages: options.pages } : undefined)
}

/**
 * The PDF as a Docling `DoclingDocument`, as an object.
 *
 * The same JSON docling-core's `DoclingDocument.model_validate_json` reads.
 * `name` is the file's stem for path inputs, `'document'` otherwise.
 */
export function toJson(source: Source, options: JsonOptions = {}): unknown {
  checkQuality(options.quality)
  const { data, name } = readSource(source)
  const doc = openPdf(data)
  const raw = doc.extractDocling(options.pages !== undefined ? { pages: options.pages } : undefined, name)
  return JSON.parse(raw) as unknown
}

// ---------------------------------------------------------------------------
// Chunks. This mirrors `pdfluent.chunks.iter_chunks` (blocks mode,
// hierarchical): one chunk per paragraph, heading and list item, one per
// table, running heads left out, every chunk carrying its page, its
// enclosing headings and its box. The two implementations share the metadata
// names (`section_path` there is `sectionPath` here), so a pipeline can move
// between them.
// ---------------------------------------------------------------------------

/** The binding's camelCase kinds as the snake_case chunk metadata names. */
function chunkKind(kind: string): string {
  if (kind === 'listItem') return 'list_item'
  if (kind === 'tableCell') return 'table_cell'
  return kind
}

function union(a: number[], b: number[]): [number, number, number, number] {
  return [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.max(a[2], b[2]), Math.max(a[3], b[3])]
}

function tableText(cells: BlockInfo[]): string {
  const rows = new Map<number, BlockInfo[]>()
  for (const cell of cells) {
    const list = rows.get(cell.row ?? 0) ?? []
    list.push(cell)
    rows.set(cell.row ?? 0, list)
  }
  return [...rows.keys()]
    .sort((x, y) => x - y)
    .map((r) =>
      (rows.get(r) ?? [])
        .sort((x, y) => (x.column ?? 0) - (y.column ?? 0))
        .map((c) => c.text.trim())
        .join(' | '),
    )
    .join('\n')
}

/** The section stack below a heading: cut to its level, then pushed. */
function enterHeading(stack: string[], text: string, level: number | undefined): string[] {
  const depth = Math.max(level ?? 1, 1)
  return [...stack.slice(0, depth - 1), text]
}

interface TablePiece {
  text: string
  cells: BlockInfo[]
  part: string
}

/** A table run as pieces, or null to keep one chunk. A row is never split. */
function splitTable(run: BlockInfo[], maxChars: number): TablePiece[] | null {
  const rows = new Map<number, BlockInfo[]>()
  for (const cell of run) {
    const list = rows.get(cell.row ?? 0) ?? []
    list.push(cell)
    rows.set(cell.row ?? 0, list)
  }
  const ordered = [...rows.keys()]
    .sort((x, y) => x - y)
    .map((r) => (rows.get(r) ?? []).sort((x, y) => (x.column ?? 0) - (y.column ?? 0)))
  const lines = ordered.map((row) => row.map((c) => c.text.trim()).join(' | '))
  if (lines.join('\n').length <= maxChars) return null
  const header = lines[0]
  const headerCells = ordered[0]
  const pieces: Array<Array<{ line: string; cells: BlockInfo[] }>> = []
  let current: Array<{ line: string; cells: BlockInfo[] }> = []
  let size = header.length
  for (let i = 1; i < lines.length; i++) {
    if (current.length > 0 && size + 1 + lines[i].length > maxChars) {
      pieces.push(current)
      current = []
      size = header.length
    }
    current.push({ line: lines[i], cells: ordered[i] })
    size += 1 + lines[i].length
  }
  if (current.length > 0) pieces.push(current)
  if (pieces.length < 2) return null
  return pieces.map((piece, at) => ({
    text: [header, ...piece.map((p) => p.line)].join('\n'),
    cells: [...headerCells, ...piece.flatMap((p) => p.cells)],
    part: `${at + 1} of ${pieces.length}`,
  }))
}

function blockChunks(blocks: BlockInfo[], maxChars: number): Chunk[] {
  const out: Chunk[] = []
  let section: string | null = null
  let path: string[] = []
  let at = 0
  const cellKey = (b: BlockInfo): [number, number] => [b.row ?? 0, b.column ?? 0]
  while (at < blocks.length) {
    const block = blocks[at]
    if (chunkKind(block.kind) === 'table_cell') {
      const run = [block]
      at += 1
      while (at < blocks.length) {
        const next = blocks[at]
        const [r0, c0] = cellKey(run[run.length - 1])
        const [r1, c1] = cellKey(next)
        if (chunkKind(next.kind) !== 'table_cell' || next.page !== block.page || r1 < r0 || (r1 === r0 && c1 <= c0)) break
        run.push(next)
        at += 1
      }
      const split = splitTable(run, maxChars)
      const pieces = split ?? [{ text: tableText(run), cells: run, part: null as string | null }]
      for (const piece of pieces) {
        let bbox = [...piece.cells[0].bbox] as [number, number, number, number]
        for (const cell of piece.cells.slice(1)) bbox = union(bbox, cell.bbox)
        const metadata: ChunkMetadata = {
          page: block.page,
          kind: 'table',
          bbox,
          sectionPath: [...path],
        }
        if (section !== null) metadata.section = section
        if (piece.part !== null) metadata.tablePart = piece.part
        out.push({ text: piece.text, metadata })
      }
      continue
    }
    at += 1
    const kind = chunkKind(block.kind)
    if (kind === 'header' || kind === 'footer') continue
    const text = block.text.trim()
    if (!text) continue
    let ancestors: string[] | null = null
    if (kind === 'heading') {
      section = text
      const stack = enterHeading(path, text, block.level)
      ancestors = stack.slice(0, -1)
      path = stack
    }
    const metadata: ChunkMetadata = {
      page: block.page,
      kind,
      bbox: [...block.bbox] as [number, number, number, number],
      sectionPath: ancestors !== null ? [...ancestors] : [...path],
    }
    if (block.confidence !== undefined) metadata.confidence = block.confidence
    if (kind === 'heading' && block.level !== undefined) metadata.level = block.level
    if (block.continuesPrevious) metadata.continuesPrevious = true
    if (section !== null && kind !== 'heading') metadata.section = section
    out.push({ text, metadata })
  }
  return out
}

/**
 * The PDF as RAG chunks, in reading order.
 *
 * One chunk per paragraph, heading and list item, and one per table. Every
 * chunk carries `page`, `sectionPath` and `bbox` in its metadata, plus
 * `source` for path inputs. `pages` lists 0-based page indices; chunks come
 * back per page in the listed order.
 */
export function toChunks(source: Source, options: ChunksOptions = {}): Chunk[] {
  checkQuality(options.quality)
  const maxChars = checkMaxTokens(options.maxTokens) * CHARS_PER_TOKEN
  const { data, path } = readSource(source)
  const doc = openPdf(data)
  const pages = options.pages
  const blocks: BlockInfo[] =
    pages === undefined
      ? doc.extractBlocks()
      : pages.flatMap((page) => doc.extractBlocks({ firstPage: page, pageCount: 1 }))
  const chunks = blockChunks(blocks, maxChars)
  if (path !== null) {
    for (const chunk of chunks) chunk.metadata = { source: path, ...chunk.metadata }
  }
  return chunks
}
