// Node binding overhead: open+pageCount loop + malformed loop, per-iter ns percentiles.
// Usage: node node_perf.cjs <bindingDir> <validPdf> <iters>
const fs = require('fs'); const path = require('path');
const dir = process.argv[2], valid = process.argv[3], iters = parseInt(process.argv[4]||'400',10);
const mod = require(path.resolve(dir, 'index.js'));
const Doc = mod.PdfDocument;
if (typeof Doc?.open !== 'function' && typeof Doc?.fromBytes !== 'function') { console.error('no PdfDocument.open/fromBytes'); process.exit(2); }
const bytes = fs.readFileSync(valid);
const open = (b) => (Doc.fromBytes ? Doc.fromBytes(b) : Doc.open(b));
const pct = (a,q)=>a[Math.min(a.length-1,Math.floor(q*(a.length-1)))];
function loop(buf, expectOk, n){ const t=[]; let oks=0,errs=0,pages=-1;
  for(let i=0;i<n;i++){ const a=process.hrtime.bigint(); try{ const d=open(buf); pages=(typeof d.pageCount==='function'?d.pageCount():d.pageCount); oks++; }catch(e){ errs++; } t.push(Number(process.hrtime.bigint()-a)); }
  t.sort((x,y)=>x-y); return {samples:t.length,oks,errs,pages,min_ns:t[0],p50_ns:pct(t,.5),p95_ns:pct(t,.95),p99_ns:pct(t,.99),max_ns:t[t.length-1]}; }
// warmup
for(let i=0;i<30;i++){ try{open(bytes);}catch(e){} }
const valid_loop = loop(bytes, true, iters);
const garbage = Buffer.from([0xDE,0xAD,0xBE,0xEF,0x00,0x42]);
const malformed_loop = loop(garbage, false, iters);
const rss = process.memoryUsage().rss;
console.log(JSON.stringify({binding:'node',identity:require.resolve(path.resolve(dir,'index.js')),valid:valid_loop,malformed:malformed_loop,rss_bytes:rss,status:'green_measured'}));
