package com.pdfluent;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;

/**
 * Loads the PDFluent native library ({@code libpdfluent_java}) from the system library
 * path, the classpath, or an explicit environment-variable path.
 *
 * <h2>Load order</h2>
 * <ol>
 *   <li><strong>System library path</strong> — {@code java.library.path}.
 *       Set with {@code -Djava.library.path=/path/to/dir} or the OS-native equivalent
 *       ({@code LD_LIBRARY_PATH}, {@code DYLD_LIBRARY_PATH}, etc.).</li>
 *   <li><strong>Environment variable</strong> — {@code PDFLUENT_NATIVE_LIB}.
 *       Set to the absolute path of the shared library file.</li>
 *   <li><strong>Classpath extraction</strong> — the library bundled inside the JAR at
 *       {@code /native/<arch>/libpdfluent_java.<ext>} is extracted to a temporary file
 *       and loaded. This is the recommended distribution path for the Maven Central
 *       release artifact; native binaries for each platform are packaged separately.</li>
 * </ol>
 *
 * <h2>Thread safety</h2>
 * <p>This class is thread-safe. {@link #load()} is guarded by a {@code synchronized}
 * lock and is idempotent: subsequent calls after the first successful load return
 * immediately without re-loading the library.
 *
 * <h2>Architecture strings</h2>
 * <p>The classpath path component uses the value of the {@code os.arch} system property.
 * Common values: {@code aarch64} (Apple Silicon, AWS Graviton), {@code x86_64} / {@code amd64}.
 */
final class NativeLoader {

    private static volatile boolean loaded = false;
    private static final String LIB_NAME = "pdfluent_java";

    private NativeLoader() {
        // utility class; not instantiable
    }

    /**
     * Load the native library. Idempotent: safe to call from multiple threads and
     * from multiple class instances.
     *
     * @throws UnsatisfiedLinkError if the library cannot be found or loaded from any
     *                              of the three search locations
     */
    static synchronized void load() {
        if (loaded) {
            return;
        }

        // 1. System library path
        try {
            System.loadLibrary(LIB_NAME);
            loaded = true;
            return;
        } catch (UnsatisfiedLinkError ignored) {
            // fall through
        }

        // 2. PDFLUENT_NATIVE_LIB environment variable
        String envPath = System.getenv("PDFLUENT_NATIVE_LIB");
        if (envPath != null) {
            try {
                System.load(envPath);
                loaded = true;
                return;
            } catch (UnsatisfiedLinkError ignored) {
                // fall through
            }
        }

        // 3. Classpath extraction (JAR-bundled binary)
        String osName = System.getProperty("os.name", "").toLowerCase(java.util.Locale.ROOT);
        String osArch = System.getProperty("os.arch", "").toLowerCase(java.util.Locale.ROOT);

        // Normalise amd64 → x86_64 to match Rust target triple convention
        if ("amd64".equals(osArch)) {
            osArch = "x86_64";
        }

        String libFileName;
        if (osName.contains("mac") || osName.contains("darwin")) {
            libFileName = "lib" + LIB_NAME + ".dylib";
        } else if (osName.contains("win")) {
            libFileName = LIB_NAME + ".dll";
        } else {
            libFileName = "lib" + LIB_NAME + ".so";
        }

        String resourcePath = "/native/" + osArch + "/" + libFileName;

        try (InputStream is = NativeLoader.class.getResourceAsStream(resourcePath)) {
            if (is != null) {
                Path tempFile = Files.createTempFile("pdfluent_java_", libFileName);
                Files.copy(is, tempFile, StandardCopyOption.REPLACE_EXISTING);
                tempFile.toFile().deleteOnExit();
                System.load(tempFile.toString());
                loaded = true;
                return;
            }
        } catch (IOException | UnsatisfiedLinkError ignored) {
            // fall through
        }

        throw new UnsatisfiedLinkError(
            "Failed to load native library '" + LIB_NAME + "'. "
            + "Options:\n"
            + "  1. Set -Djava.library.path to the directory containing " + libFileName + "\n"
            + "  2. Set the PDFLUENT_NATIVE_LIB environment variable to the full library path\n"
            + "  3. Bundle the library in the JAR at classpath " + resourcePath);
    }
}
