// @pdfluent-typed-error-layer
// ---------------------------------------------------------------------------
// Hand-maintained typed-error layer for the Node binding.
//
// SOURCE OF TRUTH: this file. `napi build` regenerates `index.js` from the Rust
// source and does NOT know about these classes/wrappers, so `scripts/build/
// postbuild.cjs` appends this block to the generated `index.js` after every
// build (idempotent — keyed on the marker comment on the first line above).
// Do NOT edit the copy inside `index.js`; edit THIS file.
//
// It relies on `nativeBinding` being in scope (the module-level const that the
// napi-generated `index.js` defines).
// ---------------------------------------------------------------------------

class PdfluentError extends Error {
  constructor(message, opts) {
    super(message)
    this.name = 'PdfluentError'
    this.code = (opts && opts.code) || null
    this.operation = (opts && opts.operation) || null
    this.cause = (opts && opts.cause != null) ? opts.cause : null
    if (Error.captureStackTrace) Error.captureStackTrace(this, PdfluentError)
  }
}

module.exports.PdfluentError = PdfluentError
