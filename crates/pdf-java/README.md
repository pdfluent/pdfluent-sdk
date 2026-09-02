# pdf-java — XFA Java SDK

> **Deprecated / non-publishable.** This crate's `com.xfa.pdf.*` Java classes
> are legacy. The canonical, published Java binding is
> **`com.pdfluent:pdfluent`** under [`bindings/java/`](../../bindings/java/)
> (the JNI symbols built here are `Java_com_pdfluent_PdfluentDocument_*`, which
> only the `com.pdfluent.PdfluentDocument` class can bind). Use the canonical
> binding for AcroForm fill (`getFormFields` / `setFormField` /
> `setMultiSelect`) and all other features.

Java JNI bindings for the XFA PDF engine.  Provides the same feature set as
the Python and Node.js SDKs.

## Requirements

| Tool      | Version      |
|-----------|--------------|
| Rust      | stable ≥ 1.80 |
| JDK       | 11+           |
| `javac`   | any JDK       |

## Build the native library

```bash
cd crates/pdf-java
cargo build --release -p pdf-java
```

The output is placed in `../../target/release/`:

| Platform  | File                   |
|-----------|------------------------|
| macOS     | `libpdfluent_java.dylib` |
| Linux     | `libpdfluent_java.so`    |
| Windows   | `pdfluent_java.dll`      |

The Java classes call `System.loadLibrary("pdfluent_java")`, so the name has to
match `[lib] name` in `Cargo.toml`. CI runs the JUnit suite against a debug build
(`.github/workflows/java-bindings.yml`); locally:

```bash
cargo build -p pdf-java
mvn -f crates/pdf-java/pom.xml -Dnative.lib.dir="$PWD/target/debug" test
```

## Compile the Java wrapper classes

From the repository root:

```bash
javac -d out/ crates/pdf-java/java/com/xfa/pdf/*.java
```

## Run the demo

```bash
# macOS / Linux
javac -cp crates/pdf-java/java/ crates/pdf-java/examples/Demo.java -d out/
java  -cp "crates/pdf-java/java/:out/" \
      -Djava.library.path=target/release \
      Demo /path/to/sample.pdf

# Windows (PowerShell)
javac -cp crates/pdf-java/java/ crates/pdf-java/examples/Demo.java -d out/
java  -cp "crates/pdf-java/java/;out/" `
      -Djava.library.path=target/release `
      Demo C:\path\to\sample.pdf
```

## API overview

### `PdfDocument`

All instance methods throw `PdfException` on failure.  Implements
`AutoCloseable` — always use **try-with-resources**.

#### Round 1 — basics

| Method | Description |
|--------|-------------|
| `PdfDocument.open(path)` | Open from file path |
| `PdfDocument.open(bytes)` | Open from byte array |
| `PdfDocument.openWithPassword(path, password)` | Open encrypted PDF |
| `doc.getPageCount()` | Total number of pages |
| `doc.getPageWidth(n)` | Width of page n in points |
| `doc.getPageHeight(n)` | Height of page n in points |
| `doc.getPageRotation(n)` | Rotation of page n (0/90/180/270°) |
| `doc.extractText(n)` | Extract text from page n |
| `doc.searchText(query)` | Pages containing the query (0-based indices) |
| `doc.renderPage(n, dpi)` | Render to RGBA pixels; parse with `parsePixelBuffer()` |
| `doc.renderThumbnail(n, maxPx)` | Render thumbnail |
| `doc.getMetadata(key)` | Title, Author, Subject, Keywords, Creator, Producer |
| `doc.getBookmarkCount()` | Number of top-level outline entries |
| `doc.save(path)` | Save (mutated state or original bytes) |

#### Round 2 — forms, annotations, redaction, encryption

| Method | Description |
|--------|-------------|
| `doc.getFormFields()` | All AcroForm fields → `List<FormField>` |
| `doc.setFormField(name, value)` | Fill a field (text, checkbox, radio, or choice); updates `/V`, `/AS`, and `/AP` |
| `doc.getAnnotations(page)` | All annotations on page n → `List<Annotation>` |

`setFormField` supports hierarchical names (`"parent.child"`) resolved through
`/Kids` recursion.  The call keeps `/V`, `/AS`, and `/AP` consistent so the
fill is visible in all viewers without `/NeedAppearances` processing.
Read-only fields throw `PdfException`.  For checkbox and radio fields pass the
on-state export name (e.g. `"On"`, `"Yes"`, `"NL"`); for choice fields pass
the option value.
| `doc.addAnnotation(page, type, x0, y0, x1, y1, content)` | Add `"highlight"` or `"freetext"` |
| `doc.redactText(page, term)` | Redact all occurrences; `page=-1` → all pages |
| `doc.encrypt(outputPath, password)` | Save RC4-128 encrypted copy |
| `doc.decrypt(outputPath, password)` | Save unencrypted copy |

### `PdfUtils`

Static helpers that do not require an open document.

| Method | Description |
|--------|-------------|
| `PdfUtils.mergePdfs(paths[], outputPath)` | Merge PDFs in order |
| `PdfUtils.validatePdfA(path, level)` | Validate against PDF/A level → `ComplianceReport` |

Supported level strings: `"1b"`, `"2b"` (default), `"3b"`, `"1a"`, `"2a"`,
`"3a"`, `"2u"`, `"3u"`, `"4"`.

## Data classes

| Class | Fields |
|-------|--------|
| `FormField` | `name`, `fieldType`, `value`, `page` |
| `Annotation` | `annotationType`, `page`, `x0/y0/x1/y1`, `contents`, `author` |
| `ComplianceReport` | `compliant`, `errorCount`, `warningCount`, `issues` |
| `ComplianceIssue` | `rule`, `severity`, `message` |
| `RedactReport` | `matchesFound`, `areasRedacted`, `pagesAffected` |

## Error handling

All errors are surfaced as `PdfException` (checked exception, extends
`Exception`).  A missing or invalid PDF/A `level` string falls back to `"2b"`.

## Render pixel buffer format

`renderPage()` and `renderThumbnail()` return a byte array with an 8-byte header:

```
[width : 4 bytes big-endian int32]
[height: 4 bytes big-endian int32]
[RGBA pixels…]
```

Use `PdfDocument.parsePixelBuffer(buf)` to extract `{width, height}`.

## Not supported

- Rendering bindings (render works, but no image-format conversion layer)
- Digital signing
- Maven / Gradle publish pipeline
- Android / Dalvik VM
