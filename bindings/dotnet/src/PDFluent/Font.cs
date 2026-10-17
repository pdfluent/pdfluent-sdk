// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{

    /// <summary>
    /// A compatible font.
    /// </summary>
    public sealed class Font : IDisposable
    {
        private IntPtr _handle;

        /// <summary>
        /// Build a font with default options.
        /// </summary>
        public Font()
            : this(new FontOptions())
        {
        }

        /// <summary>
        /// Build a font from a name, file, or buffer.
        /// </summary>
        /// <param name="options">Font options.</param>
        public unsafe Font(FontOptions options)
        {
            fixed (byte* bufPtr = options.Fontbuffer)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_font_new(options.Fontname,
                    options.Fontfile, bufPtr, CompatSupport.Count(options.Fontbuffer),
                    options.Script, options.Language, options.Ordering,
                    CompatSupport.Flag(options.IsBold), CompatSupport.Flag(options.IsItalic),
                    CompatSupport.Flag(options.IsSerif), CompatSupport.Flag(options.Embed),
                    out IntPtr hPtr), "Font");
                _handle = hPtr;
            }
        }

        internal Font(IntPtr handle)
        {
            _handle = handle;
        }


        /// <summary>
        /// Typeset width of a string.
        /// </summary>
        /// <param name="text">String to measure.</param>
        /// <param name="options">Measurement options.</param>
        /// <returns>The width in points.</returns>
        public double TextLength(string? text, FontMeasureOptions options)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_font_text_length(RequireOpen(), text,
                options.Fontsize, options.Language, options.Script, options.Wmode,
                CompatSupport.Flag(options.SmallCaps), options.Fallback,
                out double v), "TextLength");
            return v;
        }


        /// <summary>Convenience overload with defaults.</summary>
        public double TextLength(string? text)
        {
            return TextLength(text, new FontMeasureOptions());
        }

        /// <summary>
        /// Close the handle. Idempotent.
        /// </summary>
        public void Dispose()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.pdf_compat_font_free(_handle);
                _handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        /// <summary>Finalize the handle.</summary>
        ~Font()
        {
            if (_handle != IntPtr.Zero)
                NativeMethods.pdf_compat_font_free(_handle);
        }

        /// <summary>
        /// The native handle, failing when closed.
        /// </summary>
        /// <returns>The live handle.</returns>
        public IntPtr RequireOpen()
        {
            if (_handle == IntPtr.Zero)
                throw new PdfluentException("font is closed");
            return _handle;
        }
    }
}
