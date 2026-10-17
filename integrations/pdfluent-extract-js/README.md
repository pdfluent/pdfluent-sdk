# @pdfluent/extract

PDF files to Markdown, JSON and RAG chunks, in three calls. It is a thin
wrapper: the engine underneath is
[`@pdfluent/node`](https://www.npmjs.com/package/@pdfluent/node), which npm
pulls in automatically. There is nothing to compile and no system dependency
to install — on Linux arm64 and Alpine/musl as much as anywhere else, because
`@pdfluent/node` 1.1.0 ships a binary for every platform in the matrix below.

## Install

```bash
npm install @pdfluent/extract
```

It needs Node 18 or later and `@pdfluent/node` 1.1.0 or later (the first
release with `extractMarkdown`/`extractBlocks`/`extractDocling`). Both
`require('@pdfluent/extract')` and `import ... from '@pdfluent/extract'` work;
types ship in the package.

## Use

```ts
import { toMarkdown } from '@pdfluent/extract'

console.log(toMarkdown('report.pdf'))
```

`toMarkdown` takes a path, a `Buffer` or a `Uint8Array`. `pages` lists
0-based page indices (`undefined` for the whole file).

```ts
import { toJson } from '@pdfluent/extract'

const doc = toJson('report.pdf') as { name: string; texts: unknown[] }
console.log(doc.name, doc.texts.length)
```

`toJson` returns the file as a Docling `DoclingDocument`, as an object — the
same JSON docling-core reads.

## Command line

```console
$ pdfluent-extract report.pdf -o report.md
$ pdfluent-extract report.pdf --format json --pages 1-3 > doc.json
$ cat report.pdf | pdfluent-extract - --format text
```

`--format` is one of `md`, `json`, `chunks` or `text`; `--pages` counts
from 1 (`1-3,5`). Without `-o` (or with `-o -`) the output goes to stdout.

## Chunks for RAG

```ts
import { toChunks } from '@pdfluent/extract'

for (const chunk of toChunks('report.pdf', { maxTokens: 512 })) {
  console.log(chunk.metadata.page, chunk.metadata.sectionPath, chunk.text.slice(0, 80))
}
```

One chunk per paragraph, heading and list item, and one per table. Every chunk
carries `page`, `sectionPath` (the enclosing headings) and `bbox` in its
metadata, plus `source` for path inputs. Tables longer than the budget split
by rows with the header repeated.

## Supported platforms

Every platform `@pdfluent/node` 1.1.0 ships a binary for: macOS x64 and
arm64, Windows x64, Linux x86_64 and arm64 on glibc and musl.

## Quality

`quality: 'fast'` is the only level in v1; anything else is a `TypeError`.
Further levels (vision/OCR) are designed later. PDF only in v1.

## Licence

AGPL-3.0 or a commercial licence, at your option; see `LICENSE` and
`LICENSE-COMMERCIAL`. The commercial licence is sold yearly and self-service
at <https://pdfluent.com/sdk/pricing/>.
