package com.xfa.oracle;

import com.itextpdf.forms.PdfAcroForm;
import com.itextpdf.forms.xfa.XfaForm;
import com.itextpdf.kernel.pdf.PdfArray;
import com.itextpdf.kernel.pdf.PdfDocument;
import com.itextpdf.kernel.pdf.PdfIndirectReference;
import com.itextpdf.kernel.pdf.PdfName;
import com.itextpdf.kernel.pdf.PdfObject;
import com.itextpdf.kernel.pdf.PdfReader;
import com.itextpdf.kernel.pdf.PdfStream;
import com.itextpdf.kernel.pdf.PdfWriter;
import com.itextpdf.kernel.pdf.PdfDictionary;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.util.ArrayList;
import java.util.List;

/**
 * XFA oracle using iText 8.
 *
 * Usage: java -jar itext-xfa-oracle.jar <pdf-path>
 *
 * stdout: JSON with XFA detection and flatten results
 * exit 0: success (detection completed, flatten attempted)
 * exit 1: fatal error (file not found, not a PDF, etc.)
 */
public class ITextXfaOracle {

    public static void main(String[] args) {
        if (args.length < 1) {
            printJson(false, 0, false, 0, 0L,
                    List.of("Usage: ITextXfaOracle <pdf-path>"));
            System.exit(1);
        }

        String pdfPath = args[0];
        File pdfFile = new File(pdfPath);
        if (!pdfFile.exists() || !pdfFile.isFile()) {
            printJson(false, 0, false, 0, 0L,
                    List.of("File not found: " + pdfPath));
            System.exit(1);
        }

        boolean hasXfa = false;
        int xfaPacketCount = 0;
        boolean flattenSuccess = false;
        int flattenPageCount = 0;
        long flattenFileSize = 0L;
        List<String> errors = new ArrayList<>();

        try {
            ByteArrayOutputStream baos = new ByteArrayOutputStream();
            PdfReader reader = new PdfReader(pdfPath);
            PdfWriter writer = new PdfWriter(baos);
            PdfDocument pdfDoc = new PdfDocument(reader, writer);

            // --- XFA detection ---
            // Primary: iText API
            PdfAcroForm acroForm = PdfAcroForm.getAcroForm(pdfDoc, false);
            if (acroForm != null) {
                XfaForm xfaForm = acroForm.getXfaForm();
                hasXfa = xfaForm != null && xfaForm.isXfaPresent();
            }

            // Secondary: direct /AcroForm /XFA dictionary lookup (more robust)
            if (!hasXfa) {
                PdfDictionary catalog = pdfDoc.getCatalog().getPdfObject();
                PdfDictionary acroFormDict = catalog.getAsDictionary(PdfName.AcroForm);
                if (acroFormDict != null) {
                    PdfObject xfaVal = acroFormDict.get(new PdfName("XFA"));
                    if (xfaVal != null) {
                        hasXfa = true;
                        // Resolve indirect ref if needed
                        if (xfaVal instanceof PdfIndirectReference) {
                            xfaVal = ((PdfIndirectReference) xfaVal).getRefersTo();
                        }
                        if (xfaVal instanceof PdfArray arr) {
                            // Packets stored as alternating [name, stream, name, stream …]
                            xfaPacketCount = arr.size() / 2;
                        } else if (xfaVal instanceof PdfStream) {
                            xfaPacketCount = 1;
                        }
                    }
                }
            }

            // Count packets via XfaForm if we found XFA via the primary path
            if (hasXfa && xfaPacketCount == 0 && acroForm != null) {
                XfaForm xfaForm = acroForm.getXfaForm();
                if (xfaForm != null) {
                    xfaPacketCount = countPacketsFromXfaForm(xfaForm, pdfDoc);
                }
            }

            int pageCountBeforeFlat = pdfDoc.getNumberOfPages();

            // --- XFA flattening ---
            if (hasXfa && acroForm != null) {
                try {
                    acroForm.flattenFields();
                    flattenSuccess = true;
                } catch (Exception e) {
                    errors.add("flattenFields: " + sanitize(e.getMessage()));
                }
            }

            int pageCountAfterFlat = pdfDoc.getNumberOfPages();
            pdfDoc.close();

            flattenPageCount = pageCountAfterFlat > 0 ? pageCountAfterFlat : pageCountBeforeFlat;
            if (flattenSuccess) {
                flattenFileSize = baos.size();
            }

        } catch (Exception e) {
            errors.add(sanitize(e.getMessage()));
            printJson(hasXfa, xfaPacketCount, false, 0, 0L, errors);
            System.exit(1);
        }

        printJson(hasXfa, xfaPacketCount, flattenSuccess, flattenPageCount, flattenFileSize, errors);
        System.exit(0);
    }

    /** Count XFA packets by inspecting the /AcroForm /XFA entry via the document catalog. */
    private static int countPacketsFromXfaForm(XfaForm xfaForm, PdfDocument pdfDoc) {
        try {
            PdfDictionary catalog = pdfDoc.getCatalog().getPdfObject();
            PdfDictionary acroFormDict = catalog.getAsDictionary(PdfName.AcroForm);
            if (acroFormDict == null) return 0;
            PdfObject xfaVal = acroFormDict.get(new PdfName("XFA"));
            if (xfaVal == null) return 0;
            if (xfaVal instanceof PdfIndirectReference) {
                xfaVal = ((PdfIndirectReference) xfaVal).getRefersTo();
            }
            if (xfaVal instanceof PdfArray arr) {
                return arr.size() / 2;
            } else if (xfaVal instanceof PdfStream) {
                return 1;
            }
        } catch (Exception ignored) {
        }
        return 0;
    }

    /** Strip control characters and truncate long messages for safe JSON embedding. */
    private static String sanitize(String msg) {
        if (msg == null) return "unknown error";
        return msg.replace("\\", "\\\\")
                  .replace("\"", "\\\"")
                  .replace("\n", " ")
                  .replace("\r", " ")
                  .replace("\t", " ");
    }

    private static void printJson(boolean hasXfa, int packetCount, boolean flattenOk,
                                   int pageCount, long fileSize, List<String> errors) {
        StringBuilder sb = new StringBuilder();
        sb.append("{\n");
        sb.append("  \"has_xfa\": ").append(hasXfa).append(",\n");
        sb.append("  \"xfa_packet_count\": ").append(packetCount).append(",\n");
        sb.append("  \"flatten_success\": ").append(flattenOk).append(",\n");
        sb.append("  \"flatten_page_count\": ").append(pageCount).append(",\n");
        sb.append("  \"flatten_file_size\": ").append(fileSize).append(",\n");
        sb.append("  \"errors\": [");
        for (int i = 0; i < errors.size(); i++) {
            if (i > 0) sb.append(", ");
            sb.append("\"").append(sanitize(errors.get(i))).append("\"");
        }
        sb.append("]\n}");
        System.out.println(sb);
    }
}
