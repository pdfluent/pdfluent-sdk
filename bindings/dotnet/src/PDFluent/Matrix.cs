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
    /// A 2D affine matrix: six coefficients in a b c d e f order.
    /// </summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct Matrix : IEquatable<Matrix>
    {
        /// <summary>Horizontal scale factor.</summary>
        public double A;

        /// <summary>Vertical shear factor.</summary>
        public double B;

        /// <summary>Horizontal shear factor.</summary>
        public double C;

        /// <summary>Vertical scale factor.</summary>
        public double D;

        /// <summary>Horizontal translation, in points.</summary>
        public double E;

        /// <summary>Vertical translation, in points.</summary>
        public double F;

        /// <summary>Build a matrix from its coefficients.</summary>
        public Matrix(double a, double b, double c, double d, double e, double f)
        {
            A = a;
            B = b;
            C = c;
            D = d;
            E = e;
            F = f;
        }

        /// <summary>
        /// Rotate this matrix in place by a quarter-turn count.
        /// </summary>
        /// <param name="theta">Rotation amount.</param>
        /// <returns>This matrix, rotated.</returns>
        public unsafe Matrix Prerotate(double theta)
        {
            var self = this;
            Matrix rot = default;
            CompatSupport.Check(
                NativeMethods.pdf_compat_matrix_prerotate(&self, theta, &rot), "Prerotate");
            this = rot;
            return this;
        }

        /// <inheritdoc/>
        public bool Equals(Matrix other) =>
            A == other.A && B == other.B && C == other.C
            && D == other.D && E == other.E && F == other.F;

        /// <inheritdoc/>
        public override bool Equals(object? obj) => obj is Matrix other && Equals(other);

        /// <inheritdoc/>
        public override int GetHashCode() => HashCode.Combine(A, B, C, D, E, F);

        /// <inheritdoc/>
        public override string ToString() => $"Matrix({A}, {B}, {C}, {D}, {E}, {F})";
    }
}
