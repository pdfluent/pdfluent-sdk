// @pdfluent-typed-error-layer
// Hand-maintained typings for the typed-error layer (see typed-error-layer.js).
// `scripts/build/postbuild.cjs` appends this to the napi-generated `index.d.ts`
// after every build. SOURCE OF TRUTH: this file — do not edit the copy inside
// `index.d.ts`.

/**
 * Base class for all PDFluent SDK errors.
 *
 * Always branch on `.code` — never on `.message`.
 */
export declare class PdfluentError extends Error {
  /** Stable machine-readable error code from the C8 error catalogue. */
  readonly code: string | null
  /** Short verb describing the operation that failed (e.g. "activate"). */
  readonly operation: string | null
  /** Optional human-readable detail string. May be `null`. */
  readonly cause: string | null
}

/** License-specific error subclass thrown by all license activation surfaces. */
export declare class PdfluentLicenseError extends PdfluentError {}
