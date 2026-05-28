// ESM smoke test — verify `import` from the ESM entry point exposes the
// canonical license surface and `status()` works without an active license.
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const mod = await import(path.resolve(__dirname, '..', 'index.mjs'))

if (typeof mod.activate !== 'function') {
  console.error('FAIL: activate is not a function')
  process.exit(1)
}
if (typeof mod.setLicenseKey !== 'function') {
  console.error('FAIL: setLicenseKey is not a function')
  process.exit(1)
}
if (typeof mod.status !== 'function') {
  console.error('FAIL: status is not a function')
  process.exit(1)
}
if (typeof mod.licenseStatus !== 'function') {
  console.error('FAIL: licenseStatus is not a function')
  process.exit(1)
}
if (typeof mod.PdfluentLicenseError !== 'function') {
  console.error('FAIL: PdfluentLicenseError is not exported')
  process.exit(1)
}

const s = mod.status()
if (typeof s !== 'object' || typeof s.tier !== 'string') {
  console.error('FAIL: status() returned unexpected shape:', s)
  process.exit(1)
}

console.log('OK ESM: tier=' + s.tier + ' active=' + s.active)
