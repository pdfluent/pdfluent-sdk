import XCTest
@testable import XfaPdf

/// Tests for PdfDocument Swift bindings.
///
/// These tests require the native library to be built and linked.
/// Build the native library first:
///   cargo build -p pdf-capi --release
///
/// Then run tests:
///   swift test --package-path bindings/ios
final class PdfDocumentTests: XCTestCase {

    private static func createTestPdf() -> Data {
        let pdf = """
        %PDF-1.4
        1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj
        2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj
        3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj
        xref
        0 4
        0000000000 65535 f \r
        0000000009 00000 n \r
        0000000058 00000 n \r
        0000000115 00000 n \r
        trailer<</Size 4/Root 1 0 R>>
        startxref
        191
        %%EOF
        """
        return pdf.data(using: .ascii)!
    }

    func testOpenAndClose() throws {
        let doc = try PdfDocument(data: Self.createTestPdf())
        XCTAssertTrue(doc.isOpen)
        doc.close()
        XCTAssertFalse(doc.isOpen)
    }

    func testPageCount() throws {
        let doc = try PdfDocument(data: Self.createTestPdf())
        XCTAssertEqual(doc.pageCount, 1)
    }

    func testPageDimensions() throws {
        let doc = try PdfDocument(data: Self.createTestPdf())
        XCTAssertEqual(doc.pageWidth(0), 612.0, accuracy: 1.0)
        XCTAssertEqual(doc.pageHeight(0), 792.0, accuracy: 1.0)
    }

    func testPageRotation() throws {
        let doc = try PdfDocument(data: Self.createTestPdf())
        XCTAssertEqual(doc.pageRotation(0), 0)
    }

    func testMediaBox() throws {
        let doc = try PdfDocument(data: Self.createTestPdf())
        let box = try doc.mediaBox(0)
        XCTAssertEqual(box.x0, 0.0, accuracy: 1.0)
        XCTAssertEqual(box.y0, 0.0, accuracy: 1.0)
        XCTAssertEqual(box.x1, 612.0, accuracy: 1.0)
        XCTAssertEqual(box.y1, 792.0, accuracy: 1.0)
        XCTAssertEqual(box.width, 612.0, accuracy: 1.0)
        XCTAssertEqual(box.height, 792.0, accuracy: 1.0)
    }

    func testExtractTextFromEmptyPage() throws {
        let doc = try PdfDocument(data: Self.createTestPdf())
        let text = try doc.extractText(page: 0)
        XCTAssertNotNil(text)
    }

    func testInvalidPdfThrows() {
        XCTAssertThrowsError(try PdfDocument(data: Data([1, 2, 3]))) { error in
            XCTAssertTrue(error is PdfError)
        }
    }

    func testClosedDocumentThrows() throws {
        let doc = try PdfDocument(data: Self.createTestPdf())
        doc.close()
        XCTAssertThrowsError(try doc.extractText(page: 0))
    }

    func testMetadataReturnsNilForMissing() throws {
        let doc = try PdfDocument(data: Self.createTestPdf())
        XCTAssertNil(doc.metadata(forKey: "Title"))
    }

    func testBookmarkCount() throws {
        let doc = try PdfDocument(data: Self.createTestPdf())
        XCTAssertEqual(doc.bookmarkCount, 0)
    }

    func testRenderPage() throws {
        let doc = try PdfDocument(data: Self.createTestPdf())
        let img = try doc.renderPage(0, dpi: 72.0)
        XCTAssertGreaterThan(img.width, 0)
        XCTAssertGreaterThan(img.height, 0)
        XCTAssertEqual(img.pixels.count, img.width * img.height * 4)
    }

    func testRenderThumbnail() throws {
        let doc = try PdfDocument(data: Self.createTestPdf())
        let img = try doc.renderThumbnail(0, maxDimension: 100)
        XCTAssertLessThanOrEqual(img.width, 100)
        XCTAssertLessThanOrEqual(img.height, 100)
    }

    func testCGImageConversion() throws {
        let doc = try PdfDocument(data: Self.createTestPdf())
        let img = try doc.renderPage(0, dpi: 72.0)
        let cgImage = img.toCGImage()
        XCTAssertNotNil(cgImage)
        XCTAssertEqual(cgImage?.width, img.width)
        XCTAssertEqual(cgImage?.height, img.height)
    }

    // MARK: - Scenario 5: Read AcroForm fields

    // TODO: AcroForm field reading not yet exposed in Swift binding
    // func testReadFormFields() throws {
    //     let doc = try PdfDocument(data: loadFixture("acroform.pdf"))
    //     let fields = doc.formFields()
    //     XCTAssertFalse(fields.isEmpty)
    // }

    // MARK: - Scenario 6: Fill text field, save

    // TODO: Form field writing not yet exposed in Swift binding
    // func testWriteFormField() throws {
    //     let doc = try PdfDocument(data: loadFixture("acroform.pdf"))
    //     doc.setFieldValue("name", value: "test")
    //     let saved = doc.save()
    //     let doc2 = try PdfDocument(data: saved)
    //     XCTAssertEqual(doc2.fieldValue("name"), "test")
    // }

    // MARK: - Scenario 7: Read annotations

    // TODO: Annotation reading not yet exposed in Swift binding
    // func testReadAnnotations() throws {
    //     let doc = try PdfDocument(data: Self.createTestPdf())
    //     let annots = try doc.annotations(page: 0)
    //     XCTAssertNotNil(annots)
    // }

    // MARK: - Scenario 8: Add highlight, save

    // TODO: Annotation creation not yet exposed in Swift binding
    // func testAddHighlightAnnotation() throws { }

    // MARK: - Scenario 9: Validate PDF/A

    // TODO: PDF/A validation not yet exposed in Swift binding
    // func testValidatePdfA() throws {
    //     let doc = try PdfDocument(data: Self.createTestPdf())
    //     let report = try doc.validatePdfA(level: "2b")
    //     XCTAssertNotNil(report)
    // }

    // MARK: - Scenario 10: Merge 2 PDFs

    // TODO: PDF merge not yet exposed in Swift binding
    // func testMergePdfs() throws { }

    // MARK: - Scenario 11: Verify signature

    // TODO: Signature verification not yet exposed in Swift binding
    // func testVerifySignature() throws {
    //     let doc = try PdfDocument(data: loadFixture("signed.pdf"))
    //     let sigs = doc.verifySignatures()
    //     XCTAssertFalse(sigs.isEmpty)
    // }

    // MARK: - Scenario 12: Extract images

    // TODO: Image extraction not yet exposed in Swift binding
    // func testExtractImages() throws {
    //     let doc = try PdfDocument(data: Self.createTestPdf())
    //     let images = try doc.extractImages(page: 0)
    //     XCTAssertNotNil(images)
    // }
}
