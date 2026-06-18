using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>
    /// Cryptographic validity of a single digital signature.
    /// </summary>
    /// <remarks>
    /// Mirrors the tri-state returned by the C ABI
    /// <c>pdf_signature_is_valid</c> (1 = valid, 0 = invalid, -1 = unknown).
    /// </remarks>
    public enum SignatureStatus
    {
        /// <summary>The signature is cryptographically valid.</summary>
        Valid = 1,

        /// <summary>The signature is invalid (tampered, broken, or untrusted).</summary>
        Invalid = 0,

        /// <summary>
        /// Validity could not be determined (unsupported handler, missing data,
        /// or an out-of-range index).
        /// </summary>
        Unknown = -1,
    }

    /// <summary>
    /// Validation result for one signature field in a <see cref="PdfDocument"/>.
    /// </summary>
    /// <remarks>
    /// Returned (one per signature field, ordered by index) from
    /// <see cref="PdfDocument.VerifySignatures"/>.
    /// </remarks>
    public readonly struct SignatureValidation : IEquatable<SignatureValidation>
    {
        /// <summary>Zero-based index of the signature within the document.</summary>
        public int Index { get; }

        /// <summary>The cryptographic validity of this signature.</summary>
        public SignatureStatus Status { get; }

        /// <summary>
        /// Convenience flag — <see langword="true"/> only when
        /// <see cref="Status"/> is <see cref="SignatureStatus.Valid"/>.
        /// </summary>
        public bool IsValid => Status == SignatureStatus.Valid;

        /// <summary>
        /// Initializes a new validation result.
        /// </summary>
        /// <param name="index">Zero-based signature index.</param>
        /// <param name="status">The validation status.</param>
        public SignatureValidation(int index, SignatureStatus status)
        {
            Index = index;
            Status = status;
        }

        /// <inheritdoc/>
        public override string ToString() => $"SignatureValidation(Index={Index}, Status={Status})";

        /// <inheritdoc/>
        public bool Equals(SignatureValidation other) =>
            Index == other.Index && Status == other.Status;

        /// <inheritdoc/>
        public override bool Equals(object? obj) => obj is SignatureValidation other && Equals(other);

        /// <inheritdoc/>
        public override int GetHashCode() => (Index * 397) ^ (int)Status;

        /// <summary>Value equality.</summary>
        public static bool operator ==(SignatureValidation left, SignatureValidation right) => left.Equals(right);

        /// <summary>Value inequality.</summary>
        public static bool operator !=(SignatureValidation left, SignatureValidation right) => !left.Equals(right);
    }

    /// <summary>
    /// Convenience extensions over a collection of <see cref="SignatureValidation"/>.
    /// </summary>
    public static class SignatureValidationExtensions
    {
        /// <summary>
        /// Returns <see langword="true"/> when the collection is non-empty and
        /// every signature in it is <see cref="SignatureStatus.Valid"/>.
        /// </summary>
        /// <param name="validations">The per-signature validations.</param>
        /// <returns><see langword="true"/> iff at least one signature is present and all are valid.</returns>
        public static bool AllValid(this IEnumerable<SignatureValidation> validations)
        {
            if (validations is null) throw new ArgumentNullException(nameof(validations));
            bool any = false;
            foreach (SignatureValidation v in validations)
            {
                any = true;
                if (v.Status != SignatureStatus.Valid) return false;
            }
            return any;
        }
    }
}
