using System;
using System.Runtime.InteropServices;

namespace PDFluent
{
    /// <summary>
    /// Commercial tier granted by the active license.
    /// </summary>
    public enum LicenseTier
    {
        /// <summary>No paid tier; running under the default Trial allowance.</summary>
        Trial = 0,
        /// <summary>Single-developer tier.</summary>
        Developer = 1,
        /// <summary>Small-team tier.</summary>
        Team = 2,
        /// <summary>Business tier.</summary>
        Business = 3,
        /// <summary>Enterprise tier (top SKU).</summary>
        Enterprise = 4,
    }

    /// <summary>
    /// Where the currently-effective license tier was resolved from.
    /// </summary>
    public enum LicenseSource
    {
        /// <summary>No key has been provided; running in Trial.</summary>
        Default = 0,

        /// <summary>Resolved from the <c>PDFLUENT_LICENSE_KEY</c> environment variable.</summary>
        EnvVar = 1,

        /// <summary>Set explicitly via <see cref="Licensing.ActivateKey"/> or <see cref="Licensing.ActivateFile"/>.</summary>
        Explicit = 2,
    }

    /// <summary>
    /// Read-only snapshot of the currently-active license.
    /// </summary>
    /// <remarks>
    /// Mirrors the C ABI <c>PdfluentLicenseStatus</c> struct populated by
    /// <c>pdfluent_license_status</c>.
    /// </remarks>
    public readonly struct LicenseStatus : IEquatable<LicenseStatus>
    {
        /// <summary>The currently-effective tier.</summary>
        public LicenseTier Tier { get; }

        /// <summary>Where the active tier was resolved from.</summary>
        public LicenseSource Source { get; }

        /// <summary>
        /// <c>true</c> when the active tier renders output with a watermark or
        /// other trial mark; <c>false</c> for paid tiers.
        /// </summary>
        public bool OutputIsMarked { get; }

        /// <summary>
        /// <c>true</c> when a paid tier is active (anything above
        /// <see cref="LicenseTier.Trial"/>). Convenience flag mirroring the
        /// Python <c>LicenseStatus.active</c> attribute.
        /// </summary>
        public bool Active => Tier != LicenseTier.Trial;

        internal LicenseStatus(LicenseTier tier, LicenseSource source, bool outputIsMarked)
        {
            Tier = tier;
            Source = source;
            OutputIsMarked = outputIsMarked;
        }

        /// <inheritdoc/>
        public override string ToString() =>
            $"LicenseStatus(Tier={Tier}, Source={Source}, OutputIsMarked={OutputIsMarked}, Active={Active})";

        /// <inheritdoc/>
        public bool Equals(LicenseStatus other) =>
            Tier == other.Tier && Source == other.Source && OutputIsMarked == other.OutputIsMarked;

        /// <inheritdoc/>
        public override bool Equals(object? obj) => obj is LicenseStatus other && Equals(other);

        /// <inheritdoc/>
        public override int GetHashCode() =>
            ((int)Tier * 397) ^ ((int)Source * 17) ^ (OutputIsMarked ? 1 : 0);

        /// <summary>Value equality.</summary>
        public static bool operator ==(LicenseStatus left, LicenseStatus right) => left.Equals(right);

        /// <summary>Value inequality.</summary>
        public static bool operator !=(LicenseStatus left, LicenseStatus right) => !left.Equals(right);
    }

    /// <summary>
    /// Process-global license activation.
    /// </summary>
    /// <remarks>
    /// <para>
    /// The active tier is stored in a process-global <c>OnceLock</c> in the
    /// Rust core; once set, it can only be re-set to the <em>same</em> tier
    /// (idempotent). Activating a different tier returns
    /// <see cref="PdfStatus.ErrorLicenseAlreadySet"/> and raises
    /// <see cref="PdfluentLicenseException"/>.
    /// </para>
    /// <example>
    /// <code>
    /// PDFluent.Licensing.ActivateKey("tier:enterprise");
    /// LicenseStatus status = PDFluent.Licensing.GetStatus();
    /// Console.WriteLine(status.Tier);   // Enterprise
    /// Console.WriteLine(status.Active); // True
    /// </code>
    /// </example>
    /// </remarks>
    public static class Licensing
    {
        /// <summary>
        /// Activate the process-global license from a key string.
        /// </summary>
        /// <param name="key">License key (e.g. <c>"tier:enterprise"</c>).</param>
        /// <exception cref="ArgumentNullException">If <paramref name="key"/> is <c>null</c>.</exception>
        /// <exception cref="PdfluentLicenseException">
        /// If the key is malformed, names an unknown tier, or a different tier
        /// is already active in this process. <see cref="PdfluentException.Code"/>
        /// is <c>E-LICENSE-INVALID</c>.
        /// </exception>
        /// <exception cref="PdfluentValidationException">If the C ABI rejects
        /// the argument as structurally invalid.</exception>
        public static void ActivateKey(string key)
        {
            if (key is null) throw new ArgumentNullException(nameof(key));
            PdfStatus s = NativeMethods.pdfluent_license_activate_key(key);
            ThrowOnStatus(s);
        }

        /// <summary>
        /// Activate the license by reading the key from a UTF-8 text file.
        /// </summary>
        /// <param name="path">File path to a UTF-8 text file whose contents are
        /// a single license key (leading/trailing whitespace is stripped).</param>
        /// <exception cref="ArgumentNullException">If <paramref name="path"/> is <c>null</c>.</exception>
        /// <exception cref="PdfluentIoException">If the file cannot be read.</exception>
        /// <exception cref="PdfluentLicenseException">If the key in the file
        /// is invalid or conflicts with the already-active tier.</exception>
        public static void ActivateFile(string path)
        {
            if (path is null) throw new ArgumentNullException(nameof(path));
            PdfStatus s = NativeMethods.pdfluent_license_activate_file(path);
            ThrowOnStatus(s);
        }

        /// <summary>
        /// Inject the Ed25519 public key used to verify signed JSON license
        /// payloads passed to <see cref="ActivatePayload"/>.
        /// </summary>
        /// <remarks>
        /// <para>
        /// The key is process-global and may only be set once. Re-injecting the
        /// <em>same</em> 32-byte key is idempotent; injecting a <em>different</em>
        /// key after one is already set raises <see cref="PdfluentLicenseException"/>.
        /// </para>
        /// </remarks>
        /// <param name="publicKey">The raw 32-byte Ed25519 public key.</param>
        /// <exception cref="ArgumentNullException">If <paramref name="publicKey"/> is <c>null</c>.</exception>
        /// <exception cref="PdfluentValidationException">If the key length is not
        /// exactly 32 bytes (C ABI <see cref="PdfStatus.ErrorInvalidArgument"/>).</exception>
        /// <exception cref="PdfluentLicenseException">If a different key was
        /// already injected this process.</exception>
        public static void SetPublicKey(byte[] publicKey)
        {
            if (publicKey is null) throw new ArgumentNullException(nameof(publicKey));
            PdfStatus s = NativeMethods.pdfluent_license_set_public_key(
                publicKey, (UIntPtr)publicKey.Length);
            ThrowOnStatus(s);
        }

        /// <summary>
        /// Activate the process-global license from a cryptographically-signed
        /// JSON payload.
        /// </summary>
        /// <remarks>
        /// <para>
        /// The payload is the full signed license JSON —
        /// <c>{"payload": {...}, "signature": "..."}</c> — where
        /// <c>signature</c> is the base64-encoded Ed25519 signature over the
        /// canonical payload JSON. <see cref="SetPublicKey"/> must have been
        /// called first.
        /// </para>
        /// </remarks>
        /// <param name="payloadJson">The signed payload JSON string.</param>
        /// <exception cref="ArgumentNullException">If <paramref name="payloadJson"/> is <c>null</c>.</exception>
        /// <exception cref="PdfluentLicenseException">
        /// If the Ed25519 signature does not verify
        /// (<see cref="PdfStatus.ErrorLicenseInvalidSignature"/>, C8 code
        /// <c>E-LICENSE-INVALID-SIGNATURE</c>); if the payload has expired
        /// (<see cref="PdfStatus.ErrorLicenseExpired"/>, <c>E-LICENSE-EXPIRED</c>);
        /// if a different tier is already active
        /// (<see cref="PdfStatus.ErrorLicenseAlreadySet"/>); or if the JSON is
        /// malformed / the public key was never set
        /// (<see cref="PdfStatus.ErrorInvalidLicense"/>, <c>E-LICENSE-INVALID</c>).
        /// </exception>
        /// <exception cref="PdfluentValidationException">If the C ABI rejects the
        /// argument as structurally invalid.</exception>
        public static void ActivatePayload(string payloadJson)
        {
            if (payloadJson is null) throw new ArgumentNullException(nameof(payloadJson));
            PdfStatus s = NativeMethods.pdfluent_license_activate_payload(payloadJson);
            ThrowOnStatus(s);
        }

        /// <summary>
        /// Return the current license status. Always succeeds — returns a
        /// Trial-tier snapshot when no key has been activated.
        /// </summary>
        /// <returns>The current <see cref="LicenseStatus"/> snapshot.</returns>
        /// <exception cref="PdfluentException">
        /// If the underlying C ABI call returns a non-OK status (should not
        /// happen in practice; included for completeness).
        /// </exception>
        public static LicenseStatus GetStatus()
        {
            PdfStatus s = NativeMethods.pdfluent_license_status(
                out NativeMethods.PdfluentLicenseStatusNative native);
            if (s != PdfStatus.Ok)
            {
                throw PdfluentException.FromStatus(s, GetLastError() ?? "license_status failed");
            }
            return new LicenseStatus(
                (LicenseTier)native.Tier,
                (LicenseSource)native.Source,
                native.OutputIsMarked != 0);
        }

        /// <summary>
        /// Convenience accessor — returns the current
        /// <see cref="LicenseStatus"/>. Equivalent to <see cref="GetStatus"/>.
        /// </summary>
        public static LicenseStatus Status => GetStatus();

        /// <summary>
        /// Effective tier as an integer. Mirrors the C ABI
        /// <c>pdfluent_license_effective_tier()</c> return value.
        /// </summary>
        /// <remarks>
        /// Returns 0 for Trial, 1 for Developer, 2 for Team, 3 for Business,
        /// 4 for Enterprise. Always non-negative.
        /// </remarks>
        public static int EffectiveTier => NativeMethods.pdfluent_license_effective_tier();

        /// <summary>
        /// Strongly-typed convenience for <see cref="EffectiveTier"/>.
        /// </summary>
        public static LicenseTier EffectiveTierEnum => (LicenseTier)NativeMethods.pdfluent_license_effective_tier();

        private static void ThrowOnStatus(PdfStatus s)
        {
            if (s == PdfStatus.Ok) return;
            string msg = GetLastError() ?? s.ToString();
            throw PdfluentException.FromStatus(s, msg);
        }

        private static string? GetLastError()
        {
            IntPtr p = NativeMethods.pdf_get_last_error();
            return p == IntPtr.Zero ? null : Marshal.PtrToStringAnsi(p);
        }
    }
}
