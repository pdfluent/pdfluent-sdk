// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/**
 * The one typed-error class the Node binding throws, and the guards that
 * throw it.
 *
 * SOURCE OF TRUTH for the class: this file. The layer appended to the
 * napi-generated `index.js` (see `scripts/build/typed-error-layer.js`, applied
 * by `scripts/build/postbuild.cjs`) only requires this module and applies the
 * guards; the ESM wrapper (`index.mjs`) re-exports the class from `index.js`.
 * Every entry point and every guarded call-site therefore throws the same
 * class object.
 *
 * The class lives on the real `process` object, not in module scope and not
 * on `globalThis`, because `index.js` can be evaluated more than once per
 * process over a single native binding: every jest file gets a fresh module
 * registry AND a fresh `globalThis` (one VM context per file) while the
 * `.node` binary (and its prototypes, which the guards mutate in place) is
 * shared process-wide — one worker runs many files in sequence — and a
 * process can likewise hold both the CJS entry and the ESM wrapper. A
 * per-evaluation class made instance-method errors fail `instanceof` against
 * the evaluating file's own export (the second file's 14 cases), while
 * statics and top-level functions — guarded afresh per evaluation — passed.
 * Outside jest `process` is one object per process, so the singleton keeps
 * exactly one class however often the entry point is evaluated, and a second
 * copy of this package nested under another `node_modules` shares it too; the
 * catalogue shape is identical, so an error stays `instanceof` either copy's
 * export.
 *
 * Under jest the singleton alone is NOT enough: jest hands every test file a
 * deep copy of `process` (jest-util `createProcessObject`), so each file still
 * gets its own class, while the native prototypes stay shared across the
 * files of a worker. The guards therefore re-wrap an already-guarded native
 * member whenever the class it converts to is not the current evaluation's:
 * the evaluation that runs now (jest runs one file at a time per worker) owns
 * the shared prototypes. Keeping the first evaluation's wrapper instead made
 * every instance-method error of the second file in a worker fail
 * `instanceof` against that file's own export — which file came second
 * depended on how jest spread the files over the workers, so it showed up on
 * the 4-core Windows runner and not on the wider macOS one.
 *
 * Consequence: under jest the most recent evaluation of `index.js` decides
 * the class of instance-method errors. A test that calls `jest.resetModules()`
 * and requires the package again makes errors reached through an earlier
 * `mod` fail `instanceof` against that earlier export.
 */
'use strict'

const SINGLETON_KEY = '__pdfluentErrors__'

function singletonHolder() {
  if (typeof process === 'object' && process !== null) return process
  return globalThis
}

function errorNamespace() {
  const holder = singletonHolder()
  const ns = holder[SINGLETON_KEY] || {}
  holder[SINGLETON_KEY] = ns
  return ns
}

if (!errorNamespace().PdfluentError) {
  errorNamespace().PdfluentError = class PdfluentError extends Error {
    constructor(message, opts) {
      super(message)
      this.name = 'PdfluentError'
      // Never null: the typings promise strings, and a caller that constructs
      // one without metadata gets the catalogue's "should never happen" code.
      this.code = (opts && typeof opts.code === 'string' && opts.code) || 'E-INTERNAL'
      this.operation = (opts && typeof opts.operation === 'string' && opts.operation) || 'unknown'
      this.cause = opts && opts.cause != null ? opts.cause : null
      if (Error.captureStackTrace) Error.captureStackTrace(this, errorNamespace().PdfluentError)
    }
  }
}

const PdfluentError = errorNamespace().PdfluentError

// The Rust side (crates/pdf-node/src/error.rs) puts a JSON payload in the
// error's message: {"code", "message", "operation", "cause"}. Turn that into a
// PdfluentError. Anything else (napi's own argument-conversion errors, which
// fire before an operation starts) passes through untouched, as does an error
// an earlier guard layer already converted.
function toPdfluentError(e) {
  if (e instanceof PdfluentError) return e
  if (!e || typeof e.message !== 'string' || e.message.charAt(0) !== '{') return e
  let payload
  try {
    payload = JSON.parse(e.message)
  } catch (_) {
    return e
  }
  if (!payload || typeof payload.code !== 'string') return e
  return new PdfluentError(payload.message, payload)
}

// Set on every wrapper: the class it converts to, and the native function it
// wraps. A second evaluation over the same native prototypes never wraps a
// wrapper — it keeps the stored one when that converts to this evaluation's
// class, and otherwise wraps the original native function afresh (see the
// header: under jest each test file has its own class).
const GUARDED = '__pdfluentGuarded'
const ORIGINAL = '__pdfluentOriginal'

function guard(fn) {
  if (fn && fn[GUARDED]) {
    if (fn[GUARDED] === PdfluentError) return fn
    // A wrapper from an older build carries GUARDED but no ORIGINAL: keep it as the function to wrap.
    fn = fn[ORIGINAL] || fn
  }
  function pdfluentGuarded(...args) {
    let result
    try {
      result = fn.apply(this, args)
    } catch (e) {
      throw toPdfluentError(e)
    }
    if (result && typeof result.then === 'function') {
      return result.then(undefined, (e) => {
        throw toPdfluentError(e)
      })
    }
    return result
  }
  pdfluentGuarded[GUARDED] = PdfluentError
  pdfluentGuarded[ORIGINAL] = fn
  return pdfluentGuarded
}

// Re-define every method and getter of a native prototype in place, so
// instances the native side creates (doc.page(i) returns a PdfPage) are
// covered too.
function guardMembers(proto) {
  for (const key of Object.getOwnPropertyNames(proto)) {
    if (key === 'constructor') continue
    const d = Object.getOwnPropertyDescriptor(proto, key)
    if (!d || !d.configurable) continue
    if (typeof d.value === 'function') {
      if (d.value[GUARDED] === PdfluentError) continue
      d.value = guard(d.value)
    } else if (typeof d.get === 'function') {
      if (d.get[GUARDED] === PdfluentError) continue
      d.get = guard(d.get)
      if (typeof d.set === 'function') d.set = guard(d.set)
    } else {
      continue
    }
    Object.defineProperty(proto, key, d)
  }
}

// napi defines a class's static factories (PdfDocument.open) as frozen
// properties, so they cannot be re-defined on the class, and a Proxy may not
// return anything else for them. The export becomes a stand-in that shares
// the native prototype -- `instanceof` holds for instances either side makes
// -- and carries guarded copies of the statics.
function guardClass(Native) {
  guardMembers(Native.prototype)
  const Guarded = function (...args) {
    try {
      return Reflect.construct(Native, args, new.target || Guarded)
    } catch (e) {
      throw toPdfluentError(e)
    }
  }
  Object.defineProperty(Guarded, 'name', { value: Native.name })
  Guarded.prototype = Native.prototype
  Object.setPrototypeOf(Guarded, Native)
  for (const key of Object.getOwnPropertyNames(Native)) {
    if (key === 'prototype' || key === 'length' || key === 'name') continue
    const d = Object.getOwnPropertyDescriptor(Native, key)
    if (typeof d.value === 'function') {
      Object.defineProperty(Guarded, key, { ...d, value: guard(d.value) })
    }
  }
  return Guarded
}

// Everything the addon exports, not a list: a surface added later is covered
// without anyone remembering this file.
function applyTypedErrorLayer(nativeBinding, exports) {
  for (const name of Object.keys(nativeBinding)) {
    const value = nativeBinding[name]
    if (typeof value !== 'function' || exports[name] !== value) continue
    exports[name] = /^[A-Z]/.test(name) ? guardClass(value) : guard(value)
  }
  exports.PdfluentError = PdfluentError
}

module.exports = { PdfluentError, toPdfluentError, applyTypedErrorLayer }
