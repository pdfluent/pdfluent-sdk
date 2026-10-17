#!/usr/bin/env node
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
/**
 * The `pdfluent-extract` console script: PDFs to Markdown, JSON and chunks.
 *
 * `pdfluent-extract <input> [-o OUT] [--format md|json|chunks|text]
 * [--quality fast|balanced|best] [--pages SPEC]`
 *
 * `-` as input reads stdin; `-o -` or no `-o` writes stdout. `--pages`
 * counts from 1 (`1-3,5`), like the main CLI. Exit 2 on a usage error, 1
 * on an extraction error, with a message on stderr either way.
 *
 * This module is the bin: it is executed, never imported. `main` sets
 * `process.exitCode` and returns instead of calling `process.exit`, so a
 * piped stdout flushes before the process ends.
 */

import { accessSync, constants, readFileSync, writeFileSync } from 'node:fs'
import { toChunks, toJson, toMarkdown, type Quality } from './index.js'
import { parsePages } from './pages.js'

const FORMATS = ['md', 'json', 'chunks', 'text'] as const
type Format = (typeof FORMATS)[number]
const QUALITIES = ['fast', 'balanced', 'best'] as const

interface Options {
  input: string
  output: string | null
  format: Format
  quality: string
  pages: number[] | undefined
}

function usage(): string {
  return [
    'Usage: pdfluent-extract <input> [-o OUT] [--format md|json|chunks|text]',
    '                          [--quality fast|balanced|best] [--pages SPEC]',
    '',
    '  <input>            input file, or - for stdin',
    '  -o OUT, --output   write here instead of stdout (- for stdout)',
    '  --format F         output shape, one of md, json, chunks, text (default md)',
    '  --quality Q        extraction quality, one of fast, balanced, best (default fast)',
    '  --pages SPEC       pages to extract, counting from 1 (e.g. 1-3,5)',
  ].join('\n')
}

function parseArgs(argv: string[]): Options {
  const options: Options = {
    input: '',
    output: null,
    format: 'md',
    quality: 'fast',
    pages: undefined,
  }
  let positional: string | null = null
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i]
    if (arg === '-o' || arg === '--output') {
      const value = argv[++i]
      if (value === undefined) throw new RangeError(`${arg} needs a file (or -)`)
      options.output = value
    } else if (arg === '--format') {
      const value = argv[++i]
      if (value === undefined || !(FORMATS as readonly string[]).includes(value)) {
        throw new RangeError(
          `--format must be one of ${FORMATS.join(', ')}, not ${JSON.stringify(value)}`,
        )
      }
      options.format = value as Format
    } else if (arg === '--quality') {
      const value = argv[++i]
      if (value === undefined || !(QUALITIES as readonly string[]).includes(value)) {
        throw new RangeError(
          `--quality must be one of ${QUALITIES.join(', ')}, not ${JSON.stringify(value)}`,
        )
      }
      options.quality = value
    } else if (arg === '--pages') {
      const value = argv[++i]
      if (value === undefined) throw new RangeError('--pages needs a spec (e.g. 1-3,5)')
      options.pages = parsePages(value)
    } else if (arg.startsWith('-') && arg !== '-') {
      throw new RangeError(`unknown option: ${arg}`)
    } else if (positional === null) {
      positional = arg
    } else {
      throw new RangeError(`unexpected argument: ${arg}`)
    }
  }
  if (positional === null) throw new RangeError('missing <input> (a file, or - for stdin)')
  options.input = positional
  return options
}

function readInput(input: string): string | Buffer {
  // A path stays a path (so chunks cite it and JSON names it); `-` reads
  // stdin as bytes.
  if (input !== '-') return input
  const data = readFileSync(0)
  if (data.length === 0) throw new Error('nothing on stdin')
  return data
}

function render(source: string | Buffer, options: Options): string {
  const quality = options.quality as Quality
  const pages = options.pages
  if (options.format === 'md') return toMarkdown(source, { quality, pages })
  if (options.format === 'json') {
    return JSON.stringify(toJson(source, { quality, pages }), null, 2)
  }
  const chunks = toChunks(source, { quality, pages })
  if (options.format === 'text') return chunks.map((chunk) => chunk.text).join('\n\n')
  return JSON.stringify(chunks, null, 2)
}

function writeOutput(text: string, output: string | null): void {
  if (!text.endsWith('\n')) text += '\n'
  if (output === null || output === '-') {
    process.stdout.write(text)
    return
  }
  writeFileSync(output, text, 'utf8')
}

function main(argv: string[]): void {
  if (argv.includes('-h') || argv.includes('--help')) {
    process.stdout.write(usage() + '\n')
    return
  }
  let options: Options
  try {
    options = parseArgs(argv)
  } catch (error) {
    console.error(`pdfluent-extract: ${error instanceof Error ? error.message : error}`)
    process.exitCode = 2
    return
  }
  let source: string | Buffer
  try {
    source = readInput(options.input)
    if (typeof source === 'string') accessSync(source, constants.R_OK)
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code
    if (code === 'ENOENT') {
      console.error(`pdfluent-extract: no such file: ${options.input}`)
    } else {
      console.error(
        `pdfluent-extract: cannot read ${options.input}: ${error instanceof Error ? error.message : error}`,
      )
    }
    process.exitCode = 1
    return
  }
  let text: string
  try {
    text = render(source, options)
  } catch (error) {
    console.error(
      `pdfluent-extract: extraction failed: ${error instanceof Error ? error.message : error}`,
    )
    process.exitCode = 1
    return
  }
  try {
    writeOutput(text, options.output)
  } catch (error) {
    console.error(
      `pdfluent-extract: cannot write ${options.output}: ${error instanceof Error ? error.message : error}`,
    )
    process.exitCode = 1
  }
}

main(process.argv.slice(2))
