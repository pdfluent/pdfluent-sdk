using System;
using System.IO;
using Xunit;

namespace PDFluent.Tests
{
    /// <summary>
    /// xUnit tests for the Licensing P/Invoke surface.
    ///
    /// Requires libpdf_capi to be reachable; see PdfDocumentTests for setup.
    /// All tests use fake-format keys only. The Rust core uses a process-global
    /// OnceLock for the active tier, so state-mutating tests tolerate the case
    /// where another test in this run has already activated.
    /// </summary>
    public class LicensingTests
    {
        [Fact]
        public void Status_HasKnownShape()
        {
            LicenseStatus s = Licensing.Status;
            Assert.True(Enum.IsDefined(typeof(LicenseTier), s.Tier));
            Assert.True(Enum.IsDefined(typeof(LicenseSource), s.Source));
            // OutputIsMarked is bool — nothing to validate beyond shape.
            Assert.IsType<bool>(s.OutputIsMarked);
        }

        [Fact]
        public void EffectiveTier_IsInRange()
        {
            LicenseTier t = Licensing.EffectiveTier;
            Assert.True((int)t >= 0 && (int)t <= 4);
        }

        [Fact]
        public void ActivateKey_NullThrowsArgumentNull()
        {
            Assert.Throws<ArgumentNullException>(() => Licensing.ActivateKey(null!));
        }

        [Fact]
        public void ActivateKey_InvalidThrowsPdfException()
        {
            var ex = Assert.Throws<PdfException>(() => Licensing.ActivateKey("totally-not-a-license"));
            Assert.Equal(PdfStatus.ErrorInvalidLicense, ex.Status);
        }

        [Fact]
        public void ActivateKey_UnknownTierThrowsPdfException()
        {
            Assert.Throws<PdfException>(() => Licensing.ActivateKey("tier:platinum"));
        }

        [Fact]
        public void ActivateFile_MissingPathThrows()
        {
            // Could throw PdfException (with ErrorLicenseFile -> rewrapped FileNotFound)
            // or FileNotFoundException depending on the exact mapping.
            var ex = Assert.ThrowsAny<Exception>(() =>
                Licensing.ActivateFile("/nonexistent/never-exists.lic"));
            Assert.True(ex is FileNotFoundException || ex is IOException || ex is PdfException);
        }

        [Fact]
        public void ActivationLifecycle()
        {
            // Tolerates parallel ordering with other tests.
            try
            {
                Licensing.ActivateKey("tier:developer");
                LicenseStatus s = Licensing.Status;
                Assert.Equal(LicenseTier.Developer, s.Tier);
                Assert.Equal(LicenseSource.Explicit, s.Source);
                Assert.False(s.OutputIsMarked);

                // Idempotent re-activate
                Licensing.ActivateKey("tier:developer");

                // Conflict
                Assert.Throws<InvalidOperationException>(() =>
                    Licensing.ActivateKey("tier:enterprise"));
            }
            catch (InvalidOperationException)
            {
                // Another test activated to a different tier first; ok.
            }
        }

        [Fact]
        public void ActivateFile_ReadsKey()
        {
            string tmp = Path.Combine(Path.GetTempPath(), $"pdfluent-fake-{Guid.NewGuid()}.lic");
            File.WriteAllText(tmp, "tier:team\n");
            try
            {
                try
                {
                    Licensing.ActivateFile(tmp);
                    LicenseTier t = Licensing.EffectiveTier;
                    Assert.True(t == LicenseTier.Team
                                || t == LicenseTier.Developer
                                || t == LicenseTier.Business
                                || t == LicenseTier.Enterprise);
                }
                catch (InvalidOperationException)
                {
                    // Already activated — ok.
                }
            }
            finally
            {
                File.Delete(tmp);
            }
        }
    }
}
