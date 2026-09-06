namespace PDFluent
{
    /// <summary>
    /// Status codes returned by native PDFluent operations.
    /// Mirrors the C ABI <c>PdfStatus</c> enum in <c>pdf_capi.h</c>.
    /// </summary>
    /// <remarks>
    /// <para>
    /// Integer values are part of the stable C ABI contract — see
    /// <c>docs/c_abi_stability.md</c> §3 — do not renumber.
    /// </para>
    /// <para>
    /// Prefer catching typed <see cref="PdfluentException"/> subclasses rather
    /// than inspecting <see cref="PdfluentException.NativeStatus"/> directly.
    /// </para>
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
        /// A PDF/A conversion step failed.
        /// Maps to <see cref="PdfluentException"/>.
        /// </summary>
        ErrorConvert = 7,

        /// <summary>
        /// Text or image redaction failed.
        /// Maps to <see cref="PdfluentException"/>.
        /// </summary>
        ErrorRedact = 8,

        /// <summary>
        /// PKCS#12 loading, cryptographic signing, or the PDF write-back failed.
        /// Maps to <see cref="PdfluentException"/>.
        /// </summary>
        ErrorSign = 9,

        /// <summary>
        /// An annotation read or write operation failed.
        /// Maps to <see cref="PdfluentException"/>.
        /// </summary>
        ErrorAnnotation = 10,

        /// <summary>
        /// A multi-document merge failed.
        /// Maps to <see cref="PdfluentException"/>.
        /// </summary>
        ErrorMerge = 11,

        /// <summary>
        /// Image or content extraction failed.
        /// Maps to <see cref="PdfluentException"/>.
        /// </summary>
        ErrorExtract = 12,

        /// <summary>
        /// A page-range split failed.
        /// Maps to <see cref="PdfluentException"/>.
        /// </summary>
        ErrorSplit = 13,

        /// <summary>
        /// Watermark application failed.
        /// Maps to <see cref="PdfluentException"/>.
        /// </summary>
        ErrorWatermark = 14,

        /// <summary>
        /// Stream-compression optimisation failed.
        /// Maps to <see cref="PdfluentException"/>.
        /// </summary>
        ErrorCompress = 15,

        // 16 to 20 were the licence-activation codes, and 22 said a tier did
        // not cover the requested operation. There is no key and no tier, so
        // nothing returns them; the numbers stay retired rather than reused so
        // an older caller can never read a new meaning out of an old constant.
        // (#226)

        /// <summary>A text-edit operation failed (stale match id, unsupported
        /// container, encoding failure, signed document refused, …).</summary>
        ErrorTextEdit = 21,

        /// <summary>
        /// An unclassified error occurred. Maps to <see cref="PdfluentException"/>.
        /// </summary>
        ErrorUnknown = 99,
    }
}
