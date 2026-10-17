/**
 * Journal smoke tests for the pdf-node binding (#643).
 *
 * The facade owns one undo log per open document; these drive it through
 * the compat Document: enable, one named op around a rotation, undo/redo
 * restoring exact states, and a save/load round-trip.
 *
 * Run:
 *   cd crates/pdf-node
 *   npm run build
 *   npx jest tests/journal.test.js
 */

const fs = require('fs');
const os = require('os');
const path = require('path');

const FIXTURES = path.join(__dirname, '..', '..', '..', 'fixtures');
const SAMPLE_PDF = path.join(FIXTURES, 'sample.pdf');

let Document;
try {
  ({ Document } = require('../index'));
} catch (e) {
  process.stderr.write('SKIPPED (not a pass): native addon not built — journal smoke not checked\n');
  describe.skip('journal (native module not built)', () => {
    test('placeholder', () => {});
  });
}

function tmpPath(name) {
  return path.join(os.tmpdir(), `pdf-node-journal-${name}-${Date.now()}.bin`);
}

if (Document) {
  test('enable starts an empty log', () => {
    const doc = new Document(SAMPLE_PDF);
    expect(doc.journalIsEnabled()).toBe(false);
    expect(doc.journalCanDo()).toEqual([false, false]);
    expect(doc.journalPosition()).toBe(0);
    doc.journalEnable();
    expect(doc.journalIsEnabled()).toBe(true);
    expect(doc.journalCanDo()).toEqual([false, false]);
    expect(doc.journalPosition()).toBe(0);
    doc.close();
  });

  test('op records, undo/redo restore', () => {
    const doc = new Document(SAMPLE_PDF);
    expect(doc.getitem(0).rotation).toBe(0);
    doc.journalEnable();
    doc.journalStartOp('op1');
    doc.getitem(0).setRotation(90);
    doc.journalStopOp();
    expect(doc.journalPosition()).toBe(1);
    expect(doc.journalOpName(0)).toBe('op1');
    expect(doc.journalCanDo()).toEqual([true, false]);
    expect(doc.getitem(0).rotation).toBe(90);

    doc.journalUndo();
    expect(doc.getitem(0).rotation).toBe(0);
    expect(doc.journalCanDo()).toEqual([false, true]);

    doc.journalRedo();
    expect(doc.getitem(0).rotation).toBe(90);
    doc.close();
  });

  test('undo without a step throws', () => {
    const doc = new Document(SAMPLE_PDF);
    expect(() => doc.journalUndo()).toThrow(/nothing to undo/);
    doc.close();
  });

  test('save/load round-trip restores the log', () => {
    const saved = tmpPath('round-trip');
    const doc = new Document(SAMPLE_PDF);
    doc.journalEnable();
    doc.journalStartOp('op1');
    doc.getitem(0).setRotation(90);
    doc.journalStopOp();
    doc.journalSave(saved);
    doc.close();

    const loaded = new Document(SAMPLE_PDF);
    loaded.journalLoad(saved);
    expect(loaded.journalIsEnabled()).toBe(true);
    expect(loaded.journalPosition()).toBe(1);
    expect(loaded.journalOpName(0)).toBe('op1');
    loaded.journalUndo();
    expect(loaded.getitem(0).rotation).toBe(0);
    loaded.close();
    fs.unlinkSync(saved);
  });
}
