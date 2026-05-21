package com.pdfluent;

import org.junit.jupiter.api.Test;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.Arrays;

/** Java binding overhead perf timer: valid open+getPageCount loop + malformed
 *  typed-error loop. Writes a JSON line to JD_OUT. Run via:
 *  mvn test -Dtest=PerfTimerTest  (native libs on java.library.path). */
public class PerfTimerTest {
    private static long pct(long[] a, double q) {
        return a[Math.min(a.length - 1, (int) (q * (a.length - 1)))];
    }

    @Test
    void perf() throws Exception {
        String valid = System.getenv().getOrDefault("JD_VALID", "../../tests/corpus-mini/multi-page.pdf");
        int iters = Integer.parseInt(System.getenv().getOrDefault("JD_ITERS", "400"));
        byte[] b = Files.readAllBytes(Paths.get(valid));
        byte[] bad = {(byte) 0xDE, (byte) 0xAD, (byte) 0xBE, (byte) 0xEF, 0x00, 0x42};

        for (int i = 0; i < 30; i++) {
            try (PdfluentDocument d = PdfluentDocument.open(b)) { d.getPageCount(); } catch (Exception e) { /* warmup */ }
        }
        long[] vt = new long[iters];
        int voks = 0, pages = -1;
        for (int i = 0; i < iters; i++) {
            long t0 = System.nanoTime();
            try (PdfluentDocument d = PdfluentDocument.open(b)) { pages = d.getPageCount(); voks++; }
            catch (Exception e) { /* unexpected */ }
            vt[i] = System.nanoTime() - t0;
        }
        long[] mt = new long[iters];
        int merrs = 0;
        for (int i = 0; i < iters; i++) {
            long t0 = System.nanoTime();
            try (PdfluentDocument d = PdfluentDocument.open(bad)) { d.getPageCount(); }
            catch (Exception e) { merrs++; }
            mt[i] = System.nanoTime() - t0;
        }
        Arrays.sort(vt);
        Arrays.sort(mt);
        String json = String.format(
            "{\"binding\":\"java\",\"valid\":{\"samples\":%d,\"oks\":%d,\"pages\":%d,\"min_ns\":%d,\"p50_ns\":%d,\"p95_ns\":%d,\"p99_ns\":%d,\"max_ns\":%d},"
            + "\"malformed\":{\"samples\":%d,\"errs\":%d,\"p50_ns\":%d,\"p95_ns\":%d},\"status\":\"green_measured\"}",
            iters, voks, pages, vt[0], pct(vt, .5), pct(vt, .95), pct(vt, .99), vt[iters - 1],
            iters, merrs, pct(mt, .5), pct(mt, .95));
        String out = System.getenv().getOrDefault("JD_OUT", "/tmp/jd_java_perf.json");
        Files.write(Paths.get(out), json.getBytes());
        System.out.println("JD_JAVA_PERF " + json);
    }
}
