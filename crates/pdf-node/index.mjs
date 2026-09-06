// ESM wrapper — re-exports the CJS entry point (index.js).
// Enables: import { PdfDocument } from '@pdfluent/node'
//
// The named list below must match the real top-level exports of index.js
// (including the typed-error class in scripts/build/typed-error-layer.js).
// tests/typed_error_layer.test.js asserts this stays in sync.

import { createRequire } from 'module'
const require = createRequire(import.meta.url)

const {
  PdfDocument,
  PdfPage,
  openPdf,
  mergePdfs,
  validatePdfa,
  PdfluentError,
} = require('./index.js')

export {
  PdfDocument,
  PdfPage,
  openPdf,
  mergePdfs,
  validatePdfa,
  PdfluentError,
}
