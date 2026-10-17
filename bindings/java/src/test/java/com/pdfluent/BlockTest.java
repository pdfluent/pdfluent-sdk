package com.pdfluent;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * {@link Block} and its grid evidence through the public constructors (#636).
 *
 * <p>{@code GridDoubt} is a plain final class with the record's constructor,
 * accessors and equals/hashCode/toString contract, because the jar targets
 * Java 11 and records need 16. These cases pin that contract, so a rewrite
 * that drops an accessor or weakens equality fails here.
 */
class BlockTest {

    private static Block.GridDoubt doubt() {
        return new Block.GridDoubt(2, 0.75, 1, 3, 4);
    }

    @Test
    void gridDoubtAccessorsReturnTheConstructorArguments() {
        Block.GridDoubt grid = doubt();
        assertEquals(2, grid.thinRows());
        assertEquals(0.75, grid.fill());
        assertEquals(Integer.valueOf(1), grid.retriedFrom());
        assertEquals(Integer.valueOf(3), grid.fullRows());
        assertEquals(Integer.valueOf(4), grid.columns());
    }

    @Test
    void gridDoubtEqualsAndHashCodeCompareEveryField() {
        assertEquals(doubt(), doubt());
        assertEquals(doubt().hashCode(), doubt().hashCode());
        assertNotEquals(doubt(), new Block.GridDoubt(9, 0.75, 1, 3, 4));
        assertNotEquals(doubt(), new Block.GridDoubt(2, 0.5, 1, 3, 4));
        assertNotEquals(doubt(), new Block.GridDoubt(2, 0.75, 0, 3, 4));
        assertNotEquals(doubt(), new Block.GridDoubt(2, 0.75, 1, 0, 4));
        assertNotEquals(doubt(), new Block.GridDoubt(2, 0.75, 1, 3, 0));
        assertNotEquals(doubt(), null);
        assertNotEquals(doubt(), "GridDoubt[thinRows=2, fill=0.75]");
    }

    @Test
    void gridDoubtToleratesNullFields() {
        Block.GridDoubt grid = new Block.GridDoubt(0, 0.0, null, null, null);
        assertNull(grid.retriedFrom());
        assertEquals(grid, new Block.GridDoubt(0, 0.0, null, null, null));
        assertEquals(grid.hashCode(),
            new Block.GridDoubt(0, 0.0, null, null, null).hashCode());
        assertNotEquals(grid, doubt());
    }

    @Test
    void gridDoubtToStringKeepsTheRecordSpelling() {
        assertEquals(
            "GridDoubt[thinRows=2, fill=0.75, retriedFrom=1, fullRows=3, columns=4]",
            doubt().toString());
    }

    @Test
    void blockCarriesTheGridTheLongConstructorWasGiven() {
        Block.GridDoubt grid = doubt();
        Block cell = new Block(Block.Kind.TABLE_CELL, "x", 0,
            new double[] {0, 0, 1, 1}, 0, false, null, null,
            0, 0, 1, 1, true, 0.9, grid);
        assertSame(grid, cell.getGrid());
        assertEquals(Double.valueOf(0.9), cell.getConfidence());
    }

    @Test
    void blockWithoutGridReportsNull() {
        Block para = new Block(Block.Kind.PARAGRAPH, "x", 0,
            new double[] {0, 0, 1, 1}, 0, false, null, null,
            null, null, null, null, null);
        assertNull(para.getGrid());
        assertNull(para.getConfidence());
    }
}
