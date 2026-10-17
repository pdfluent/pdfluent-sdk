package com.pdfluent;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.CsvSource;

import java.nio.file.Files;
import java.nio.file.Path;

import static org.junit.jupiter.api.Assertions.*;

/**
 * The platform key maps every JVM to the {@code /native/} directory that
 * holds its libraries.
 *
 * <p>The key is {@code <os>-<arch>}, with {@code -musl} on musl libc: the
 * seven directories {@code stage_java_natives.sh} fills and the artefact gate
 * requires. A wrong key loads nothing (UnsatisfiedLink) or, worse, the wrong
 * libc's library, so every platform is pinned here, including the two spellings
 * of each arch the JVM reports ({@code os.arch} says {@code aarch64} on Linux
 * and {@code arm64} on macOS for the same silicon).
 */
class NativeLoaderTest {

    @ParameterizedTest(name = "{0} on {1} (musl={2}) -> {3}")
    @CsvSource({
        "Linux,   amd64,   false, linux-x64",
        "Linux,   x86_64,  false, linux-x64",
        "Linux,   aarch64, false, linux-arm64",
        "Linux,   arm64,   false, linux-arm64",
        "Linux,   amd64,   true,  linux-x64-musl",
        "Linux,   x86_64,  true,  linux-x64-musl",
        "Linux,   aarch64, true,  linux-arm64-musl",
        "Linux,   arm64,   true,  linux-arm64-musl",
        "Mac OS X, x86_64,  false, osx-x64",
        "Mac OS X, arm64,   false, osx-arm64",
        "Darwin,   arm64,   false, osx-arm64",
        "Windows 10, amd64, false, win-x64",
        "Windows 11, x86_64, false, win-x64",
    })
    void platformKeyCoversEveryShippedPlatform(
            String osName, String osArch, boolean musl, String expected) {
        assertEquals(
            expected,
            NativeLoader.platformKey(osName.trim(), osArch.trim(), musl));
    }

    @Test
    void muslFlagIsIgnoredOffLinux() {
        // A musl flag on macOS or Windows is a caller error, not a fourth
        // macOS directory: the key stays the platform's own.
        assertEquals("osx-arm64", NativeLoader.platformKey("Mac OS X", "arm64", true));
        assertEquals("win-x64", NativeLoader.platformKey("Windows 10", "amd64", true));
    }

    @Test
    void unknownArchSurvivesVerbatimForADiagnosableLookupFailure() {
        assertEquals(
            "linux-riscv64",
            NativeLoader.platformKey("Linux", "riscv64", false));
    }

    @Test
    void thisMachineReportsARealKey() {
        String key = NativeLoader.platformKey();
        assertTrue(
            key.matches("(linux|osx|win)-(x64|arm64)(-musl)?"),
            "platformKey() on this machine: " + key);
    }

    // The musl decision reads the JVM's own interpreter, not a /lib glob: a
    // glibc host with the musl package installed still runs a glibc JVM.

    private static byte[] elf64(String interp) {
        byte[] blob = new byte[512];
        blob[0] = 0x7f;
        blob[1] = 'E';
        blob[2] = 'L';
        blob[3] = 'F';
        blob[4] = 2; // 64-bit
        put64(blob, 32, 64); // e_phoff
        put16(blob, 54, 56); // e_phentsize
        put16(blob, 56, interp == null ? 1 : 2); // e_phnum
        put32(blob, 64, 1); // PT_LOAD
        if (interp != null) {
            put32(blob, 120, 3); // PT_INTERP
            put64(blob, 128, 256); // p_offset
            byte[] raw = interp.getBytes(java.nio.charset.StandardCharsets.US_ASCII);
            System.arraycopy(raw, 0, blob, 256, raw.length);
        }
        return blob;
    }

    private static void put16(byte[] blob, int off, int v) {
        blob[off] = (byte) v;
        blob[off + 1] = (byte) (v >>> 8);
    }

    private static void put32(byte[] blob, int off, long v) {
        blob[off] = (byte) v;
        blob[off + 1] = (byte) (v >>> 8);
        blob[off + 2] = (byte) (v >>> 16);
        blob[off + 3] = (byte) (v >>> 24);
    }

    private static void put64(byte[] blob, int off, long v) {
        put32(blob, off, v);
        put32(blob, off + 4, v >>> 32);
    }

    @Test
    void muslInterpreterReadsMusl(@TempDir Path dir) throws Exception {
        Path file = dir.resolve("java-musl");
        Files.write(file, elf64("/lib/ld-musl-x86_64.so.1"));
        assertEquals("/lib/ld-musl-x86_64.so.1", NativeLoader.readInterpreter(file));
        assertTrue(NativeLoader.isMuslInterpreter("/lib/ld-musl-x86_64.so.1"));
    }

    @Test
    void glibcInterpreterReadsGnu(@TempDir Path dir) throws Exception {
        Path file = dir.resolve("java-gnu");
        Files.write(file, elf64("/lib64/ld-linux-x86-64.so.2"));
        assertEquals("/lib64/ld-linux-x86-64.so.2", NativeLoader.readInterpreter(file));
        assertFalse(NativeLoader.isMuslInterpreter("/lib64/ld-linux-x86-64.so.2"));
    }

    @Test
    void nonElfAndMissingFilesReadNull(@TempDir Path dir) throws Exception {
        Path macho = dir.resolve("java-macho");
        Files.write(macho, new byte[] {
            (byte) 0xcf, (byte) 0xfa, (byte) 0xed, (byte) 0xfe,
            0x0c, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0,
        });
        assertNull(NativeLoader.readInterpreter(macho));
        assertNull(NativeLoader.readInterpreter(dir.resolve("missing")));
        Path nointerp = dir.resolve("lib.so");
        Files.write(nointerp, elf64(null));
        assertNull(NativeLoader.readInterpreter(nointerp));
    }
}
