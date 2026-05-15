package com.xfa.pdf;

import java.io.IOException;
import java.util.Objects;

/**
 * Process-global license activation.
 *
 * <p>Example usage:
 * <pre>{@code
 *   PdfluentLicensing.activateKey("tier:enterprise");
 *   PdfluentLicensing.LicenseStatus s = PdfluentLicensing.status();
 *   System.out.println(s.tier);            // "Enterprise"
 *   System.out.println(s.source);          // "Explicit"
 *   System.out.println(s.outputIsMarked);  // false
 * }</pre>
 *
 * <p>The Rust core uses a process-global write-once tier; the JVM must be
 * restarted to switch tiers.
 */
public final class PdfluentLicensing {

    static {
        NativeLoader.load();
    }

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

        public LicenseStatus(Tier tier, Source source, boolean outputIsMarked) {
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

    /**
     * Activate the process-global license from a key string.
     *
     * @throws PdfException if the key is malformed or names an unknown tier
     * @throws IllegalStateException if a different tier is already active
     */
    public static void activateKey(String key) {
        Objects.requireNonNull(key, "key");
        nativeActivateKey(key);
    }

    /**
     * Activate the license by reading the key from a UTF-8 text file.
     *
     * @throws IOException if the file cannot be read
     * @throws PdfException if the file contents are not a valid key
     * @throws IllegalStateException if a different tier is already active
     */
    public static void activateFile(String path) throws IOException {
        Objects.requireNonNull(path, "path");
        nativeActivateFile(path);
    }

    /** Return the current license status. Always succeeds — Trial when no key. */
    public static LicenseStatus status() {
        int[] raw = nativeStatus();
        if (raw == null || raw.length < 3) {
            return new LicenseStatus(Tier.TRIAL, Source.DEFAULT, true);
        }
        return new LicenseStatus(
            Tier.fromCode(raw[0]),
            Source.fromCode(raw[1]),
            raw[2] != 0);
    }

    /** Effective tier. Convenience for {@code status().tier}. */
    public static Tier effectiveTier() {
        return Tier.fromCode(nativeEffectiveTier());
    }

    // ---- native methods --------------------------------------------------

    private static native void nativeActivateKey(String key);

    private static native void nativeActivateFile(String path) throws IOException;

    private static native int nativeEffectiveTier();

    private static native int[] nativeStatus();
}
