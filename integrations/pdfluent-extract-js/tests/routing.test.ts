// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
import { describe, expect, test } from 'vitest'

import { kindOf, readSource } from '../src/source.js'

/**
 * Routing tolerates a PDF header anywhere in the first 1024 bytes: the PDF
 * spec allows leading garbage there, and requiring it at offset 0 refused
 * real-world files. Hermetic: `source.ts` imports nothing but node builtins,
 * so this runs with no engine build.
 */
describe('PDF header window', () => {
  test('a header at offset zero is pdf', () => {
    expect(kindOf(Buffer.from('%PDF-1.7\n'))).toBe('pdf')
  })

  test('a header within the first 1024 bytes is pdf', () => {
    const data = Buffer.concat([Buffer.alloc(100, 0x0a), Buffer.from('%PDF-1.7\nbody')])
    expect(kindOf(data)).toBe('pdf')
  })

  test('a header at the edge of the window is pdf', () => {
    const data = Buffer.concat([Buffer.alloc(1019, 0x20), Buffer.from('%PDF-')])
    expect(kindOf(data)).toBe('pdf')
  })

  test('a header past the window is unknown', () => {
    const data = Buffer.concat([Buffer.alloc(1024, 0x20), Buffer.from('%PDF-1.7\n')])
    expect(kindOf(data)).toBe('unknown')
  })

  test('garbage is unknown', () => {
    expect(kindOf(Buffer.from('not a document'))).toBe('unknown')
  })

  test('a ZIP still routes office', () => {
    expect(kindOf(Buffer.from([0x50, 0x4b, 0x03, 0x04, 0x00]))).toBe('office')
  })

  test('readSource keeps buffers byte-identical for the routing', () => {
    const data = Buffer.concat([Buffer.alloc(50, 0x0a), Buffer.from('%PDF-1.7\n')])
    const { data: back, name } = readSource(data)
    expect(back.equals(data)).toBe(true)
    expect(kindOf(back)).toBe('pdf')
    expect(name).toBe('document')
  })
})
