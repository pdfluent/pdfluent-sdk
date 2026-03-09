import Foundation

/// Errors thrown by PDF operations.
public enum PdfError: Error, LocalizedError {
    case invalidArgument(String)
    case fileNotFound(String)
    case invalidPassword(String)
    case corruptPdf(String)
    case pageOutOfRange(String)
    case renderFailed(String)
    case unknown(String)

    public var errorDescription: String? {
        switch self {
        case .invalidArgument(let msg): return msg
        case .fileNotFound(let msg): return msg
        case .invalidPassword(let msg): return msg
        case .corruptPdf(let msg): return msg
        case .pageOutOfRange(let msg): return msg
        case .renderFailed(let msg): return msg
        case .unknown(let msg): return msg
        }
    }

    internal static func from(status: PdfStatus, fallback: String) -> PdfError {
        let nativeMsg = Self.lastErrorMessage() ?? fallback
        switch status {
        case PDF_STATUS_ERROR_INVALID_ARGUMENT:
            return .invalidArgument(nativeMsg)
        case PDF_STATUS_ERROR_FILE_NOT_FOUND:
            return .fileNotFound(nativeMsg)
        case PDF_STATUS_ERROR_INVALID_PASSWORD:
            return .invalidPassword(nativeMsg)
        case PDF_STATUS_ERROR_CORRUPT_PDF:
            return .corruptPdf(nativeMsg)
        case PDF_STATUS_ERROR_PAGE_RANGE:
            return .pageOutOfRange(nativeMsg)
        case PDF_STATUS_ERROR_RENDER:
            return .renderFailed(nativeMsg)
        default:
            return .unknown(nativeMsg)
        }
    }

    private static func lastErrorMessage() -> String? {
        guard let ptr = pdf_get_last_error() else { return nil }
        return String(cString: ptr)
    }
}
