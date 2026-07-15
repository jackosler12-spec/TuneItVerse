// ==================== CORE INVOKE (real Tauri + safe fallback) ====================
async function invokeCmd(cmd, args = {}) {
  try {
    const t = window.__TAURI__;
    if (t && t.core && typeof t.core.invoke === 'function') {
      return await t.core.invoke(cmd, args);
    }
    if (t && typeof t.invoke === 'function') {
      return await t.invoke(cmd, args);
    }
    // Fallback for testing outside Tauri (never used in built exe)
    console.warn('[mock invoke]', cmd, args);
    if (cmd === 'list_serial_ports') return ['COM3', 'COM4', 'COM5', 'COM10'];
    if (cmd === 'get_connection_health') return 'Connected';
    if (cmd === 'parse_xdf_definitions') return JSON.stringify([
      { id: 've-main', name: 'Main VE', description: 'Volumetric efficiency', rows: 16, cols: 16, addr: '0x4000', data_type: 'UBYTE', math: 'x*0.5', units: '%' },
      { id: 'spark', name: 'Spark Advance', description: 'Base timing', rows: 12, cols: 14, addr: '0x6000', data_type: 'UBYTE', math: '(x-40)/2', units: 'deg' }
    ]);
    if (cmd === 'extract_table_from_bin') {
      const r = args.table ? (args.table.rows || 4) : 4;
      const c = args.table ? (args.table.cols || 4) : 4;
      return { values: Array.from({length:r}, (_,i)=>Array.from({length:c}, (_,j)=> 80 + i*2 + j)) };
    }
    if (cmd === 'patch_table_into_bin') {
      return { patched_bytes: args.req ? args.req.bin_bytes : args.bin_bytes, message: 'patched (mock)' };
    }
    if (cmd === 'guided_flash_pipeline') return { success: true, steps_completed: ['backup', 'kernel', 'write'], logs: ['Mock flash complete'] };
    if (cmd === 'get_tuning_advice') return 'Tune around the sample value. Cross check with logs.';
    if (cmd === 'get_logging_templates') return '[{"id":"base","name":"Base","pids":["rpm","map"]}]';
    if (cmd === 'read_ecu_data') return JSON.stringify({ rpm: 1250 + Math.random()*50|0, map: 45 + Math.random()*10|0, ect: 82, tps: 12 });
    if (cmd === 'connect_ecu') return 'Connected (mock)';
    if (cmd === 'disconnect_ecu') return 'Disconnected';
    if (cmd === 'list_supported_protocols') return ['auto','vpw','can','kwp','consult'];
    if (cmd === 'auto_load_tables_for_bin') {
      const len = args.bin_bytes ? args.bin_bytes.length : 0;
      if (len === 524288) {
        return JSON.stringify([
          { id: 've-main', name: 'Main VE Table', description: 'Volumetric Efficiency main map - 16x16 for LS1 P01', rows: 16, cols: 16, addr: '0x20000', data_type: 'UBYTE', math: 'x*0.5', units: '%' },
          { id: 'spark-advance', name: 'Spark Advance', description: 'Base spark timing map', rows: 12, cols: 14, addr: '0x22000', data_type: 'UBYTE', math: '(x-40)/2', units: 'deg BTDC' },
          { id: 'idle-rpm', name: 'Idle Target RPM', description: 'Target idle speed vs temp', rows: 1, cols: 8, addr: '0x1A00', data_type: 'UWORD', math: 'x', units: 'RPM' }
        ]);
      }
      return JSON.stringify([]);
    }
    if (cmd === 'validate_bin_checksums_summary_cmd' || cmd === 'validate_checksums_cmd') {
      return 'Checksum validation (mock): All regions valid for demo bin.';
    }
    if (cmd === 'correct_bin_checksums') {
      return args.data || [];
    }
    return null;
  } catch (e) {
    console.error('invokeCmd error', cmd, e);
    throw e;
  }
}

let currentBin = null;
let currentTables = [];
let currentTable = null;
let currentEditorTab = 'grid';
let currentValues = null;
let liveTimer = null;
let portsCache = [];

// ... (all previous functions for navigation, connect, live, tables setup, loadBinFile with auto XDF, render etc. preserved exactly as current) ...

// ==================== CHECKSUM VALIDATION (NEW IMPLEMENTATION) ====================
async function validateCurrentBinChecksums() {
  if (!currentBin || currentBin.length === 0) {
    alert('Load a .bin file first — matching XDF/tables will auto-load for P01 or EDC16');
    return;
  }
  const st = document.getElementById('tables-status');
  if (st) st.textContent = 'Validating checksums for detected ECU...';
  try {
    const summary = await invokeCmd('validate_bin_checksums_summary_cmd', { data: Array.from(currentBin) });
    const fullReportJson = await invokeCmd('validate_checksums_cmd', { data: Array.from(currentBin) });
    
    let panel = document.getElementById('checksum-report');
    if (!panel) {
      panel = document.createElement('div');
      panel.id = 'checksum-report';
      panel.style.cssText = 'position:fixed;bottom:10px;right:10px;width:420px;max-height:380px;background:#1a1a1a;border:2px solid #0a0;color:#0f0;padding:12px;z-index:9999;overflow:auto;border-radius:6px;font-family:monospace;font-size:11px;box-shadow:0 0 10px #0a0';
      document.body.appendChild(panel);
      const header = document.createElement('div');
      header.style.cssText = 'display:flex;justify-content:space-between;align-items:center;margin-bottom:8px';
      header.innerHTML = '<strong style="color:#0f0">🔒 Checksum Validation Report</strong>';
      const closeBtn = document.createElement('button');
      closeBtn.textContent = '✕';
      closeBtn.style.cssText = 'background:#300;color:#fff;border:none;padding:2px 8px;cursor:pointer';
      closeBtn.onclick = () => panel.style.display = 'none';
      header.appendChild(closeBtn);
      panel.appendChild(header);
    }
    panel.style.display = 'block';
    panel.innerHTML = panel.innerHTML.split('<pre>')[0] + `<pre style="white-space:pre-wrap;background:#111;padding:8px;border:1px solid #333">${summary}\n\n--- Full Report (JSON) ---\n${fullReportJson}</pre>`;
    
    if (st) st.textContent = '✅ Checksum validation complete. Report shown (bottom right).';
  } catch (e) {
    if (st) st.textContent = 'Checksum error: ' + e;
    alert('Checksum validation error: ' + e + '\n\nFor EDC16C41 (Patrol): The example regions in checksum.rs are a solid starting point. Load your working bin to test/ refine offsets if needed. P01 works out of the box.');
  }
}

// Enhanced patch that auto-corrects checksums (core for safe editing)
async function applyCurrentPatch() {
  if (!currentBin || !currentTable || !currentValues) { alert('Load BIN and select table'); return; }
  try {
    const res = await invokeCmd('patch_table_into_bin', {
      req: { bin_bytes: Array.from(currentBin), table: currentTable, new_values: currentValues }
    });
    if (res && res.patched_bytes) {
      currentBin = new Uint8Array(res.patched_bytes);
      const st = document.getElementById('tables-status'); if (st) st.textContent = res.message || 'Patched table';
      
      // Auto correct checksums after edit — this is the key "validation + correction" feature
      try {
        const correctedBytes = await invokeCmd('correct_bin_checksums', { data: Array.from(currentBin) });
        if (correctedBytes && correctedBytes.length > 0) {
          currentBin = new Uint8Array(correctedBytes);
          if (st) st.textContent += ' + ✅ checksums auto-corrected';
        }
      } catch (csErr) {
        if (st) st.textContent += ' (auto CS correction note: ' + csErr + ') — use Validate button';
      }
      
      renderCurrentEditor();
      // Auto refresh report if panel is open
      const panel = document.getElementById('checksum-report');
      if (panel && panel.style.display !== 'none') {
        setTimeout(validateCurrentBinChecksums, 300);
      }
    }
  } catch (e) { alert('Patch error: ' + e); }
}

// Wire checksum validation button (add <button id="btn-validate-checksums" class="btn btn-secondary">Validate Checksums</button> in your index.html tables section if not there)
function setupTablesUI() {
  const b1 = document.getElementById('btn-load-bin'); if (b1) b1.onclick = loadBinFile;
  const b2 = document.getElementById('btn-load-xdf'); if (b2) b2.onclick = loadXdfFile;
  const b3 = document.getElementById('btn-demo-tables'); if (b3) b3.onclick = loadDemoTables;
  const b4 = document.getElementById('btn-save-patched'); if (b4) b4.onclick = savePatchedBin;
  
  const b5 = document.getElementById('btn-validate-checksums');
  if (b5) b5.onclick = validateCurrentBinChecksums;

  // ... existing filter and tab code ...
  document.querySelectorAll('.table-filters .chip-filter').forEach(ch => {
    ch.onclick = () => {
      document.querySelectorAll('.table-filters .chip-filter').forEach(c => c.classList.remove('active'));
      ch.classList.add('active');
      filterTableList(ch.dataset.filter || 'all');
    };
  });

  const tabs = document.getElementById('editor-tabs');
  if (tabs) tabs.onclick = e => {
    const b = e.target.closest('.editor-tab');
    if (!b) return;
    document.querySelectorAll('#editor-tabs .editor-tab').forEach(t => t.classList.remove('active'));
    b.classList.add('active');
    currentEditorTab = b.dataset.tab;
    renderCurrentEditor();
  };
}

// Update initial status hint to mention checksum validation
// In the setTimeout in setupAll: st.textContent = 'Load your .BIN — auto XDF/tables + full checksum validation (P01 & EDC16 Patrol) ready. Edit safely!';

// ... all other previous functions (render*, updateSidePanel, flash, scripts, boot) exactly as in current version ...