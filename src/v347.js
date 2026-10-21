// TuneItVerse v3.47.0 — capability matrix, status bar, recent files, keyboard shortcuts.
// Display and navigation only. Nothing here enables or performs a write.
(function () {
  'use strict';
  const RECENT_KEY = 'tiv_recent_files';
  const RECENT_MAX = 10;
  const VIEW_KEYS = ['dashboard', 'connect', 'live', 'diagnostics', 'tables', 'flash', 'scripts', 'inspect', 'compare', 'capabilities'];
  const STATUS_LABEL = {
    verified_dump: 'verified (dump)',
    verified_log: 'verified (log)',
    public_source: 'public source',
    reference_doc: 'reference doc',
    experimental: 'experimental',
    report_only: 'report only',
    unverified: 'unverified',
    not_supported: 'no'
  };
  const CAP_LABEL = { identify: 'Identify', read: 'Read', checksum: 'Checksum', seed_key: 'Seed/key', write: 'Write', map_hints: 'Map hints' };
  let matrix = null;
  let appVersion = '';

  function $(id) { return document.getElementById(id); }
  function esc(s) {
    return String(s == null ? '' : s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  }
  function parseMaybe(raw) {
    if (raw == null) return null;
    if (typeof raw === 'string') { try { return JSON.parse(raw); } catch (_) { return raw; } }
    return raw;
  }
  async function cmd(name, args) {
    if (typeof window.invokeCmd !== 'function') throw new Error('backend unavailable');
    return window.invokeCmd(name, args || {});
  }
  function badge(c) {
    if (!c) return '<span class="cap-badge cap-missing">missing</span>';
    return '<span class="cap-badge cap-' + esc(c.status) + '" title="' + esc(c.source) + '">' + esc(STATUS_LABEL[c.status] || c.status) + '</span>';
  }

  // ---------- Capability matrix ----------
  async function loadMatrix() {
    matrix = parseMaybe(await cmd('family_capabilities'));
    return matrix;
  }
  function renderLegend() {
    const el = $('cap-legend');
    if (!el || !matrix) return;
    el.innerHTML = (matrix.status_enum || []).map(function (s) {
      return badge({ status: s, source: s }) ;
    }).join(' ') + '<p class="muted">Hover a badge for its source. "Verified" means checked against a real dump or logged session in reference/ — not a bench test. Bench-tested: none.</p>';
  }
  function renderMatrix() {
    const el = $('cap-matrix');
    if (!el || !matrix) return;
    const keys = matrix.capability_keys || Object.keys(CAP_LABEL);
    let html = '<table class="data-table cap-table"><thead><tr><th>Family</th><th>Size</th>' +
      keys.map(function (k) { return '<th>' + esc(CAP_LABEL[k] || k) + '</th>'; }).join('') + '<th>Write gates</th></tr></thead><tbody>';
    (matrix.families || []).forEach(function (f) {
      const caps = f.capabilities || {};
      html += '<tr><td><strong>' + esc(f.family) + '</strong><div class="muted">' + esc(f.display_name) + '</div></td>' +
        '<td class="mono">' + Number(f.bin_size_bytes || 0).toLocaleString() + '</td>' +
        keys.map(function (k) { return '<td>' + badge(caps[k]) + '</td>'; }).join('') +
        '<td>' + (f.write_path_in_code ? esc((f.write_requires || []).join(' + ')) : '—') + '</td></tr>';
    });
    html += '</tbody></table>';
    const errs = matrix.validation_errors || [];
    if (errs.length) {
      html = '<div class="banner-warn">ECU database validation errors (entries dropped, fail-closed):<br>' + errs.map(esc).join('<br>') + '</div>' + html;
    }
    el.innerHTML = html;
    const st = $('cap-status');
    if (st) st.textContent = (matrix.families || []).length + ' families · ' + (errs.length ? errs.length + ' DB errors' : 'DB schema OK') + ' · bench-tested: none';
  }
  async function refreshMatrix() {
    try {
      await loadMatrix();
      renderLegend();
      renderMatrix();
      renderCapStrip();
    } catch (e) {
      const st = $('cap-status');
      if (st) st.textContent = 'Capabilities unavailable: ' + e;
    }
  }
  function familyCaps(fam) {
    if (!matrix || !fam) return null;
    return (matrix.families || []).find(function (f) { return String(f.family).toUpperCase() === String(fam).toUpperCase(); }) || null;
  }
  function currentFamily() {
    const id = window.lastIdentify;
    return id && (id.family || null);
  }
  function renderCapStrip() {
    const el = $('bin-cap-strip');
    if (!el) return;
    const f = familyCaps(currentFamily());
    if (!f) { el.innerHTML = ''; return; }
    const c = f.capabilities || {};
    el.innerHTML = ['identify', 'checksum', 'write'].map(function (k) {
      return '<span class="cap-strip-item">' + esc(CAP_LABEL[k]) + ' ' + badge(c[k]) + '</span>';
    }).join('');
  }

  // ---------- Status bar ----------
  let binState = { dirty: false, label: '', size: 0 };
  function shortName(p) { return String(p || '').replace(/^.*[\\/]/, ''); }
  function renderStatusBar() {
    const left = $('status-left');
    if (left) {
      let t = 'TuneItVerse ' + (appVersion || '');
      if (binState.size) t += ' · ' + shortName(binState.label) + ' (' + binState.size.toLocaleString() + ' B)' + (binState.dirty ? ' • unsaved' : '');
      left.textContent = t;
      left.classList.toggle('status-dirty', !!binState.dirty);
    }
    const right = $('status-right');
    if (right) {
      const fam = currentFamily();
      const f = familyCaps(fam);
      const conn = ($('connection-status') && $('connection-status').textContent) || '';
      const bits = [];
      bits.push(fam ? fam : (binState.size ? 'family unresolved' : 'no BIN'));
      if (f && f.capabilities) {
        const cs = f.capabilities.checksum, w = f.capabilities.write;
        if (cs) bits.push('CS: ' + (STATUS_LABEL[cs.status] || cs.status));
        if (w) bits.push('write: ' + (STATUS_LABEL[w.status] || w.status));
      }
      if (conn) bits.push(conn);
      right.textContent = bits.join(' · ');
    }
    const dirtyChip = $('bin-dirty-chip');
    if (dirtyChip) dirtyChip.hidden = !binState.dirty;
  }

  // ---------- Recent files ----------
  function loadRecent() {
    try { const v = JSON.parse(localStorage.getItem(RECENT_KEY) || '[]'); return Array.isArray(v) ? v : []; } catch (_) { return []; }
  }
  function saveRecent(list) {
    try { localStorage.setItem(RECENT_KEY, JSON.stringify(list.slice(0, RECENT_MAX))); } catch (_) {}
  }
  function addRecent(path, kind) {
    if (!path || !kind) return;
    const list = loadRecent().filter(function (r) { return !(r.path === path && r.kind === kind); });
    list.unshift({ path: path, kind: kind, at: Date.now() });
    saveRecent(list);
    renderRecent();
  }
  function renderRecent() {
    const sel = $('recent-files');
    if (!sel) return;
    const list = loadRecent();
    sel.innerHTML = '<option value="">Recent files…</option>' + list.map(function (r, i) {
      return '<option value="' + i + '" title="' + esc(r.path) + '">' + esc(r.kind.toUpperCase() + ' · ' + shortName(r.path)) + '</option>';
    }).join('') + (list.length ? '<option value="clear">Clear list</option>' : '');
  }
  async function openRecent(idx) {
    const list = loadRecent();
    const r = list[idx];
    if (!r) return;
    if (r.kind === 'bin' && typeof window.confirmDiscard === 'function' && !window.confirmDiscard('Open ' + shortName(r.path))) return;
    try {
      const res = parseMaybe(await cmd('open_recent_file', { path: r.path, kind: r.kind }));
      if (!res || res.cancelled) return;
      if (r.kind === 'bin') {
        if (typeof window.ingestBinBytes !== 'function') throw new Error('Maps not ready');
        await window.ingestBinBytes(res.bytes || [], res.path || r.path);
      } else {
        if (typeof window.applyDefinitionText !== 'function') throw new Error('Maps not ready');
        await window.applyDefinitionText(res.text || '', res.path || r.path);
      }
      addRecent(r.path, r.kind);
      if (typeof window.showView === 'function') window.showView('tables');
    } catch (e) {
      const st = $('tables-status');
      if (st) st.textContent = 'Recent file failed: ' + e + ' (removed from list)';
      saveRecent(list.filter(function (_, i) { return i !== idx; }));
      renderRecent();
    }
  }

  // ---------- Keyboard shortcuts ----------
  const SHORTCUTS = [
    ['Ctrl+O', 'Open BIN (asks first if edits are unsaved)'],
    ['Ctrl+S', 'Save working BIN'],
    ['Ctrl+Alt+Z / Ctrl+Alt+Y', 'BIN-level undo / redo'],
    ['Alt+1 … Alt+9, Alt+0', 'Dashboard, Connect, Logging, Diagnostics, Maps, Flash, Scripts, Inspect, Compare, Capabilities'],
    ['Ctrl+F', 'Search parameters (Maps)'],
    ['In the grid', '+/− step · Ctrl+C/V copy/paste · Ctrl+Z/Y undo/redo · Ctrl+A select all'],
    ['F1 or ?', 'This help · Esc closes']
  ];
  function toggleHelp(force) {
    const el = $('shortcut-help');
    if (!el) return;
    const show = force == null ? el.hidden : force;
    if (show) {
      el.innerHTML = '<div class="shortcut-card"><h3>Keyboard shortcuts</h3><table class="data-table">' +
        SHORTCUTS.map(function (s) { return '<tr><td class="mono">' + esc(s[0]) + '</td><td>' + esc(s[1]) + '</td></tr>'; }).join('') +
        '</table><button class="btn" type="button" id="btn-shortcut-close">Close</button></div>';
    }
    el.hidden = !show;
  }
  function typing(t) {
    if (!t) return false;
    const tag = (t.tagName || '').toLowerCase();
    return tag === 'input' || tag === 'textarea' || tag === 'select' || t.isContentEditable;
  }
  function call(name) {
    const fn = window[name];
    if (typeof fn === 'function') Promise.resolve(fn()).catch(function (e) { if (typeof window.setStatus === 'function') window.setStatus(name + ': ' + e); });
  }
  function onKey(e) {
    const k = (e.key || '').toLowerCase();
    if (e.key === 'Escape') { toggleHelp(false); return; }
    if (e.key === 'F1' || (e.key === '?' && !typing(e.target))) { e.preventDefault(); toggleHelp(); return; }
    if (e.altKey && !e.ctrlKey && /^[0-9]$/.test(e.key)) {
      const i = e.key === '0' ? 9 : Number(e.key) - 1;
      if (VIEW_KEYS[i] && typeof window.showView === 'function') { e.preventDefault(); window.showView(VIEW_KEYS[i]); }
      return;
    }
    if (!(e.ctrlKey || e.metaKey)) return;
    if (e.altKey && k === 'z') { e.preventDefault(); call('undoBin'); return; }
    if (e.altKey && k === 'y') { e.preventDefault(); call('redoBin'); return; }
    if (e.altKey) return;
    if (k === 'o') { e.preventDefault(); if (typeof window.showView === 'function') window.showView('tables'); call('loadBinFile'); return; }
    if (k === 's') { e.preventDefault(); call('savePatchedBin'); return; }
    if (k === 'f' && !typing(e.target)) {
      const s = $('table-search');
      if (s) { e.preventDefault(); if (typeof window.showView === 'function') window.showView('tables'); s.focus(); s.select(); }
    }
  }

  async function boot() {
    try {
      const info = parseMaybe(await cmd('app_info'));
      appVersion = (info && (info.version || info.app_version)) || '';
    } catch (_) {}
    document.addEventListener('tiv:bin-state', function (ev) { binState = ev.detail || binState; renderStatusBar(); renderCapStrip(); });
    document.addEventListener('tiv:file-opened', function (ev) { const d = ev.detail || {}; addRecent(d.path, d.kind); });
    document.addEventListener('keydown', onKey);
    document.body.addEventListener('click', function (ev) {
      const btn = ev.target && ev.target.closest ? ev.target.closest('button') : null;
      if (!btn) return;
      if (btn.id === 'btn-cap-refresh') { ev.preventDefault(); refreshMatrix(); }
      else if (btn.id === 'btn-shortcut-close') { ev.preventDefault(); toggleHelp(false); }
      else if (btn.matches('.nav-item[data-view="capabilities"]')) { refreshMatrix(); }
    });
    const sel = $('recent-files');
    if (sel) {
      sel.addEventListener('change', function () {
        const v = sel.value;
        sel.value = '';
        if (v === 'clear') { saveRecent([]); renderRecent(); return; }
        if (v !== '') openRecent(Number(v));
      });
    }
    renderRecent();
    await refreshMatrix();
    renderStatusBar();
    // Identify results and the connection label change elsewhere; refresh cheaply.
    setInterval(function () { renderStatusBar(); renderCapStrip(); }, 1500);
  }
  window.tivCapabilities = { refresh: refreshMatrix, familyCaps: familyCaps };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
  else boot();
})();
