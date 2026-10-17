// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
/**
 * Page specs in, 0-based indices out. Split from `cli.ts` so the parsing is
 * unit-testable without the engine: this module imports nothing but types.
 *
 * Users count pages from 1 (`1-3,5`), like the main CLI; the engine counts
 * from 0. A bad spec throws a `RangeError` naming it — the CLI reports that
 * as a usage error (exit 2).
 */

export function parsePages(spec: string): number[] {
  const pages: number[] = []
  for (const raw of spec.split(',')) {
    const item = raw.trim()
    if (item === '') {
      throw new RangeError(`empty item in page spec '${spec}' (want e.g. \`1-3,5\`)`)
    }
    const dash = item.indexOf('-')
    if (dash >= 0) {
      const lo = Number(item.slice(0, dash))
      const hi = Number(item.slice(dash + 1))
      if (!Number.isInteger(lo) || !Number.isInteger(hi)) {
        throw new RangeError(`not a page range: '${item}' (in '${spec}')`)
      }
      if (lo < 1 || hi < 1) {
        throw new RangeError(`pages count from 1, so \`0\` names nothing (in '${spec}')`)
      }
      if (lo > hi) {
        throw new RangeError(`reversed range '${item}' (in '${spec}'; want \`lo-hi\`)`)
      }
      for (let page = lo - 1; page <= hi - 1; page++) pages.push(page)
    } else {
      const page = Number(item)
      if (!Number.isInteger(page)) {
        throw new RangeError(`not a page number: '${item}' (in '${spec}')`)
      }
      if (page < 1) {
        throw new RangeError(`pages count from 1, so \`0\` names nothing (in '${spec}')`)
      }
      pages.push(page - 1)
    }
  }
  return [...new Set(pages)].sort((a, b) => a - b)
}
