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
            LicenseStatus s = Licensing.GetStatus();
            Assert.True(Enum.IsDefined(typeof(LicenseTier), s.Tier));
            Assert.True(Enum.IsDefined(typeof(LicenseSource), s.Source));
            // OutputIsMarked is bool — nothing to validate beyond shape.
            Assert.IsType<bool>(s.OutputIsMarked);
            // Active is computed: true iff tier > Trial.
            Assert.Equal(s.Tier != LicenseTier.Trial, s.Active);
        }

        [Fact]
        public void EffectiveTier_IsNonNegativeInt()
        {
            int t = Licensing.EffectiveTier;
            Assert.True(t >= 0);
            Assert.True(t <= 4);
        }

        [Fact]
        public void EffectiveTierEnum_MatchesEffectiveTier()
        {
            Assert.Equal(Licensing.EffectiveTier, (int)Licensing.EffectiveTierEnum);
        }

        [Fact]
        public void ActivateKey_NullThrowsArgumentNull()
        {
            Assert.Throws<ArgumentNullException>(() => Licensing.ActivateKey(null!));
        }

        [Fact]
        public void ActivateFile_NullThrowsArgumentNull()
        {
            Assert.Throws<ArgumentNullException>(() => Licensing.ActivateFile(null!));
        }

        [Fact]
        public void ActivateKey_InvalidThrowsTypedLicenseException()
        {
            var ex = Assert.Throws<PdfluentLicenseException>(
                () => Licensing.ActivateKey("totally-not-a-license"));
            Assert.Equal(PdfStatus.ErrorInvalidLicense, ex.NativeStatus);
            Assert.Equal("E-LICENSE-INVALID", ex.Code);
        }

        [Fact]
        public void ActivateKey_UnknownTierThrowsTypedLicenseException()
        {
            var ex = Assert.Throws<PdfluentLicenseException>(
                () => Licensing.ActivateKey("tier:platinum"));
            Assert.Equal("E-LICENSE-INVALID", ex.Code);
        }

        [Fact]
        public void ActivateFile_MissingPathThrowsIo()
        {
            // ErrorLicenseFile (18) maps to PdfluentIoException via FromStatus.
            var ex = Assert.Throws<PdfluentIoException>(() =>
                Licensing.ActivateFile("/nonexistent/never-exists.lic"));
            Assert.Equal(PdfStatus.ErrorLicenseFile, ex.NativeStatus);
        }

        [Fact]
        public void StatusBeforeActivation_IsTrialOrPaid()
        {
            // Status must always succeed and produce a well-formed snapshot,
            // regardless of whether another test has activated a tier earlier.
            LicenseStatus s = Licensing.GetStatus();
            if (s.Tier == LicenseTier.Trial)
            {
                Assert.False(s.Active);
                Assert.True(s.OutputIsMarked); // Trial output is marked
            }
            else
            {
                Assert.True(s.Active);
            }
        }

        [Fact]
        public void ActivationLifecycle()
        {
            // Tolerates parallel ordering with other tests.
            try
            {
                Licensing.ActivateKey("tier:developer");
                LicenseStatus s = Licensing.GetStatus();
                Assert.Equal(LicenseTier.Developer, s.Tier);
                Assert.Equal(LicenseSource.Explicit, s.Source);
                Assert.False(s.OutputIsMarked);
                Assert.True(s.Active);

                // Idempotent re-activate to the same tier is OK.
                Licensing.ActivateKey("tier:developer");

                // Conflicting tier: typed license exception with C8 code.
                var ex = Assert.Throws<PdfluentLicenseException>(() =>
                    Licensing.ActivateKey("tier:enterprise"));
                Assert.Equal(PdfStatus.ErrorLicenseAlreadySet, ex.NativeStatus);
                Assert.Equal("E-LICENSE-INVALID", ex.Code);
            }
            catch (PdfluentLicenseException ex) when (ex.NativeStatus == PdfStatus.ErrorLicenseAlreadySet)
            {
                // Another test activated to a different tier first; OK.
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
                    int t = Licensing.EffectiveTier;
                    Assert.InRange(t, (int)LicenseTier.Developer, (int)LicenseTier.Enterprise);
                }
                catch (PdfluentLicenseException ex) when (ex.NativeStatus == PdfStatus.ErrorLicenseAlreadySet)
                {
                    // Already activated to a different tier — OK.
                }
            }
            finally
            {
                File.Delete(tmp);
            }
        }
    }
}
