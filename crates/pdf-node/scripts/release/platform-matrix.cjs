'use strict'

// Single source of truth for the N-API platform matrix.
//
// The deterministic dir generator, the main-manifest validator, the publish
// orchestrator, AND the regression tests all import THIS file. There is exactly
// one place the six-platform set is defined, so the main meta-package's
// `optionalDependencies` can never silently drift to a subset — which is the
// exact defect that shipped `@pdfluent/node@1.0.0-beta.17` with only 3 of 6
// platforms (a `napi prepublish` lifecycle hook rewrote the manifest from the
// 3 dirs `napi create-npm-dir` happened to emit).
//
// Order is canonical and must stay stable (tests assert against it).

const SCOPE = '@pdfluent'
const MAIN_PKG = `${SCOPE}/node`

/** @type {{tag:string, os:string, cpu:string, libc?:string}[]} */
const MATRIX = [
  { tag: 'darwin-arm64', os: 'darwin', cpu: 'arm64' },
  { tag: 'darwin-x64', os: 'darwin', cpu: 'x64' },
  { tag: 'linux-x64-gnu', os: 'linux', cpu: 'x64', libc: 'glibc' },
  { tag: 'linux-x64-musl', os: 'linux', cpu: 'x64', libc: 'musl' },
  { tag: 'linux-arm64-gnu', os: 'linux', cpu: 'arm64', libc: 'glibc' },
  { tag: 'win32-x64-msvc', os: 'win32', cpu: 'x64' },
]

/** Fully-qualified npm names of the six platform sub-packages. */
const PLATFORM_PKG_NAMES = MATRIX.map((m) => `${SCOPE}/node-${m.tag}`)

/** The `index.<tag>.node` artefact filename the loader (index.js) requires. */
const nodeFileFor = (tag) => `index.${tag}.node`

// Pure builder for a platform sub-package manifest. Fixed key order → identical
// JSON on every run (the generator's determinism, and a regression test, both
// rely on this being a pure function of its inputs).
function buildPlatformPkg(entry, version, base) {
  const nodeFile = nodeFileFor(entry.tag)
  return {
    name: `${SCOPE}/node-${entry.tag}`,
    version,
    os: [entry.os],
    cpu: [entry.cpu],
    ...(entry.libc ? { libc: [entry.libc] } : {}),
    main: nodeFile,
    files: [nodeFile, 'LICENSE', 'README.md'],
    description: base.description,
    keywords: base.keywords,
    homepage: base.homepage,
    license: base.license,
    engines: base.engines,
    publishConfig: { access: 'public' },
    repository: base.repository,
    bugs: base.bugs,
  }
}

module.exports = { SCOPE, MAIN_PKG, MATRIX, PLATFORM_PKG_NAMES, nodeFileFor, buildPlatformPkg }
