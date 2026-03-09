/**
 * Keyboard shortcut tests — verify shortcut-to-action mapping.
 *
 * Tests the keyboard shortcut dispatch logic extracted from main.js.
 * Table-driven: each entry maps a key combination to the expected action.
 */

import { describe, it, expect } from 'vitest';

/**
 * Resolve a keyboard event to an action name.
 *
 * This is a pure-function extraction of the keyboard shortcut logic
 * from main.js setupKeyboardShortcuts(), enabling table-driven testing.
 */
function resolveShortcut(key, { ctrl = false, shift = false, inInput = false, annotToolbar = false, searchVisible = false } = {}) {
  const mod = ctrl;

  // Modifier shortcuts (always active).
  if (mod && key === 'o') return 'open';
  if (mod && key === 'w') return 'close_tab';
  if (mod && key === '=') return 'zoom_in';
  if (mod && key === '-') return 'zoom_out';
  if (mod && key === '0') return 'fit_page';
  if (mod && key === '1') return 'fit_width';
  if (mod && key === '2') return 'fit_page';
  if (mod && key === 'z' && !shift) return 'undo';
  if (mod && key === 'z' && shift) return 'redo';
  if (mod && key === 'Z') return 'redo';
  if (mod && key === 'y') return 'redo';
  if (mod && key === 's' && !shift) return 'save';
  if (mod && shift && key === 'S') return 'save_as';
  if (mod && key === 'p') return 'print';
  if (mod && key === 'c') return 'copy';
  if (mod && key === 'a') return 'select_all';
  if (mod && key === 'f') return 'find';
  if (mod && key === 'b') return 'toggle_sidebar';
  if (mod && key === 'd') return 'toggle_dark_mode';
  if (mod && key === 'g') return 'go_to_page';
  if (mod && key === 'i') return 'document_info';

  // Non-modifier shortcuts (only when not in an input field).
  if (!inInput && !mod) {
    if (key === '+' || key === '=') return 'zoom_in';
    if (key === '-') return 'zoom_out';
    if (key === 'ArrowLeft') return 'prev_page';
    if (key === 'ArrowRight') return 'next_page';
    if (key === 'PageUp') return 'prev_page';
    if (key === 'PageDown') return 'next_page';
    if (key === 'Home') return 'first_page';
    if (key === 'End') return 'last_page';
    if (key === 'Escape' && annotToolbar) return 'cancel_annotation';
    if (key === 'Escape' && searchVisible) return 'toggle_search';
    if (key === 'v' && annotToolbar) return 'select_tool';
  }

  return null;
}

describe('modifier shortcuts (Ctrl/Cmd + key)', () => {
  const cases = [
    ['o', 'open'],
    ['w', 'close_tab'],
    ['=', 'zoom_in'],
    ['-', 'zoom_out'],
    ['0', 'fit_page'],
    ['1', 'fit_width'],
    ['2', 'fit_page'],
    ['s', 'save'],
    ['p', 'print'],
    ['c', 'copy'],
    ['a', 'select_all'],
    ['f', 'find'],
    ['b', 'toggle_sidebar'],
    ['d', 'toggle_dark_mode'],
    ['g', 'go_to_page'],
    ['i', 'document_info'],
  ];

  for (const [key, action] of cases) {
    it(`Ctrl+${key} → ${action}`, () => {
      expect(resolveShortcut(key, { ctrl: true })).toBe(action);
    });
  }
});

describe('undo/redo shortcuts', () => {
  it('Ctrl+Z → undo', () => {
    expect(resolveShortcut('z', { ctrl: true })).toBe('undo');
  });

  it('Ctrl+Shift+Z → redo', () => {
    expect(resolveShortcut('z', { ctrl: true, shift: true })).toBe('redo');
  });

  it('Ctrl+Z (uppercase) → redo', () => {
    expect(resolveShortcut('Z', { ctrl: true })).toBe('redo');
  });

  it('Ctrl+Y → redo', () => {
    expect(resolveShortcut('y', { ctrl: true })).toBe('redo');
  });
});

describe('save shortcuts', () => {
  it('Ctrl+S → save', () => {
    expect(resolveShortcut('s', { ctrl: true })).toBe('save');
  });

  it('Ctrl+Shift+S → save_as', () => {
    expect(resolveShortcut('S', { ctrl: true, shift: true })).toBe('save_as');
  });
});

describe('navigation shortcuts (no modifier)', () => {
  const cases = [
    ['ArrowLeft', 'prev_page'],
    ['ArrowRight', 'next_page'],
    ['PageUp', 'prev_page'],
    ['PageDown', 'next_page'],
    ['Home', 'first_page'],
    ['End', 'last_page'],
  ];

  for (const [key, action] of cases) {
    it(`${key} → ${action}`, () => {
      expect(resolveShortcut(key)).toBe(action);
    });
  }
});

describe('zoom shortcuts (no modifier)', () => {
  it('+ → zoom_in', () => {
    expect(resolveShortcut('+')).toBe('zoom_in');
  });

  it('= → zoom_in', () => {
    expect(resolveShortcut('=')).toBe('zoom_in');
  });

  it('- → zoom_out', () => {
    expect(resolveShortcut('-')).toBe('zoom_out');
  });
});

describe('annotation shortcuts', () => {
  it('Escape cancels annotation when toolbar visible', () => {
    expect(resolveShortcut('Escape', { annotToolbar: true })).toBe('cancel_annotation');
  });

  it('V selects select tool when toolbar visible', () => {
    expect(resolveShortcut('v', { annotToolbar: true })).toBe('select_tool');
  });

  it('Escape does not cancel when toolbar hidden', () => {
    expect(resolveShortcut('Escape', { annotToolbar: false, searchVisible: false })).toBeNull();
  });
});

describe('search shortcuts', () => {
  it('Escape closes search when visible', () => {
    expect(resolveShortcut('Escape', { searchVisible: true })).toBe('toggle_search');
  });

  it('Ctrl+F opens find', () => {
    expect(resolveShortcut('f', { ctrl: true })).toBe('find');
  });
});

describe('input field suppression', () => {
  it('navigation keys are suppressed in input fields', () => {
    expect(resolveShortcut('ArrowLeft', { inInput: true })).toBeNull();
    expect(resolveShortcut('ArrowRight', { inInput: true })).toBeNull();
    expect(resolveShortcut('Home', { inInput: true })).toBeNull();
    expect(resolveShortcut('End', { inInput: true })).toBeNull();
  });

  it('modifier shortcuts still work in input fields', () => {
    expect(resolveShortcut('s', { ctrl: true, inInput: true })).toBe('save');
    expect(resolveShortcut('z', { ctrl: true, inInput: true })).toBe('undo');
    expect(resolveShortcut('o', { ctrl: true, inInput: true })).toBe('open');
  });
});

describe('unknown keys return null', () => {
  it('unbound key returns null', () => {
    expect(resolveShortcut('x')).toBeNull();
    expect(resolveShortcut('q', { ctrl: true })).toBeNull();
    expect(resolveShortcut('F5')).toBeNull();
  });
});
