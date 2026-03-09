using System;
using System.Runtime.InteropServices;

namespace XfaPdf
{
    /// <summary>
    /// SafeHandle for the native PdfDocument pointer.
    /// Ensures the native handle is freed even if the caller forgets to dispose.
    /// </summary>
    internal sealed class PdfDocumentHandle : SafeHandle
    {
        private PdfDocumentHandle()
            : base(IntPtr.Zero, ownsHandle: true)
        {
        }

        internal PdfDocumentHandle(IntPtr ptr)
            : base(ptr, ownsHandle: true)
        {
        }

        public override bool IsInvalid => handle == IntPtr.Zero;

        protected override bool ReleaseHandle()
        {
            NativeMethods.pdf_document_free(handle);
            return true;
        }
    }
}
