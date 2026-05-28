// QR-11 Node runtime error-mapping. Imports the LOCAL napi artifact by
// absolute path (identity guard) and asserts canonical error cases throw,
// with a valid control succeeding. Exit 0 green, 2 skip, 1 fail.
const path = require('path');
const fs = require('fs');
const ROOT = path.resolve(__dirname, '../../..');
const idx = path.join(ROOT, 'crates/pdf-node/index.js');
if (!fs.existsSync(idx)) { console.log('SKIP node artifact missing:', idx); process.exit(2); }
const m = require(idx);
if (typeof m.PdfDocument !== 'function') { console.log('SKIP not the PDFluent node binding (no PdfDocument)'); process.exit(2); }
console.log('identity OK:', idx);

const fails = [], observed = {};
function expectThrow(label, fn) {
  try { fn(); fails.push(`${label}: no throw (silent success)`); observed[label] = 'NO_THROW'; }
  catch (e) {
    const msg = (e && e.message) ? e.message : String(e);
    if (!msg || msg.length === 0) { fails.push(`${label}: empty error`); observed[label] = 'EMPTY'; }
    else observed[label] = msg.slice(0, 60);
  }
}
// valid control
try {
  const bytes = fs.readFileSync(path.join(ROOT, 'tests/corpus-mini/multi-page.pdf'));
  const d = m.PdfDocument.open(bytes);
  const pc = typeof d.pageCount === 'function' ? d.pageCount() : d.pageCount;
  observed.valid_control = `pages=${pc}`;
  if (!(pc >= 1)) fails.push('valid_control: pages < 1');
} catch (e) { fails.push('valid_control: valid PDF failed: ' + (e.message || e)); }
// hostile
expectThrow('malformed', () => m.PdfDocument.open(Buffer.from([0xDE,0xAD,0xBE,0xEF,0x00,0x42])));
expectThrow('empty', () => m.PdfDocument.open(Buffer.alloc(0)));
expectThrow('truncated', () => m.PdfDocument.open(Buffer.from('%PDF-1.7\n1 0 obj')));

for (const [k,v] of Object.entries(observed)) console.log(`  ${k}: ${v}`);
if (fails.length) { console.error('FAIL:', fails.join('; ')); process.exit(1); }
console.log('QR-11 node error mapping: OK'); process.exit(0);
