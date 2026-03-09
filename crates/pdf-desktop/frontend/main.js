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

// ── Thumbnails ──────────────────────────────────────────────────────

async function loadThumbnails(handle, pageCount) {
  dom.thumbnailPanel.innerHTML = '';
  for (let i = 0; i < pageCount; i++) {
    const item = document.createElement('div');
    item.className = 'thumbnail-item';
    if (i === (activeTab()?.currentPage ?? 0)) item.classList.add('active');
    item.dataset.page = i;

    const img = document.createElement('img');
    img.alt = `Page ${i + 1}`;
    img.style.width = '150px';
    img.style.height = '200px';
    img.style.background = '#f0f0f0';

    const label = document.createElement('div');
    label.className = 'thumbnail-label';
    label.textContent = i + 1;

    item.appendChild(img);
    item.appendChild(label);
    item.addEventListener('click', () => goToPage(i));
    dom.thumbnailPanel.appendChild(item);

    // Load thumbnail asynchronously
    const thumbKey = `${handle}-${i}`;
    if (state.thumbnailCache[thumbKey]) {
      img.src = `data:image/png;base64,${state.thumbnailCache[thumbKey]}`;
      img.style.width = '';
      img.style.height = '';
      img.style.background = '';
    } else {
      loadThumbnailAsync(handle, i, img, thumbKey);
    }
  }
}

async function loadThumbnailAsync(handle, pageIndex, imgElement, cacheKey) {
  try {
    const b64 = await invoke('render_thumbnail', { handle, pageIndex });
    state.thumbnailCache[cacheKey] = b64;
    imgElement.src = `data:image/png;base64,${b64}`;
    imgElement.style.width = '';
    imgElement.style.height = '';
    imgElement.style.background = '';
  } catch (err) {
    console.error(`Failed to load thumbnail ${pageIndex}:`, err);
  }
}

function updateThumbnailHighlight() {
  const tab = activeTab();
  if (!tab) return;
  $$('.thumbnail-item').forEach((el, i) => {
    el.classList.toggle('active', parseInt(el.dataset.page) === tab.currentPage);
  });
  // Scroll active thumbnail into view
  const active = $('.thumbnail-item.active');
  if (active) active.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
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
}

function renderTabs() {
  dom.tabs.innerHTML = '';
  state.tabs.forEach((tab, i) => {
    const el = document.createElement('div');
    el.className = 'tab' + (i === state.activeTab ? ' active' : '');

    const title = document.createElement('span');
    title.className = 'tab-title';
    title.textContent = tab.fileName;
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
    const mod = e.metaKey || e.ctrlKey;

    if (mod && e.key === 'o') { e.preventDefault(); promptOpen(); }
    if (mod && e.key === 'w') { e.preventDefault(); if (state.activeTab !== null) closeTab(state.activeTab); }
    if (mod && e.key === '=') { e.preventDefault(); zoomIn(); }
    if (mod && e.key === '-') { e.preventDefault(); zoomOut(); }
    if (mod && e.key === '1') { e.preventDefault(); setZoom('fit-width'); }
    if (mod && e.key === '2') { e.preventDefault(); setZoom('fit-page'); }
    if (mod && e.key === 'b') { e.preventDefault(); toggleSidebar(); }
    if (mod && e.key === 'd') { e.preventDefault(); toggleDarkMode(); }

    // Arrow key navigation
    if (!mod && e.key === 'ArrowLeft') { const t = activeTab(); if (t) goToPage(t.currentPage - 1); }
    if (!mod && e.key === 'ArrowRight') { const t = activeTab(); if (t) goToPage(t.currentPage + 1); }
    if (e.key === 'Home') { goToPage(0); }
    if (e.key === 'End') { const t = activeTab(); if (t) goToPage(t.pageCount - 1); }
  });
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
      case 'doc_info': showDocumentInfo(); break;
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
