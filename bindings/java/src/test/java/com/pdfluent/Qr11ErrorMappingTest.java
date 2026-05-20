package com.pdfluent;

import static org.junit.jupiter.api.Assertions.*;
import org.junit.jupiter.api.Test;
import java.nio.file.Files;
import java.nio.file.Paths;

/** QR-11 Java runtime error-mapping: malformed/empty -> typed PdfluentException;
 *  valid control opens. Native libs via java.library.path. */
public class Qr11ErrorMappingTest {
    private static final String VALID = System.getenv().getOrDefault("QR11_VALID_PDF", "../../tests/corpus-mini/multi-page.pdf");

    @Test
    void validControlOpens() throws Exception {
        byte[] b = Files.readAllBytes(Paths.get(VALID));
        PdfluentDocument d = PdfluentDocument.open(b);
        try {
            assertTrue(d.getPageCount() >= 1);
        } finally {
            d.close();
        }
    }

    @Test
    void malformedThrowsTyped() {
        byte[] bad = {(byte) 0xDE, (byte) 0xAD, (byte) 0xBE, (byte) 0xEF, 0x00, 0x42};
        assertThrows(PdfluentException.class, () -> PdfluentDocument.open(bad));
    }

    @Test
    void emptyThrowsTyped() {
        assertThrows(PdfluentException.class, () -> PdfluentDocument.open(new byte[0]));
    }
}
