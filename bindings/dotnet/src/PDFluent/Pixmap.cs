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
    /// A compatible pixel buffer.
    /// </summary>
    public sealed class Pixmap : IDisposable
    {
        private IntPtr _handle;

        /// <summary>
        /// Allocate a pixel buffer.
        /// </summary>
        /// <param name="colorspace">Colour space.</param>
        /// <param name="bounds">Pixel bounds.</param>
        /// <param name="alpha">Keep an alpha channel.</param>
        public unsafe Pixmap(Colorspace colorspace, IRect bounds, bool alpha)
        {
            IRect boundsTmp = bounds;
            CompatSupport.Check(NativeMethods.pdf_compat_pixmap_new((int)colorspace,
                &boundsTmp, CompatSupport.Flag(alpha), out IntPtr hPtr), "Pixmap");
            _handle = hPtr;
        }

        internal Pixmap(IntPtr handle)
        {
            _handle = handle;
        }


        /// <summary>Whether an alpha channel is present.</summary>
        public bool Alpha()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_pixmap_alpha(RequireOpen(), out outValue), "Alpha");
            return outValue != 0;
        }

        /// <summary>Clear to a value, optionally clipped.</summary>
        public unsafe void ClearWith(int? value, IRect? bbox)
        {
            IRect bboxTmp = bbox.GetValueOrDefault();
            IRect* bboxPtr = bbox.HasValue ? &bboxTmp : null;
            CompatSupport.Check(NativeMethods.pdf_compat_pixmap_clear_with(RequireOpen(), value ?? -1, bboxPtr), "ClearWith");
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe void ClearWith()
        {
            ClearWith(null, null);
        }

        /// <summary>Colour-space code: 1 grey, 3 RGB, 4 CMYK.</summary>
        public int Colorspace()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_pixmap_colorspace(RequireOpen(), out outValue), "Colorspace");
            return outValue;
        }

        /// <summary>Height in pixels.</summary>
        public int H()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_pixmap_h(RequireOpen(), out outValue), "H");
            return outValue;
        }

        /// <summary>Height in pixels.</summary>
        public int Height()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_pixmap_height(RequireOpen(), out outValue), "Height");
            return outValue;
        }

        /// <summary>Pixel bounds.</summary>
        public unsafe IRect Irect()
        {
            IRect outValue = default;
            CompatSupport.Check(NativeMethods.pdf_compat_pixmap_irect(RequireOpen(), &outValue), "Irect");
            return outValue;
        }

        /// <summary>Channel count, excluding alpha.</summary>
        public int N()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_pixmap_n(RequireOpen(), out outValue), "N");
            return outValue;
        }

        /// <summary>A copy of the raw samples.</summary>
        public byte[] Samples()
        {
            IntPtr dataPtr;
            UIntPtr dataLen;
            CompatSupport.Check(NativeMethods.pdf_compat_pixmap_samples(RequireOpen(), out dataPtr, out dataLen), "Samples");
            return CompatSupport.ReadBytes(dataPtr, dataLen);
        }

        /// <summary>Write the pixmap to a file.</summary>
        public void Save(string? path, string? output, int jpg_quality)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_pixmap_save(RequireOpen(), path, output, jpg_quality), "Save");
        }

        /// <summary>Convenience overload with defaults.</summary>
        public void Save(string? path)
        {
            Save(path, null, 95);
        }

        /// <summary>Fill a box with a colour.</summary>
        public void SetRect(IRect bbox, byte[]? color)
        {
            unsafe
            {
                fixed (byte* colorPtr = color)
                {
                IRect bboxTmp = bbox;
                CompatSupport.Check(NativeMethods.pdf_compat_pixmap_set_rect(RequireOpen(), &bboxTmp, colorPtr, CompatSupport.Count(color)), "SetRect");
                }
            }
        }

        /// <summary>Row stride in bytes.</summary>
        public int Stride()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_pixmap_stride(RequireOpen(), out outValue), "Stride");
            return outValue;
        }

        /// <summary>Encode the pixmap.</summary>
        public byte[] Tobytes(string? output, int jpg_quality)
        {
            IntPtr dataPtr;
            UIntPtr dataLen;
            CompatSupport.Check(NativeMethods.pdf_compat_pixmap_tobytes(RequireOpen(), output, jpg_quality, out dataPtr, out dataLen), "Tobytes");
            return CompatSupport.ReadBytes(dataPtr, dataLen);
        }

        /// <summary>Convenience overload with defaults.</summary>
        public byte[] Tobytes()
        {
            return Tobytes("png", 95);
        }

        /// <summary>Width in pixels.</summary>
        public int W()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_pixmap_w(RequireOpen(), out outValue), "W");
            return outValue;
        }

        /// <summary>Width in pixels.</summary>
        public int Width()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_pixmap_width(RequireOpen(), out outValue), "Width");
            return outValue;
        }

        /// <summary>
        /// Close the handle. Idempotent.
        /// </summary>
        public void Dispose()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.pdf_compat_pixmap_free(_handle);
                _handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        /// <summary>Finalize the handle.</summary>
        ~Pixmap()
        {
            if (_handle != IntPtr.Zero)
                NativeMethods.pdf_compat_pixmap_free(_handle);
        }

        /// <summary>
        /// The native handle, failing when closed.
        /// </summary>
        /// <returns>The live handle.</returns>
        public IntPtr RequireOpen()
        {
            if (_handle == IntPtr.Zero)
                throw new PdfluentException("pixmap is closed");
            return _handle;
        }
    }
}
