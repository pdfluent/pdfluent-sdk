package com.pdfluent;

import java.util.Collections;
import java.util.List;

/**
 * Thrown when {@link NativeLoader} cannot locate or load a required native
 * library ({@code libpdfluent_java} or {@code libpdf_capi}) from any of the
 * configured search locations.
 *
 * <p>Maps to the catalogued error code
 * {@code E-ENV-MISSING-DEPENDENCY} (see {@code docs/error_catalogue.md}).
 *
 * <p>Inspect {@link #getLibName()} for the missing library and
 * {@link #getAttemptedLocations()} for the full list of search paths and
 * failure reasons. The combined detail message in {@link #getMessage()}
 * embeds the same information for log capture.
 */
public final class PdfluentNativeLoadException extends PdfluentException {

    private static final long serialVersionUID = 1L;

    /** Catalogued error code; see {@code docs/error_catalogue.md}. */
    public static final String CODE = "E-ENV-MISSING-DEPENDENCY";

    private final String libName;
    // List is unmodifiable + populated with String elements only,
    // both of which are Serializable. The field is marked transient
    // to satisfy -Xlint:serial under -Werror.
    private final transient List<String> attemptedLocations;

    /**
     * Construct a {@code PdfluentNativeLoadException} for a missing native
     * library, recording the search locations that were tried.
     *
     * @param libName            short library stem (e.g. {@code pdfluent_java})
     * @param attemptedLocations human-readable list of search-location +
     *                           failure-reason pairs; order matches search
     *                           order
     */
    public PdfluentNativeLoadException(String libName, List<String> attemptedLocations) {
        super(buildMessage(libName, attemptedLocations), CODE);
        this.libName = libName;
        this.attemptedLocations = Collections.unmodifiableList(
            new java.util.ArrayList<>(attemptedLocations));
    }

    /** Short library stem that failed to load (no {@code lib} prefix, no extension). */
    public String getLibName() {
        return libName;
    }

    /**
     * Unmodifiable list of search-location + failure-reason pairs, in the
     * order they were attempted. Useful for diagnostics and CI logs.
     */
    public List<String> getAttemptedLocations() {
        return attemptedLocations;
    }

    private static String buildMessage(String libName, List<String> attempts) {
        StringBuilder sb = new StringBuilder()
            .append("[")
            .append(CODE)
            .append("] failed to load native library '")
            .append(libName)
            .append("'. Tried:");
        for (String a : attempts) {
            sb.append("\n  - ").append(a);
        }
        sb.append("\nRemedies:\n")
          .append("  1. Set -Djava.library.path to the directory containing the library.\n")
          .append("  2. Set the matching environment variable (PDFLUENT_NATIVE_LIB or PDFLUENT_CAPI_LIB) to the absolute library path.\n")
          .append("  3. Bundle the library inside the JAR at /native/<arch>/.");
        return sb.toString();
    }
}
