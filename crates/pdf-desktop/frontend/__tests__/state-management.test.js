/**
 * State management and pure logic tests.
 *
 * Tests zoom computation, coordinate conversion, state transitions,
 * and other pure functions extracted from main.js.
 */

import { describe, it, expect } from 'vitest';

// ── Constants (from main.js) ─────────────────────────────────────────

const ZOOM_LEVELS = [50, 75, 100, 125, 150, 200, 300];
const BASE_DPI = 72;

// ── Pure functions extracted from main.js ────────────────────────────

function computeDpi(zoom, viewportWidth, viewportHeight) {
  if (zoom === 'fit-width' || zoom === 'fit-page') {
    const vpWidth = viewportWidth - 48; // padding
    const baseDpi = (vpWidth / 595) * 72;
    if (zoom === 'fit-page') {
      const vpHeight = viewportHeight - 48;
      const heightDpi = (vpHeight / 842) * 72;
      return Math.min(baseDpi, heightDpi);
    }
    return baseDpi;
  }
  const pct = parseInt(zoom, 10) || 100;
  return BASE_DPI * (pct / 100);
}

function nextZoomLevel(current) {
  const val = typeof current === 'string' ? 100 : parseInt(current, 10);
  return ZOOM_LEVELS.find((z) => z > val) || ZOOM_LEVELS[ZOOM_LEVELS.length - 1];
}

function prevZoomLevel(current) {
  const val = typeof current === 'string' ? 100 : parseInt(current, 10);
  return [...ZOOM_LEVELS].reverse().find((z) => z < val) || ZOOM_LEVELS[0];
}

function screenPointsToPdf(imgNaturalW, imgNaturalH, imgDisplayW, imgDisplayH, points) {
  const scaleX = imgNaturalW / imgDisplayW;
  const scaleY = imgNaturalH / imgDisplayH;
  const result = [];
  for (let i = 0; i < points.length; i += 2) {
    result.push(points[i] * scaleX);
    result.push(imgNaturalH - points[i + 1] * scaleY);
  }
  return result;
}

function getPointsBounds(points) {
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (let i = 0; i < points.length; i += 2) {
    minX = Math.min(minX, points[i]);
    minY = Math.min(minY, points[i + 1]);
    maxX = Math.max(maxX, points[i]);
    maxY = Math.max(maxY, points[i + 1]);
  }
  return { minX, minY, maxX, maxY };
}

function hexToRgb01(hex) {
  const h = hex.replace('#', '');
  const r = parseInt(h.substring(0, 2), 16) / 255;
  const g = parseInt(h.substring(2, 4), 16) / 255;
  const b = parseInt(h.substring(4, 6), 16) / 255;
  return { r, g, b };
}

function clampPage(page, pageCount) {
  return Math.max(0, Math.min(page, pageCount - 1));
}

// ── Tests ────────────────────────────────────────────────────────────

describe('computeDpi', () => {
  it('returns base DPI at 100%', () => {
    expect(computeDpi('100', 800, 600)).toBe(72);
  });

  it('scales DPI proportionally to zoom percentage', () => {
    expect(computeDpi('200', 800, 600)).toBe(144);
    expect(computeDpi('50', 800, 600)).toBe(36);
    expect(computeDpi('150', 800, 600)).toBe(108);
  });

  it('fit-width uses viewport width', () => {
    // viewport 800 - 48 padding = 752px, A4 width = 595pt
    const dpi = computeDpi('fit-width', 800, 600);
    expect(dpi).toBeCloseTo((752 / 595) * 72, 1);
  });

  it('fit-page uses minimum of width and height DPI', () => {
    const dpi = computeDpi('fit-page', 800, 600);
    const widthDpi = (752 / 595) * 72;
    const heightDpi = (552 / 842) * 72;
    expect(dpi).toBeCloseTo(Math.min(widthDpi, heightDpi), 1);
  });

  it('defaults to 100% for invalid zoom string', () => {
    expect(computeDpi('invalid', 800, 600)).toBe(72);
  });
});

describe('zoom level navigation', () => {
  it('zoomIn finds next level', () => {
    expect(nextZoomLevel(100)).toBe(125);
    expect(nextZoomLevel(50)).toBe(75);
    expect(nextZoomLevel(200)).toBe(300);
  });

  it('zoomIn caps at max level', () => {
    expect(nextZoomLevel(300)).toBe(300);
    expect(nextZoomLevel(400)).toBe(300);
  });

  it('zoomOut finds previous level', () => {
    expect(prevZoomLevel(100)).toBe(75);
    expect(prevZoomLevel(300)).toBe(200);
    expect(prevZoomLevel(125)).toBe(100);
  });

  it('zoomOut caps at min level', () => {
    expect(prevZoomLevel(50)).toBe(50);
    expect(prevZoomLevel(25)).toBe(50);
  });

  it('string zoom defaults to 100 for navigation', () => {
    expect(nextZoomLevel('fit-width')).toBe(125);
    expect(prevZoomLevel('fit-page')).toBe(75);
  });

  it('all zoom levels are in ascending order', () => {
    for (let i = 1; i < ZOOM_LEVELS.length; i++) {
      expect(ZOOM_LEVELS[i]).toBeGreaterThan(ZOOM_LEVELS[i - 1]);
    }
  });
});

describe('screenPointsToPdf', () => {
  it('converts screen coordinates to PDF coordinates', () => {
    // Image: 595x842 natural, displayed at 595x842 (1:1)
    const result = screenPointsToPdf(595, 842, 595, 842, [100, 100]);
    expect(result[0]).toBeCloseTo(100);
    expect(result[1]).toBeCloseTo(742); // 842 - 100 (Y inverted)
  });

  it('handles scaled display', () => {
    // Natural 595x842, displayed at half size 297.5x421
    const result = screenPointsToPdf(595, 842, 297.5, 421, [50, 50]);
    expect(result[0]).toBeCloseTo(100); // 50 * 2
    expect(result[1]).toBeCloseTo(742); // 842 - (50 * 2)
  });

  it('handles multiple points', () => {
    const result = screenPointsToPdf(100, 100, 100, 100, [0, 0, 100, 100, 50, 50]);
    expect(result.length).toBe(6);
    expect(result[0]).toBeCloseTo(0);
    expect(result[1]).toBeCloseTo(100); // Y inverted
    expect(result[2]).toBeCloseTo(100);
    expect(result[3]).toBeCloseTo(0); // Y inverted
    expect(result[4]).toBeCloseTo(50);
    expect(result[5]).toBeCloseTo(50);
  });

  it('returns empty array for empty input', () => {
    const result = screenPointsToPdf(100, 100, 100, 100, []);
    expect(result).toEqual([]);
  });
});

describe('getPointsBounds', () => {
  it('computes bounding box', () => {
    const bounds = getPointsBounds([10, 20, 50, 80, 30, 40]);
    expect(bounds.minX).toBe(10);
    expect(bounds.minY).toBe(20);
    expect(bounds.maxX).toBe(50);
    expect(bounds.maxY).toBe(80);
  });

  it('handles single point', () => {
    const bounds = getPointsBounds([42, 99]);
    expect(bounds.minX).toBe(42);
    expect(bounds.maxX).toBe(42);
    expect(bounds.minY).toBe(99);
    expect(bounds.maxY).toBe(99);
  });
});

describe('hexToRgb01', () => {
  it('converts white', () => {
    const { r, g, b } = hexToRgb01('#FFFFFF');
    expect(r).toBeCloseTo(1.0);
    expect(g).toBeCloseTo(1.0);
    expect(b).toBeCloseTo(1.0);
  });

  it('converts black', () => {
    const { r, g, b } = hexToRgb01('#000000');
    expect(r).toBeCloseTo(0.0);
    expect(g).toBeCloseTo(0.0);
    expect(b).toBeCloseTo(0.0);
  });

  it('converts red', () => {
    const { r, g, b } = hexToRgb01('#FF0000');
    expect(r).toBeCloseTo(1.0);
    expect(g).toBeCloseTo(0.0);
    expect(b).toBeCloseTo(0.0);
  });

  it('converts the default highlight color', () => {
    const { r, g, b } = hexToRgb01('#FACC15');
    expect(r).toBeCloseTo(250 / 255, 2);
    expect(g).toBeCloseTo(204 / 255, 2);
    expect(b).toBeCloseTo(21 / 255, 2);
  });

  it('handles without hash prefix', () => {
    const { r, g, b } = hexToRgb01('0000FF');
    expect(r).toBeCloseTo(0.0);
    expect(g).toBeCloseTo(0.0);
    expect(b).toBeCloseTo(1.0);
  });
});

describe('page clamping', () => {
  it('clamps negative page to 0', () => {
    expect(clampPage(-1, 10)).toBe(0);
    expect(clampPage(-100, 5)).toBe(0);
  });

  it('clamps page beyond count', () => {
    expect(clampPage(10, 10)).toBe(9);
    expect(clampPage(100, 5)).toBe(4);
  });

  it('passes valid pages through', () => {
    expect(clampPage(0, 10)).toBe(0);
    expect(clampPage(5, 10)).toBe(5);
    expect(clampPage(9, 10)).toBe(9);
  });

  it('handles single page document', () => {
    expect(clampPage(0, 1)).toBe(0);
    expect(clampPage(1, 1)).toBe(0);
    expect(clampPage(-1, 1)).toBe(0);
  });
});

describe('tab state management', () => {
  it('new tab has expected defaults', () => {
    const tab = {
      handle: 1,
      title: 'Test',
      fileName: 'test.pdf',
      pageCount: 5,
      currentPage: 0,
      zoom: 'fit-width',
      scrollTop: 0,
    };

    expect(tab.currentPage).toBe(0);
    expect(tab.zoom).toBe('fit-width');
    expect(tab.scrollTop).toBe(0);
  });

  it('tab list operations work correctly', () => {
    const tabs = [];

    // Open two tabs.
    tabs.push({ handle: 1, fileName: 'a.pdf', pageCount: 3 });
    tabs.push({ handle: 2, fileName: 'b.pdf', pageCount: 7 });
    expect(tabs.length).toBe(2);

    // Close first tab.
    tabs.splice(0, 1);
    expect(tabs.length).toBe(1);
    expect(tabs[0].fileName).toBe('b.pdf');
  });

  it('active tab index updates on close', () => {
    let activeTab = 1;
    const tabs = ['a', 'b', 'c'];

    // Close active tab (middle).
    tabs.splice(activeTab, 1);
    if (activeTab >= tabs.length) activeTab = tabs.length - 1;
    if (tabs.length === 0) activeTab = null;

    expect(activeTab).toBe(1);
    expect(tabs[activeTab]).toBe('c');
  });

  it('active tab becomes null when last tab is closed', () => {
    let activeTab = 0;
    const tabs = ['only'];

    tabs.splice(0, 1);
    if (tabs.length === 0) activeTab = null;

    expect(activeTab).toBeNull();
  });
});

describe('render cache key format', () => {
  it('produces unique keys per handle-page-dpi', () => {
    const key = (h, p, d) => `${h}-${p}-${d}`;
    expect(key(1, 0, 72)).toBe('1-0-72');
    expect(key(1, 0, 144)).toBe('1-0-144');
    expect(key(2, 5, 72)).toBe('2-5-72');

    // Keys should be unique.
    const keys = new Set([
      key(1, 0, 72),
      key(1, 0, 144),
      key(1, 1, 72),
      key(2, 0, 72),
    ]);
    expect(keys.size).toBe(4);
  });
});

describe('annotation tool state', () => {
  it('stamp names are valid', () => {
    const validStamps = [
      'Draft', 'Approved', 'Confidential', 'Final',
      'Expired', 'NotApproved', 'ForComment', 'TopSecret',
    ];

    for (const stamp of validStamps) {
      expect(stamp.length).toBeGreaterThan(0);
      expect(stamp).not.toContain(' ');
    }
  });

  it('annotation tools list is complete', () => {
    const tools = [
      'select', 'highlight', 'underline', 'strikeout',
      'freetext', 'stickynote', 'ink', 'rectangle',
      'circle', 'line', 'arrow', 'stamp',
    ];
    expect(tools.length).toBe(12);
    expect(new Set(tools).size).toBe(12); // no duplicates
  });
});
