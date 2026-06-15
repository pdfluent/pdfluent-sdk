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
// napi-generated `index.js` defines) and overrides the raw license exports with
// versions that parse the Rust JSON error payload into typed errors.
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

class PdfluentLicenseError extends PdfluentError {
  constructor(message, opts) {
    super(message, opts)
    this.name = 'PdfluentLicenseError'
    if (Error.captureStackTrace) Error.captureStackTrace(this, PdfluentLicenseError)
  }
}

module.exports.PdfluentError = PdfluentError
module.exports.PdfluentLicenseError = PdfluentLicenseError

// License function wrappers — parse the structured JSON payload that the Rust
// layer embeds in error.message and re-throw as typed errors.
function _unwrapLicenseError(err, defaultOperation) {
  let parsed = null
  try { parsed = JSON.parse(err.message) } catch (_) {}
  if (parsed && typeof parsed.code === 'string') {
    throw new PdfluentLicenseError(parsed.message || String(err), {
      code: parsed.code,
      operation: parsed.operation || defaultOperation,
      cause: parsed.cause != null ? parsed.cause : null,
    })
  }
  throw err
}

module.exports.activate = function activate(licenseKey) {
  try { return nativeBinding.activate(licenseKey) } catch (e) { _unwrapLicenseError(e, 'activate') }
}

module.exports.setLicenseKey = function setLicenseKey(licenseKey) {
  try { return nativeBinding.setLicenseKey(licenseKey) } catch (e) { _unwrapLicenseError(e, 'setLicenseKey') }
}

module.exports.setLicensePublicKey = function setLicensePublicKey(key) {
  try { return nativeBinding.setLicensePublicKey(key) } catch (e) { _unwrapLicenseError(e, 'setLicensePublicKey') }
}

module.exports.setLicensePayload = function setLicensePayload(payloadJson) {
  try { return nativeBinding.setLicensePayload(payloadJson) } catch (e) { _unwrapLicenseError(e, 'setLicensePayload') }
}
