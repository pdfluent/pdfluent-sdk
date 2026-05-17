// ESM wrapper — re-exports everything from the CJS entry point.
// Enables: import { PdfDocument } from '@pdfluent/node'

import { createRequire } from 'module'
const require = createRequire(import.meta.url)

const {
  PdfDocument,
  PdfPage,
  openPdf,
  mergePdfs,
  validatePdfa,
  activate,
  setLicenseKey,
  status,
  licenseStatus,
  PdfluentError,
  PdfluentIoError,
  PdfluentParseError,
  PdfluentPasswordError,
  PdfluentPageError,
  PdfluentFormError,
  PdfluentOperationError,
  PdfluentLicenseError,
} = require('./index.js')

export {
  PdfDocument,
  PdfPage,
  openPdf,
  mergePdfs,
  validatePdfa,
  activate,
  setLicenseKey,
  status,
  licenseStatus,
  PdfluentError,
  PdfluentIoError,
  PdfluentParseError,
  PdfluentPasswordError,
  PdfluentPageError,
  PdfluentFormError,
  PdfluentOperationError,
  PdfluentLicenseError,
}
