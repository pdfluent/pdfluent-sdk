using System;
using System.IO;
using System.Runtime.InteropServices;

namespace PDFluent
{
    /// <summary>
    /// Commercial tier granted by the active license.
    /// </summary>
    public enum LicenseTier
    {
        Trial = 0,
        Developer = 1,
        Team = 2,
        Business = 3,
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
    /// Read-only view of the currently-active license.
    /// </summary>
    public readonly struct LicenseStatus
    {
        public LicenseTier Tier { get; }
        public LicenseSource Source { get; }
        public bool OutputIsMarked { get; }

        internal LicenseStatus(LicenseTier tier, LicenseSource source, bool outputIsMarked)
        {
            Tier = tier;
            Source = source;
            OutputIsMarked = outputIsMarked;
        }

        public override string ToString() =>
            $"LicenseStatus(Tier={Tier}, Source={Source}, OutputIsMarked={OutputIsMarked})";
    }

    /// <summary>
    /// Process-global license activation.
    ///
    /// <example>
    /// <code>
    /// PDFluent.Licensing.ActivateKey("tier:enterprise");
    /// var status = PDFluent.Licensing.Status;
    /// Console.WriteLine(status.Tier); // Enterprise
    /// </code>
    /// </example>
    /// </summary>
    public static class Licensing
    {
        /// <summary>
        /// Activate the process-global license from a key string.
        /// </summary>
        /// <exception cref="PdfException">If the key is malformed or names an unknown tier.</exception>
        /// <exception cref="InvalidOperationException">If a different tier is already active.</exception>
        public static void ActivateKey(string key)
        {
            if (key is null) throw new ArgumentNullException(nameof(key));
            PdfStatus s = NativeMethods.pdfluent_license_activate_key(key);
            ThrowOnStatus(s);
        }

        /// <summary>
        /// Activate the license by reading the key from a UTF-8 text file.
        /// </summary>
        public static void ActivateFile(string path)
        {
            if (path is null) throw new ArgumentNullException(nameof(path));
            PdfStatus s = NativeMethods.pdfluent_license_activate_file(path);
            if (s == PdfStatus.ErrorLicenseFile)
                throw new FileNotFoundException(GetLastError() ?? $"could not read license file: {path}", path);
            ThrowOnStatus(s);
        }

        /// <summary>
        /// Return the current license status. Always succeeds — Trial when no key is active.
        /// </summary>
        public static LicenseStatus Status
        {
            get
            {
                PdfStatus s = NativeMethods.pdfluent_license_status(
                    out NativeMethods.PdfluentLicenseStatusNative native);
                if (s != PdfStatus.Ok)
                    throw new PdfException(s, GetLastError() ?? "license_status failed");
                return new LicenseStatus(
                    (LicenseTier)native.Tier,
                    (LicenseSource)native.Source,
                    native.OutputIsMarked != 0);
            }
        }

        /// <summary>
        /// Effective tier as an enum. Convenience for <c>Status.Tier</c>.
        /// </summary>
        public static LicenseTier EffectiveTier =>
            (LicenseTier)NativeMethods.pdfluent_license_effective_tier();

        private static void ThrowOnStatus(PdfStatus s)
        {
            string msg = GetLastError() ?? s.ToString();
            switch (s)
            {
                case PdfStatus.Ok:
                    return;
                case PdfStatus.ErrorInvalidLicense:
                    throw new PdfException(s, $"invalid license: {msg}");
                case PdfStatus.ErrorLicenseAlreadySet:
                    throw new InvalidOperationException(
                        $"license already set; restart the process to switch tiers: {msg}");
                case PdfStatus.ErrorInvalidArgument:
                    throw new ArgumentException($"invalid argument: {msg}");
                case PdfStatus.ErrorLicenseFile:
                    throw new IOException($"could not read license file: {msg}");
                default:
                    throw new PdfException(s, msg);
            }
        }

        private static string? GetLastError()
        {
            IntPtr p = NativeMethods.pdf_get_last_error();
            return p == IntPtr.Zero ? null : Marshal.PtrToStringAnsi(p);
        }
    }
}
