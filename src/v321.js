// v3.2.1 overlay — wire the COMPLETION.md UI bugs without rewriting main.js
(function () {
  const origInvoke = window.invokeCmd;
  if (typeof origInvoke === 'function') {
    window.invokeCmd = async function (cmd, args) {
      if (cmd === 'guided_flash_pipeline' && !(window.__TAURI__ && (window.__TAURI__.core || window.__TAURI__.invoke))) {
        return JSON.stringify({
          success: false,
          steps_completed: [],
          verified_live: false,
          logs: ['Browser mock — flash is fail-closed. Run the Tauri app with a connected adapter.'],
          error: 'Not connected (browser mock)'
        });
      }
      if (cmd === 'bosch_uds_unlock' && !(window.__TAURI__ && (window.__TAURI__.core || window.__TAURI__.invoke))) {
        return JSON.stringify({ success: false, message: 'Bosch UDS unlock refused offline (browser mock). Connect an adapter.' });
      }
      return origInvoke(cmd, args);
    };
  }

  const origLoad = window.loadBinFile;
  if (typeof origLoad === 'function') {
    const wrapped = origLoad;
    window.loadBinFile = async function () {
      await wrapped();
      try {
        if (window.currentBin && typeof window.identifyCurrentBin === 'function') {
          await window.identifyCurrentBin();
        } else if (window.currentBin && typeof window.invokeCmd === 'function') {
          const raw = await window.invokeCmd('identify_bin_cmd', { data: Array.from(window.currentBin) });
          const info = typeof raw === 'string' ? JSON.parse(raw) : raw;
          const st = document.getElementById('tables-status');
          if (st && info) st.textContent = 'Identified: ' + (info.family_by_size || 'unknown') + ' (' + info.bin_size_bytes + ' bytes)';
        }
      } catch (e) { console.warn('identify-on-load', e); }
    };
  }

  const origRender = window.renderCurrentEditor;
  if (typeof origRender === 'function') {
    window.renderCurrentEditor = function () {
      origRender();
      if (window.currentEditorTab === 'hex' && window.currentBin && window.currentTable) {
        const el = document.getElementById('editor-content');
        if (!el) return;
        let start = 0;
        const parsed = parseInt(String(window.currentTable.addr || '0'), 16);
        if (!isNaN(parsed) && parsed >= 0 && parsed < window.currentBin.length) start = parsed;
        const len = Math.min(512, Math.max(0, window.currentBin.length - start));
        let html = '<pre style="font-size:10px;line-height:1.3;">';
        for (let i = 0; i < len; i += 16) {
          const addr = (start + i).toString(16).padStart(6, '0');
          let hex = '', ascii = '';
          for (let j = 0; j < 16; j++) {
            if (start + i + j < window.currentBin.length) {
              const b = window.currentBin[start + i + j];
              hex += b.toString(16).padStart(2, '0') + ' ';
              ascii += (b >= 32 && b < 127) ? String.fromCharCode(b) : '.';
            }
          }
          html += addr + ': ' + hex + ' | ' + ascii + '\n';
        }
        el.innerHTML = html + '</pre>';
      }
    };
  }

  window.addEventListener('DOMContentLoaded', () => {
    document.getElementById('btn-log-import')?.addEventListener('click', async () => {
      const input = document.createElement('input');
      input.type = 'file';
      input.accept = '.csv,text/csv';
      input.onchange = async (ev) => {
        const file = ev.target.files[0];
        if (!file) return;
        try {
          const text = await file.text();
          const raw = await window.invokeCmd('log_import_csv', { csv: text });
          alert('CSV import: ' + (typeof raw === 'string' ? raw : JSON.stringify(raw)));
        } catch (e) { alert('CSV import failed: ' + e); }
      };
      input.click();
    });

    document.getElementById('btn-refresh-scripts')?.addEventListener('click', async () => {
      try {
        const t = await window.invokeCmd('list_script_helpers');
        const list = document.getElementById('custom-scripts-list');
        if (list) list.innerHTML = '<pre style="font-size:11px;">' + (typeof t === 'string' ? t : JSON.stringify(t, null, 2)) + '</pre>';
      } catch (e) { console.error(e); }
    });

    const btn = document.getElementById('btn-run-flash');
    if (btn) {
      btn.addEventListener('click', (ev) => {
        const risksOk = ['risk-backup','risk-power','risk-ground','risk-understand'].every(i => document.getElementById(i)?.checked);
        if (!risksOk) {
          ev.stopImmediatePropagation();
          const log = document.getElementById('flash-log');
          if (log) log.textContent = 'Fail-closed: tick every risk checkbox before flashing.\n';
        }
      }, true);
    }
  });
})();
