# Image extraction: the agreed public route per binding (1.1.0, #643)

Two surfaces, one implementation: the shared image core in
`pdfluent::compat::pixmap` (`page_image_facts_in`/`page_images_in`,
`extract_image_in`, `pixmap_from_image_in`) sits underneath both the
compat route and the plain `PdfDocument` route. No new error variants
anywhere: bad pages give the existing page-range error, unknown image
ids the core's bad-reference error.

## Compat route (r3)

One route per binding, every one tested on the same construction fixture
(fixtures/known-images.pdf + known-images.expected.json: count,
dimensions, colour space, components, bpc, decoded-pixel sha256).

| binding | list the page's images | get one image | layer underneath | test (r4) |
|---|---|---|---|---|
| Rust compat facade | `pdfluent::compat::Document::get_page_images(page, full)` | `Document::extract_image(xref) -> ImageData` | compat/pixmap.rs (`page_images_in`, `extract_image_in`) | crates/pdfluent tests (to add in r4: same fixture, Rust) |
| Python | `pdfluent.open(path)` → `Document.get_page_images(page)` (or `Page.get_images()`) | `Document.extract_image(xref)` | pdf-python compat door → Rust compat facade | crates/pdf-python/tests/test_known_images.py |
| Node | `new Document(path).getPageImages(page, full?)` | `extractImage(xref)` | napi compat exports → Rust compat facade | crates/pdf-node/tests/known_images.test.js |
| Java | `new Document(path).getPageImages(page[, full])` | `extractImage(xref)` | C-ABI `pdf_compat_document_get_page_images` / `pdf_compat_document_extract_image` | bindings/java/.../KnownImagesTest.java |
| .NET | `new Document(path).GetPageImages(page)` | `ExtractImage(xref)` | same C-ABI functions via P/Invoke | PDFluent.Tests `ExtractImages` |
| C-ABI | `pdf_compat_document_get_page_images` | `pdf_compat_document_extract_image` (+ legacy `pdf_page_extract_image`) | Rust compat facade | crates/pdf-capi/tests (to add in r4) |

## Plain `PdfDocument` route (r4)

Added in r4 (t5/r3-pdfdoc-images): the non-compat Rust facade
`pdfluent::PdfDocument` and its projections. `extract_image` was already
taken on `PdfDocument` by the compat lane, so the plain verbs are
`page_images` / `extract_image_data` / `decode_image_pixels` in each
binding's casing. Pages are 1-based in Rust, 0-based in every binding
(the boundary adds the one).

| Binding | Listing | Extract (bytes + header) | Decode (pixels) |
|---|---|---|---|
| Rust `pdfluent::PdfDocument` | `page_images(page: usize) -> Result<Vec<ImageInfo>>` | `extract_image_data(id: ImageId) -> Result<ExtractedImage>` | `decode_image_pixels(id: ImageId) -> Result<DecodedImage>` |
| Python `pdfluent.Document` | `page_images(page) -> list[dict]` | `extract_image_data(image_id) -> dict` | `decode_image_pixels(image_id) -> dict` |
| Node `PdfDocument` | `pageImages(index) -> PageImageInfo[]` | `extractImageData(imageId) -> ExtractedImageInfo` | `decodeImagePixels(imageId) -> DecodedImageInfo` |
| C ABI | `pdf_page_images_json(doc, page_index) -> char*` (JSON array, `pdf_string_free`) | `pdf_image_extract_data(doc, id, out PdfExtractedImage*)` + `pdf_extracted_image_free` | `pdf_image_decode_pixels(doc, id, out PdfDecodedImage*)` + `pdf_decoded_image_free` |
| Java `PdfluentDocument` | `pageImages(int) -> List<ImageInfo>` (JNI `nativePageImages`, flat `String[]` stride 10) | `extractImageData(int) -> ExtractedImage` (JNI `nativeExtractImageData`, framed `byte[]`) | `decodeImagePixels(int) -> DecodedImage` (JNI `nativeDecodeImagePixels`, framed `byte[]`) |
| .NET `PdfDocument` | `PageImages(int) -> string` (JSON array; no-deps rule, same as `ExtractBlocks`) | `ExtractImageData(uint) -> ExtractedImage` (P/Invoke struct) | `DecodeImagePixels(uint) -> DecodedImage` (P/Invoke struct) |

Rust types (`pdfluent::images`, re-exported at crate root + prelude):
`ImageId(u32)`, `ImageColorSpace { Gray, Rgb, Cmyk }`, `ImageInfo`,
`ExtractedImage` (`encoding` is `"jpeg"` passthrough else re-encoded
`"png"`), `DecodedImage`. `ImageInfo`/`ExtractedImage`/`DecodedImage` are
`#[non_exhaustive]` read-only views, following `AnnotationInfo`.

Decisions worth recording:

- The plain listing needs component counts, which pinned `ImageRef` has
  no room for, so the core gained an additive `PageImageFacts { image,
  comps }` + `page_image_facts_in` (same walk as `page_images_in`,
  untouched).
- Colour-space classification from component counts (1 grey / 4 CMYK /
  else RGB) mirrors the decode core's own rule.
- The older C pair `pdf_page_image_count`/`pdf_page_extract_image`
  (`pdf_extract` implementation) is left as is; the new C functions are
  the facade-routed ones.
- Java's `ImageRef`/`ImageData` names were taken by compat types, hence
  `ImageInfo`/`ExtractedImage`/`DecodedImage`.

## Test coverage

The compat-route known-images tests listed above run on the shared
construction fixture. The plain-route known-images tests (same fixture
through the plain verbs) follow in a separate test-role commit; the
plain route currently ships the per-binding functional suites from
t5/r3-pdfdoc-images (operator-built fixtures, not the shared one).
