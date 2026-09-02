// v3.5.0 overlay — workspace export, heatmap render, unverified-write flag
(function () {
  function parseMaybe(raw) {
    if (raw == null) return null;
    if (typeof raw === 'string') {
      try { return JSON.parse(raw); } catch (e) { return raw; }
    }
    return raw;
  }

  async function exportWorkspace() {
    const bin = window.currentBin ? Array.from(window.currentBin) : null;
    try {
      const raw = await window.invokeCmd('export_workspace_cmd', { data: bin });
      const text = typeof raw === 'string' ? raw : JSON.stringify(raw, null, 2);
      const blob = new Blob([text], { type: 'application/json' });
      const a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = 'tuneitverse-workspace.json';
      a.click();
      const st = document.getElementById('tables-status');
      if (st) st.textContent = 'Workspace JSON exported';
    } catch (e) {
      alert('Workspace export failed: ' + e);
    }
  }

  function renderHeatmap(info) {
    const adv = document.getElementById('side-advice');
    if (!adv || !info || !info.occupancy_16x16) {
      if (adv && info && info.advice) adv.textContent = info.advice;
      return;
    }
    const grid = info.occupancy_16x16;
    let max = 1;
    for (let r = 0; r < grid.length; r++) for (let c = 0; c < grid[r].length; c++) max = Math.max(max, grid[r][c]);
    let html = '<div style="font-size:11px;margin-bottom:6px;">' + (info.advice || '') + '</div>';
    html += '<div style="display:grid;grid-template-columns:repeat(16,12px);gap:1px;">';
    for (let r = 15; r >= 0; r--) {
      for (let c = 0; c < 16; c++) {
        const v = grid[r][c];
        const t = v / max;
        const bg = 'rgba(0,200,80,' + (0.08 + t * 0.92) + ')';
        html += '<div title="r' + r + ' c' + c + ' hits=' + v + '" style="width:12px;height:12px;background:' + bg + ';"></div>';
      }
    }
    html += '</div><div style="font-size:10px;color:#888;margin-top:4px;">rows RPM↑  cols MAP→</div>';
    adv.innerHTML = html;
  }

  const prevMap = window.mapFromLog;
  window.mapFromLog = async function () {
    if (typeof prevMap === 'function') {
      try { await prevMap(); } catch (e) { console.warn(e); }
    }
    try {
      const raw = await window.invokeCmd('map_from_log_cmd');
      renderHeatmap(parseMaybe(raw));
    } catch (e) { console.warn('heatmap', e); }
  };

  window.addEventListener('DOMContentLoaded', function () {
    document.getElementById('btn-export-workspace') && document.getElementById('btn-export-workspace').addEventListener('click', exportWorkspace);
    document.getElementById('btn-export-workspace-tables') && document.getElementById('btn-export-workspace-tables').addEventListener('click', exportWorkspace);
  });

  const prev = window.invokeCmd;
  window.invokeCmd = async function (cmd, args) {
    args = args || {};
    if (cmd === 'guided_flash_pipeline') {
      try {
        const obj = typeof args.request_json === 'string' ? JSON.parse(args.request_json) : (args.request_json || {});
        const risksOk = ['risk-backup','risk-power','risk-ground','risk-understand'].every(function (i) {
          const el = document.getElementById(i);
          return el && el.checked;
        });
        obj.user_confirmed_risks = !!risksOk;
        obj.accept_unverified_write = !!(document.getElementById('risk-unverified') && document.getElementById('risk-unverified').checked);
        args.request_json = JSON.stringify(obj);
      } catch (e) { /* leave as-is */ }
    }
    if (typeof prev === 'function') return prev(cmd, args);
    if (cmd === 'export_workspace_cmd') return JSON.stringify({ tool: 'TuneItVerse', version: '3.5.0', mock: true });
    return null;
  };
})();
