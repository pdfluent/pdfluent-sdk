/**
 * The balanced tier through the Node binding (#661).
 *
 * The CLI is the reference: `extract --quality balanced` runs the learned
 * layout model, and `--table-model-dir` opts table regions into TATR
 * structure. The addon ships both behind the `ort-layout` / `ort-tables`
 * cargo features (always on in addon builds), with weights installed by
 * `pdfluent models install layout tables`.
 *
 * BINDING CONTRACT (defined here; L7b implements it). The options mirror
 * the CLI option names and values exactly, in camelCase:
 *
 * - `PdfDocument.extractMarkdown({ quality: 'balanced', modelDir,
 *   tableModelDir })`. `quality` is `'fast'` (default) or `'balanced'`;
 *   anything else throws `E-UNSUPPORTED` naming the value, like the
 *   existing `ocr` mapping. `modelDir` and `tableModelDir` are optional
 *   directory paths; an absent `modelDir` resolves through
 *   `PDFLUENT_MODEL_DIR` to the per-user model dir, exactly as the shared
 *   runtime resolves it, while an absent `tableModelDir` leaves TATR off.
 * - `PdfDocument.extractBlocks` takes the same options (both methods take
 *   the same options struct), but `quality: 'balanced'` is markdown-only
 *   in the engine: it throws `E-UNSUPPORTED` with the engine's
 *   "markdown-only" message, never the fast tier under a balanced name.
 * - Errors. A balanced call without the `ort-layout` build (musl, or any
 *   platform without ONNX Runtime) throws the product's typed model
 *   error: a `PdfluentError` with code `E-IO-GENERIC` whose message
 *   contains "is not available" and the install command
 *   `pdfluent models install layout`. The same holds with the feature but
 *   without the weights, where the message additionally names the path
 *   looked at. A `tableModelDir` opt-in without the `ort-tables` build
 *   throws `E-UNSUPPORTED` naming `ort-tables`; with the build but
 *   without the tables weights it throws the typed model error naming the
 *   `tatr-v1.1-all` path and the `pdfluent models install tables` command.
 *
 * When the weights are not installed the parity suite skips whole with
 * the tier's own message on stderr -- never a silent pass -- while the
 * error-contract suite runs on any build.
 *
 * References live in `tests/data/balanced/`: per fixture the CLI's exact
 * `--quality balanced --table-model-dir` markdown plus the block kinds in
 * order, the headings, and the table cell grids parsed from it. The parity
 * tests assert both the exact markdown and the parsed kinds/order/grids.
 * Parity runs balanced with `tableModelDir` at the installed models
 * (`PDFLUENT_MODEL_DIR` else the per-user default); `modelDir` stays
 * absent so the tier's own resolution is what parity exercises.
 *
 * Run:
 *   cd crates/pdf-node
 *   npm run build
 *   pdfluent models install layout tables
 *   npx jest tests/balanced_bindings.test.js
 */

const fs = require('fs');
const os = require('os');
const path = require('path');

const ROOT = path.join(__dirname, '..', '..', '..');
const DATA = path.join(__dirname, 'data', 'balanced');

const INSTALL_LAYOUT = 'pdfluent models install layout';
const INSTALL_TABLES = 'pdfluent models install tables';
const TATR_MODEL = 'tatr-v1.1-all';
const NOT_AVAILABLE = 'is not available';

/**
 * Where `pdfluent models install` put the weights: the `PDFLUENT_MODEL_DIR`
 * override else the per-user default, the same root the shared runtime
 * resolves to -- the parity `tableModelDir`.
 */
function installedModelRoot() {
  if (process.env.PDFLUENT_MODEL_DIR) {
    return process.env.PDFLUENT_MODEL_DIR;
  }
  const home = os.homedir();
  if (process.platform === 'win32') {
    return path.join(process.env.LOCALAPPDATA || home, 'pdfluent', 'models');
  }
  if (process.platform === 'darwin') {
    return path.join(home, 'Library', 'Application Support', 'pdfluent', 'models');
  }
  return path.join(
    process.env.XDG_DATA_HOME || path.join(home, '.local', 'share'),
    'pdfluent',
    'models',
  );
}

const REFERENCE_FILES = [
  'structured.json',
  'uksi_20200121_p50.json',
  'nasdaq_wwd_2002_p7.json',
  'pmc2438475_p1_table_notes.json',
];

let mod = null;
let loadError = null;
try {
  mod = require('../index');
} catch (e) {
  loadError = e;
}

if (!mod) {
  // jest does not print console output from a suite that is skipped whole;
  // a raw stream write is the one thing that reaches the log.
  process.stderr.write(
    `SKIPPED (not a pass): balanced_bindings.test.js -- native addon not built: ${loadError && loadError.message}\n`,
  );
  if (process.env.PDFLUENT_REQUIRE_ADDON === '1') {
    test('the native addon loads', () => {
      throw loadError;
    });
  }
}

function capture(fn) {
  try {
    fn();
  } catch (e) {
    return e;
  }
  throw new Error('expected the call to throw');
}

/** A table cell's inner text: tags out, surrounding space off. */
function stripTags(text) {
  return text.replace(/<[^>]*>/g, '').trim();
}

/** One `<table>` block as a grid: rows of cell texts. */
function parseTable(block) {
  const rows = [];
  for (const row of block.matchAll(/<tr[^>]*>([\s\S]*?)<\/tr>/g)) {
    const cells = [...row[1].matchAll(/<t[hd][^>]*>([\s\S]*?)<\/t[hd]>/g)].map((m) =>
      stripTags(m[1]),
    );
    rows.push(cells);
  }
  return rows;
}

/**
 * Block kinds in order, headings, and table grids from markdown.
 *
 * Blocks split on blank lines: `#`-led is a heading (level kept), `- `-led
 * is a list item, `<table>`-led is a table (grid kept), anything else a
 * paragraph. The same rules the references were parsed with, so a drift in
 * either direction fails the comparison.
 */
function parseMarkdown(markdown) {
  const kinds = [];
  const headings = [];
  const tables = [];
  for (const raw of markdown.trim().split(/\n\s*\n/)) {
    const block = raw.trim();
    if (!block) {
      continue;
    }
    const heading = block.match(/^(#{1,3})\s+([\s\S]*)$/);
    if (heading) {
      kinds.push('heading');
      headings.push({ level: heading[1].length, text: heading[2].trim() });
    } else if (block.startsWith('- ')) {
      kinds.push('list_item');
    } else if (block.startsWith('<table>')) {
      kinds.push('table');
      tables.push(parseTable(block));
    } else {
      kinds.push('paragraph');
    }
  }
  return { kinds, headings, tables };
}

function tmpDir(name) {
  return fs.mkdtempSync(path.join(os.tmpdir(), `pdf-node-balanced-${name}-`));
}

if (mod) {
  const open = (fixture) =>
    mod.PdfDocument.open(fs.readFileSync(path.join(ROOT, fixture)));
  const loadReference = (name) =>
    JSON.parse(fs.readFileSync(path.join(DATA, name), 'utf-8'));

  // Probe once, at load: a models-missing parity suite skips whole with the
  // tier's message as the reason. Anything but the product's own model
  // error is rethrown -- real breakage stays loud.
  let skipParity = null;
  try {
    open('fixtures/structured.pdf').extractMarkdown({
      quality: 'balanced',
      tableModelDir: installedModelRoot(),
    });
  } catch (e) {
    if (e && typeof e.message === 'string' && e.message.includes(NOT_AVAILABLE)) {
      skipParity = `balanced tier not installed: ${e.message}`;
    } else {
      throw e;
    }
  }
  if (skipParity) {
    process.stderr.write(`SKIPPED (not a pass): balanced parity -- ${skipParity}\n`);
  }
  const parity = skipParity ? describe.skip : describe;

  parity('balanced parity with the CLI [L7b]', () => {
    for (const referenceFile of REFERENCE_FILES) {
      test(`balanced markdown matches the CLI reference (${referenceFile}) [L7b]`, () => {
        const reference = loadReference(referenceFile);
        const markdown = open(reference.fixture).extractMarkdown({
          quality: 'balanced',
          tableModelDir: installedModelRoot(),
        });
        expect(markdown).toBe(reference.markdown);
        const { kinds, headings, tables } = parseMarkdown(markdown);
        expect(kinds).toEqual(reference.kinds);
        expect(headings).toEqual(reference.headings);
        expect(tables).toEqual(reference.tables);
      });
    }
  });

  describe('balanced binding contract [L7b]', () => {
    test('quality defaults to fast [L7b]', () => {
      const doc = open('fixtures/structured.pdf');
      expect(doc.extractMarkdown()).toBe(doc.extractMarkdown({ quality: 'fast' }));
    });

    test('balanced blocks is markdown-only [L7b]', () => {
      const err = capture(() =>
        open('fixtures/structured.pdf').extractBlocks({ quality: 'balanced' }),
      );
      expect(err).toBeInstanceOf(mod.PdfluentError);
      expect(err.code).toBe('E-UNSUPPORTED');
      expect(err.message).toMatch('markdown-only');
    });

    test('an unknown quality is rejected [L7b]', () => {
      const err = capture(() =>
        open('fixtures/structured.pdf').extractMarkdown({ quality: 'ultra' }),
      );
      expect(err).toBeInstanceOf(mod.PdfluentError);
      expect(err.code).toBe('E-UNSUPPORTED');
      expect(err.message).toMatch('quality');
    });

    test('an empty modelDir raises the typed model error [L7b]', () => {
      const err = capture(() =>
        open('fixtures/structured.pdf').extractMarkdown({
          quality: 'balanced',
          modelDir: tmpDir('empty-models'),
        }),
      );
      expect(err).toBeInstanceOf(mod.PdfluentError);
      expect(err.code).toBe('E-IO-GENERIC');
      expect(err.message).toMatch(NOT_AVAILABLE);
      expect(err.message).toMatch(INSTALL_LAYOUT);
    });

    test('tableModelDir without weights refuses [L7b]', () => {
      const err = capture(() =>
        open('fixtures/structured.pdf').extractMarkdown({
          tableModelDir: tmpDir('empty-tables'),
        }),
      );
      expect(err).toBeInstanceOf(mod.PdfluentError);
      // Without the `ort-tables` build the options error names it;
      // otherwise the typed model error names the `tatr-v1.1-all` path.
      if (err.code === 'E-UNSUPPORTED') {
        expect(err.message).toMatch('ort-tables');
        return;
      }
      expect(err.code).toBe('E-IO-GENERIC');
      expect(err.message).toMatch(NOT_AVAILABLE);
      expect(err.message).toMatch(TATR_MODEL);
      expect(err.message).toMatch(INSTALL_TABLES);
    });
  });
}
