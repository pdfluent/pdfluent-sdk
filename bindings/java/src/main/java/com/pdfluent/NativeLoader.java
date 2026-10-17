package com.pdfluent;

import com.sun.jna.Native;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
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
 *       {@code /native/<platform>/<lib>} is extracted to a temporary file and
 *       loaded.</li>
 * </ol>
 *
 * <h2>Thread safety</h2>
 * <p>This class is thread-safe. {@link #load()} and {@link #get()} are guarded
 * by a {@code synchronized} lock and are idempotent: subsequent calls after
 * the first successful load return the cached result immediately.
 *
 * <h2>Platform keys</h2>
 * <p>The classpath path component is {@code <os>-<arch>}, with {@code -musl}
 * appended on musl libc (Alpine): one of {@code linux-x64}, {@code
 * linux-arm64}, {@code linux-x64-musl}, {@code linux-arm64-musl}, {@code
 * osx-x64}, {@code osx-arm64}, {@code win-x64}. The OS comes from {@code
 * os.name}, the arch from {@code os.arch} ({@code aarch64} and {@code arm64}
 * both mean {@code arm64}; {@code x86_64} and {@code amd64} both mean {@code
 * x64}), and musl is read off the JVM's own interpreter (the PT_INTERP of
 * the java launcher). See {@link #platformKey(String, String, boolean)}.
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
     * classpath-bundled {@code /native/<platform>/libpdf_capi.<ext>}.
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

    /**
     * The platform key for this JVM: {@code <os>-<arch>}, with {@code -musl}
     * appended on musl libc. One of {@code linux-x64}, {@code linux-arm64},
     * {@code linux-x64-musl}, {@code linux-arm64-musl}, {@code osx-x64},
     * {@code osx-arm64}, {@code win-x64}.
     *
     * @throws PdfluentNativeLoadException (unchecked, via the callers' error
     *         paths) is NOT thrown here; an unrecognised OS or arch yields its
     *         raw lower-cased form so the resulting "classpath ... not found"
     *         message names what was looked up
     */
    static String platformKey() {
        return platformKey(
            System.getProperty("os.name", ""),
            System.getProperty("os.arch", ""),
            isMuslLibc());
    }

    /**
     * The platform key for a stated OS, arch and libc. Package-visible so the
     * key is tested per platform without stubbing system properties.
     *
     * @param osName raw {@code os.name}, e.g. {@code "Linux"}
     * @param osArch raw {@code os.arch}, e.g. {@code "aarch64"}
     * @param musl whether the C library is musl (Alpine)
     * @return e.g. {@code "linux-arm64-musl"}
     */
    static String platformKey(String osName, String osArch, boolean musl) {
        String os = osName.toLowerCase(java.util.Locale.ROOT);
        String arch = osArch.toLowerCase(java.util.Locale.ROOT);
        String osPart;
        if (os.contains("mac") || os.contains("darwin")) {
            osPart = "osx";
        } else if (os.contains("win")) {
            osPart = "win";
        } else {
            osPart = "linux";
        }
        String archPart;
        if (arch.equals("aarch64") || arch.equals("arm64")) {
            archPart = "arm64";
        } else if (arch.equals("x86_64") || arch.equals("amd64") || arch.equals("x64")) {
            archPart = "x64";
        } else {
            archPart = arch;
        }
        String key = osPart + "-" + archPart;
        if (musl && "linux".equals(osPart)) {
            key += "-musl";
        }
        return key;
    }

    /**
     * Whether this JVM runs on musl libc. Read off the JVM's OWN loader — the
     * PT_INTERP of the java launcher — not off a {@code /lib} glob: a glibc
     * host with the musl package installed still runs a glibc JVM, and the
     * glob called that musl.
     */
    static boolean isMuslLibc() {
        String javaHome = System.getProperty("java.home");
        if (javaHome != null) {
            String interp = readInterpreter(Paths.get(javaHome, "bin", "java"));
            if (interp != null) {
                return isMuslInterpreter(interp);
            }
        }
        return false;
    }

    static boolean isMuslInterpreter(String interp) {
        return interp.contains("ld-musl");
    }

    /**
     * The PT_INTERP of an ELF file, or null when it has none to read (not an
     * ELF file, no interpreter — a shared library, say — or unreadable).
     * Package-visible so the parse is tested against fixture bytes.
     */
    static String readInterpreter(Path file) {
        try {
            byte[] data = Files.readAllBytes(file);
            if (data.length < 64
                    || data[0] != 0x7f || data[1] != 'E'
                    || data[2] != 'L' || data[3] != 'F') {
                return null; // Not ELF (macOS/Windows binaries, garbage).
            }
            int eiClass = data[4] & 0xff;
            long phoff;
            int phentsize;
            int phnum;
            if (eiClass == 1) { // 32-bit: e_phoff @28, e_phentsize @42, e_phnum @44
                if (data.length < 46) {
                    return null;
                }
                phoff = u32(data, 28);
                phentsize = u16(data, 42);
                phnum = u16(data, 44);
            } else if (eiClass == 2) { // 64-bit: e_phoff @32, e_phentsize @54, e_phnum @56
                if (data.length < 58) {
                    return null;
                }
                phoff = u64(data, 32);
                phentsize = u16(data, 54);
                phnum = u16(data, 56);
            } else {
                return null;
            }
            for (int i = 0; i < Math.min(phnum, 32); i++) {
                long off = phoff + (long) i * phentsize;
                if (off < 0 || off + 8 > data.length) {
                    return null;
                }
                if (u32(data, (int) off) != 3) { // PT_INTERP
                    continue;
                }
                long strOff = eiClass == 1
                        ? u32(data, (int) off + 4)
                        : u64(data, (int) off + 8);
                if (strOff < 0 || strOff >= data.length) {
                    return null;
                }
                int end = (int) strOff;
                while (end < data.length && end < strOff + 256 && data[end] != 0) {
                    end++;
                }
                return new String(
                        data, (int) strOff, end - (int) strOff,
                        java.nio.charset.StandardCharsets.US_ASCII);
            }
            return null;
        } catch (IOException | IndexOutOfBoundsException e) {
            return null;
        }
    }

    private static int u16(byte[] data, int off) {
        return (data[off] & 0xff) | ((data[off + 1] & 0xff) << 8);
    }

    private static long u32(byte[] data, int off) {
        return (data[off] & 0xffL)
                | ((data[off + 1] & 0xffL) << 8)
                | ((data[off + 2] & 0xffL) << 16)
                | ((data[off + 3] & 0xffL) << 24);
    }

    private static long u64(byte[] data, int off) {
        return u32(data, off) | (u32(data, off + 4) << 32);
    }

    private static String classpathResource(String stem) {
        return "/native/" + platformKey() + "/" + libFileName(stem);
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
