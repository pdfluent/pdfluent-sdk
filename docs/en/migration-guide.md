# Migration Guide

Practical side-by-side API translations for teams moving from iText 8 or PDFBox 3 to the XFA SDK.

---

## Migrating from iText to XFA SDK

### Opening a document

**iText (Java)**
```java
PdfReader reader = new PdfReader("invoice.pdf");
PdfDocument pdf = new PdfDocument(reader);
System.out.println(pdf.getNumberOfPages());
```

**XFA SDK (Rust)**
```rust
use pdf_engine::PdfDocument;

let data = std::fs::read("invoice.pdf")?;
let doc = PdfDocument::open(data)?;
println!("{}", doc.page_count());
```

---

### Reading document metadata

**iText (Java)**
```java
PdfDocumentInfo info = pdf.getDocumentInfo();
String title = info.getTitle();
String author = info.getAuthor();
```

**XFA SDK (Rust)**
```rust
let info = doc.info();
println!("{:?}", info.title);
println!("{:?}", info.author);
```

---

### Text extraction

**iText (Java)**
```java
PdfTextExtractor extractor = new PdfTextExtractor(pdf);
for (int i = 1; i <= pdf.getNumberOfPages(); i++) {
    String text = extractor.getTextFromPage(i);
    System.out.println(text);
}
```

**XFA SDK (Rust)**
```rust
for i in 0..doc.page_count() {
    let text = doc.extract_text(i)?;
    println!("{text}");
}

// Structured text with per-span positions
for block in doc.extract_text_blocks(0)? {
    for span in &block.spans {
        println!("[{:.0}, {:.0}] {}", span.x, span.y, span.text);
    }
}

// Full-text search — returns 0-based page indices
let hits = doc.search_text("quarterly revenue");
```

---

### PDF/A validation

**iText (Java)**
```java
PdfADocument pdfA = new PdfADocument(
    new PdfReader("document.pdf"),
    PdfAConformance.PDF_A_2B
);
ValidationResult result = pdfA.checkReadingMode(CheckReadingMode.ON_DEMAND);
```

**XFA SDK (Rust)**
```rust
use pdf_syntax::Pdf;
use pdf_compliance::{validate_pdfa, detect_pdfa_level, PdfALevel};

let data = std::fs::read("document.pdf")?;
let pdf = Pdf::new(data)?;

let level = detect_pdfa_level(&pdf).unwrap_or(PdfALevel::A2b);
let report = validate_pdfa(&pdf, level);

if report.compliant {
    println!("PDF/A-{}{}", level.part(), level.conformance());
} else {
    for issue in &report.issues {
        println!("[{}] {}", issue.rule, issue.message);
    }
}
```

---

### Digital signing

**iText (Java)**
```java
PdfSigner signer = new PdfSigner(
    new PdfReader("input.pdf"),
    new FileOutputStream("signed.pdf"),
    new StampingProperties()
);
IExternalSignature signature = new PrivateKeySignature(privateKey, "SHA-256", "BC");
signer.signDetached(digest, signature, chain, null, null, null, 0,
    PdfSigner.CryptoStandard.CADES);
```

**XFA SDK (Rust)**
```rust
use pdf_sign::{Pkcs12Signer, sign_pdf, SignOptions};

let p12_data = std::fs::read("identity.p12")?;
let signer = Pkcs12Signer::from_pkcs12(&p12_data, "password")?;

let pdf_bytes = std::fs::read("input.pdf")?;
let opts = SignOptions {
    reason: Some("Approved".into()),
    location: Some("Amsterdam".into()),
    ..Default::default()
};
let signed = sign_pdf(&signer, &pdf_bytes, opts)?;
std::fs::write("signed.pdf", signed)?;
```

---

### Signature validation

**iText (Java)**
```java
SignatureUtil signatureUtil = new SignatureUtil(pdfDocument);
List<String> names = signatureUtil.getSignatureNames();
for (String name : names) {
    PdfPKCS7 pkcs7 = signatureUtil.readSignatureData(name);
    System.out.println(pkcs7.verifySignatureIntegrityAndAuthenticity());
}
```

**XFA SDK (Rust)**
```rust
use pdf_sign::validate_signatures;
use pdf_syntax::Pdf;

let pdf = Pdf::new(std::fs::read("signed.pdf")?)?;
for result in validate_signatures(&pdf) {
    println!("{}: {:?}", result.field_name, result.status);
    if let Some(signer) = &result.signer {
        println!("  Signed by: {}", signer.subject);
    }
}
```

---

### Reading form fields

**iText (Java)**
```java
PdfAcroForm form = PdfAcroForm.getAcroForm(pdfDocument, false);
Map<String, PdfFormField> fields = form.getAllFormFields();
for (Map.Entry<String, PdfFormField> entry : fields.entrySet()) {
    System.out.println(entry.getKey() + " = " + entry.getValue().getValueAsString());
}
```

**XFA SDK (Rust)**
```rust
use pdf_syntax::Pdf;
use pdf_forms::{parse_acroform, FieldValue};

let pdf = Pdf::new(std::fs::read("form.pdf")?)?;
let tree = parse_acroform(&pdf).expect("no AcroForm");

for id in tree.terminal_fields() {
    let name = tree.fully_qualified_name(id);
    let value = tree.effective_value(id);
    println!("{name} = {value:?}");
}
```

---

### Redaction

**iText (Java)**
```java
PdfCleanUpTool cleaner = new PdfCleanUpTool(pdfDocument);
List<PdfCleanUpLocation> areas = List.of(
    new PdfCleanUpLocation(1, new Rectangle(100, 700, 200, 20))
);
cleaner.cleanUp(areas);
```

**XFA SDK (Rust)**
```rust
use pdf_redact::{search_and_redact, RedactSearchOptions};

let data = std::fs::read("document.pdf")?;
let mut doc = lopdf::Document::load_mem(&data)?;

let opts = RedactSearchOptions::exact("John Doe");
let report = search_and_redact(&mut doc, &opts)?;
println!("Redacted {} occurrences", report.total_redacted);
doc.save_to("redacted.pdf")?;
```

---

### Rendering a page

**iText (Java)**
```java
// iText does not provide rendering — requires iText pdfRender add-on
```

**XFA SDK (Rust)**
```rust
use pdf_engine::{PdfDocument, RenderOptions};

let doc = PdfDocument::open(std::fs::read("brochure.pdf")?)?;
let opts = RenderOptions { dpi: 150.0, ..Default::default() };
let rendered = doc.render_page(0, &opts)?;
// rendered.pixels = RGBA bytes, rendered.width / rendered.height
```

---

## Migrating from PDFBox to XFA SDK

### Opening a document

**PDFBox (Java)**
```java
PDDocument document = PDDocument.load(new File("invoice.pdf"));
System.out.println(document.getNumberOfPages());
document.close();
```

**XFA SDK (Rust)**
```rust
use pdf_engine::PdfDocument;

let doc = PdfDocument::open(std::fs::read("invoice.pdf")?)?;
println!("{}", doc.page_count());
// No explicit close — dropped at end of scope
```

---

### Text extraction

**PDFBox (Java)**
```java
PDFTextStripper stripper = new PDFTextStripper();
for (int i = 1; i <= document.getNumberOfPages(); i++) {
    stripper.setStartPage(i);
    stripper.setEndPage(i);
    System.out.println(stripper.getText(document));
}
```

**XFA SDK (Rust)**
```rust
for i in 0..doc.page_count() {
    let text = doc.extract_text(i)?;
    println!("{text}");
}
```

---

### Structured text with positions

**PDFBox (Java)**
```java
PDFTextStripperByArea stripper = new PDFTextStripperByArea();
stripper.addRegion("header", new Rectangle(0, 0, 600, 100));
stripper.extractRegions(page);
String headerText = stripper.getTextForRegion("header");
```

**XFA SDK (Rust)**
```rust
for block in doc.extract_text_blocks(0)? {
    for span in &block.spans {
        // Filter by bounding-box position
        if span.y < 100.0 {
            println!("Header: {}", span.text);
        }
    }
}
```

---

### Document metadata

**PDFBox (Java)**
```java
PDDocumentInformation info = document.getDocumentInformation();
System.out.println(info.getTitle());
System.out.println(info.getAuthor());
System.out.println(info.getCreationDate());
```

**XFA SDK (Rust)**
```rust
let info = doc.info();
println!("{:?}", info.title);
println!("{:?}", info.author);
// Creation date is in the XMP stream — use pdf_compliance::repair_xmp to access
```

---

### PDF/A validation

**PDFBox (Java)**
```java
// PDFBox does not have built-in PDF/A validation
// Requires external library (e.g. preflight module, deprecated)
```

**XFA SDK (Rust)**
```rust
use pdf_syntax::Pdf;
use pdf_compliance::{validate_pdfa, PdfALevel};

let pdf = Pdf::new(std::fs::read("document.pdf")?)?;
let report = validate_pdfa(&pdf, PdfALevel::A2b);
println!("Compliant: {}", report.compliant);
println!("Errors: {}", report.error_count());
```

---

### Reading form fields

**PDFBox (Java)**
```java
PDDocumentCatalog catalog = document.getDocumentCatalog();
PDAcroForm acroForm = catalog.getAcroForm();
for (PDField field : acroForm.getFieldTree()) {
    System.out.println(field.getFullyQualifiedName() + " = " + field.getValueAsString());
}
```

**XFA SDK (Rust)**
```rust
use pdf_syntax::Pdf;
use pdf_forms::parse_acroform;

let pdf = Pdf::new(std::fs::read("form.pdf")?)?;
let tree = parse_acroform(&pdf).expect("no AcroForm");
for id in tree.terminal_fields() {
    println!("{} = {:?}",
        tree.fully_qualified_name(id),
        tree.effective_value(id));
}
```

---

### Merging PDFs

**PDFBox (Java)**
```java
PDFMergerUtility merger = new PDFMergerUtility();
merger.setDestinationFileName("merged.pdf");
merger.addSource("a.pdf");
merger.addSource("b.pdf");
merger.mergeDocuments(MemoryUsageSetting.setupMainMemoryOnly());
```

**XFA SDK (Rust)**
```rust
use pdf_manip::pages::merge;

let merged = merge(&["a.pdf", "b.pdf"])?;
merged.save_to("merged.pdf")?;
```

---

### Splitting a PDF

**PDFBox (Java)**
```java
Splitter splitter = new Splitter();
List<PDDocument> pages = splitter.split(document);
for (int i = 0; i < pages.size(); i++) {
    pages.get(i).save("page_" + i + ".pdf");
    pages.get(i).close();
}
```

**XFA SDK (Rust)**
```rust
use pdf_manip::pages::split_per_page;

let data = std::fs::read("document.pdf")?;
let source = lopdf::Document::load_mem(&data)?;
let pages = split_per_page(&source)?;
for (i, page) in pages.iter().enumerate() {
    page.save_to(format!("page_{i}.pdf"))?;
}
```

---

### Page rendering

**PDFBox (Java)**
```java
PDFRenderer renderer = new PDFRenderer(document);
BufferedImage image = renderer.renderImageWithDPI(0, 150);
ImageIO.write(image, "PNG", new File("page1.png"));
```

**XFA SDK (Rust)**
```rust
use pdf_engine::{PdfDocument, RenderOptions};

let doc = PdfDocument::open(std::fs::read("document.pdf")?)?;
let opts = RenderOptions { dpi: 150.0, ..Default::default() };
let rendered = doc.render_page(0, &opts)?;
// rendered.pixels = raw RGBA bytes, encode with the `png` crate
```

---

### Signing a PDF

**PDFBox (Java)**
```java
CreateSignature signing = new CreateSignature(keystore, password.toCharArray());
signing.signDetached(new File("input.pdf"), new File("signed.pdf"));
```

**XFA SDK (Rust)**
```rust
use pdf_sign::{Pkcs12Signer, sign_pdf, SignOptions};

let signer = Pkcs12Signer::from_pkcs12(&std::fs::read("key.p12")?, "password")?;
let signed = sign_pdf(&signer, &std::fs::read("input.pdf")?, SignOptions::default())?;
std::fs::write("signed.pdf", signed)?;
```

---

### Key differences

| Aspect | iText 8 / PDFBox | XFA SDK |
|--------|-----------------|---------|
| Language | Java (JVM) | Rust |
| Memory management | GC | Ownership / zero-copy |
| Licensing | AGPL / Apache 2 | Commercial |
| WASM deployment | Not possible | Native (970 KB) |
| PDF/A validation | iText only (add-on) | Built-in |
| XFA support | iText only (AGPL) | Built-in |
| Rendering | External (iText add-on) / PDFBox renderer | Pure Rust |
| Thread safety | Synchronized objects | `Send + Sync` throughout |
