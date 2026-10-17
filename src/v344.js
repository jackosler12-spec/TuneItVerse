// TuneItVerse v3.44.0 — Operational status panel on Dashboard.
(function () {
  'use strict';
  function invoke(cmd, args) {
    const t = window.__TAURI__;
    if (t && t.core && typeof t.core.invoke === 'function') return t.core.invoke(cmd, args || {});
    if (t && typeof t.invoke === 'function') return t.invoke(cmd, args || {});
    return Promise.reject(new Error('No Tauri invoke'));
  }
  function addStatusPanel() {
    const dash = document.getElementById('view-dashboard');
    if (!dash || document.getElementById('ops-status-panel')) return;
    const panel = document.createElement('div');
    panel.className = 'panel';
    panel.id = 'ops-status-panel';
    panel.style.marginTop = '12px';
    panel.innerHTML = `
      <div class="panel-header"><span class="panel-title">Operational Status (v3.44.0)</span>
        <button id="btn-refresh-ops" class="btn btn-sm" type="button">Refresh</button>
      </div>
      <pre id="ops-status-out" class="mono-block">Loading…</pre>
      <p class="muted">Honest matrix. Hardware untested. EDC16 checksum report-only. Other families need your dumps for real algorithms.</p>
    `;
    const body = dash.querySelector('.view-body');
    if (body) body.appendChild(panel);
    const btn = document.getElementById('btn-refresh-ops');
    if (btn) btn.addEventListener('click', loadStatus);
    loadStatus();
  }
  async function loadStatus() {
    const out = document.getElementById('ops-status-out');
    if (!out) return;
    try {
      const raw = await invoke('get_operational_status');
      const data = typeof raw === 'string' ? JSON.parse(raw) : raw;
      out.textContent = JSON.stringify(data, null, 2);
    } catch (e) {
      out.textContent = 'Status command unavailable: ' + (e.message || e);
    }
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', addStatusPanel);
  } else {
    addStatusPanel();
  }
})();
