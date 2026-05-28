import com.pdfluent.PdfluentDocument;
import com.pdfluent.PdfluentEncryptedDocumentException;
import com.pdfluent.PdfluentException;
import com.pdfluent.PdfluentIoException;
import com.pdfluent.PdfluentPageRangeException;
import com.pdfluent.RenderedImage;

import java.nio.file.Path;

/**
 * Strict API example for the PDFluent Java SDK.
 *
 * <p>Demonstrates the four G-track operations using try-with-resources and the
 * typed exception hierarchy. Compile and run with:
 *
 * <pre>
 * # Build the native library first:
 * #   cargo build -p pdf-java --release
 *
 * javac -Werror -cp path/to/pdfluent-0.1.0.jar StrictApiExample.java
 * java -cp .:path/to/pdfluent-0.1.0.jar \
 *      -DPDFLUENT_NATIVE_LIB=target/release/libpdfluent_java.dylib \
 *      StrictApiExample input.pdf
 * </pre>
 *
 * <p>Expected output (one-page PDF):
 * <pre>
 * [G1] Pages: 1
 * [G1] Page 0: 612.0 x 792.0 pts  rotation=0
 * [G2] Text (first 80 chars): ...
 * [G3] Rendered page 0 at 72 DPI: 612 x 792 px
 * [G4] Thumbnail: 79 x 100 px
 * </pre>
 */
public class StrictApiExample {

    public static void main(String[] args) {
        if (args.length < 1) {
            System.err.println("Usage: StrictApiExample <path-to-pdf>");
            System.exit(1);
        }

        Path pdfPath = Path.of(args[0]);

        try {
            runExample(pdfPath);
        } catch (PdfluentEncryptedDocumentException e) {
            System.err.println("Document is password-protected: " + e.getMessage());
            System.exit(2);
        } catch (PdfluentIoException e) {
            System.err.println("Cannot read file: " + e.getMessage());
            System.exit(3);
        } catch (PdfluentPageRangeException e) {
            System.err.println("Page index error: " + e.getMessage());
            System.exit(4);
        } catch (PdfluentException e) {
            System.err.println("PDF error: " + e.getMessage());
            System.exit(5);
        }
    }

    /**
     * Execute all four G-track operations on the given PDF.
     *
     * <p>Uses try-with-resources to ensure the document is closed even if an
     * exception is thrown during processing.
     *
     * @param path path to the PDF file
     */
    private static void runExample(Path path) {
        try (PdfluentDocument doc = PdfluentDocument.open(path)) {

            // G1 — Page geometry
            int pageCount = doc.getPageCount();
            System.out.println("[G1] Pages: " + pageCount);

            double width  = doc.getPageWidth(0);
            double height = doc.getPageHeight(0);
            int    rot    = doc.getPageRotation(0);
            System.out.printf("[G1] Page 0: %.1f x %.1f pts  rotation=%d%n",
                width, height, rot);

            // G2 — Text extraction
            String text = doc.extractText(0);
            String preview = text.length() > 80
                ? text.substring(0, 80) + "…"
                : text;
            System.out.println("[G2] Text (first 80 chars): " + preview);

            // G3 — Full-resolution render
            RenderedImage page = doc.renderPage(0, 72.0);
            System.out.printf("[G3] Rendered page 0 at 72 DPI: %d x %d px%n",
                page.getWidth(), page.getHeight());

            // G4 — Thumbnail render
            RenderedImage thumb = doc.renderThumbnail(0, 100);
            System.out.printf("[G4] Thumbnail: %d x %d px%n",
                thumb.getWidth(), thumb.getHeight());
        }
        // PdfluentDocument.close() is called here automatically.
    }
}
