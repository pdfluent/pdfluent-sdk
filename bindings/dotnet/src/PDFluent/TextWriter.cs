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
    /// Staged text for a page rectangle.
    /// </summary>
    public sealed class TextWriter : IDisposable
    {
        private IntPtr _handle;

        /// <summary>
        /// Stage text for a page rectangle.
        /// </summary>
        /// <param name="rect">Page rectangle.</param>
        public TextWriter(Rect rect)
            : this(rect, 1.0, null)
        {
        }

        /// <summary>
        /// Stage text for a page rectangle.
        /// </summary>
        /// <param name="rect">Page rectangle.</param>
        /// <param name="opacity">Opacity.</param>
        /// <param name="color">Text colour, or null.</param>
        public unsafe TextWriter(Rect rect, double opacity, double[]? color)
        {
            Rect rectTmp = rect;
            fixed (double* colorPtr = color)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_text_writer_new(&rectTmp,
                    opacity, colorPtr, CompatSupport.Count(color), out IntPtr hPtr),
                    "TextWriter");
                _handle = hPtr;
            }
        }

        internal TextWriter(IntPtr handle)
        {
            _handle = handle;
        }


        /// <summary>
        /// Appended text span: the used box and the end point.
        /// </summary>
        public sealed class AppendResult
        {
            /// <summary>Used box.</summary>
            public Rect Rect { get; }

            /// <summary>End point.</summary>
            public Point Point { get; }

            internal AppendResult(Rect rect, Point point)
            {
                Rect = rect;
                Point = point;
            }
        }

        /// <summary>
        /// Append text; returns the used box and end point.
        /// </summary>
        /// <param name="pos">Insertion point.</param>
        /// <param name="text">Text to append.</param>
        /// <param name="options">Layout options, or null for defaults.</param>
        /// <returns>The used box and end point.</returns>
        public unsafe AppendResult Append(Point pos, string? text, AppendOptions? options)
        {
            Font? font = options == null ? null : options.Font;
            double fontsize = options == null ? 11.0 : options.Fontsize;
            string? language = options == null ? null : options.Language;
            bool rightToLeft = options != null && options.RightToLeft;
            bool smallCaps = options != null && options.SmallCaps;
            Point posTmp = pos;
            Rect rectOut = default;
            Point pointOut = default;
            CompatSupport.Check(NativeMethods.pdf_compat_text_writer_append(RequireOpen(),
                &posTmp, text, font == null ? IntPtr.Zero : font.RequireOpen(), fontsize,
                language, CompatSupport.Flag(rightToLeft), CompatSupport.Flag(smallCaps),
                &rectOut, &pointOut), "Append");
            return new AppendResult(rectOut, pointOut);
        }

        /// <summary>
        /// Append text with default options.
        /// </summary>
        /// <param name="pos">Insertion point.</param>
        /// <param name="text">Text to append.</param>
        /// <returns>The used box and end point.</returns>
        public AppendResult Append(Point pos, string? text)
        {
            return Append(pos, text, null);
        }


        /// <summary>
        /// Flush staged text onto a page.
        /// </summary>
        /// <param name="page">Target page.</param>
        /// <param name="options">Flush options.</param>
        public unsafe void WriteText(Page page, FlushOptions options)
        {
            Matrix mTmp = options.Matrix.GetValueOrDefault();
            Matrix* mPtr = options.Matrix.HasValue ? &mTmp : null;
            fixed (double* colorPtr = options.Color)
            fixed (double* morphPtr = options.Morph)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_text_writer_write_text(
                    RequireOpen(), page.Doc.RequireOpen(), page.Number, colorPtr,
                    CompatSupport.Count(options.Color), options.Opacity,
                    CompatSupport.Flag(options.Overlay), morphPtr,
                    CompatSupport.Count(options.Morph), mPtr, options.RenderMode,
                    options.Oc), "WriteText");
            }
        }


        /// <summary>Convenience overload with defaults.</summary>
        public unsafe void WriteText(Page page)
        {
            WriteText(page, new FlushOptions());
        }

        /// <summary>
        /// Close the handle. Idempotent.
        /// </summary>
        public void Dispose()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.pdf_compat_text_writer_free(_handle);
                _handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        /// <summary>Finalize the handle.</summary>
        ~TextWriter()
        {
            if (_handle != IntPtr.Zero)
                NativeMethods.pdf_compat_text_writer_free(_handle);
        }

        /// <summary>
        /// The native handle, failing when closed.
        /// </summary>
        /// <returns>The live handle.</returns>
        public IntPtr RequireOpen()
        {
            if (_handle == IntPtr.Zero)
                throw new PdfluentException("writer is closed");
            return _handle;
        }
    }
}
