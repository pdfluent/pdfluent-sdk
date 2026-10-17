// @pdfluent-typed-error-layer
// Hand-maintained typings for the typed-error layer (see typed-error-layer.js).
// `scripts/build/postbuild.cjs` appends this to the napi-generated `index.d.ts`
// after every build. SOURCE OF TRUTH: this file — do not edit the copy inside
// `index.d.ts`.

/**
 * The error every PDFluent operation throws, synchronous or rejected.
 *
 * Always branch on `.code` — never on `.message`. There is one class, as in
 * the Python binding; `.code` names the family. The codes are the C8 error
 * catalogue (`docs/error_catalogue.md`), e.g. `E-PARSE-INVALID-PDF`,
 * `E-SECURITY-DECRYPTION-FAILED`, `E-PAGE-OUT-OF-RANGE`, `E-UNSUPPORTED`.
 *
 * Not a `PdfluentError`: napi's own argument-conversion errors (a string where
 * a number belongs), which fire before any operation starts.
 */
export declare class PdfluentError extends Error {
  /**
   * The binding constructs these; a caller that does (a test double, say)
   * has to supply the code and operation the type promises.
   */
  constructor(
    message: string,
    opts: { code: string; operation: string; cause?: string | null },
  )
  /** Stable machine-readable error code from the C8 error catalogue. */
  readonly code: string
  /** The name of the call that failed, e.g. "open" or "renderPage". */
  readonly operation: string
  /** Optional human-readable detail string. May be `null`. */
  readonly cause: string | null
}
