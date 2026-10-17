package com.pdfluent;

import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.List;

import static org.junit.jupiter.api.Assertions.*;

/**
 * The structured-extraction API through the Java binding (#573).
 *
 * <p>Two methods, one document. {@code fixtures/structured.pdf} is built and
 * pinned by {@code crates/pdf-engine/tests/the_api_over_the_block_model.rs}: a
 * heading and a bulleted list on page 0, a ruled table on page 1. Every
 * binding suite reads that same file and asserts the same three things, so a
 * fault in one crossing stands next to five that still work.
 *
 * <p>Build the native library first, as {@link PdfluentDocumentTest} says.
 */
class StructuredExtractionTest {

    /**
     * The repository's {@code fixtures} directory, found by walking up from the
     * working directory.
     *
     * <p>Walked rather than hard-coded, because {@code mvn} runs with the
     * module directory as its working directory and a developer may run the
     * suite from the repository root instead.
     */
    private static Path fixture() {
        Path here = Paths.get("").toAbsolutePath();
        for (Path at = here; at != null; at = at.getParent()) {
            Path candidate = at.resolve("fixtures").resolve("structured.pdf");
            if (Files.exists(candidate)) {
                return candidate;
            }
        }
        throw new IllegalStateException(
            "fixtures/structured.pdf not found above " + here
            + "; it is built by crates/pdf-engine/tests/the_api_over_the_block_model.rs");
    }

    private static PdfluentDocument open() throws IOException {
        return PdfluentDocument.open(fixture());
    }

    @Test
    void markdownCarriesAHeadingAListItemAndATableCell() throws Exception {
        try (PdfluentDocument doc = open()) {
            String markdown = doc.extractMarkdown();

            assertTrue(markdown.contains("# Safety rules"), markdown);
            assertTrue(markdown.contains("- Wear the issued boots on the floor."), markdown);
            assertTrue(markdown.contains("| Band | Units | Rate |"), markdown);
        }
    }

    @Test
    void blocksCarryAHeadingAListItemAndATableCell() throws Exception {
        try (PdfluentDocument doc = open()) {
            List<Block> blocks = doc.extractBlocks();

            assertTrue(blocks.stream().anyMatch(b ->
                b.getKind() == Block.Kind.HEADING
                    && "Safety rules".equals(b.getText())
                    && Integer.valueOf(1).equals(b.getLevel())),
                "no level-1 heading among " + blocks.size() + " blocks");
            assertTrue(blocks.stream().anyMatch(b ->
                b.getKind() == Block.Kind.LIST_ITEM
                    && "Wear the issued boots on the floor.".equals(b.getText())),
                "no list item among " + blocks.size() + " blocks");
            assertTrue(blocks.stream().anyMatch(b ->
                b.getKind() == Block.Kind.TABLE_CELL
                    && "Band".equals(b.getText())
                    && Integer.valueOf(0).equals(b.getRow())
                    && Integer.valueOf(0).equals(b.getColumn())
                    && Boolean.TRUE.equals(b.isHeader())),
                "no header cell among " + blocks.size() + " blocks");
        }
    }

    @Test
    void aBlockCarriesItsPageAndItsBox() throws Exception {
        try (PdfluentDocument doc = open()) {
            Block first = doc.extractBlocks().get(0);

            assertEquals(0, first.getPage());
            assertEquals(4, first.getBbox().length);
            assertTrue(first.getBbox()[2] > first.getBbox()[0]);
            assertEquals(0, first.getIndex());
            assertFalse(first.continuesPrevious());
        }
    }

    @Test
    void doclingCarriesAHeadingAListAndATable() throws Exception {
        try (PdfluentDocument doc = open()) {
            String json = doc.extractDocling(0, -1, "structured");

            assertTrue(json.contains("\"schema_name\": \"DoclingDocument\""), json);
            assertTrue(json.contains("\"name\": \"structured\""), json);
            assertTrue(json.contains("\"label\": \"section_header\""), json);
            assertTrue(json.contains("\"label\": \"list_item\""), json);
            assertTrue(json.contains("\"label\": \"table\""), json);
            assertTrue(doc.extractDocling().contains("\"name\": \"document\""));
        }
    }

    @Test
    void aWindowHoldsThePagesItAsksFor() throws Exception {
        try (PdfluentDocument doc = open()) {
            List<Block> second = doc.extractBlocks(1, 1);

            assertFalse(second.isEmpty());
            assertTrue(second.stream().allMatch(b -> b.getPage() == 1));
            assertTrue(second.stream().noneMatch(b -> "Safety rules".equals(b.getText())));
            assertTrue(doc.extractMarkdown(1, 1).contains("| Band | Units | Rate |"));
        }
    }

    @Test
    void aWindowPastTheEndIsEmptyRatherThanAnError() throws Exception {
        try (PdfluentDocument doc = open()) {
            assertTrue(doc.extractBlocks(9, 4).isEmpty());
            assertEquals("", doc.extractMarkdown(9, 4));
        }
    }
}
