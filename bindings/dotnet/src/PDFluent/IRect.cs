// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Runtime.InteropServices;

namespace PDFluent
{
    /// <summary>
    /// An integer rectangle in pixels: four edges.
    /// </summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct IRect : IEquatable<IRect>
    {
        /// <summary>Left edge, in pixels.</summary>
        public int X0;

        /// <summary>Top edge, in pixels.</summary>
        public int Y0;

        /// <summary>Right edge, in pixels.</summary>
        public int X1;

        /// <summary>Bottom edge, in pixels.</summary>
        public int Y1;

        /// <summary>Build an integer rectangle from its edges.</summary>
        public IRect(int x0, int y0, int x1, int y1)
        {
            X0 = x0;
            Y0 = y0;
            X1 = x1;
            Y1 = y1;
        }

        /// <inheritdoc/>
        public bool Equals(IRect other) =>
            X0 == other.X0 && Y0 == other.Y0 && X1 == other.X1 && Y1 == other.Y1;

        /// <inheritdoc/>
        public override bool Equals(object? obj) => obj is IRect other && Equals(other);

        /// <inheritdoc/>
        public override int GetHashCode() => HashCode.Combine(X0, Y0, X1, Y1);

        /// <inheritdoc/>
        public override string ToString() => $"IRect({X0}, {Y0}, {X1}, {Y1})";
    }
}
