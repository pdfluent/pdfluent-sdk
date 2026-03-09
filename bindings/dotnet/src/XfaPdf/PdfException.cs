using System;

namespace XfaPdf
{
    /// <summary>
    /// Exception thrown when a PDF operation fails.
    /// </summary>
    public class PdfException : Exception
    {
        /// <summary>
        /// The native status code that caused this exception.
        /// </summary>
        public PdfStatus Status { get; }

        public PdfException(PdfStatus status, string message)
            : base(message)
        {
            Status = status;
        }

        public PdfException(string message)
            : base(message)
        {
            Status = PdfStatus.ErrorUnknown;
        }

        public PdfException(string message, Exception innerException)
            : base(message, innerException)
        {
            Status = PdfStatus.ErrorUnknown;
        }
    }
}
