// XFA PDF Viewer — Frontend Application
// Communicates with the Tauri Rust backend via IPC.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ── Application State ───────────────────────────────────────────────

const state = {
  tabs: [],          // { handle, title, fileName, pageCount, currentPage, zoom, scrollTop }
  activeTab: null,   // index into tabs
  sidebarVisible: true,
  sidebarPanel: 'thumbnails',
  renderCache: {},   // `${handle}-${page}-${dpi}` → base64
  thumbnailCache: {},// `${handle}-${page}` → base64
  darkMode: false,
  annotToolbarVisible: false,
  annotTool: 'select',        // current annotation tool
  annotDrawing: false,        // currently drawing?
  annotStartX: 0,
  annotStartY: 0,
  inkPoints: [],              // current ink stroke points
};

const ZOOM_LEVELS = [50, 75, 100, 125, 150, 200, 300];
const BASE_DPI = 72;

// ── DOM References ──────────────────────────────────────────────────

const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => document.querySelectorAll(sel);

const dom = {
  tabs: $('#tabs'),
  toolbar: $('#toolbar'),
  btnOpen: $('#btn-open'),
  btnPrev: $('#btn-prev'),
  btnNext: $('#btn-next'),
  pageInput: $('#page-input'),
  pageTotal: $('#page-total'),
  zoomSelect: $('#zoom-select'),
  btnZoomIn: $('#btn-zoom-in'),
  btnZoomOut: $('#btn-zoom-out'),
  sidebar: $('#sidebar'),
  thumbnailPanel: $('#thumbnail-panel'),
  bookmarkPanel: $('#bookmark-panel'),
  viewport: $('#viewport'),
  viewportScroll: $('#viewport-scroll'),
  pageContainer: $('#page-container'),
  emptyState: $('#empty-state'),
};

// ── Initialization ──────────────────────────────────────────────────

async function init() {
  setupToolbarEvents();
  setupSidebarEvents();
  setupKeyboardShortcuts();
  setupDragDrop();
  setupMenuEvents();
  setupSearchEvents();
  setupAnnotationToolbar();
  detectDarkModePreference();
  updateUI();
}

// ── Document Management ─────────────────────────────────────────────

async function openFile(path) {
  try {
    const result = await invoke('open_document', { path, password: null });
    const tab = {
      handle: result.handle,
      title: result.title,
      fileName: result.file_name,
      pageCount: result.page_count,
      currentPage: 0,
      zoom: 'fit-width',
      scrollTop: 0,
    };
    state.tabs.push(tab);
    state.activeTab = state.tabs.length - 1;
    updateUI();
    await renderCurrentView();
    await loadThumbnails(tab.handle, tab.pageCount);
    await loadBookmarks(tab.handle);
  } catch (err) {
    console.error('Failed to open document:', err);
  }
}

async function closeTab(index) {
  const tab = state.tabs[index];
  if (!tab) return;

  // Confirm close if there are unsaved changes.
  if (tab.dirty) {
    const ok = confirm(`"${tab.fileName}" has unsaved changes. Close without saving?`);
    if (!ok) return;
  }

  try {
    await invoke('close_document', { handle: tab.handle });
  } catch (_) { /* ignore */ }

  // Clean caches
  for (const key of Object.keys(state.renderCache)) {
    if (key.startsWith(`${tab.handle}-`)) delete state.renderCache[key];
  }
  for (const key of Object.keys(state.thumbnailCache)) {
    if (key.startsWith(`${tab.handle}-`)) delete state.thumbnailCache[key];
  }

  state.tabs.splice(index, 1);
  if (state.tabs.length === 0) {
    state.activeTab = null;
  } else if (state.activeTab >= state.tabs.length) {
    state.activeTab = state.tabs.length - 1;
  }
  updateUI();
  if (state.activeTab !== null) {
    await renderCurrentView();
    const t = activeTab();
    await loadThumbnails(t.handle, t.pageCount);
    await loadBookmarks(t.handle);
  }
}

function activeTab() {
  if (state.activeTab === null) return null;
  return state.tabs[state.activeTab];
}

// ── Rendering ───────────────────────────────────────────────────────

async function renderCurrentView() {
  const tab = activeTab();
  if (!tab) return;

  const dpi = computeDpi(tab);
  dom.pageContainer.innerHTML = '';

  // Render visible pages (current page + neighbors for smooth scrolling).
  const start = Math.max(0, tab.currentPage - 1);
  const end = Math.min(tab.pageCount - 1, tab.currentPage + 2);

  for (let i = start; i <= end; i++) {
    const wrapper = document.createElement('div');
    wrapper.className = 'page-wrapper';
    wrapper.dataset.page = i;

    const img = document.createElement('img');
    img.alt = `Page ${i + 1}`;

    const cacheKey = `${tab.handle}-${i}-${Math.round(dpi)}`;
    if (state.renderCache[cacheKey]) {
      img.src = `data:image/png;base64,${state.renderCache[cacheKey]}`;
    } else {
      // Show placeholder while loading
      img.style.width = '600px';
      img.style.height = '800px';
      img.style.background = '#f0f0f0';

      renderPageAsync(tab.handle, i, dpi, cacheKey).then(b64 => {
        if (b64) {
          img.src = `data:image/png;base64,${b64}`;
          img.style.width = '';
          img.style.height = '';
          img.style.background = '';
        }
      });
    }

    wrapper.appendChild(img);
    dom.pageContainer.appendChild(wrapper);
  }
}

async function renderPageAsync(handle, pageIndex, dpi, cacheKey) {
  try {
    const b64 = await invoke('render_page', { handle, pageIndex, dpi });
    state.renderCache[cacheKey] = b64;
    return b64;
  } catch (err) {
    console.error(`Failed to render page ${pageIndex}:`, err);
    return null;
  }
}

function computeDpi(tab) {
  if (tab.zoom === 'fit-width' || tab.zoom === 'fit-page') {
    // Use viewport width to compute approximate DPI.
    const vpWidth = dom.viewport.clientWidth - 48; // padding
    // Assume A4 width = 595pt at 72 DPI → vpWidth pixels
    const baseDpi = (vpWidth / 595) * 72;
    if (tab.zoom === 'fit-page') {
      const vpHeight = dom.viewport.clientHeight - 48;
      const heightDpi = (vpHeight / 842) * 72;
      return Math.min(baseDpi, heightDpi);
    }
    return baseDpi;
  }
  const pct = parseInt(tab.zoom, 10) || 100;
  return BASE_DPI * (pct / 100);
}

// ── Thumbnails (lazy loading via IntersectionObserver) ──────────────

let thumbnailObserver = null;

function loadThumbnails(handle, pageCount) {
  dom.thumbnailPanel.innerHTML = '';

  // Disconnect previous observer
  if (thumbnailObserver) thumbnailObserver.disconnect();

  // Create IntersectionObserver for lazy loading
  thumbnailObserver = new IntersectionObserver((entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      const item = entry.target;
      const img = item.querySelector('img');
      if (img.dataset.loaded === 'true') continue;

      const pageIndex = parseInt(item.dataset.page, 10);
      const tab = activeTab();
      if (!tab) continue;

      const thumbKey = `${tab.handle}-${pageIndex}`;
      if (state.thumbnailCache[thumbKey]) {
        applyThumbnail(img, state.thumbnailCache[thumbKey]);
      } else {
        loadThumbnailAsync(tab.handle, pageIndex, img, thumbKey);
      }
    }
  }, { root: dom.thumbnailPanel, rootMargin: '200px' });

  for (let i = 0; i < pageCount; i++) {
    const item = document.createElement('div');
    item.className = 'thumbnail-item';
    if (i === (activeTab()?.currentPage ?? 0)) item.classList.add('active');
    item.dataset.page = i;

    const img = document.createElement('img');
    img.alt = `Page ${i + 1}`;
    img.dataset.loaded = 'false';
    img.style.width = '150px';
    img.style.height = '200px';
    img.style.background = 'var(--bg-tertiary)';

    const label = document.createElement('div');
    label.className = 'thumbnail-label';
    label.textContent = i + 1;

    item.appendChild(img);
    item.appendChild(label);
    item.addEventListener('click', () => goToPage(i));
    item.addEventListener('contextmenu', (e) => showThumbnailContextMenu(e, i));
    dom.thumbnailPanel.appendChild(item);

    // Observe for lazy loading
    thumbnailObserver.observe(item);
  }
}

function applyThumbnail(img, b64) {
  img.src = `data:image/png;base64,${b64}`;
  img.style.width = '';
  img.style.height = '';
  img.style.background = '';
  img.dataset.loaded = 'true';
}

async function loadThumbnailAsync(handle, pageIndex, imgElement, cacheKey) {
  try {
    const b64 = await invoke('render_thumbnail', { handle, pageIndex });
    state.thumbnailCache[cacheKey] = b64;
    applyThumbnail(imgElement, b64);
  } catch (err) {
    console.error(`Failed to load thumbnail ${pageIndex}:`, err);
  }
}

function updateThumbnailHighlight() {
  const tab = activeTab();
  if (!tab) return;
  $$('.thumbnail-item').forEach((el) => {
    el.classList.toggle('active', parseInt(el.dataset.page) === tab.currentPage);
  });
  const active = $('.thumbnail-item.active');
  if (active) active.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
}

// ── Thumbnail Context Menu ──────────────────────────────────────────

function showThumbnailContextMenu(event, pageIndex) {
  event.preventDefault();
  hideContextMenu();

  const tab = activeTab();
  if (!tab) return;

  const menu = document.createElement('div');
  menu.className = 'context-menu';
  menu.style.left = event.clientX + 'px';
  menu.style.top = event.clientY + 'px';

  const items = [
    { label: `Go to Page ${pageIndex + 1}`, action: () => goToPage(pageIndex) },
    { label: 'separator' },
    { label: 'Rotate Clockwise', action: () => rotatePage(pageIndex, 90) },
    { label: 'Rotate Counter-clockwise', action: () => rotatePage(pageIndex, 270) },
    { label: 'separator' },
    { label: 'Delete Page', action: () => deletePage(pageIndex), danger: true },
  ];

  for (const item of items) {
    if (item.label === 'separator') {
      const sep = document.createElement('div');
      sep.className = 'context-menu-separator';
      menu.appendChild(sep);
      continue;
    }
    const el = document.createElement('div');
    el.className = 'context-menu-item' + (item.danger ? ' danger' : '');
    el.textContent = item.label;
    el.addEventListener('click', () => {
      hideContextMenu();
      item.action();
    });
    menu.appendChild(el);
  }

  document.body.appendChild(menu);

  // Ensure menu stays in viewport
  const rect = menu.getBoundingClientRect();
  if (rect.right > window.innerWidth) menu.style.left = (window.innerWidth - rect.width - 4) + 'px';
  if (rect.bottom > window.innerHeight) menu.style.top = (window.innerHeight - rect.height - 4) + 'px';

  // Close on click outside
  setTimeout(() => {
    document.addEventListener('click', hideContextMenu, { once: true });
  }, 0);
}

function hideContextMenu() {
  const existing = $('.context-menu');
  if (existing) existing.remove();
}

async function rotatePage(pageIndex, degrees) {
  const tab = activeTab();
  if (!tab) return;
  try {
    await invoke('rotate_page', { handle: tab.handle, pageIndex, degrees });
    invalidateCaches(tab.handle, pageIndex);
    await refreshAfterPageChange(tab);
  } catch (err) {
    console.error('Rotate failed:', err);
  }
}

async function deletePage(pageIndex) {
  const tab = activeTab();
  if (!tab || tab.pageCount <= 1) return;
  try {
    await invoke('delete_page', { handle: tab.handle, pageIndex });
    tab.pageCount--;
    if (tab.currentPage >= tab.pageCount) tab.currentPage = tab.pageCount - 1;
    invalidateCaches(tab.handle);
    await refreshAfterPageChange(tab);
  } catch (err) {
    console.error('Delete failed:', err);
  }
}

function invalidateCaches(handle, specificPage) {
  if (specificPage !== undefined) {
    // Invalidate specific page
    for (const key of Object.keys(state.renderCache)) {
      if (key.startsWith(`${handle}-${specificPage}-`)) delete state.renderCache[key];
    }
    delete state.thumbnailCache[`${handle}-${specificPage}`];
  } else {
    // Invalidate all pages for this handle
    for (const key of Object.keys(state.renderCache)) {
      if (key.startsWith(`${handle}-`)) delete state.renderCache[key];
    }
    for (const key of Object.keys(state.thumbnailCache)) {
      if (key.startsWith(`${handle}-`)) delete state.thumbnailCache[key];
    }
  }
}

async function refreshAfterPageChange(tab) {
  updatePageIndicator();
  await renderCurrentView();
  loadThumbnails(tab.handle, tab.pageCount);
}

// ── Bookmarks ───────────────────────────────────────────────────────

async function loadBookmarks(handle) {
  dom.bookmarkPanel.innerHTML = '';
  try {
    const bookmarks = await invoke('get_bookmarks', { handle });
    if (bookmarks.length === 0) {
      dom.bookmarkPanel.innerHTML = '<p style="padding:8px;color:var(--text-muted);font-size:12px;">No bookmarks</p>';
      return;
    }
    renderBookmarkTree(bookmarks, dom.bookmarkPanel);
  } catch (err) {
    console.error('Failed to load bookmarks:', err);
  }
}

function renderBookmarkTree(items, container) {
  for (const item of items) {
    const div = document.createElement('div');
    div.className = 'bookmark-item';
    div.textContent = item.title;
    div.addEventListener('click', () => goToPage(item.page));
    container.appendChild(div);
    if (item.children && item.children.length > 0) {
      const childContainer = document.createElement('div');
      childContainer.className = 'bookmark-children';
      renderBookmarkTree(item.children, childContainer);
      container.appendChild(childContainer);
    }
  }
}

// ── Navigation ──────────────────────────────────────────────────────

async function goToPage(pageIndex) {
  const tab = activeTab();
  if (!tab) return;
  pageIndex = Math.max(0, Math.min(pageIndex, tab.pageCount - 1));
  if (pageIndex === tab.currentPage) return;
  tab.currentPage = pageIndex;
  updatePageIndicator();
  updateThumbnailHighlight();
  // Clear cache for non-adjacent pages to manage memory
  await renderCurrentView();
}

function updatePageIndicator() {
  const tab = activeTab();
  dom.pageInput.value = tab ? tab.currentPage + 1 : '';
  dom.pageTotal.textContent = tab ? tab.pageCount : '0';
  dom.btnPrev.disabled = !tab || tab.currentPage <= 0;
  dom.btnNext.disabled = !tab || tab.currentPage >= tab.pageCount - 1;
}

// ── Zoom ────────────────────────────────────────────────────────────

async function setZoom(value) {
  const tab = activeTab();
  if (!tab) return;
  tab.zoom = value;
  dom.zoomSelect.value = value;
  await renderCurrentView();
}

function zoomIn() {
  const tab = activeTab();
  if (!tab) return;
  const current = typeof tab.zoom === 'string' ? 100 : parseInt(tab.zoom, 10);
  const next = ZOOM_LEVELS.find(z => z > current) || ZOOM_LEVELS[ZOOM_LEVELS.length - 1];
  setZoom(String(next));
}

function zoomOut() {
  const tab = activeTab();
  if (!tab) return;
  const current = typeof tab.zoom === 'string' ? 100 : parseInt(tab.zoom, 10);
  const next = [...ZOOM_LEVELS].reverse().find(z => z < current) || ZOOM_LEVELS[0];
  setZoom(String(next));
}

// ── UI Update ───────────────────────────────────────────────────────

function updateUI() {
  renderTabs();
  updatePageIndicator();

  const hasDoc = state.activeTab !== null;
  dom.emptyState.classList.toggle('hidden', hasDoc);
  dom.viewportScroll.style.display = hasDoc ? 'block' : 'none';

  // Update toolbar state
  dom.btnPrev.disabled = !hasDoc;
  dom.btnNext.disabled = !hasDoc;
  dom.btnZoomIn.disabled = !hasDoc;
  dom.btnZoomOut.disabled = !hasDoc;
  dom.zoomSelect.disabled = !hasDoc;
  dom.pageInput.disabled = !hasDoc;

  if (hasDoc) {
    const tab = activeTab();
    dom.zoomSelect.value = tab.zoom;
  }

  // Show annotation toggle only when doc is open
  const btnAnnotate = $('#btn-annotate');
  if (btnAnnotate) btnAnnotate.disabled = !hasDoc;
  const deleteBtn = $('#annot-delete');
  if (deleteBtn) deleteBtn.disabled = !hasDoc;
}

function renderTabs() {
  dom.tabs.innerHTML = '';
  state.tabs.forEach((tab, i) => {
    const el = document.createElement('div');
    el.className = 'tab' + (i === state.activeTab ? ' active' : '');

    const title = document.createElement('span');
    title.className = 'tab-title';
    title.textContent = (tab.dirty ? '* ' : '') + tab.fileName;
    title.title = tab.title;

    const close = document.createElement('button');
    close.className = 'tab-close';
    close.textContent = '\u00D7';
    close.title = 'Close';
    close.addEventListener('click', (e) => {
      e.stopPropagation();
      closeTab(i);
    });

    el.appendChild(title);
    el.appendChild(close);
    el.addEventListener('click', async () => {
      if (state.activeTab === i) return;
      state.activeTab = i;
      updateUI();
      await renderCurrentView();
      await loadThumbnails(tab.handle, tab.pageCount);
      await loadBookmarks(tab.handle);
    });

    dom.tabs.appendChild(el);
  });
}

// ── Event Handlers ──────────────────────────────────────────────────

function setupToolbarEvents() {
  dom.btnOpen.addEventListener('click', promptOpen);
  dom.btnPrev.addEventListener('click', () => {
    const tab = activeTab();
    if (tab) goToPage(tab.currentPage - 1);
  });
  dom.btnNext.addEventListener('click', () => {
    const tab = activeTab();
    if (tab) goToPage(tab.currentPage + 1);
  });
  dom.pageInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      const val = parseInt(dom.pageInput.value, 10);
      if (!isNaN(val)) goToPage(val - 1);
    }
  });
  dom.zoomSelect.addEventListener('change', () => setZoom(dom.zoomSelect.value));
  dom.btnZoomIn.addEventListener('click', zoomIn);
  dom.btnZoomOut.addEventListener('click', zoomOut);
  const btnAnnotate = $('#btn-annotate');
  if (btnAnnotate) btnAnnotate.addEventListener('click', toggleAnnotationToolbar);
}

function setupSidebarEvents() {
  // Sidebar tab switching
  $$('.sidebar-tab').forEach(btn => {
    btn.addEventListener('click', () => {
      $$('.sidebar-tab').forEach(b => b.classList.remove('active'));
      $$('.sidebar-panel').forEach(p => p.classList.remove('active'));
      btn.classList.add('active');
      const panel = btn.dataset.panel;
      $(`#${panel}-panel`).classList.add('active');
      state.sidebarPanel = panel;
    });
  });

  // Sidebar resizer
  const resizer = $('#sidebar-resizer');
  let resizing = false;
  resizer.addEventListener('mousedown', (e) => {
    resizing = true;
    e.preventDefault();
  });
  document.addEventListener('mousemove', (e) => {
    if (!resizing) return;
    const newWidth = Math.max(150, Math.min(400, e.clientX));
    dom.sidebar.style.width = newWidth + 'px';
  });
  document.addEventListener('mouseup', () => { resizing = false; });
}

function setupKeyboardShortcuts() {
  document.addEventListener('keydown', (e) => {
    // Don't intercept shortcuts when typing in an input field.
    const inInput = e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.tagName === 'SELECT';
    const mod = e.metaKey || e.ctrlKey;

    if (mod && e.key === 'o') { e.preventDefault(); promptOpen(); }
    if (mod && e.key === 'w') { e.preventDefault(); if (state.activeTab !== null) closeTab(state.activeTab); }
    if (mod && e.key === '=') { e.preventDefault(); zoomIn(); }
    if (mod && e.key === '-') { e.preventDefault(); zoomOut(); }
    if (mod && e.key === '0') { e.preventDefault(); setZoom('fit-page'); }
    if (mod && e.key === '1') { e.preventDefault(); setZoom('fit-width'); }
    if (mod && e.key === '2') { e.preventDefault(); setZoom('fit-page'); }
    if (mod && e.key === 'z' && !e.shiftKey) { e.preventDefault(); undoAction(); }
    if (mod && e.key === 'z' && e.shiftKey) { e.preventDefault(); redoAction(); }
    if (mod && e.key === 'Z') { e.preventDefault(); redoAction(); }
    if (mod && e.key === 'y') { e.preventDefault(); redoAction(); }
    if (mod && e.key === 's' && !e.shiftKey) { e.preventDefault(); saveDocument(); }
    if (mod && e.key === 'p') { e.preventDefault(); printDocument(); }
    if (mod && e.shiftKey && e.key === 'S') { e.preventDefault(); saveDocumentAs(); }
    if (mod && e.key === 'c') { e.preventDefault(); copyPageText(); }
    if (mod && e.key === 'a') { e.preventDefault(); selectAllText(); }
    if (mod && e.key === 'f') { e.preventDefault(); toggleSearch(); }
    if (mod && e.key === 'b') { e.preventDefault(); toggleSidebar(); }
    if (mod && e.key === 'd') { e.preventDefault(); toggleDarkMode(); }
    if (mod && e.key === 'g') { e.preventDefault(); focusGoToPage(); }
    if (mod && e.key === 'i') { e.preventDefault(); showDocumentInfo(); }

    // Non-modifier shortcuts (only when not in an input field)
    if (!inInput && !mod) {
      if (e.key === '+' || e.key === '=') { e.preventDefault(); zoomIn(); }
      if (e.key === '-') { e.preventDefault(); zoomOut(); }
      if (e.key === 'ArrowLeft') { const t = activeTab(); if (t) goToPage(t.currentPage - 1); }
      if (e.key === 'ArrowRight') { const t = activeTab(); if (t) goToPage(t.currentPage + 1); }
      if (e.key === 'PageUp') { e.preventDefault(); const t = activeTab(); if (t) goToPage(t.currentPage - 1); }
      if (e.key === 'PageDown') { e.preventDefault(); const t = activeTab(); if (t) goToPage(t.currentPage + 1); }
      if (e.key === 'Home') { e.preventDefault(); goToPage(0); }
      if (e.key === 'End') { e.preventDefault(); const t = activeTab(); if (t) goToPage(t.pageCount - 1); }
      if (e.key === 'Escape' && state.annotToolbarVisible) { selectAnnotTool('select'); }
      if (e.key === 'Escape' && searchState.visible) { toggleSearch(); }
      if (e.key === 'v' && state.annotToolbarVisible) { selectAnnotTool('select'); }
    }
  });
}

function focusGoToPage() {
  dom.pageInput.focus();
  dom.pageInput.select();
}

function setupDragDrop() {
  document.addEventListener('dragover', (e) => { e.preventDefault(); });
  document.addEventListener('drop', async (e) => {
    e.preventDefault();
    // Tauri v2 handles file drop events via the backend.
    // The files are provided via the tauri file-drop event.
  });

  // Listen for Tauri file drop events
  listen('tauri://drag-drop', async (event) => {
    const paths = event.payload?.paths || [];
    for (const path of paths) {
      if (path.toLowerCase().endsWith('.pdf')) {
        await openFile(path);
      }
    }
  });
}

async function setupMenuEvents() {
  await listen('menu-event', async (event) => {
    const id = event.payload;
    switch (id) {
      case 'open': promptOpen(); break;
      case 'close_tab': if (state.activeTab !== null) closeTab(state.activeTab); break;
      case 'zoom_in': zoomIn(); break;
      case 'zoom_out': zoomOut(); break;
      case 'fit_width': setZoom('fit-width'); break;
      case 'fit_page': setZoom('fit-page'); break;
      case 'toggle_sidebar': toggleSidebar(); break;
      case 'toggle_dark': toggleDarkMode(); break;
      case 'undo': undoAction(); break;
      case 'redo': redoAction(); break;
      case 'save': saveDocument(); break;
      case 'print': printDocument(); break;
      case 'save_as': saveDocumentAs(); break;
      case 'copy': copyPageText(); break;
      case 'select_all': selectAllText(); break;
      case 'find': toggleSearch(); break;
      case 'doc_info': showDocumentInfo(); break;
      case 'go_to_page': focusGoToPage(); break;
    }
  });
}

// ── Actions ─────────────────────────────────────────────────────────

async function promptOpen() {
  try {
    const { open } = window.__TAURI__.dialog;
    const selected = await open({
      multiple: true,
      filters: [{ name: 'PDF Files', extensions: ['pdf'] }],
    });
    if (selected) {
      const paths = Array.isArray(selected) ? selected : [selected];
      for (const path of paths) {
        await openFile(path);
      }
    }
  } catch (err) {
    console.error('File dialog error:', err);
  }
}

function toggleSidebar() {
  state.sidebarVisible = !state.sidebarVisible;
  dom.sidebar.classList.toggle('collapsed', !state.sidebarVisible);
  // Re-render viewport on sidebar toggle (affects fit-width)
  const tab = activeTab();
  if (tab && (tab.zoom === 'fit-width' || tab.zoom === 'fit-page')) {
    renderCurrentView();
  }
}

function toggleDarkMode() {
  state.darkMode = !state.darkMode;
  document.body.classList.toggle('dark', state.darkMode);
  try {
    localStorage.setItem('xfa-dark-mode', state.darkMode ? '1' : '0');
  } catch (_) { /* storage unavailable */ }
}

function detectDarkModePreference() {
  // Check saved preference first, then OS preference.
  try {
    const saved = localStorage.getItem('xfa-dark-mode');
    if (saved !== null) {
      state.darkMode = saved === '1';
      document.body.classList.toggle('dark', state.darkMode);
      return;
    }
  } catch (_) { /* storage unavailable */ }

  if (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) {
    state.darkMode = true;
    document.body.classList.add('dark');
  }

  // Listen for OS theme changes.
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => {
    // Only follow OS if user hasn't manually toggled.
    try {
      if (localStorage.getItem('xfa-dark-mode') !== null) return;
    } catch (_) { /* ignore */ }
    state.darkMode = e.matches;
    document.body.classList.toggle('dark', state.darkMode);
  });
}

async function showDocumentInfo() {
  const tab = activeTab();
  if (!tab) return;
  try {
    const info = await invoke('document_info', { handle: tab.handle });
    alert(
      `Title: ${info.title || '(none)'}\n` +
      `Author: ${info.author || '(none)'}\n` +
      `Subject: ${info.subject || '(none)'}\n` +
      `Creator: ${info.creator || '(none)'}\n` +
      `Producer: ${info.producer || '(none)'}\n` +
      `Pages: ${info.page_count}`
    );
  } catch (err) {
    console.error('Failed to get document info:', err);
  }
}

// ── Undo / Redo / Save ───────────────────────────────────────────────

async function undoAction() {
  const tab = activeTab();
  if (!tab) return;
  try {
    const success = await invoke('undo_document', { handle: tab.handle });
    if (success) {
      invalidateCaches(tab.handle);
      await renderCurrentView();
      if (state.annotTool !== 'select') addDrawOverlay();
      await updateDirtyIndicator(tab);
    }
  } catch (err) {
    console.error('Undo failed:', err);
  }
}

async function redoAction() {
  const tab = activeTab();
  if (!tab) return;
  try {
    const success = await invoke('redo_document', { handle: tab.handle });
    if (success) {
      invalidateCaches(tab.handle);
      await renderCurrentView();
      if (state.annotTool !== 'select') addDrawOverlay();
      await updateDirtyIndicator(tab);
    }
  } catch (err) {
    console.error('Redo failed:', err);
  }
}

async function saveDocument() {
  const tab = activeTab();
  if (!tab) return;
  try {
    await invoke('save_document', { handle: tab.handle });
    await updateDirtyIndicator(tab);
  } catch (err) {
    console.error('Save failed:', err);
  }
}

async function updateDirtyIndicator(tab) {
  if (!tab) return;
  try {
    tab.dirty = await invoke('is_document_dirty', { handle: tab.handle });
    renderTabs();
  } catch (_) { /* ignore */ }
}

// ── Print & Save ─────────────────────────────────────────────────────

async function printDocument() {
  const tab = activeTab();
  if (!tab) return;
  try {
    await invoke('print_document', { handle: tab.handle });
  } catch (err) {
    console.error('Print failed:', err);
  }
}

async function saveDocumentAs() {
  const tab = activeTab();
  if (!tab) return;
  try {
    const { save } = window.__TAURI__.dialog;
    const path = await save({
      defaultPath: tab.fileName,
      filters: [{ name: 'PDF Files', extensions: ['pdf'] }],
    });
    if (path) {
      await invoke('save_document_as', { handle: tab.handle, path });
    }
  } catch (err) {
    console.error('Save failed:', err);
  }
}

// ── Text Selection & Copy ────────────────────────────────────────────

async function copyPageText() {
  const tab = activeTab();
  if (!tab) return;
  try {
    const text = await invoke('extract_page_text', {
      handle: tab.handle,
      pageIndex: tab.currentPage,
    });
    await navigator.clipboard.writeText(text);
  } catch (err) {
    console.error('Copy failed:', err);
  }
}

async function selectAllText() {
  // Select all text on current page (visual feedback + prepare for copy)
  const tab = activeTab();
  if (!tab) return;
  // Add selection overlay to current page wrapper
  const wrapper = $(`.page-wrapper[data-page="${tab.currentPage}"]`);
  if (wrapper) {
    // Clear existing overlays
    wrapper.querySelectorAll('.selection-overlay').forEach(el => el.remove());
    const overlay = document.createElement('div');
    overlay.className = 'selection-overlay';
    overlay.style.inset = '0';
    wrapper.appendChild(overlay);
  }
}

// ── Search ──────────────────────────────────────────────────────────

const searchState = {
  visible: false,
  query: '',
  results: [],     // page indices with matches
  currentIndex: 0, // index into results
};

function toggleSearch() {
  const bar = $('#search-bar');
  searchState.visible = !searchState.visible;
  bar.classList.toggle('hidden', !searchState.visible);
  if (searchState.visible) {
    const input = $('#search-input');
    input.focus();
    input.select();
  } else {
    clearSearchHighlights();
  }
}

function setupSearchEvents() {
  const input = $('#search-input');
  const prevBtn = $('#search-prev');
  const nextBtn = $('#search-next');
  const closeBtn = $('#search-close');

  input.addEventListener('keydown', async (e) => {
    if (e.key === 'Enter') {
      if (e.shiftKey) navigateSearch(-1);
      else if (searchState.query === input.value && searchState.results.length > 0) navigateSearch(1);
      else await performSearch(input.value);
    }
    if (e.key === 'Escape') toggleSearch();
  });

  prevBtn.addEventListener('click', () => navigateSearch(-1));
  nextBtn.addEventListener('click', () => navigateSearch(1));
  closeBtn.addEventListener('click', toggleSearch);
}

async function performSearch(query) {
  searchState.query = query;
  searchState.currentIndex = 0;
  if (!query) {
    searchState.results = [];
    updateSearchIndicator();
    clearSearchHighlights();
    return;
  }
  const tab = activeTab();
  if (!tab) return;
  try {
    searchState.results = await invoke('search_document', {
      handle: tab.handle,
      query,
    });
    updateSearchIndicator();
    if (searchState.results.length > 0) {
      await goToPage(searchState.results[0]);
    }
  } catch (err) {
    console.error('Search failed:', err);
  }
}

function navigateSearch(direction) {
  if (searchState.results.length === 0) return;
  searchState.currentIndex = (searchState.currentIndex + direction + searchState.results.length) % searchState.results.length;
  updateSearchIndicator();
  goToPage(searchState.results[searchState.currentIndex]);
}

function updateSearchIndicator() {
  const el = $('#search-results');
  if (searchState.results.length === 0) {
    el.textContent = searchState.query ? 'No results' : '';
  } else {
    el.textContent = `${searchState.currentIndex + 1} / ${searchState.results.length}`;
  }
}

function clearSearchHighlights() {
  $$('.search-highlight').forEach(el => el.remove());
}

// ── Annotation Toolbar ───────────────────────────────────────────────

function toggleAnnotationToolbar() {
  state.annotToolbarVisible = !state.annotToolbarVisible;
  const bar = $('#annot-toolbar');
  bar.classList.toggle('hidden', !state.annotToolbarVisible);
  if (!state.annotToolbarVisible) {
    selectAnnotTool('select');
    removeDrawOverlay();
  }
}

function setupAnnotationToolbar() {
  $$('.annot-tool').forEach(btn => {
    btn.addEventListener('click', () => selectAnnotTool(btn.dataset.tool));
  });

  $('#annot-delete').addEventListener('click', deleteSelectedAnnotation);
}

function selectAnnotTool(tool) {
  state.annotTool = tool;
  $$('.annot-tool').forEach(b => b.classList.toggle('active', b.dataset.tool === tool));

  // Show/hide conditional property controls
  const fontsizeLabel = $('#annot-fontsize-label');
  const stampLabel = $('#annot-stamp-label');
  if (fontsizeLabel) fontsizeLabel.classList.toggle('hidden', tool !== 'freetext');
  if (stampLabel) stampLabel.classList.toggle('hidden', tool !== 'stamp');

  // Add or remove draw overlay based on tool
  if (tool === 'select') {
    removeDrawOverlay();
  } else {
    addDrawOverlay();
  }
}

function addDrawOverlay() {
  removeDrawOverlay();
  const wrappers = dom.pageContainer.querySelectorAll('.page-wrapper');
  wrappers.forEach(wrapper => {
    if (state.annotTool === 'ink') {
      const canvas = document.createElement('canvas');
      canvas.className = 'annot-ink-canvas';
      canvas.width = wrapper.offsetWidth;
      canvas.height = wrapper.offsetHeight;
      setupInkCanvas(canvas, wrapper);
      wrapper.appendChild(canvas);
    } else {
      const overlay = document.createElement('div');
      overlay.className = 'annot-draw-overlay';
      setupDrawOverlay(overlay, wrapper);
      wrapper.appendChild(overlay);
    }
  });
}

function removeDrawOverlay() {
  $$('.annot-draw-overlay').forEach(el => el.remove());
  $$('.annot-ink-canvas').forEach(el => el.remove());
  $$('.annot-draw-preview').forEach(el => el.remove());
}

function setupDrawOverlay(overlay, wrapper) {
  let preview = null;

  overlay.addEventListener('mousedown', (e) => {
    state.annotDrawing = true;
    const rect = overlay.getBoundingClientRect();
    state.annotStartX = e.clientX - rect.left;
    state.annotStartY = e.clientY - rect.top;

    preview = document.createElement('div');
    preview.className = 'annot-draw-preview';
    preview.style.left = state.annotStartX + 'px';
    preview.style.top = state.annotStartY + 'px';
    preview.style.width = '0px';
    preview.style.height = '0px';
    wrapper.appendChild(preview);
  });

  overlay.addEventListener('mousemove', (e) => {
    if (!state.annotDrawing || !preview) return;
    const rect = overlay.getBoundingClientRect();
    const curX = e.clientX - rect.left;
    const curY = e.clientY - rect.top;
    const x = Math.min(state.annotStartX, curX);
    const y = Math.min(state.annotStartY, curY);
    const w = Math.abs(curX - state.annotStartX);
    const h = Math.abs(curY - state.annotStartY);
    preview.style.left = x + 'px';
    preview.style.top = y + 'px';
    preview.style.width = w + 'px';
    preview.style.height = h + 'px';
  });

  overlay.addEventListener('mouseup', async (e) => {
    if (!state.annotDrawing) return;
    state.annotDrawing = false;
    if (preview) preview.remove();
    preview = null;

    const rect = overlay.getBoundingClientRect();
    const endX = e.clientX - rect.left;
    const endY = e.clientY - rect.top;
    const pageIndex = parseInt(wrapper.dataset.page, 10);
    await createAnnotation(wrapper, pageIndex, state.annotStartX, state.annotStartY, endX, endY);
  });
}

function setupInkCanvas(canvas, wrapper) {
  const ctx = canvas.getContext('2d');
  let drawing = false;
  let points = [];

  canvas.addEventListener('mousedown', (e) => {
    drawing = true;
    points = [];
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    points.push(x, y);
    ctx.strokeStyle = $('#annot-color').value;
    ctx.lineWidth = parseFloat($('#annot-border-width').value) || 1;
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    ctx.beginPath();
    ctx.moveTo(x, y);
  });

  canvas.addEventListener('mousemove', (e) => {
    if (!drawing) return;
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    points.push(x, y);
    ctx.lineTo(x, y);
    ctx.stroke();
  });

  canvas.addEventListener('mouseup', async () => {
    if (!drawing) return;
    drawing = false;
    if (points.length < 4) return; // Need at least 2 points

    const pageIndex = parseInt(wrapper.dataset.page, 10);
    const pdfPoints = screenPointsToPdf(wrapper, points);
    const bounds = getPointsBounds(pdfPoints);
    await createInkAnnotation(pageIndex, bounds, [pdfPoints]);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
  });
}

function screenPointsToPdf(wrapper, points) {
  const img = wrapper.querySelector('img');
  if (!img) return points;
  const imgW = img.naturalWidth || img.width;
  const imgH = img.naturalHeight || img.height;
  const displayW = img.clientWidth;
  const displayH = img.clientHeight;
  // Scale from screen to PDF coordinates (72 DPI basis)
  const scaleX = (imgW / displayW);
  const scaleY = (imgH / displayH);
  const result = [];
  for (let i = 0; i < points.length; i += 2) {
    result.push(points[i] * scaleX);
    // PDF y-axis is inverted (origin at bottom-left)
    result.push(imgH - (points[i + 1] * scaleY));
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
  return { x0: minX, y0: minY, x1: maxX, y1: maxY };
}

async function createAnnotation(wrapper, pageIndex, sx, sy, ex, ey) {
  const tab = activeTab();
  if (!tab) return;

  const img = wrapper.querySelector('img');
  if (!img) return;
  const imgW = img.naturalWidth || img.width;
  const imgH = img.naturalHeight || img.height;
  const displayW = img.clientWidth;
  const displayH = img.clientHeight;
  const scaleX = imgW / displayW;
  const scaleY = imgH / displayH;

  // Convert screen coords to PDF coords (y inverted)
  const pdfX0 = Math.min(sx, ex) * scaleX;
  const pdfX1 = Math.max(sx, ex) * scaleX;
  const pdfY0 = imgH - (Math.max(sy, ey) * scaleY);
  const pdfY1 = imgH - (Math.min(sy, ey) * scaleY);

  // Minimum size check
  if (Math.abs(pdfX1 - pdfX0) < 5 && Math.abs(pdfY1 - pdfY0) < 5) {
    // Click (not drag) — for stickynote/stamp, use a fixed size
    if (state.annotTool === 'stickynote' || state.annotTool === 'stamp') {
      // Use click position as top-right, create 24x24 rect
      const cx = sx * scaleX;
      const cy = imgH - (sy * scaleY);
      await sendAnnotation(tab, pageIndex, cx, cy, cx + 24, cy + 24);
      return;
    }
    return; // Too small for other tools
  }

  await sendAnnotation(tab, pageIndex, pdfX0, pdfY0, pdfX1, pdfY1);
}

async function sendAnnotation(tab, pageIndex, x0, y0, x1, y1) {
  const colorHex = $('#annot-color').value;
  const r = parseInt(colorHex.slice(1, 3), 16) / 255;
  const g = parseInt(colorHex.slice(3, 5), 16) / 255;
  const b = parseInt(colorHex.slice(5, 7), 16) / 255;
  const opacity = parseFloat($('#annot-opacity').value);
  const borderWidth = parseFloat($('#annot-border-width').value);

  const request = {
    handle: tab.handle,
    page: pageIndex + 1, // lopdf uses 1-based pages
    type: state.annotTool,
    x0, y0, x1, y1,
    color: [r, g, b],
    opacity,
    borderWidth,
    contents: null,
    fontSize: null,
    stampName: null,
    icon: null,
    lineEndingStart: null,
    lineEndingEnd: null,
    inkPaths: null,
  };

  if (state.annotTool === 'freetext') {
    request.contents = prompt('Enter text:') || '';
    request.fontSize = parseFloat($('#annot-fontsize').value) || 12;
    if (!request.contents) return;
  }
  if (state.annotTool === 'stickynote') {
    request.contents = prompt('Note text:') || '';
    if (!request.contents) return;
  }
  if (state.annotTool === 'stamp') {
    request.stampName = $('#annot-stamp-name').value;
  }

  try {
    await invoke('add_annotation', { request });
    invalidateCaches(tab.handle, pageIndex);
    await renderCurrentView();
    if (state.annotTool !== 'select') addDrawOverlay();
    await updateDirtyIndicator(tab);
  } catch (err) {
    console.error('Failed to add annotation:', err);
  }
}

async function createInkAnnotation(pageIndex, bounds, inkPaths) {
  const tab = activeTab();
  if (!tab) return;

  const colorHex = $('#annot-color').value;
  const r = parseInt(colorHex.slice(1, 3), 16) / 255;
  const g = parseInt(colorHex.slice(3, 5), 16) / 255;
  const b = parseInt(colorHex.slice(5, 7), 16) / 255;
  const opacity = parseFloat($('#annot-opacity').value);
  const borderWidth = parseFloat($('#annot-border-width').value);

  const request = {
    handle: tab.handle,
    page: pageIndex + 1,
    type: 'ink',
    x0: bounds.x0, y0: bounds.y0, x1: bounds.x1, y1: bounds.y1,
    color: [r, g, b],
    opacity,
    borderWidth,
    contents: null,
    fontSize: null,
    stampName: null,
    icon: null,
    lineEndingStart: null,
    lineEndingEnd: null,
    inkPaths,
  };

  try {
    await invoke('add_annotation', { request });
    invalidateCaches(tab.handle, pageIndex);
    await renderCurrentView();
    if (state.annotTool !== 'select') addDrawOverlay();
    await updateDirtyIndicator(tab);
  } catch (err) {
    console.error('Failed to add ink annotation:', err);
  }
}

async function deleteSelectedAnnotation() {
  // For now, delete the last annotation on the current page
  const tab = activeTab();
  if (!tab) return;

  try {
    const annots = await invoke('list_annotations', {
      handle: tab.handle,
      page: tab.currentPage + 1,
    });
    if (annots.length === 0) return;
    const last = annots[annots.length - 1];
    await invoke('delete_annotation', {
      handle: tab.handle,
      page: tab.currentPage + 1,
      annotIndex: last.index,
    });
    invalidateCaches(tab.handle, tab.currentPage);
    await renderCurrentView();
    if (state.annotTool !== 'select') addDrawOverlay();
    await updateDirtyIndicator(tab);
  } catch (err) {
    console.error('Failed to delete annotation:', err);
  }
}

// ── Viewport scroll → page tracking ─────────────────────────────────

dom.viewportScroll.addEventListener('scroll', () => {
  const tab = activeTab();
  if (!tab) return;
  tab.scrollTop = dom.viewportScroll.scrollTop;

  // Determine which page is most visible
  const wrappers = dom.pageContainer.querySelectorAll('.page-wrapper');
  const scrollCenter = dom.viewportScroll.scrollTop + dom.viewportScroll.clientHeight / 2;
  for (const w of wrappers) {
    const rect = w.getBoundingClientRect();
    const containerRect = dom.viewportScroll.getBoundingClientRect();
    const top = rect.top - containerRect.top + dom.viewportScroll.scrollTop;
    const bottom = top + rect.height;
    if (scrollCenter >= top && scrollCenter <= bottom) {
      const page = parseInt(w.dataset.page, 10);
      if (page !== tab.currentPage) {
        tab.currentPage = page;
        updatePageIndicator();
        updateThumbnailHighlight();
      }
      break;
    }
  }
});

// ── Boot ────────────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', init);
