// @pdfluent-typed-error-layer
// ---------------------------------------------------------------------------
// Typed-error layer application stub for the Node binding.
//
// `napi build` regenerates `index.js` from the Rust source and does NOT know
// about the guards, so `scripts/build/postbuild.cjs` appends this block to
// the generated `index.js` after every build (idempotent — keyed on the
// marker comment on the first line above). Do NOT edit the copy inside
// `index.js`; edit THIS file.
//
// The class and the guards live in `pdfluent-error.js` (the source of truth),
// required here so the CJS entry, the ESM wrapper and every guarded compat
// call-site share exactly one `PdfluentError` class object, however often
// `index.js` is evaluated over the process's single native binding.
//
// It relies on `nativeBinding` being in scope (the module-level const that the
// napi-generated `index.js` defines).
// ---------------------------------------------------------------------------

const { PdfluentError, applyTypedErrorLayer } = require('./pdfluent-error')
applyTypedErrorLayer(nativeBinding, module.exports)
// A literal assignment, not only the one inside applyTypedErrorLayer: the
// ESM-surface test greps this file's text for `module.exports.<name>`, and
// the same object is exported either way.
module.exports.PdfluentError = PdfluentError
