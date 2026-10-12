// v340.js — P01 (OS 12225074) full read-back backup step. Shows progress, path, SHA-256, quality.
// The saved path is passed to guided_flash_pipeline as backup_path; the backend re-verifies it.
(function () {
  'use strict';
  function $(id) { return document.getElementById(id); }
  function set(id, t) { const e = $(id); if (e) e.textContent = t; }
  function invoke(cmd, args) {
    const t = window.__TAURI__;
    if (t && t.core && typeof t.core.invoke === 'function') return t.core.invoke(cmd, args || {});
    if (t && typeof t.invoke === 'function') return t.invoke(cmd, args || {});
    return Promise.reject(new Error('Tauri backend not available'));
  }
  function listen() {
    const ev = window.__TAURI__ && window.__TAURI__.event;
    if (!ev || typeof ev.listen !== 'function') return;
    ev.listen('p01-backup-progress', function (event) {
      const p = event && event.payload ? event.payload : event;
      if (!p) return;
      set('p01-backup-progress', (p.percent || 0) + '%');
      set('p01-backup-stage', p.stage ? '(' + p.stage + ')' : '');
    });
  }
  async function run() {
    const panel = $('p01-backup-panel'); if (panel) panel.hidden = false;
    const btn = $('btn-p01-backup'); if (btn) btn.disabled = true;
    const log = $('flash-log');
    window.__p01BackupPath = null;
    set('p01-backup-progress', '0%'); set('p01-backup-path', '—'); set('p01-backup-sha', '—'); set('p01-backup-quality', 'running…');
    try {
      const raw = await invoke('p01_full_backup', {});
      const res = typeof raw === 'string' ? JSON.parse(raw) : raw;
      const b = (res && res.backup) || {};
      set('p01-backup-path', b.path || '—');
      set('p01-backup-sha', b.sha256 || '—');
      const q = b.quality || 'failed';
      set('p01-backup-quality', q === 'full_image' ? 'FullImage' : 'Failed: ' + (b.reasons || []).join('; '));
      if (q === 'full_image') window.__p01BackupPath = b.path;
      if (log && res && res.logs) log.textContent += res.logs.join('\n') + '\n';
    } catch (e) {
      set('p01-backup-quality', 'Failed: ' + e);
    } finally {
      if (btn) btn.disabled = false;
    }
  }
  function init() { const b = $('btn-p01-backup'); if (b) b.addEventListener('click', run); listen(); }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
