namespace XfaPdf
{
    /// <summary>
    /// Status codes returned by native PDFluent operations.
    /// Mirrors the C ABI <c>PdfStatus</c> enum in <c>pdf_capi.h</c>.
    /// </summary>
    /// <remarks>
    /// Prefer catching typed <see cref="PdfluentException"/> subclasses rather
    /// than inspecting <see cref="PdfluentException.NativeStatus"/> directly.
    /// </remarks>
    public enum PdfStatus
    {
        /// <summary>Operation succeeded. No exception is thrown.</summary>
        Ok = 0,

        /// <summary>
        /// A required argument was null, empty, or outside a valid range.
        /// Maps to <see cref="PdfluentValidationException"/>.
        /// </summary>
        ErrorInvalidArgument = 1,

        /// <summary>
        /// The requested file was not found on disk.
        /// Maps to <see cref="PdfluentIoException"/>.
        /// </summary>
        ErrorFileNotFound = 2,

        /// <summary>
        /// The PDF is password-protected and no (or the wrong) password was supplied.
        /// Maps to <see cref="PdfluentPermissionException"/>.
        /// </summary>
        ErrorInvalidPassword = 3,

        /// <summary>
        /// The input bytes are not a valid PDF, or the PDF structure is corrupt.
        /// Maps to <see cref="PdfluentParseException"/>.
        /// </summary>
        ErrorCorruptPdf = 4,

        /// <summary>
        /// The requested page index is outside the range <c>[0, PageCount)</c>.
        /// Maps to <see cref="PdfluentPageRangeException"/>.
        /// </summary>
        ErrorPageRange = 5,

        /// <summary>
        /// The renderer could not rasterize the page.
        /// Maps to <see cref="PdfluentRenderException"/>.
        /// </summary>
        ErrorRender = 6,

        /// <summary>
        /// An unclassified error occurred. Maps to <see cref="PdfluentException"/>.
        /// </summary>
        ErrorUnknown = 99,
    }
}
