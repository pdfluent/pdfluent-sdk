// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

// Hand-written typings for the chunker (see chunks.js).
// `scripts/build/postbuild.cjs` re-exports these from the napi-generated
// `index.d.ts` after every build. SOURCE OF TRUTH: this file — do not edit
// the re-export lines inside `index.d.ts`.

import type { PdfDocument } from './index'

/** One table on a page: its box and its grid's extent. */
export interface ChunkTable {
  /** `[x0, y0, x1, y1]` in PDF user space, y up: the union of its cells' boxes. */
  bbox: Array<number>
  /** How many rows the grid holds. */
  rows: number
  /** How many columns the grid holds. */
  columns: number
}

/**
 * An outline entry pointing at a page, as `[level, title, page]`
 * (pymupdf4llm's shape): levels from 1, pages 0-based like
 * {@link ChunkMetadata.page}.
 */
export type TocItem = [level: number, title: string, page: number]

/** Where a chunk came from. */
export interface ChunkMetadata {
  /** The path the document was opened from, when it was opened from one. */
  source?: string
  /** The 0-based page index (the numbering of `extractBlocks`). */
  page: number
  /**
   * `paragraph`, `heading`, `listItem`, `table`, `header`, `footer` -- or
   * `page` in markdown mode. The binding's own kind spellings, as
   * `extractBlocks` reports them.
   */
  kind: string
  /** `[x0, y0, x1, y1]` in points, y up (blocks mode only). */
  bbox?: Array<number>
  /**
   * The learned model's probability for `kind`, present only where a model
   * decided it (absent is "not measured", not "low").
   */
  confidence?: number
  /** The text of the last heading above the chunk, when there is one. */
  section?: string
  /** A heading's level. */
  level?: number
  /** On the second half of a paragraph a page break cut in two. */
  continuesPrevious?: boolean
  /** The enclosing headings (only with `hierarchical`). */
  sectionPath?: Array<string>
  /** Which piece of a split table this is, `"i of n"` (only with `hierarchical`). */
  tablePart?: string
  /** The outline entries pointing at this page (markdown mode only). */
  toc_items?: Array<TocItem>
  /** One entry per table on this page (markdown mode only). */
  tables?: Array<ChunkTable>
}

/** One piece of a document to embed, and where it came from. */
export interface Chunk {
  text: string
  metadata: ChunkMetadata
}

/** What `iterChunks` reads. Every field is optional. */
export interface IterChunksOptions {
  /** `'blocks'` (default: one chunk per block) or `'markdown'` (one per page). */
  mode?: 'blocks' | 'markdown'
  /** Keep running heads and page numbers. Default false. */
  includeFurniture?: boolean
  /** First page of the window, counted from zero. Default 0. */
  firstPage?: number
  /** How many pages from `firstPage`; absent for the rest of the document. */
  pageCount?: number
  /**
   * Add `sectionPath` to every chunk and split tables longer than `maxChars`
   * by rows, repeating the header row in each piece. Default false.
   */
  hierarchical?: boolean
  /** The table-splitting budget. Default 2000. */
  maxChars?: number
}

/**
 * The document as chunks, in reading order.
 *
 * Preview: output quality is still improving and the API may change in a minor release.
 *
 * A port of `pdfluent.chunks.iter_chunks` (Python) with the same semantics.
 * `document` is an open `PdfDocument`, or a path or `Buffer` to open.
 *
 * @experimental
 */
export declare function iterChunks(
  document: PdfDocument | string | Buffer,
  options?: IterChunksOptions,
): Array<Chunk>
