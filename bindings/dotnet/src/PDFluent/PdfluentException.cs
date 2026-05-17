using System;

namespace PDFluent
{
    /// <summary>
    /// Base exception for all PDFluent operations.
    /// </summary>
    /// <remarks>
    /// Maps to the top-level exception surface of the C2 Python binding.
    /// Catch this type to handle any PDFluent-originated failure uniformly.
    /// </remarks>
    public class PdfluentException : Exception
    {
        /// <summary>The raw C ABI status code that triggered this exception.</summary>
        public PdfStatus NativeStatus { get; }

        /// <summary>Initializes a new instance with a status code and message.</summary>
        /// <param name="status">The C ABI status code.</param>
        /// <param name="message">Human-readable description.</param>
        public PdfluentException(PdfStatus status, string message)
            : base(message)
        {
            NativeStatus = status;
        }

        /// <summary>Initializes a new instance with a message and unknown status.</summary>
        /// <param name="message">Human-readable description.</param>
        public PdfluentException(string message)
            : base(message)
        {
            NativeStatus = PdfStatus.ErrorUnknown;
        }

        /// <summary>Initializes a new instance wrapping an inner exception.</summary>
        /// <param name="message">Human-readable description.</param>
        /// <param name="innerException">The originating exception.</param>
        public PdfluentException(string message, Exception innerException)
            : base(message, innerException)
        {
            NativeStatus = PdfStatus.ErrorUnknown;
        }

        /// <summary>
        /// Create the most specific <see cref="PdfluentException"/> subtype for
        /// the given <paramref name="status"/> code.
        /// </summary>
        /// <param name="status">C ABI status code.</param>
        /// <param name="message">Diagnostic message.</param>
        /// <returns>A typed <see cref="PdfluentException"/>.</returns>
        internal static PdfluentException FromStatus(PdfStatus status, string message) =>
            status switch
            {
                PdfStatus.ErrorFileNotFound    => new PdfluentIoException(status, message),
                PdfStatus.ErrorCorruptPdf      => new PdfluentParseException(status, message),
                PdfStatus.ErrorInvalidArgument => new PdfluentValidationException(status, message),
                PdfStatus.ErrorInvalidPassword => new PdfluentPermissionException(status, message),
                PdfStatus.ErrorPageRange       => new PdfluentPageRangeException(status, message),
                PdfStatus.ErrorRender          => new PdfluentRenderException(status, message),
                _                              => new PdfluentException(status, message),
            };
    }

    // -------------------------------------------------------------------------
    // IO / file-system errors  (mirrors Python IOError / OSError)
    // -------------------------------------------------------------------------

    /// <summary>
    /// Raised when a file cannot be read or written (file not found, permission
    /// denied, disk full, etc.). Mirrors Python <c>IOError</c> in the C2 binding.
    /// </summary>
    public sealed class PdfluentIoException : PdfluentException
    {
        /// <inheritdoc cref="PdfluentException(PdfStatus, string)"/>
        public PdfluentIoException(PdfStatus status, string message) : base(status, message) { }

        /// <inheritdoc cref="PdfluentException(string)"/>
        public PdfluentIoException(string message) : base(message) { }

        /// <inheritdoc cref="PdfluentException(string, Exception)"/>
        public PdfluentIoException(string message, Exception inner) : base(message, inner) { }
    }

    // -------------------------------------------------------------------------
    // PDF parse errors  (mirrors Python ValueError "invalid PDF: …")
    // -------------------------------------------------------------------------

    /// <summary>
    /// Raised when a PDF cannot be parsed: the bytes are corrupt, truncated, or
    /// not a PDF at all. Mirrors Python <c>ValueError("invalid PDF: …")</c> in
    /// the C2 binding.
    /// </summary>
    public sealed class PdfluentParseException : PdfluentException
    {
        /// <inheritdoc cref="PdfluentException(PdfStatus, string)"/>
        public PdfluentParseException(PdfStatus status, string message) : base(status, message) { }

        /// <inheritdoc cref="PdfluentException(string)"/>
        public PdfluentParseException(string message) : base(message) { }

        /// <inheritdoc cref="PdfluentException(string, Exception)"/>
        public PdfluentParseException(string message, Exception inner) : base(message, inner) { }
    }

    // -------------------------------------------------------------------------
    // Validation / argument errors  (mirrors Python ValueError "invalid …")
    // -------------------------------------------------------------------------

    /// <summary>
    /// Raised when an argument fails validation: bad page geometry, unsupported
    /// annotation type, etc. Mirrors Python <c>ValueError</c> for structural
    /// validation in the C2 binding.
    /// </summary>
    public sealed class PdfluentValidationException : PdfluentException
    {
        /// <inheritdoc cref="PdfluentException(PdfStatus, string)"/>
        public PdfluentValidationException(PdfStatus status, string message) : base(status, message) { }

        /// <inheritdoc cref="PdfluentException(string)"/>
        public PdfluentValidationException(string message) : base(message) { }

        /// <inheritdoc cref="PdfluentException(string, Exception)"/>
        public PdfluentValidationException(string message, Exception inner) : base(message, inner) { }
    }

    // -------------------------------------------------------------------------
    // Encryption / permission errors  (mirrors Python PermissionError)
    // -------------------------------------------------------------------------

    /// <summary>
    /// Raised when access is denied: wrong password, or operating on an encrypted
    /// document without the owner key. Mirrors Python <c>PermissionError</c> in
    /// the C2 binding.
    /// </summary>
    public sealed class PdfluentPermissionException : PdfluentException
    {
        /// <inheritdoc cref="PdfluentException(PdfStatus, string)"/>
        public PdfluentPermissionException(PdfStatus status, string message) : base(status, message) { }

        /// <inheritdoc cref="PdfluentException(string)"/>
        public PdfluentPermissionException(string message) : base(message) { }

        /// <inheritdoc cref="PdfluentException(string, Exception)"/>
        public PdfluentPermissionException(string message, Exception inner) : base(message, inner) { }
    }

    // -------------------------------------------------------------------------
    // Page-range errors  (mirrors Python IndexError)
    // -------------------------------------------------------------------------

    /// <summary>
    /// Raised when a page index is out of range for the document.
    /// Mirrors Python <c>IndexError</c> in the C2 binding.
    /// </summary>
    public sealed class PdfluentPageRangeException : PdfluentException
    {
        /// <inheritdoc cref="PdfluentException(PdfStatus, string)"/>
        public PdfluentPageRangeException(PdfStatus status, string message) : base(status, message) { }

        /// <inheritdoc cref="PdfluentException(string)"/>
        public PdfluentPageRangeException(string message) : base(message) { }

        /// <inheritdoc cref="PdfluentException(string, Exception)"/>
        public PdfluentPageRangeException(string message, Exception inner) : base(message, inner) { }
    }

    // -------------------------------------------------------------------------
    // Render errors  (mirrors Python RuntimeError "render error: …")
    // -------------------------------------------------------------------------

    /// <summary>
    /// Raised when the renderer fails to rasterize a page. Mirrors Python
    /// <c>RuntimeError("render error: …")</c> in the C2 binding.
    /// </summary>
    public sealed class PdfluentRenderException : PdfluentException
    {
        /// <inheritdoc cref="PdfluentException(PdfStatus, string)"/>
        public PdfluentRenderException(PdfStatus status, string message) : base(status, message) { }

        /// <inheritdoc cref="PdfluentException(string)"/>
        public PdfluentRenderException(string message) : base(message) { }

        /// <inheritdoc cref="PdfluentException(string, Exception)"/>
        public PdfluentRenderException(string message, Exception inner) : base(message, inner) { }
    }

    // -------------------------------------------------------------------------
    // XFA errors  (mirrors Python RuntimeError "XFA flatten failed: …")
    // -------------------------------------------------------------------------

    /// <summary>
    /// Raised when an XFA-specific operation fails (flatten, parse, scripting).
    /// Mirrors Python <c>RuntimeError("XFA flatten failed: …")</c> in the C2
    /// binding.
    /// </summary>
    public sealed class PdfluentXfaException : PdfluentException
    {
        /// <inheritdoc cref="PdfluentException(PdfStatus, string)"/>
        public PdfluentXfaException(PdfStatus status, string message) : base(status, message) { }

        /// <inheritdoc cref="PdfluentException(string)"/>
        public PdfluentXfaException(string message) : base(message) { }

        /// <inheritdoc cref="PdfluentException(string, Exception)"/>
        public PdfluentXfaException(string message, Exception inner) : base(message, inner) { }
    }

    // -------------------------------------------------------------------------
    // Processing-limit errors  (mirrors Python RuntimeError "processing limit …")
    // -------------------------------------------------------------------------

    /// <summary>
    /// Raised when a processing limit is exceeded (page count, recursion depth,
    /// memory cap). Mirrors Python <c>RuntimeError("processing limit exceeded: …")</c>
    /// in the C2 binding.
    /// </summary>
    public sealed class PdfluentLimitException : PdfluentException
    {
        /// <inheritdoc cref="PdfluentException(PdfStatus, string)"/>
        public PdfluentLimitException(PdfStatus status, string message) : base(status, message) { }

        /// <inheritdoc cref="PdfluentException(string)"/>
        public PdfluentLimitException(string message) : base(message) { }

        /// <inheritdoc cref="PdfluentException(string, Exception)"/>
        public PdfluentLimitException(string message, Exception inner) : base(message, inner) { }
    }
}
