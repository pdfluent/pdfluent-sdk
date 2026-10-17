// The 1.1.0 browser tools (#613), called through the built wasm package.
//
// crates/xfa-wasm/tests/browser_tools.rs checks the logic natively. It cannot
// see what only happens on wasm32: signing called SystemTime::now(), which
// compiles there and traps at runtime, so every browser signature died as
// "RuntimeError: unreachable" while the whole native suite stayed green
// (#617). This script loads the real .wasm and calls every new export once.
//
// Usage: node crates/xfa-wasm/tests/browser_tools_node.mjs [pkg-dir]
//        (default crates/xfa-wasm/pkg, as built by
//         `wasm-pack build --release --target web crates/xfa-wasm`)

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "../../..");
const pkg = resolve(process.argv[2] ?? join(repo, "crates/xfa-wasm/pkg"));

const wasm = await import(pathToFileURL(join(pkg, "xfa_wasm.js")).href);
wasm.initSync({ module: readFileSync(join(pkg, "xfa_wasm_bg.wasm")) });
const { PdfDoc, encryptPdf, decryptPdf, extractImages, signPdf } = wasm;

const read = (p) => new Uint8Array(readFileSync(join(repo, p)));
const simple = read("tests/corpus-mini/simple.pdf");
const scanned = read("tests/corpus-mini/scanned.pdf");
const rsa = read("crates/pdf-sign/tests/fixtures/test-rsa.p12");
const p256 = read("crates/pdf-sign/tests/fixtures/test-ec-p256.p12");

let failures = 0;
function check(name, fn) {
  try {
    fn();
    console.log(`ok   ${name}`);
  } catch (e) {
    failures++;
    console.log(`FAIL ${name}: ${e?.stack ?? e}`);
  }
}
function assert(cond, msg) {
  if (!cond) throw new Error(msg);
}
function throwsCode(fn, code) {
  try {
    fn();
  } catch (e) {
    assert(e?.name === "PdfluentError", `expected a PdfluentError, got ${e}`);
    assert(e.code === code, `expected ${code}, got ${e.code}: ${e.message}`);
    return;
  }
  throw new Error(`expected a throw with ${code}`);
}
function pages(bytes) {
  const doc = PdfDoc.open(bytes);
  try {
    return doc.pageCount();
  } finally {
    doc.free();
  }
}

check("#614 encrypt, refuse a wrong password, decrypt, reopen", () => {
  const enc = encryptPdf(simple, "open-me", "", undefined);
  throwsCode(() => PdfDoc.open(enc), "E-PARSE-INVALID-PDF");
  throwsCode(() => decryptPdf(enc, "nope"), "E-SECURITY-DECRYPTION-FAILED");
  assert(pages(decryptPdf(enc, "open-me")) === 1, "decrypted file does not open");
});

check("#614 owner-only file: restrictions lift only for the owner password", () => {
  const enc = encryptPdf(simple, "", "the-owner", { print: false });
  throwsCode(() => decryptPdf(enc, ""), "E-SECURITY-DECRYPTION-FAILED");
  assert(pages(decryptPdf(enc, "the-owner")) === 1, "owner-decrypted file does not open");
});

check("#614 a mistyped permission is named", () => {
  throwsCode(() => encryptPdf(simple, "a", "b", { printing: false }), "E-WASM-INVALID-ARGUMENT");
});

check("#615 addWatermark with options, addTextWatermark kept", () => {
  const doc = PdfDoc.open(simple);
  try {
    const out = doc.addWatermark({
      preset: "draft",
      color: "#0000ff",
      tile: { hSpacing: 150 },
      position: { anchor: "bottomLeft", x: 20, y: 20 },
      pages: [0],
    });
    assert(pages(out) === 1, "watermarked file does not open");
    assert(pages(doc.addTextWatermark("OLD API", 0.4)) === 1, "compat alias broke");
    throwsCode(() => doc.addWatermark({}), "E-WASM-INVALID-ARGUMENT");
    throwsCode(() => doc.addWatermark({ text: "x", pages: [9] }), "E-WASM-PAGE-OUT-OF-RANGE");
  } finally {
    doc.free();
  }
});

check("#616 extractImages returns a real PNG and an empty skip list", () => {
  const { images, skipped } = extractImages(scanned);
  assert(Array.isArray(images) && Array.isArray(skipped), "shape");
  assert(images.length === 1 && skipped.length === 0, `got ${images.length}/${skipped.length}`);
  const img = images[0];
  assert(img.page === 0 && img.width === 10 && img.height === 10, JSON.stringify(img));
  assert(img.mime === "image/png" && img.data instanceof Uint8Array, "mime/data");
  assert(img.data[0] === 0x89 && img.data[1] === 0x50, "not a PNG signature");
});

for (const [name, p12] of [["RSA", rsa], ["P-256", p256]]) {
  check(`#617 signPdf with ${name} does not trap and verifies`, () => {
    const signed = signPdf(simple, p12, "test123", {
      reason: "Approved",
      location: "Amsterdam",
      visibleRect: { page: 0, x: 72, y: 72, width: 180, height: 50 },
    });
    const doc = PdfDoc.open(signed);
    try {
      const sigs = JSON.parse(doc.verifySignatures());
      assert(sigs.length === 1, `${sigs.length} signatures`);
      assert(sigs[0].structural_integrity === true, "structural integrity");
      assert(typeof sigs[0].signing_time === "string", "no /M signing time");
    } finally {
      doc.free();
    }
  });
}

check("#617 a wrong certificate password is a password error", () => {
  throwsCode(() => signPdf(simple, rsa, "wrong", undefined), "E-SECURITY-DECRYPTION-FAILED");
});

if (failures) {
  console.log(`${failures} failed`);
  process.exit(1);
}
console.log("all browser tools passed");
