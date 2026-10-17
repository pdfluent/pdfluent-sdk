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
    /// A point in points: an x/y pair.
    /// </summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct Point : IEquatable<Point>
    {
        /// <summary>Horizontal position, in points.</summary>
        public double X;

        /// <summary>Vertical position, in points.</summary>
        public double Y;

        /// <summary>Build a point from its coordinates.</summary>
        public Point(double x, double y)
        {
            X = x;
            Y = y;
        }

        /// <inheritdoc/>
        public bool Equals(Point other) => X == other.X && Y == other.Y;

        /// <inheritdoc/>
        public override bool Equals(object? obj) => obj is Point other && Equals(other);

        /// <inheritdoc/>
        public override int GetHashCode() => HashCode.Combine(X, Y);

        /// <inheritdoc/>
        public override string ToString() => $"Point({X}, {Y})";
    }
}
