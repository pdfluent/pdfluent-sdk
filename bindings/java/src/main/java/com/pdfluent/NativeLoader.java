package com.pdfluent;

import com.sun.jna.Native;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.List;

/**
 * Loads the PDFluent native libraries from the system library path, the
 * classpath, or an explicit environment-variable path.
 *
 * <h2>Two native libraries</h2>
 * <ul>
 *   <li><b>{@code libpdfluent_java}</b> — JNI library backing
 *       {@link PdfluentDocument}. Loaded via {@link #load()} +
 *       {@link System#loadLibrary(String)}.</li>
 *   <li><b>{@code libpdf_capi}</b> — flat C ABI consumed via JNA by
 *       {@link PdfDocument}. Loaded via
 *       {@link #get()}.</li>
 * </ul>
 * Both libraries expose mostly disjoint surface area; some applications will
 * only need one of them. Each loader is independent and idempotent.
 *
 * <h2>Load order (both libraries)</h2>
 * <ol>
 *   <li><b>System library path</b> — {@code java.library.path}.
 *       Set with {@code -Djava.library.path=/path/to/dir} or the OS-native
 *       equivalent ({@code LD_LIBRARY_PATH}, {@code DYLD_LIBRARY_PATH}).</li>
 *   <li><b>Environment variable</b> — {@code PDFLUENT_NATIVE_LIB} for the JNI
 *       library, {@code PDFLUENT_CAPI_LIB} for the C ABI library. Both take
 *       an absolute path to the shared library file.</li>
 *   <li><b>Classpath extraction</b> — the library bundled inside the JAR at
 *       {@code /native/<arch>/<lib>} is extracted to a temporary file and
 *       loaded.</li>
 * </ol>
 *
 * <h2>Thread safety</h2>
 * <p>This class is thread-safe. {@link #load()} and {@link #get()} are guarded
 * by a {@code synchronized} lock and are idempotent: subsequent calls after
 * the first successful load return the cached result immediately.
 *
 * <h2>Architecture strings</h2>
 * <p>The classpath path component uses the value of the {@code os.arch}
 * system property. Common values: {@code aarch64} (Apple Silicon, AWS
 * Graviton), {@code x86_64} / {@code amd64}.
 */
public final class NativeLoader {

    /** JNI library backing {@link PdfluentDocument}. */
    private static final String JNI_LIB_NAME = "pdfluent_java";

    /** C ABI library backing JNA mappings ({@link PdfCapiLibrary}). */
    private static final String CAPI_LIB_NAME = "pdf_capi";

    /** Environment variable that overrides the JNI library path. */
    private static final String ENV_JNI = "PDFLUENT_NATIVE_LIB";

    /** Environment variable that overrides the C ABI library path. */
    private static final String ENV_CAPI = "PDFLUENT_CAPI_LIB";

    private static final Object JNI_LOCK = new Object();
    private static final Object CAPI_LOCK = new Object();

    private static volatile boolean jniLoaded = false;
    private static volatile PdfCapiLibrary capiInstance;

    private NativeLoader() {
        // utility class; not instantiable
    }

    // ---------------------------------------------------------------------
    // JNI library (libpdfluent_java) — used by PdfluentDocument
    // ---------------------------------------------------------------------

    /**
     * Load the JNI library backing {@link PdfluentDocument}. Idempotent:
     * safe to call from multiple threads and from multiple class instances.
     *
     * @throws PdfluentNativeLoadException if the library cannot be found or
     *         loaded from any of the three search locations
     */
    public static void load() {
        if (jniLoaded) {
            return;
        }
        synchronized (JNI_LOCK) {
            if (jniLoaded) {
                return;
            }
            List<String> attempted = new ArrayList<>();

            // 1. System library path
            try {
                System.loadLibrary(JNI_LIB_NAME);
                jniLoaded = true;
                return;
            } catch (UnsatisfiedLinkError e) {
                attempted.add("java.library.path lookup for '" + JNI_LIB_NAME
                    + "': " + e.getMessage());
            }

            // 2. PDFLUENT_NATIVE_LIB environment variable
            String envPath = System.getenv(ENV_JNI);
            if (envPath != null && !envPath.isEmpty()) {
                try {
                    System.load(envPath);
                    jniLoaded = true;
                    return;
                } catch (UnsatisfiedLinkError e) {
                    attempted.add("$" + ENV_JNI + "=" + envPath + ": "
                        + e.getMessage());
                }
            } else {
                attempted.add("$" + ENV_JNI + " not set");
            }

            // 3. Classpath extraction
            String resourcePath = classpathResource(JNI_LIB_NAME);
            try (InputStream is =
                    NativeLoader.class.getResourceAsStream(resourcePath)) {
                if (is != null) {
                    Path tempFile = Files.createTempFile(
                        "pdfluent_java_", libFileName(JNI_LIB_NAME));
                    Files.copy(is, tempFile, StandardCopyOption.REPLACE_EXISTING);
                    tempFile.toFile().deleteOnExit();
                    System.load(tempFile.toString());
                    jniLoaded = true;
                    return;
                }
                attempted.add("classpath " + resourcePath + ": not found");
            } catch (IOException | UnsatisfiedLinkError e) {
                attempted.add("classpath " + resourcePath + ": " + e.getMessage());
            }

            throw new PdfluentNativeLoadException(JNI_LIB_NAME, attempted);
        }
    }

    // ---------------------------------------------------------------------
    // C ABI library (libpdf_capi) — used by PdfDocument
    // ---------------------------------------------------------------------

    /**
     * Return a JNA proxy for the {@link PdfCapiLibrary} C ABI. Idempotent:
     * the first call loads the library and constructs the proxy; subsequent
     * calls return the cached instance.
     *
     * <p>The library lookup order matches {@link #load()}:
     * {@code java.library.path} → {@code $PDFLUENT_CAPI_LIB} →
     * classpath-bundled {@code /native/<arch>/libpdf_capi.<ext>}.
     *
     * @return a JNA-backed implementation of {@link PdfCapiLibrary}
     * @throws PdfluentNativeLoadException if the library cannot be found
     */
    public static PdfCapiLibrary get() {
        PdfCapiLibrary local = capiInstance;
        if (local != null) {
            return local;
        }
        synchronized (CAPI_LOCK) {
            if (capiInstance != null) {
                return capiInstance;
            }
            List<String> attempted = new ArrayList<>();

            // 1. JNA default path resolution (uses java.library.path + jna.library.path).
            try {
                capiInstance = Native.load(CAPI_LIB_NAME, PdfCapiLibrary.class);
                return capiInstance;
            } catch (UnsatisfiedLinkError e) {
                attempted.add("JNA Native.load(\"" + CAPI_LIB_NAME + "\"): "
                    + e.getMessage());
            }

            // 2. PDFLUENT_CAPI_LIB env override (absolute path to the lib file).
            String envPath = System.getenv(ENV_CAPI);
            if (envPath != null && !envPath.isEmpty()) {
                try {
                    capiInstance = Native.load(envPath, PdfCapiLibrary.class);
                    return capiInstance;
                } catch (UnsatisfiedLinkError e) {
                    attempted.add("$" + ENV_CAPI + "=" + envPath + ": "
                        + e.getMessage());
                }
            } else {
                attempted.add("$" + ENV_CAPI + " not set");
            }

            // 3. Classpath extraction.
            String resourcePath = classpathResource(CAPI_LIB_NAME);
            try (InputStream is =
                    NativeLoader.class.getResourceAsStream(resourcePath)) {
                if (is != null) {
                    Path tempFile = Files.createTempFile(
                        "pdf_capi_", libFileName(CAPI_LIB_NAME));
                    Files.copy(is, tempFile, StandardCopyOption.REPLACE_EXISTING);
                    tempFile.toFile().deleteOnExit();
                    capiInstance = Native.load(
                        tempFile.toString(), PdfCapiLibrary.class);
                    return capiInstance;
                }
                attempted.add("classpath " + resourcePath + ": not found");
            } catch (IOException | UnsatisfiedLinkError e) {
                attempted.add("classpath " + resourcePath + ": " + e.getMessage());
            }

            throw new PdfluentNativeLoadException(CAPI_LIB_NAME, attempted);
        }
    }

    // ---------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------

    private static String classpathResource(String stem) {
        String osArch = System.getProperty("os.arch", "")
            .toLowerCase(java.util.Locale.ROOT);
        if ("amd64".equals(osArch)) {
            osArch = "x86_64";
        }
        return "/native/" + osArch + "/" + libFileName(stem);
    }

    private static String libFileName(String stem) {
        String osName = System.getProperty("os.name", "")
            .toLowerCase(java.util.Locale.ROOT);
        if (osName.contains("mac") || osName.contains("darwin")) {
            return "lib" + stem + ".dylib";
        }
        if (osName.contains("win")) {
            return stem + ".dll";
        }
        return "lib" + stem + ".so";
    }
}
