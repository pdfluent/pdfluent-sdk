// ESM typings for @pdfluent/node.
//
// `index.d.ts` is a CommonJS declaration file (napi-rs generates it alongside
// the CJS `index.js`). Hanging it off the `import` condition as well made the
// package "masquerade as CJS": an ESM consumer under `moduleResolution:
// node16` got ESM JavaScript (`index.mjs`) described by CJS typings, which is
// the dual-package hazard `attw` flags and the reason for this file (#585).
//
// This declaration file is ESM by extension, so the `import` condition now
// resolves to a matching module kind. It re-exports the generated surface;
// nothing is maintained twice.
export * from './index.js'
