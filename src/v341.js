// v341.js — read-only viewer for the append-only flash/backup audit log (audit_log_entries).
(function () {
  'use strict';
  function invoke(cmd, args) {
    const t = window.__TAURI__;
    if (t && t.core && typeof t.core.invoke === 'function') return t.core.invoke(cmd, args || {});
    if (t && typeof t.invoke === 'function') return t.invoke(cmd, args || {});
    return Promise.reject(new Error('Tauri backend not available'));
  }
  function esc(s) { return String(s == null ? '' : s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c])); }
  async function load() {
    const out = document.getElementById('audit-log-table');
    const path = document.getElementById('audit-log-path');
    if (!out) return;
    try {
      const raw = await invoke('audit_log_entries', { limit: 200 });
      const res = typeof raw === 'string' ? JSON.parse(raw) : raw;
      if (path) path.textContent = res.path || '';
      const rows = (res.entries || []).slice().reverse().map(e =>
        '<tr><td>' + esc(new Date(Number(e.ts_unix_ms)).toLocaleString()) + '</td><td>' + esc(e.kind) + '</td><td>' + esc(e.family) +
        '</td><td>' + esc(e.os_id) + '</td><td>' + esc(e.result) + '</td><td class="mono">' + esc((e.image_sha256 || e.backup_sha256 || '').slice(0, 12)) +
        '</td><td>' + esc(e.gate_failed || (e.gates_passed || []).join(', ')) + '</td></tr>').join('');
      out.innerHTML = '<table class="data-table"><thead><tr><th>Time</th><th>Kind</th><th>Family</th><th>OS</th><th>Result</th><th>SHA-256</th><th>Gates / reason</th></tr></thead><tbody>' +
        (rows || '<tr><td colspan="7">No attempts recorded yet.</td></tr>') + '</tbody></table>';
    } catch (e) { out.textContent = 'Audit log unavailable: ' + e; }
  }
  function init() {
    const log = document.getElementById('flash-log');
    if (!log || document.getElementById('audit-log-panel')) return;
    const d = document.createElement('details');
    d.id = 'audit-log-panel';
    d.innerHTML = '<summary>Flash &amp; backup audit log</summary><button id="btn-audit-refresh" class="btn" type="button">Refresh</button> <span id="audit-log-path" class="mono"></span><div id="audit-log-table" class="scroll-pane"></div>';
    log.parentNode.insertBefore(d, log.nextSibling);
    d.addEventListener('toggle', () => { if (d.open) load(); });
    document.getElementById('btn-audit-refresh').addEventListener('click', load);
  }
  window.TIVAudit = { load: load };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
