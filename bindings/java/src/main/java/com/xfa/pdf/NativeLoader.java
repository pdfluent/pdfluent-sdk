package com.xfa.pdf;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;

/**
 * Native library loader for the PDF JNI bridge.
 *
 * <p>Attempts to load the native library in this order:
 * <ol>
 *   <li>From {@code java.library.path} (system default)</li>
 *   <li>From the classpath (bundled in JAR)</li>
 *   <li>From the {@code PDF_NATIVE_LIB} environment variable</li>
 * </ol>
 */
class NativeLoader {

    private static boolean loaded = false;
    private static final String LIB_NAME = "pdf_java";

    static synchronized void load() {
        if (loaded) {
            return;
        }

        // 1. Try system library path
        try {
            System.loadLibrary(LIB_NAME);
            loaded = true;
            return;
        } catch (UnsatisfiedLinkError ignored) {
            // Fall through
        }

        // 2. Try environment variable
        String envPath = System.getenv("PDF_NATIVE_LIB");
        if (envPath != null) {
            try {
                System.load(envPath);
                loaded = true;
                return;
            } catch (UnsatisfiedLinkError ignored) {
                // Fall through
            }
        }

        // 3. Try extracting from classpath
        String osName = System.getProperty("os.name", "").toLowerCase();
        String osArch = System.getProperty("os.arch", "").toLowerCase();

        String libFileName;
        if (osName.contains("mac") || osName.contains("darwin")) {
            libFileName = "libpdf_java.dylib";
        } else if (osName.contains("win")) {
            libFileName = "pdf_java.dll";
        } else {
            libFileName = "libpdf_java.so";
        }

        String resourcePath = "/native/" + osArch + "/" + libFileName;

        try (InputStream is = NativeLoader.class.getResourceAsStream(resourcePath)) {
            if (is != null) {
                Path tempFile = Files.createTempFile("pdf_java_", libFileName);
                Files.copy(is, tempFile, StandardCopyOption.REPLACE_EXISTING);
                tempFile.toFile().deleteOnExit();
                System.load(tempFile.toString());
                loaded = true;
                return;
            }
        } catch (IOException | UnsatisfiedLinkError ignored) {
            // Fall through
        }

        throw new UnsatisfiedLinkError(
            "Failed to load native library '" + LIB_NAME + "'. " +
            "Set java.library.path, PDF_NATIVE_LIB, or bundle in classpath at " + resourcePath
        );
    }
}
