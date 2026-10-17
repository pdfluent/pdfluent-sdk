// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>Annotation appearance options.</summary>
    public sealed class AnnotUpdateOptions
    {
        /// <summary>Blend mode, if any.</summary>
        public string? BlendMode = null;

        /// <summary>Opacity, if any.</summary>
        public double? Opacity = null;

        /// <summary>Font size.</summary>
        public double Fontsize = 0.0;

        /// <summary>Font name, if any.</summary>
        public string? Fontname = null;

        /// <summary>Text colour, if any.</summary>
        public double[]? TextColor = null;

        /// <summary>Border colour, if any.</summary>
        public double[]? BorderColor = null;

        /// <summary>Fill colour, if any.</summary>
        public double[]? FillColor = null;

        /// <summary>Cross out.</summary>
        public bool CrossOut = false;

        /// <summary>Rotation in degrees.</summary>
        public int Rotate = 0;

    }
}
