package com.pdfluent;

import com.sun.jna.Native;
import com.sun.jna.NativeLibrary;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.net.URL;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;

final class NativeLoader {

    private static volatile PdfCapiLibrary instance;
    private static final Object LOCK = new Object();

    static PdfCapiLibrary get() {
        if (instance != null) return instance;
        synchronized (LOCK) {
            if (instance != null) return instance;
            instance = load();
            return instance;
        }
    }

    private static PdfCapiLibrary load() {
        // 1. Try PDFLUENT_NATIVE_LIB env var (full path to the library file)
        String envPath = System.getenv("PDFLUENT_NATIVE_LIB");
        if (envPath != null && !envPath.isEmpty()) {
            NativeLibrary.addSearchPath("pdf_capi", new File(envPath).getParent());
        }

        // 2. Try classpath resource extraction
        String resourcePath = resourcePathForPlatform();
        if (resourcePath != null) {
            URL resource = NativeLoader.class.getResource(resourcePath);
            if (resource != null) {
                try {
                    Path tmp = extractToTemp(resource, libraryFilename());
                    NativeLibrary.addSearchPath("pdf_capi", tmp.getParent().toString());
                } catch (IOException ignored) {
                    // fall through to jna.library.path / java.library.path
                }
            }
        }

        return Native.load("pdf_capi", PdfCapiLibrary.class);
    }

    private static String resourcePathForPlatform() {
        String os = System.getProperty("os.name", "").toLowerCase();
        String arch = System.getProperty("os.arch", "").toLowerCase();
        String qualifier;
        if (os.contains("mac") || os.contains("darwin")) {
            qualifier = arch.contains("aarch64") || arch.contains("arm") ? "osx-aarch64" : "osx-x86_64";
        } else if (os.contains("linux")) {
            qualifier = arch.contains("aarch64") ? "linux-aarch64" : "linux-x86_64";
        } else if (os.contains("win")) {
            qualifier = "win-x86_64";
        } else {
            return null;
        }
        return "/native/" + qualifier + "/" + libraryFilename();
    }

    private static String libraryFilename() {
        String os = System.getProperty("os.name", "").toLowerCase();
        if (os.contains("win")) return "pdf_capi.dll";
        if (os.contains("mac") || os.contains("darwin")) return "libpdf_capi.dylib";
        return "libpdf_capi.so";
    }

    private static Path extractToTemp(URL resource, String filename) throws IOException {
        Path tempDir = Files.createTempDirectory("pdfluent-native-");
        Path target = tempDir.resolve(filename);
        try (InputStream in = resource.openStream()) {
            Files.copy(in, target, StandardCopyOption.REPLACE_EXISTING);
        }
        target.toFile().deleteOnExit();
        tempDir.toFile().deleteOnExit();
        return target;
    }

    private NativeLoader() {}
}
