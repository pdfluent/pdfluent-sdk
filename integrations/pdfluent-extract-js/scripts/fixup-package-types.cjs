#!/usr/bin/env node
'use strict'

// After `tsc -p tsconfig.{cjs,esm}.json`: mark each output dir with its module
// system, so `require('@pdfluent/extract')` and `import ...` resolve to the
// right file without a bundler. No code is rewritten.
const fs = require('fs')
const path = require('path')

const root = path.resolve(__dirname, '..')
fs.mkdirSync(path.join(root, 'dist', 'cjs'), { recursive: true })
fs.mkdirSync(path.join(root, 'dist', 'esm'), { recursive: true })
fs.writeFileSync(
  path.join(root, 'dist', 'cjs', 'package.json'),
  JSON.stringify({ type: 'commonjs' }) + '\n',
)
fs.writeFileSync(
  path.join(root, 'dist', 'esm', 'package.json'),
  JSON.stringify({ type: 'module' }) + '\n',
)
// The console script must execute: tsc drops the mode, so the bin gets +x
// here, where every build passes.
fs.chmodSync(path.join(root, 'dist', 'cjs', 'cli.js'), 0o755)
console.log('[fixup-package-types] dist/cjs is commonjs, dist/esm is module')
