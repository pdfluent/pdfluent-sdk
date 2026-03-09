/**
 * DOM structure tests — verify that index.html has all required UI elements.
 *
 * These tests load the HTML file and check that all expected DOM elements
 * exist with the correct IDs, classes, and attributes.
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { readFileSync } from 'fs';
import { resolve } from 'path';

let doc;

beforeAll(() => {
  const html = readFileSync(
    resolve(__dirname, '..', 'index.html'),
    'utf-8'
  );
  doc = new DOMParser().parseFromString(html, 'text/html');
});

describe('toolbar', () => {
  it('has open button', () => {
    expect(doc.getElementById('btn-open')).not.toBeNull();
  });

  it('has navigation buttons', () => {
    expect(doc.getElementById('btn-prev')).not.toBeNull();
    expect(doc.getElementById('btn-next')).not.toBeNull();
  });

  it('has page indicator', () => {
    expect(doc.getElementById('page-input')).not.toBeNull();
    expect(doc.getElementById('page-total')).not.toBeNull();
  });

  it('has zoom controls', () => {
    expect(doc.getElementById('btn-zoom-in')).not.toBeNull();
    expect(doc.getElementById('btn-zoom-out')).not.toBeNull();
    expect(doc.getElementById('zoom-select')).not.toBeNull();
  });

  it('zoom select has expected options', () => {
    const select = doc.getElementById('zoom-select');
    const options = Array.from(select.querySelectorAll('option'));
    const values = options.map((o) => o.value);

    expect(values).toContain('fit-width');
    expect(values).toContain('fit-page');
    expect(values).toContain('100');
    expect(values).toContain('200');
  });

  it('has annotation toggle button', () => {
    expect(doc.getElementById('btn-annotate')).not.toBeNull();
  });
});

describe('annotation toolbar', () => {
  it('exists and is hidden by default', () => {
    const bar = doc.getElementById('annot-toolbar');
    expect(bar).not.toBeNull();
    expect(bar.classList.contains('hidden')).toBe(true);
  });

  it('has all annotation tool buttons', () => {
    const tools = [
      'select',
      'highlight',
      'underline',
      'strikeout',
      'freetext',
      'stickynote',
      'ink',
      'rectangle',
      'circle',
      'line',
      'arrow',
      'stamp',
    ];

    for (const tool of tools) {
      const btn = doc.querySelector(`[data-tool="${tool}"]`);
      expect(btn, `Missing tool button: ${tool}`).not.toBeNull();
    }
  });

  it('has annotation property controls', () => {
    expect(doc.getElementById('annot-color')).not.toBeNull();
    expect(doc.getElementById('annot-opacity')).not.toBeNull();
    expect(doc.getElementById('annot-border-width')).not.toBeNull();
    expect(doc.getElementById('annot-fontsize')).not.toBeNull();
    expect(doc.getElementById('annot-stamp-name')).not.toBeNull();
  });

  it('has delete button (disabled)', () => {
    const del = doc.getElementById('annot-delete');
    expect(del).not.toBeNull();
    expect(del.disabled).toBe(true);
  });
});

describe('sidebar', () => {
  it('has sidebar with panels', () => {
    expect(doc.getElementById('sidebar')).not.toBeNull();
    expect(doc.getElementById('thumbnail-panel')).not.toBeNull();
    expect(doc.getElementById('bookmark-panel')).not.toBeNull();
  });

  it('has sidebar tabs', () => {
    const tabs = doc.querySelectorAll('.sidebar-tab');
    expect(tabs.length).toBe(2);
    const panels = Array.from(tabs).map((t) => t.dataset.panel);
    expect(panels).toContain('thumbnails');
    expect(panels).toContain('bookmarks');
  });

  it('has resizer handle', () => {
    expect(doc.getElementById('sidebar-resizer')).not.toBeNull();
  });
});

describe('viewport', () => {
  it('has viewport and page container', () => {
    expect(doc.getElementById('viewport')).not.toBeNull();
    expect(doc.getElementById('viewport-scroll')).not.toBeNull();
    expect(doc.getElementById('page-container')).not.toBeNull();
  });

  it('has empty state', () => {
    const empty = doc.getElementById('empty-state');
    expect(empty).not.toBeNull();
    expect(empty.textContent).toContain('Open a PDF');
  });
});

describe('search bar', () => {
  it('has search UI elements', () => {
    expect(doc.getElementById('search-bar')).not.toBeNull();
    expect(doc.getElementById('search-input')).not.toBeNull();
    expect(doc.getElementById('search-results')).not.toBeNull();
    expect(doc.getElementById('search-prev')).not.toBeNull();
    expect(doc.getElementById('search-next')).not.toBeNull();
    expect(doc.getElementById('search-close')).not.toBeNull();
  });

  it('search bar is hidden by default', () => {
    const bar = doc.getElementById('search-bar');
    expect(bar.classList.contains('hidden')).toBe(true);
  });
});

describe('tab bar', () => {
  it('has tab container', () => {
    expect(doc.getElementById('tab-bar')).not.toBeNull();
    expect(doc.getElementById('tabs')).not.toBeNull();
  });
});
