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
    /// Module-level compatible functions.
    /// </summary>
    public static class Compat
    {
        /// <summary>Typeset width of a string in a base font.</summary>
        public static double GetTextLength(string? text, string? fontname, double fontsize, int encoding)
        {
            double outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_get_text_length(text, fontname, fontsize, encoding, out outValue), "GetTextLength");
            return outValue;
        }

        /// <summary>Convenience overload with defaults.</summary>
        public static double GetTextLength(string? text)
        {
            return GetTextLength(text, "helv", 11.0, 0);
        }

        /// <summary>
        /// Named paper size in points: width and height.
        /// </summary>
        /// <param name="name">Paper name.</param>
        /// <returns>Width and height in points.</returns>
        public static double[] PaperSize(string? name)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_paper_size(name, out double w,
                out double h), "PaperSize");
            return new double[] { w, h };
        }

    }
}
