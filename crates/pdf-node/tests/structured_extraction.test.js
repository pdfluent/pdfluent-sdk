/**
 * The structured-extraction API through the Node binding (#573).
 *
 * Two methods, one document. `fixtures/structured.pdf` is built and pinned by
 * `crates/pdf-engine/tests/the_api_over_the_block_model.rs`: a heading and a
 * bulleted list on page 0, a ruled table on page 1. Every binding suite reads
 * that same file and asserts the same three things, so a fault in one crossing
 * stands next to five that still work.
 *
 * Run:
 *   cd crates/pdf-node
 *   npm run build
 *   npx jest tests/structured_extraction.test.js
 */

const fs = require('fs');
const path = require('path');

const FIXTURES = path.join(__dirname, '..', '..', '..', 'fixtures');
const STRUCTURED_PDF = path.join(FIXTURES, 'structured.pdf');
const RUNNING_HEAD_PDF = path.join(FIXTURES, 'running-head.pdf');

let PdfDocument;
try {
  ({ PdfDocument } = require('../index'));
} catch (e) {
  process.stderr.write('SKIPPED (not a pass): native addon not built — structured extraction not checked\n');
  describe.skip('structured extraction (native module not built)', () => {
    test('placeholder', () => {});
  });
}

if (PdfDocument) {
  const open = () => PdfDocument.open(fs.readFileSync(STRUCTURED_PDF));

  test('extractMarkdown carries a heading, a list item and a table cell', () => {
    const markdown = open().extractMarkdown();

    expect(markdown).toContain('# Safety rules');
    expect(markdown).toContain('- Wear the issued boots on the floor.');
    expect(markdown).toContain('| Band | Units | Rate |');
  });

  test('extractBlocks carries a heading, a list item and a table cell', () => {
    const blocks = open().extractBlocks();

    expect(
      blocks.some((b) => b.kind === 'heading' && b.text === 'Safety rules' && b.level === 1),
    ).toBe(true);
    expect(
      blocks.some(
        (b) => b.kind === 'listItem' && b.text === 'Wear the issued boots on the floor.',
      ),
    ).toBe(true);
    expect(
      blocks.some(
        (b) => b.kind === 'tableCell' && b.text === 'Band' && b.row === 0 && b.column === 0 && b.header === true,
      ),
    ).toBe(true);
  });

  test('a block carries its page and its box', () => {
    const first = open().extractBlocks()[0];

    expect(first.page).toBe(0);
    expect(first.bbox).toHaveLength(4);
    expect(first.bbox[2]).toBeGreaterThan(first.bbox[0]);
    expect(first.index).toBe(0);
    expect(first.continuesPrevious).toBe(false);
  });

  test('extractDocling is a DoclingDocument with a heading, a list and a table', () => {
    const doc = open();
    const docling = JSON.parse(doc.extractDocling(undefined, 'structured'));
    const labels = docling.texts.map((t) => t.label);

    expect(docling.schema_name).toBe('DoclingDocument');
    expect(docling.name).toBe('structured');
    expect(labels).toContain('section_header');
    expect(labels).toContain('list_item');
    expect(docling.tables).toHaveLength(1);
    expect(JSON.parse(doc.extractDocling()).name).toBe('document');
    expect(Object.keys(JSON.parse(doc.extractDocling({ firstPage: 1, pageCount: 1 })).pages)).toEqual(['2']);
  });

  test('a window holds the pages it asks for', () => {
    const doc = open();
    const second = doc.extractBlocks({ firstPage: 1, pageCount: 1 });

    expect(second.length).toBeGreaterThan(0);
    expect(second.every((b) => b.page === 1)).toBe(true);
    expect(second.some((b) => b.text === 'Safety rules')).toBe(false);
    expect(doc.extractMarkdown({ firstPage: 1, pageCount: 1 })).toContain('| Band | Units | Rate |');
  });

  test('a window past the end is empty rather than an error', () => {
    const doc = open();

    expect(doc.extractBlocks({ firstPage: 9, pageCount: 4 })).toEqual([]);
    expect(doc.extractMarkdown({ firstPage: 9, pageCount: 4 })).toBe('');
  });

  test('the default markdown is pinned byte for byte', () => {
    expect(open().extractMarkdown()).toBe(
      '# Safety rules\n' +
        '\n' +
        '- Wear the issued boots on the floor.\n' +
        '- Keep the gangways clear of stock.\n' +
        '- Report a spill before you clean it.\n' +
        '\n' +
        '<!-- page 1 -->\n' +
        '\n' +
        '| Band | Units | Rate |\n' +
        '| --- | --- | --- |\n' +
        '| Standing | none | 18.90 |\n' +
        '| Day | 0 to 500 | 24.31 |\n' +
        '| Night | 0 to 500 | 11.07 |\n',
    );
  });

  const openReport = () => PdfDocument.open(fs.readFileSync(RUNNING_HEAD_PDF));

  test('the default drops the running head and marks every page', () => {
    const markdown = openReport().extractMarkdown();

    expect(markdown).not.toContain('Annual Report 2026');
    expect(markdown).not.toContain('Page 1 of 4');
    expect(markdown).toContain('<!-- page 1 -->');
  });

  test('includeHeaders and includeFooters write the furniture as plain paragraphs', () => {
    const markdown = openReport().extractMarkdown({ includeHeaders: true, includeFooters: true });

    expect(markdown.match(/Annual Report 2026/g)).toHaveLength(4);
    expect(markdown).toContain('Page 1 of 4');
    for (const line of markdown.split('\n')) {
      expect(line.startsWith('#') && line.includes('Annual Report')).toBe(false);
    }
  });

  test('includeHeaders without includeFooters keeps only the running head', () => {
    const markdown = openReport().extractMarkdown({ includeHeaders: true, includeFooters: false });

    expect(markdown.match(/Annual Report 2026/g)).toHaveLength(4);
    expect(markdown).not.toContain('Page 1 of 4');
    expect(markdown).not.toContain('Page 2 of 4');
  });

  test('includeFooters without includeHeaders keeps only the page number', () => {
    const markdown = openReport().extractMarkdown({ includeHeaders: false, includeFooters: true });

    expect(markdown).not.toContain('Annual Report 2026');
    expect(markdown).toContain('Page 1 of 4');
    expect(markdown).toContain('Page 4 of 4');
  });

  test('both furniture switches off is the default', () => {
    expect(openReport().extractMarkdown({ includeHeaders: false, includeFooters: false })).toBe(
      openReport().extractMarkdown(),
    );
  });

  test('pageSeparators "none" writes no marker and keeps paragraphs apart', () => {
    const markdown = openReport().extractMarkdown({ pageSeparators: 'none' });

    expect(markdown).not.toContain('<!-- page');
    expect(markdown).toContain(
      'It runs on for a second line of its own body type.\n' +
        '\n' +
        'The 1th section opens here and says something.',
    );
  });

  test('an unknown pageSeparators is rejected', () => {
    expect(() => openReport().extractMarkdown({ pageSeparators: 'rule' })).toThrow();
  });
}
