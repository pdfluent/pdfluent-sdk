package com.pdfluent;

import java.io.FileNotFoundException;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Objects;

/**
 * Process-global license activation for the PDFluent SDK.
 *
 * <p>The SDK runs in {@link Tier#TRIAL} mode by default. Trial output is
 * marked via the PDF {@code /Producer} metadata field. Activate a license
 * to unlock the paid-tier capability set.
 *
 * <p>Example:
 * <pre>{@code
 * PdfluentLicensing.activateKey("tier:enterprise");
 * PdfluentLicensing.LicenseStatus s = PdfluentLicensing.status();
 * System.out.println(s.tier);            // ENTERPRISE
 * System.out.println(s.outputIsMarked);  // false
 * }</pre>
 *
 * <p>The {@code PDFLUENT_LICENSE_KEY} environment variable is honoured
 * automatically.
 *
 * <p><b>Behavior:</b> the active tier is process-global and set-once.
 * Re-activating with the same tier is a no-op. Re-activating with a
 * different tier throws {@link IllegalStateException}; restart the JVM
 * to switch tiers.
 */
public final class PdfluentLicensing {

    private static final PdfCapiLibrary LIB = NativeLoader.get();

    private PdfluentLicensing() {
        // Static-only.
    }

    /** Commercial tier granted by the active license. */
    public enum Tier {
        TRIAL(0),
        DEVELOPER(1),
        TEAM(2),
        BUSINESS(3),
        ENTERPRISE(4);

        public final int code;

        Tier(int code) {
            this.code = code;
        }

        static Tier fromCode(int c) {
            for (Tier t : values()) {
                if (t.code == c) return t;
            }
            return TRIAL;
        }
    }

    /** Where the currently-effective tier was resolved from. */
    public enum Source {
        DEFAULT(0),
        ENV_VAR(1),
        EXPLICIT(2);

        public final int code;

        Source(int code) {
            this.code = code;
        }

        static Source fromCode(int c) {
            for (Source s : values()) {
                if (s.code == c) return s;
            }
            return DEFAULT;
        }
    }

    /** Immutable snapshot of the active license. */
    public static final class LicenseStatus {
        public final Tier tier;
        public final Source source;
        public final boolean outputIsMarked;

        LicenseStatus(Tier tier, Source source, boolean outputIsMarked) {
            this.tier = Objects.requireNonNull(tier);
            this.source = Objects.requireNonNull(source);
            this.outputIsMarked = outputIsMarked;
        }

        @Override
        public String toString() {
            return "LicenseStatus(tier=" + tier
                + ", source=" + source
                + ", outputIsMarked=" + outputIsMarked
                + ")";
        }
    }

    // ---- Status codes returned by the native layer ----
    private static final int STATUS_OK = 0;
    private static final int STATUS_INVALID_LICENSE = 16;
    private static final int STATUS_LICENSE_ALREADY_SET = 17;
    private static final int STATUS_LICENSE_FILE = 18;

    /**
     * Activate the process-global license from a key string.
     *
     * @throws PdfluentLicenseException if the key is malformed or names an unknown tier
     *         ({@link PdfluentException#getCode()} returns the canonical C8 code,
     *         e.g. {@code "E-LICENSE-INVALID"})
     * @throws IllegalStateException if a different tier is already active
     */
    public static void activateKey(String key) {
        Objects.requireNonNull(key, "key");
        int status = LIB.pdfluent_license_activate_key(key);
        throwIfStatus(status);
    }

    /**
     * Activate the license by reading the key from a UTF-8 text file.
     *
     * @throws IOException if the file cannot be read
     * @throws PdfluentLicenseException if the file contents are not a valid key
     *         ({@link PdfluentException#getCode()} returns the canonical C8 code)
     * @throws IllegalStateException if a different tier is already active
     */
    public static void activateFile(String path) throws IOException {
        Objects.requireNonNull(path, "path");
        if (!Files.exists(Path.of(path))) {
            throw new FileNotFoundException("license file not found: " + path);
        }
        int status = LIB.pdfluent_license_activate_file(path);
        if (status == STATUS_LICENSE_FILE) {
            throw new IOException("could not read license file: " + lastError());
        }
        throwIfStatus(status);
    }

    /** Return the current license status. Always succeeds — Trial when no key. */
    public static LicenseStatus status() {
        PdfCapiLibrary.PdfluentLicenseStatus.ByReference out =
            new PdfCapiLibrary.PdfluentLicenseStatus.ByReference();
        int status = LIB.pdfluent_license_status(out);
        if (status != STATUS_OK) {
            throw new PdfluentException("license_status failed: " + lastError());
        }
        return new LicenseStatus(
            Tier.fromCode(out.tier),
            Source.fromCode(out.source),
            out.outputIsMarked != 0);
    }

    /** Effective tier. Convenience for {@code status().tier}. */
    public static Tier effectiveTier() {
        return Tier.fromCode(LIB.pdfluent_license_effective_tier());
    }

    // ---- helpers ----

    private static void throwIfStatus(int status) {
        switch (status) {
            case STATUS_OK:
                return;
            case STATUS_INVALID_LICENSE:
                // Canonical C8 catalogue: E-LICENSE-INVALID
                throw new PdfluentLicenseException(
                    "invalid license: " + lastError(), "E-LICENSE-INVALID");
            case STATUS_LICENSE_ALREADY_SET:
                // Process-global tier conflict; mirror .NET behavior — surface
                // as IllegalStateException for the caller, since the JVM must
                // be restarted to switch tiers.
                throw new IllegalStateException(
                    "license already set; restart the JVM to switch tiers: " + lastError());
            default:
                throw new PdfluentLicenseException(
                    "license operation failed (status " + status + "): " + lastError(),
                    "E-LICENSE-INVALID");
        }
    }

    private static String lastError() {
        String err = LIB.pdf_get_last_error();
        return (err != null && !err.isEmpty()) ? err : "no error message available";
    }
}
