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
    /// A rectangle in points: four edges.
    /// </summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect : IEquatable<Rect>
    {
        /// <summary>Left edge, in points.</summary>
        public double X0;

        /// <summary>Top edge, in points.</summary>
        public double Y0;

        /// <summary>Right edge, in points.</summary>
        public double X1;

        /// <summary>Bottom edge, in points.</summary>
        public double Y1;

        /// <summary>Build a rectangle from its edges.</summary>
        public Rect(double x0, double y0, double x1, double y1)
        {
            X0 = x0;
            Y0 = y0;
            X1 = x1;
            Y1 = y1;
        }

        /// <summary>Rectangle height, in points.</summary>
        public unsafe double Height
        {
            get
            {
                var self = this;
                CompatSupport.Check(
                    NativeMethods.pdf_compat_rect_get_height(&self, out double h), "Height");
                return h;
            }
        }

        /// <summary>Rectangle width, in points.</summary>
        public unsafe double Width
        {
            get
            {
                var self = this;
                CompatSupport.Check(
                    NativeMethods.pdf_compat_rect_get_width(&self, out double w), "Width");
                return w;
            }
        }

        /// <summary>Rectangle area, in square points.</summary>
        public unsafe double Area
        {
            get
            {
                var self = this;
                CompatSupport.Check(
                    NativeMethods.pdf_compat_rect_get_area(&self, out double a), "Area");
                return a;
            }
        }

        /// <summary>Whether this rectangle contains another.</summary>
        public unsafe bool Contains(Rect other)
        {
            var self = this;
            CompatSupport.Check(
                NativeMethods.pdf_compat_rect_contains(&self, &other, out int v), "Contains");
            return v != 0;
        }

        /// <summary>Whether this rectangle overlaps another.</summary>
        public unsafe bool Intersects(Rect other)
        {
            var self = this;
            CompatSupport.Check(
                NativeMethods.pdf_compat_rect_intersects(&self, &other, out int v), "Intersects");
            return v != 0;
        }

        /// <summary>Whether this rectangle is empty.</summary>
        public unsafe bool IsEmpty
        {
            get
            {
                var self = this;
                CompatSupport.Check(
                    NativeMethods.pdf_compat_rect_is_empty(&self, out int v), "IsEmpty");
                return v != 0;
            }
        }

        /// <summary>Whether this rectangle is infinite.</summary>
        public unsafe bool IsInfinite
        {
            get
            {
                var self = this;
                CompatSupport.Check(
                    NativeMethods.pdf_compat_rect_is_infinite(&self, out int v), "IsInfinite");
                return v != 0;
            }
        }

        /// <summary>
        /// Grow this rectangle in place to include another.
        /// </summary>
        /// <returns>This rectangle, grown.</returns>
        public unsafe Rect IncludeRect(Rect other)
        {
            var self = this;
            Rect grown = default;
            CompatSupport.Check(
                NativeMethods.pdf_compat_rect_include_rect(&self, &other, &grown), "IncludeRect");
            this = grown;
            return this;
        }

        /// <summary>
        /// Intersect this rectangle in place with another.
        /// </summary>
        /// <returns>This rectangle, intersected.</returns>
        public unsafe Rect Intersect(Rect other)
        {
            var self = this;
            Rect cut = default;
            CompatSupport.Check(
                NativeMethods.pdf_compat_rect_intersect(&self, &other, &cut), "Intersect");
            this = cut;
            return this;
        }

        /// <summary>
        /// Normalize this rectangle in place.
        /// </summary>
        /// <returns>This rectangle, normalized.</returns>
        public unsafe Rect Normalize()
        {
            var self = this;
            Rect norm = default;
            CompatSupport.Check(
                NativeMethods.pdf_compat_rect_normalize(&self, &norm), "Normalize");
            this = norm;
            return this;
        }

        /// <inheritdoc/>
        public bool Equals(Rect other) =>
            X0 == other.X0 && Y0 == other.Y0 && X1 == other.X1 && Y1 == other.Y1;

        /// <inheritdoc/>
        public override bool Equals(object? obj) => obj is Rect other && Equals(other);

        /// <inheritdoc/>
        public override int GetHashCode() => HashCode.Combine(X0, Y0, X1, Y1);

        /// <inheritdoc/>
        public override string ToString() => $"Rect({X0}, {Y0}, {X1}, {Y1})";
    }
}
