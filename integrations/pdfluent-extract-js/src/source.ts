// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
/**
 * Sources in, kinds out. Split from `index.ts` so the routing is testable
 * without the engine: this module imports nothing but node builtins.
 */

import { readFileSync } from 'node:fs'
import { basename, extname } from 'node:path'

export type Source = string | Buffer | Uint8Array

export function readSource(source: Source): { data: Buffer; path: string | null; name: string } {
  if (typeof source === 'string') {
    const data = readFileSync(source)
    const stem = basename(source, extname(source))
    return { data, path: source, name: stem || 'document' }
  }
  if (Buffer.isBuffer(source)) return { data: source, path: null, name: 'document' }
  if (source instanceof Uint8Array) {
    return { data: Buffer.from(source.buffer, source.byteOffset, source.byteLength), path: null, name: 'document' }
  }
  throw new TypeError(`source must be a path, Buffer or Uint8Array, not ${typeof source}`)
}

/**
 * `'pdf'`, `'office'` or `'unknown'`, by magic bytes. The extension is never
 * consulted. The PDF header is searched in the first 1024 bytes rather than
 * prefix-matched — the PDF spec tolerates leading garbage there.
 */
export function kindOf(data: Buffer): 'pdf' | 'office' | 'unknown' {
  if (data.subarray(0, 1024).includes('%PDF-')) return 'pdf'
  if (data[0] === 0x50 && data[1] === 0x4b) return 'office'
  return 'unknown'
}
