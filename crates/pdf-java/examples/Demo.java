import com.xfa.pdf.*;

import java.util.List;

/**
 * Demonstrates all supported operations of the XFA Java SDK.
 *
 * Build and run:
 *   javac -cp java/ examples/Demo.java -d out/
 *   java -cp java/:out/ -Djava.library.path=target/debug Demo /path/to/sample.pdf
 */
public class Demo {

    public static void main(String[] args) throws Exception {
        if (args.length < 1) {
            System.err.println("Usage: Demo <path-to-pdf> [password]");
            System.exit(1);
        }
        String pdfPath  = args[0];
        String password = args.length > 1 ? args[1] : null;

        System.out.println("=== XFA Java SDK Demo ===\n");

        // ------------------------------------------------------------------
        // Round 1: basics
        // ------------------------------------------------------------------

        try (PdfDocument doc = password != null
                ? PdfDocument.openWithPassword(pdfPath, password)
                : PdfDocument.open(pdfPath)) {

            System.out.printf("Opened: %s%n", pdfPath);
            System.out.printf("Pages : %d%n%n", doc.getPageCount());

            // Page geometry
            if (doc.getPageCount() > 0) {
                System.out.printf("Page 0 — width=%.1f pt, height=%.1f pt, rotation=%d°%n",
                        doc.getPageWidth(0), doc.getPageHeight(0), doc.getPageRotation(0));
            }

            // Text extraction
            System.out.println("\n--- Text extraction (page 0) ---");
            String text = doc.extractText(0);
            System.out.println(text.length() > 300 ? text.substring(0, 300) + "…" : text);

            // Metadata
            System.out.println("\n--- Metadata ---");
            for (String key : new String[]{"Title", "Author", "Subject", "Creator", "Producer"}) {
                String val = doc.getMetadata(key);
                if (val != null) System.out.printf("  %-10s %s%n", key + ":", val);
            }
            System.out.printf("  %-10s %d%n", "Bookmarks:", doc.getBookmarkCount());

            // Search
            System.out.println("\n--- Search ---");
            int[] pages = doc.searchText("the");
            System.out.printf("  'the' found on %d page(s)%n", pages.length);

            // Save a copy
            String savedPath = "/tmp/xfa-demo-saved.pdf";
            doc.save(savedPath);
            System.out.printf("%nSaved copy to: %s%n", savedPath);

            // ------------------------------------------------------------------
            // Round 2: forms
            // ------------------------------------------------------------------
            System.out.println("\n--- Form fields ---");
            List<FormField> fields = doc.getFormFields();
            if (fields.isEmpty()) {
                System.out.println("  (no AcroForm fields)");
            } else {
                for (FormField f : fields) {
                    System.out.printf("  %s%n", f);
                }

                // Set the first text field if one exists.
                FormField first = fields.stream()
                        .filter(f -> "text".equals(f.fieldType))
                        .findFirst()
                        .orElse(null);
                if (first != null) {
                    boolean ok = doc.setFormField(first.name, "Hello from Java");
                    System.out.printf("%n  setFormField('%s') → %s%n", first.name, ok);
                }
            }

            // ------------------------------------------------------------------
            // Annotations
            // ------------------------------------------------------------------
            System.out.println("\n--- Annotations (page 0) ---");
            List<Annotation> annots = doc.getAnnotations(0);
            if (annots.isEmpty()) {
                System.out.println("  (none)");
            } else {
                for (Annotation a : annots) System.out.printf("  %s%n", a);
            }

            // Add a freetext annotation to page 0.
            doc.addAnnotation(0, "freetext", 50, 700, 300, 750, "Added by XFA Java SDK");
            System.out.println("  Added freetext annotation to page 0.");

            // ------------------------------------------------------------------
            // Redaction
            // ------------------------------------------------------------------
            System.out.println("\n--- Redaction ---");
            RedactReport rr = doc.redactText(-1, "the");
            System.out.printf("  Redacted '%s': %s%n", "the", rr);

            // Save the mutated document.
            String mutatedPath = "/tmp/xfa-demo-mutated.pdf";
            doc.save(mutatedPath);
            System.out.printf("  Mutated document saved to: %s%n", mutatedPath);

            // ------------------------------------------------------------------
            // Encryption / decryption
            // ------------------------------------------------------------------
            System.out.println("\n--- Encryption ---");
            String encPath = "/tmp/xfa-demo-encrypted.pdf";
            doc.encrypt(encPath, "s3cr3t");
            System.out.printf("  Encrypted copy saved to: %s%n", encPath);

            String decPath = "/tmp/xfa-demo-decrypted.pdf";
            doc.decrypt(decPath, "s3cr3t");
            System.out.printf("  Decrypted copy saved to: %s%n", decPath);
        }

        // ------------------------------------------------------------------
        // PdfUtils: merge
        // ------------------------------------------------------------------
        System.out.println("\n--- Merge ---");
        String mergedPath = "/tmp/xfa-demo-merged.pdf";
        PdfUtils.mergePdfs(new String[]{pdfPath, pdfPath}, mergedPath);
        System.out.printf("  Merged 2× '%s' → %s%n", pdfPath, mergedPath);

        // Verify merged page count.
        try (PdfDocument merged = PdfDocument.open(mergedPath)) {
            System.out.printf("  Merged page count: %d%n", merged.getPageCount());
        }

        // ------------------------------------------------------------------
        // PdfUtils: PDF/A validation
        // ------------------------------------------------------------------
        System.out.println("\n--- PDF/A validation ---");
        ComplianceReport report = PdfUtils.validatePdfA(pdfPath, "2b");
        System.out.printf("  Compliant: %s%n", report.compliant);
        System.out.printf("  Errors: %d, Warnings: %d%n", report.errorCount, report.warningCount);
        for (ComplianceIssue issue : report.issues) {
            System.out.printf("  %s%n", issue);
        }

        System.out.println("\nDemo complete.");
    }
}
