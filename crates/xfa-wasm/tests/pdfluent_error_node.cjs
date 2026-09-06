// Node-based smoke test for the PdfluentError fallback path.
//
// We cannot exercise the real WASM throw path without a wasm-pack build,
// but we can validate the JS-side class declaration that ships in the
// inline_js attribute of `crates/xfa-wasm/src/pdfluent_error.rs`.
//
// This script extracts the JS source from the Rust file, evaluates it,
// then asserts that the resulting class:
//   1. exists on globalThis.__PdfluentError
//   2. is a subclass of Error
//   3. produces instances with the right shape:
//        instance instanceof PdfluentError === true
//        instance instanceof Error === true
//        instance.code, .message, .operation, .help, .docsUrl, .legacyCode
//
// Run: node crates/xfa-wasm/tests/pdfluent_error_node.cjs

const fs = require('fs');
const path = require('path');
const assert = require('assert/strict');

const rustSrc = fs.readFileSync(
  path.join(__dirname, '..', 'src', 'pdfluent_error.rs'),
  'utf8',
);

// Extract the inline_js block.
const m = rustSrc.match(/inline_js = r#"([\s\S]*?)"#\)\]/);
if (!m) {
  console.error('FAIL: could not locate inline_js block');
  process.exit(1);
}
const js = m[1];

// Evaluate in a fresh sandbox. The block uses `export function ...`, strip it.
const ctorSrc = js.replace(/^export function/m, 'function');
// eslint-disable-next-line no-new-func
new Function('globalThis', `${ctorSrc}\nreturn __pdfluent_error_ctor;`)(globalThis)();

const PdfluentError = globalThis.__PdfluentError;
assert.ok(PdfluentError, 'PdfluentError must be installed on globalThis');
assert.equal(typeof PdfluentError, 'function');
assert.ok(PdfluentError.prototype instanceof Error, 'extends Error');

const err = new PdfluentError(
  'bytes are not a PDF',
  'E-PARSE-INVALID-PDF',
  'PdfDoc.open',
  'Ensure the input is a complete PDF.',
  'https://pdfluent.com/errors/E-PARSE-INVALID-PDF',
  'INVALID_PDF',
);

assert.ok(err instanceof PdfluentError, 'instanceof PdfluentError');
assert.ok(err instanceof Error, 'instanceof Error');
assert.equal(err.name, 'PdfluentError');
assert.equal(err.code, 'E-PARSE-INVALID-PDF');
assert.equal(err.message, 'bytes are not a PDF');
assert.equal(err.operation, 'PdfDoc.open');
assert.equal(err.help, 'Ensure the input is a complete PDF.');
assert.equal(err.docsUrl, 'https://pdfluent.com/errors/E-PARSE-INVALID-PDF');
assert.equal(err.legacyCode, 'INVALID_PDF');

// Caching: subsequent calls reuse the same class.
const before = globalThis.__PdfluentError;
new Function('globalThis', `${ctorSrc}\nreturn __pdfluent_error_ctor;`)(globalThis)();
assert.equal(globalThis.__PdfluentError, before, 'ctor must be cached on globalThis');

// Legacy code dispatch reads through.
const err2 = new PdfluentError(
  'page 9 is out of range',
  'E-WASM-PAGE-OUT-OF-RANGE',
  'document.page',
  '',
  'https://pdfluent.com/errors/E-WASM-PAGE-OUT-OF-RANGE',
  'PAGE_OUT_OF_RANGE',
);
assert.equal(err2.legacyCode, 'PAGE_OUT_OF_RANGE');
assert.equal(err2.code, 'E-WASM-PAGE-OUT-OF-RANGE');

console.log('OK: PdfluentError class shape verified (7 assertions)');
