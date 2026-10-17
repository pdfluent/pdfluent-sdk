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
    /// A quadrilateral in points: four corners.
    /// </summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct Quad : IEquatable<Quad>
    {
        /// <summary>Upper-left corner.</summary>
        public Point Ul;

        /// <summary>Upper-right corner.</summary>
        public Point Ur;

        /// <summary>Lower-left corner.</summary>
        public Point Ll;

        /// <summary>Lower-right corner.</summary>
        public Point Lr;

        /// <summary>Build a quadrilateral from its corners.</summary>
        public Quad(Point ul, Point ur, Point ll, Point lr)
        {
            Ul = ul;
            Ur = ur;
            Ll = ll;
            Lr = lr;
        }

        /// <inheritdoc/>
        public bool Equals(Quad other) =>
            Ul.Equals(other.Ul) && Ur.Equals(other.Ur)
            && Ll.Equals(other.Ll) && Lr.Equals(other.Lr);

        /// <inheritdoc/>
        public override bool Equals(object? obj) => obj is Quad other && Equals(other);

        /// <inheritdoc/>
        public override int GetHashCode() => HashCode.Combine(Ul, Ur, Ll, Lr);

        /// <inheritdoc/>
        public override string ToString() => $"Quad({Ul}, {Ur}, {Ll}, {Lr})";
    }
}
