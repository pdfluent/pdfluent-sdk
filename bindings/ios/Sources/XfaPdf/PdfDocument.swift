import Foundation

/// A PDF document backed by the native XFA PDF engine.
///
/// Usage:
/// ```swift
/// let doc = try PdfDocument(data: pdfData)
/// print("Pages: \(doc.pageCount)")
/// let text = try doc.extractText(page: 0)
/// let image = try doc.renderPage(0, dpi: 150)
/// ```
///
/// The document is automatically closed when deallocated.
public final class PdfDocument {
    private var handle: OpaquePointer?

    // MARK: - Initialization

    /// Open a PDF from raw data.
    ///
    /// - Parameter data: PDF file contents.
    /// - Throws: `PdfError` if the data cannot be parsed.
    public init(data: Data) throws {
        var ptr: OpaquePointer?
        let status = data.withUnsafeBytes { (buffer: UnsafeRawBufferPointer) -> PdfStatus in
            guard let base = buffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return PDF_STATUS_ERROR_INVALID_ARGUMENT
            }
            return pdf_document_open_from_bytes(base, buffer.count, &ptr)
        }
        guard status == PDF_STATUS_OK, let validPtr = ptr else {
            throw PdfError.from(status: status, fallback: "failed to open PDF from data")
        }
        self.handle = validPtr
    }

    /// Open a PDF from a file URL.
    ///
    /// - Parameters:
    ///   - url: File URL to the PDF.
    ///   - password: Optional document password.
    /// - Throws: `PdfError` if the file cannot be read or parsed.
    public convenience init(url: URL, password: String? = nil) throws {
        guard url.isFileURL else {
            throw PdfError.invalidArgument("only file URLs are supported")
        }
        let data = try Data(contentsOf: url)
        try self.init(data: data)
    }

    /// Open a PDF from a file path.
    ///
    /// - Parameters:
    ///   - path: Path to the PDF file.
    ///   - password: Optional document password.
    /// - Throws: `PdfError` if the file cannot be read or parsed.
    public convenience init(path: String, password: String? = nil) throws {
        var ptr: OpaquePointer?
        let status: PdfStatus
        if let pw = password {
            status = pdf_document_open(path, pw, &ptr)
        } else {
            status = pdf_document_open(path, nil, &ptr)
        }
        guard status == PDF_STATUS_OK, let validPtr = ptr else {
            throw PdfError.from(status: status, fallback: "failed to open '\(path)'")
        }
        self.handle = validPtr
    }

    // Private init for handle-based construction
    private init(handle: OpaquePointer) {
        self.handle = handle
    }

    deinit {
        close()
    }

    // MARK: - Lifecycle

    /// Close the document and free native resources.
    public func close() {
        if let h = handle {
            pdf_document_free(h)
            handle = nil
        }
    }

    /// Whether the document is still open.
    public var isOpen: Bool { handle != nil }

    // MARK: - Document queries

    /// Number of pages in the document.
    public var pageCount: Int {
        guard let h = handle else { return 0 }
        return Int(pdf_document_page_count(h))
    }

    /// Width of a page in PDF points (1/72 inch).
    public func pageWidth(_ pageIndex: Int) -> Double {
        guard let h = handle else { return 0 }
        return pdf_page_width(h, Int32(pageIndex))
    }

    /// Height of a page in PDF points (1/72 inch).
    public func pageHeight(_ pageIndex: Int) -> Double {
        guard let h = handle else { return 0 }
        return pdf_page_height(h, Int32(pageIndex))
    }

    /// Rotation of a page in degrees (0, 90, 180, 270).
    public func pageRotation(_ pageIndex: Int) -> Int {
        guard let h = handle else { return 0 }
        return Int(pdf_page_rotation(h, Int32(pageIndex)))
    }

    /// Get the MediaBox of a page.
    public func mediaBox(_ pageIndex: Int) throws -> PageBox {
        let h = try ensureOpen()
        var x0: Double = 0, y0: Double = 0, x1: Double = 0, y1: Double = 0
        let status = pdf_page_media_box(h, Int32(pageIndex), &x0, &y0, &x1, &y1)
        guard status == PDF_STATUS_OK else {
            throw PdfError.from(status: status, fallback: "failed to get media box for page \(pageIndex)")
        }
        return PageBox(x0: x0, y0: y0, x1: x1, y1: y1)
    }

    /// Get the CropBox of a page.
    public func cropBox(_ pageIndex: Int) throws -> PageBox {
        let h = try ensureOpen()
        var x0: Double = 0, y0: Double = 0, x1: Double = 0, y1: Double = 0
        let status = pdf_page_crop_box(h, Int32(pageIndex), &x0, &y0, &x1, &y1)
        guard status == PDF_STATUS_OK else {
            throw PdfError.from(status: status, fallback: "failed to get crop box for page \(pageIndex)")
        }
        return PageBox(x0: x0, y0: y0, x1: x1, y1: y1)
    }

    // MARK: - Text extraction

    /// Extract text from a page.
    ///
    /// - Parameter pageIndex: Zero-based page index.
    /// - Returns: The extracted text, or empty string if no text.
    /// - Throws: `PdfError.pageOutOfRange` if the index is invalid.
    public func extractText(page pageIndex: Int) throws -> String {
        let h = try ensureOpen()
        guard let ptr = pdf_page_extract_text(h, Int32(pageIndex)) else {
            // Check if there was an actual error or just no text
            if let errPtr = pdf_get_last_error() {
                let msg = String(cString: errPtr)
                throw PdfError.pageOutOfRange(msg)
            }
            return ""
        }
        defer { pdf_string_free(ptr) }
        return String(cString: ptr)
    }

    // MARK: - Rendering

    /// Render a page to RGBA pixels at the specified DPI.
    ///
    /// - Parameters:
    ///   - pageIndex: Zero-based page index.
    ///   - dpi: Dots per inch (72 = 1:1, 150 = standard, 300 = high quality).
    /// - Returns: Rendered image with RGBA pixel data.
    public func renderPage(_ pageIndex: Int, dpi: Double) throws -> RenderedImage {
        let h = try ensureOpen()
        var width: UInt32 = 0, height: UInt32 = 0
        var pixels: UnsafeMutablePointer<UInt8>?
        let status = pdf_page_render(h, Int32(pageIndex), dpi, &width, &height, &pixels)
        guard status == PDF_STATUS_OK, let px = pixels else {
            throw PdfError.from(status: status, fallback: "failed to render page \(pageIndex)")
        }
        let len = Int(width) * Int(height) * 4
        let data = Data(bytes: px, count: len)
        pdf_pixels_free(px, len)
        return RenderedImage(width: Int(width), height: Int(height), pixels: data)
    }

    /// Render a thumbnail constrained to a maximum dimension.
    ///
    /// - Parameters:
    ///   - pageIndex: Zero-based page index.
    ///   - maxDimension: Maximum width or height in pixels.
    public func renderThumbnail(_ pageIndex: Int, maxDimension: Int) throws -> RenderedImage {
        let h = try ensureOpen()
        var width: UInt32 = 0, height: UInt32 = 0
        var pixels: UnsafeMutablePointer<UInt8>?
        let status = pdf_page_render_thumbnail(h, Int32(pageIndex), UInt32(maxDimension), &width, &height, &pixels)
        guard status == PDF_STATUS_OK, let px = pixels else {
            throw PdfError.from(status: status, fallback: "failed to render thumbnail for page \(pageIndex)")
        }
        let len = Int(width) * Int(height) * 4
        let data = Data(bytes: px, count: len)
        pdf_pixels_free(px, len)
        return RenderedImage(width: Int(width), height: Int(height), pixels: data)
    }

    // MARK: - Metadata

    /// Get a metadata value.
    ///
    /// - Parameter key: One of: "Title", "Author", "Subject", "Keywords", "Creator", "Producer".
    /// - Returns: The metadata value, or nil if not set.
    public func metadata(forKey key: String) -> String? {
        guard let h = handle else { return nil }
        guard let ptr = pdf_document_get_meta(h, key) else { return nil }
        defer { pdf_string_free(ptr) }
        return String(cString: ptr)
    }

    /// Number of top-level bookmarks.
    public var bookmarkCount: Int {
        guard let h = handle else { return 0 }
        return Int(pdf_bookmark_count(h))
    }

    // MARK: - Private helpers

    @discardableResult
    private func ensureOpen() throws -> OpaquePointer {
        guard let h = handle else {
            throw PdfError.invalidArgument("PdfDocument is closed")
        }
        return h
    }
}
