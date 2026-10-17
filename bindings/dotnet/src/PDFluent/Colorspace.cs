// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;

namespace PDFluent
{
    /// <summary>
    /// Pixel colour spaces for pixmaps, by channel count.
    /// </summary>
    public enum Colorspace
    {
        /// <summary>One grey component.</summary>
        Gray = 1,

        /// <summary>Red, green, and blue components.</summary>
        Rgb = 3,

        /// <summary>Cyan, magenta, yellow, and black components.</summary>
        Cmyk = 4,
    }

    /// <summary>Helpers over <see cref="Colorspace"/> wire codes.</summary>
    public static class ColorspaceExtensions
    {
        /// <summary>Channel count, excluding alpha.</summary>
        public static int GetN(this Colorspace space)
        {
            CompatSupport.Check(
                NativeMethods.pdf_compat_colorspace_n((int)space, out int n), "GetN");
            return n;
        }

        /// <summary>
        /// Colour space for a native wire code.
        /// </summary>
        /// <param name="code">Channel count.</param>
        /// <exception cref="PdfluentValidationException">For an unknown code.</exception>
        public static Colorspace FromCode(int code)
        {
            foreach (Colorspace space in (Colorspace[])Enum.GetValues(typeof(Colorspace)))
            {
                if ((int)space == code)
                    return space;
            }
            throw new PdfluentValidationException("unknown colorspace code: " + code);
        }
    }
}
