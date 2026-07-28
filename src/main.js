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
    // Fallback ONLY for browser testing outside Tauri (built .exe never hits this)
    console.warn('[mock invoke]', cmd, args);
    if (cmd === 'list_serial_ports') return ['COM3', 'COM4', 'COM5', 'COM10'];
    if (cmd === 'get_connection_health') return 'Connected';
    if (cmd === 'parse_xdf_definitions') return [
      { id: 've-main', name: 'Main VE', description: 'Volumetric efficiency', rows: 16, cols: 16, addr: '0x4000', data_type: 'UBYTE', math: 'x*0.5', units: '%' },
      { id: 'spark', name: 'Spark Advance', description: 'Base timing', rows: 12, cols: 14, addr: '0x6000', data_type: 'UBYTE', math: '(x-40)/2', units: 'deg' }
    ];
    if (cmd === 'extract_table_from_bin') {
      const r = (args.table && args.table.rows) || 4;
      const c = (args.table && args.table.cols) || 4;
      return { id: (args.table && args.table.id) || 'demo', values: Array.from({length:r}, (_,i)=>Array.from({length:c}, (_,j)=> 80 + i*2 + j)), axes_x: [], axes_y: [], note: null };
    }
    if (cmd === 'patch_table_into_bin') {
      const bytes = (args.req && args.req.bin_bytes) || args.bin_bytes || [];
      return { patched_bytes: bytes, message: 'patched (mock)', checksum_report: null };
    }
    if (cmd === 'guided_flash_pipeline') return JSON.stringify({ success: true, steps_completed: ['backup', 'kernel', 'write'], logs: ['Mock flash complete'] });
    if (cmd === 'get_tuning_advice') return 'Tune around the sample value. Cross check with logs.';
    if (cmd === 'get_logging_templates') return '[{"id":"base","name":"Base","pids":["rpm","map"]}]';
    if (cmd === 'read_ecu_data') return JSON.stringify({ rpm: 1250 + Math.random()*50|0, map: 45 + Math.random()*10|0, ect: 82, tps: 12, iat: 30, spark: 22, inj_ms: 3.5, stft: 0.2, batt: 13.8 });
    if (cmd === 'connect_ecu') return 'Connected (mock)';
    if (cmd === 'disconnect_ecu') return 'Disconnected';
    if (cmd === 'list_supported_protocols') return ['auto','vpw','can','kwp','consult'];
    if (cmd === 'list_supported_ecus') return ['P01_0411','EDC16C41','GM_P59','MED17_COMMON','EDC17_COMMON'];
    if (cmd === 'auto_load_tables_for_bin') {
      const len = (args.bin_bytes && args.bin_bytes.length) || 0;
      if (len === 524288 || len === 131072) {
        return JSON.stringify([
          { id: 've-main', name: 'Main VE Table', description: 'Volumetric Efficiency main map - 16x16 for LS1 P01', rows: 16, cols: 16, addr: '0x0000', data_type: 'UBYTE', math: 'x*0.5', units: '%', category: 'Fuel', row_major: true, msb: true },
          { id: 'spark-advance', name: 'Spark Advance', description: 'Base spark timing map', rows: 12, cols: 14, addr: '0x2000', data_type: 'UBYTE', math: '(x-40)/2', units: 'deg BTDC', category: 'Ignition', row_major: true, msb: true },
          { id: 'idle-rpm', name: 'Idle Target RPM', description: 'Target idle speed vs temp', rows: 1, cols: 8, addr: '0x1A00', data_type: 'UWORD', math: 'x', units: 'RPM', category: 'Idle', row_major: true, msb: true }
        ]);
      }
      if (len === 2097152) {
        return JSON.stringify([
          { id: 'driver-wish', name: 'Driver Wish (Torque)', description: 'Driver requested torque', rows: 16, cols: 16, addr: '0x80000', data_type: 'UWORD', math: 'x*0.1', units: 'Nm', category: 'Torque', row_major: true, msb: true },
          { id: 'inj-quantity', name: 'Injection Quantity', description: 'IQ main map', rows: 16, cols: 16, addr: '0x82000', data_type: 'UWORD', math: 'x*0.01', units: 'mm3', category: 'Fuel', row_major: true, msb: true },
          { id: 'boost-setpoint', name: 'Boost Setpoint', description: 'Target boost', rows: 12, cols: 12, addr: '0xC0000', data_type: 'UWORD', math: 'x*0.1', units: 'mbar', category: 'Boost', row_major: true, msb: true },
          { id: 'rail-pressure', name: 'Rail Pressure', description: 'Rail pressure setpoint', rows: 12, cols: 12, addr: '0xC2000', data_type: 'UWORD', math: 'x', units: 'bar', category: 'Fuel', row_major: true, msb: true },
          { id: 'smoke-limiter', name: 'Smoke Limiter', description: 'Smoke limiter map', rows: 10, cols: 10, addr: '0xC6000', data_type: 'UWORD', math: 'x*0.1', units: '%', category: 'Limiters', row_major: true, msb: true }
        ]);
      }
      return JSON.stringify([]);
    }
    if (cmd === 'validate_bin_checksums_summary_cmd' || cmd === 'validate_checksums_cmd') {
      return 'Checksum validation (mock): All regions valid for demo bin.';
    }
    if (cmd === 'correct_bin_checksums') return args.data || [];
    if (cmd === 'auto_detect_protocol') return 'Detected: VPW/J1850 (mock)';
    if (cmd === 'read_properties') return JSON.stringify({ os_id: '12225074', vin: 'MOCKVIN', hardware: '0411', ecu_type: 'P01', protocol: 'VPW', status: 'Mock' });
    return null;
  } catch (e) {
    console.error('invokeCmd error', cmd, e);
    throw e;
  }
}

// ==================== GLOBAL STATE ====================
let currentBin = null;
let currentTables = [];
let currentTable = null;
let currentEditorTab = 'grid';
let currentValues = null;
let liveTimer = null;
let portsCache = [];
let isConnected = false;

// ==================== NAVIGATION ====================
function showView(name) {
  document.querySelectorAll('.content').forEach(el => el.classList.add('content--hidden'));
  const v = document.getElementById('view-' + name);
  if (v) v.classList.remove('content--hidden');
  document.querySelectorAll('.nav-item').forEach(a => a.classList.toggle('active', a.dataset.view === name));
}

function setupNav() {
  document.querySelectorAll('.nav-item').forEach(a => {
    a.onclick = (e) => { e.preventDefault(); showView(a.dataset.view); };
  });
  const topBtn = document.getElementById('btn-connect-top');
  if (topBtn) topBtn.onclick = () => showView('connect');
  const d1 = document.getElementById('dash-tables-btn'); if (d1) d1.onclick = () => showView('tables');
  const d2 = document.getElementById('dash-flash-btn'); if (d2) d2.onclick = () => showView('flash');
  const d3 = document.getElementById('dash-connect-btn'); if (d3) d3.onclick = () => showView('connect');
  const d4 = document.getElementById('dash-dtc-btn'); if (d4) d4.onclick = () => showView('diagnostics');
}

// ==================== DIAGNOSTICS / DTC ====================
function dtcTypeLabel(rec) {
  if (rec.is_permanent) return 'Permanent';
  if (rec.is_pending) return 'Pending';
  return 'Stored';
}

function renderDtcRows(result) {
  const tbody = document.getElementById('dtc-tbody');
  const summary = document.getElementById('dtc-summary');
  if (!tbody) return;
  const rows = [];
  const pushGroup = (list) => {
    (list || []).forEach((rec) => {
      rows.push(rec);
    });
  };
  pushGroup(result.stored);
  pushGroup(result.pending);
  pushGroup(result.permanent);
  if (summary) {
    summary.textContent = `Total ${result.total ?? rows.length} — stored ${result.stored?.length ?? 0}, pending ${result.pending?.length ?? 0}, permanent ${result.permanent?.length ?? 0}`;
  }
  if (rows.length === 0) {
    tbody.innerHTML = '<tr><td colspan="3" style="padding:8px; color:#0a0;">No DTCs reported.</td></tr>';
    return;
  }
  tbody.innerHTML = rows.map((rec) => {
    const code = rec.code || '????';
    const type = dtcTypeLabel(rec);
    const desc = (rec.description || '').replace(/</g, '<');
    return `<tr style="border-bottom:1px solid #222;"><td style="padding:4px 8px; color:#f66;">${code}</td><td style="padding:4px 8px;">${type}</td><td style="padding:4px 8px;">${desc}</td></tr>`;
  }).join('');
}

async function readDtcs() {
  const st = document.getElementById('dtc-status');
  if (st) st.textContent = 'Reading DTCs...';
  try {
    const raw = await invokeCmd('read_dtcs_cmd');
    const result = typeof raw === 'string' ? JSON.parse(raw) : raw;
    renderDtcRows(result || { stored: [], pending: [], permanent: [], total: 0 });
    if (st) st.textContent = 'Read complete';
  } catch (e) {
    if (st) st.textContent = 'Error: ' + e;
    const tbody = document.getElementById('dtc-tbody');
    if (tbody) tbody.innerHTML = `<tr><td colspan="3" style="padding:8px; color:#f66;">${String(e)}</td></tr>`;
  }
}

async function readFreezeFrame() {
  const st = document.getElementById('dtc-status');
  const pre = document.getElementById('freeze-frame');
  if (st) st.textContent = 'Reading freeze frame...';
  try {
    const raw = await invokeCmd('read_freeze_frame_cmd');
    const result = typeof raw === 'string' ? JSON.parse(raw) : raw;
    if (pre) pre.textContent = JSON.stringify(result, null, 2);
    if (st) st.textContent = 'Freeze frame OK';
  } catch (e) {
    if (pre) pre.textContent = String(e);
    if (st) st.textContent = 'Freeze frame error: ' + e;
  }
}

async function clearDtcs() {
  if (!confirm('Clear all DTCs and reset readiness monitors? This cannot be undone.')) return;
  const st = document.getElementById('dtc-status');
  if (st) st.textContent = 'Clearing DTCs...';
  try {
    const raw = await invokeCmd('clear_dtcs_cmd');
    const result = typeof raw === 'string' ? JSON.parse(raw) : raw;
    if (st) st.textContent = result.message || (result.success ? 'Cleared' : 'Clear failed');
    await readDtcs();
  } catch (e) {
    if (st) st.textContent = 'Clear error: ' + e;
  }
}

function setupDiagnostics() {
  document.getElementById('btn-read-dtcs')?.addEventListener('click', readDtcs);
  document.getElementById('btn-read-freeze')?.addEventListener('click', readFreezeFrame);
  document.getElementById('btn-clear-dtcs')?.addEventListener('click', clearDtcs);
}

// ==================== CONNECT ====================
async function refreshPorts() {
  try {
    const ports = await invokeCmd('list_serial_ports');
    portsCache = ports || [];
    const sel = document.getElementById('port-select');
    if (sel) {
      sel.innerHTML = '';
      portsCache.forEach(p => {
        const o = document.createElement('option');
        o.value = p; o.textContent = p;
        sel.appendChild(o);
      });
      if (portsCache.length === 0) {
        const o = document.createElement('option'); o.textContent = 'No ports found'; sel.appendChild(o);
      }
    }
  } catch (e) {
    console.error(e);
  }
}

async function doConnect() {
  const port = document.getElementById('port-select')?.value || 'COM3';
  const baud = parseInt(document.getElementById('baud-select')?.value || '115200', 10);
  const proto = document.querySelector('input[name="proto"]:checked')?.value || 'auto';
  const log = document.getElementById('connect-log');
  if (log) log.textContent = 'Connecting to ' + port + ' @ ' + baud + ' (' + proto + ')...\n';
  try {
    const msg = await invokeCmd('connect_ecu', { port_name: port, baud, protocol: proto });
    isConnected = true;
    updateConnStatus('Connected');
    if (log) log.textContent += msg + '\n';
    try {
      const props = await invokeCmd('read_properties');
      if (log) log.textContent += 'Properties: ' + (typeof props === 'string' ? props : JSON.stringify(props)) + '\n';
    } catch (_) {}
  } catch (e) {
    if (log) log.textContent += 'ERROR: ' + e + '\n';
    updateConnStatus('Error');
  }
}

async function doDisconnect() {
  try {
    await invokeCmd('disconnect_ecu');
    isConnected = false;
    updateConnStatus('Disconnected');
    const log = document.getElementById('connect-log');
    if (log) log.textContent += 'Disconnected.\n';
  } catch (e) {
    console.error(e);
  }
}

async function doAutoDetect() {
  const port = document.getElementById('port-select')?.value || 'COM3';
  const log = document.getElementById('connect-log');
  if (log) log.textContent = 'Auto-detecting on ' + port + '...\n';
  try {
    const res = await invokeCmd('auto_detect_protocol', { port_name: port });
    if (log) log.textContent += res + '\n';
    updateConnStatus('Connected (auto)');
    isConnected = true;
  } catch (e) {
    if (log) log.textContent += 'Detect error: ' + e + '\n';
  }
}

function updateConnStatus(txt) {
  const el = document.getElementById('connection-status');
  if (el) el.textContent = txt;
  const hint = document.getElementById('dash-conn-hint');
  if (hint) hint.textContent = txt;
}

function setupConnect() {
  document.getElementById('btn-refresh-ports')?.addEventListener('click', refreshPorts);
  document.getElementById('btn-do-connect')?.addEventListener('click', doConnect);
  document.getElementById('btn-do-disconnect')?.addEventListener('click', doDisconnect);
  document.getElementById('btn-auto-detect')?.addEventListener('click', doAutoDetect);
  // J2534 toggle
  document.querySelectorAll('input[name="hw"]').forEach(r => {
    r.onchange = () => {
      const g = document.getElementById('j2534-group');
      if (g) g.style.display = r.value === 'j2534' ? 'block' : 'none';
    };
  });
  refreshPorts();
}

// ==================== LIVE DATA ====================
async function readOnce() {
  try {
    const raw = await invokeCmd('read_ecu_data');
    const data = typeof raw === 'string' ? JSON.parse(raw) : raw;
    document.getElementById('kpi-rpm').textContent = data.rpm ?? '--';
    document.getElementById('kpi-map').textContent = data.map ?? '--';
    document.getElementById('kpi-ect').textContent = data.ect ?? '--';
    document.getElementById('kpi-tps').textContent = data.tps ?? '--';
    const pids = document.getElementById('live-pids');
    if (pids) pids.textContent = JSON.stringify(data, null, 2);
    const st = document.getElementById('live-status');
    if (st) st.textContent = 'Last read OK';
    return data;
  } catch (e) {
    const st = document.getElementById('live-status');
    if (st) st.textContent = 'Read error: ' + e;
  }
}

function startLive() {
  if (liveTimer) return;
  const st = document.getElementById('live-status');
  if (st) st.textContent = 'Polling...';
  liveTimer = setInterval(readOnce, 250);
}

function stopLive() {
  if (liveTimer) { clearInterval(liveTimer); liveTimer = null; }
  const st = document.getElementById('live-status');
  if (st) st.textContent = 'Stopped';
}

function setupLive() {
  document.getElementById('btn-start-log')?.addEventListener('click', startLive);
  document.getElementById('btn-stop-log')?.addEventListener('click', stopLive);
  document.getElementById('btn-read-once')?.addEventListener('click', readOnce);
}

// ==================== TABLES / MAPS ====================
async function loadBinFile() {
  // Use file input dynamically
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.bin,.BIN';
  input.onchange = async (ev) => {
    const file = ev.target.files[0];
    if (!file) return;
    const st = document.getElementById('tables-status');
    if (st) st.textContent = 'Loading ' + file.name + ' (' + file.size + ' bytes)...';
    const buf = await file.arrayBuffer();
    currentBin = new Uint8Array(buf);
    // Auto load tables
    try {
      const tablesJson = await invokeCmd('auto_load_tables_for_bin', { bin_bytes: Array.from(currentBin) });
      currentTables = typeof tablesJson === 'string' ? JSON.parse(tablesJson) : (tablesJson || []);
      renderTableList();
      if (st) st.textContent = 'Loaded ' + file.name + ' — auto XDF/tables: ' + currentTables.length + ' maps. Checksums ready.';
      // Optional auto validate
      setTimeout(validateCurrentBinChecksums, 400);
    } catch (e) {
      if (st) st.textContent = 'BIN loaded but auto-tables failed: ' + e;
      currentTables = [];
      renderTableList();
    }
    document.getElementById('btn-save-patched').disabled = false;
  };
  input.click();
}

async function loadXdfFile() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.xdf,.xml,.XML,.XDF';
  input.onchange = async (ev) => {
    const file = ev.target.files[0];
    if (!file) return;
    const text = await file.text();
    const st = document.getElementById('tables-status');
    try {
      const defs = await invokeCmd('parse_xdf_definitions', { xml: text });
      currentTables = Array.isArray(defs) ? defs : (typeof defs === 'string' ? JSON.parse(defs) : []);
      renderTableList();
      if (st) st.textContent = 'XDF/XML loaded: ' + currentTables.length + ' tables';
    } catch (e) {
      if (st) st.textContent = 'XDF parse error: ' + e;
    }
  };
  input.click();
}

function loadDemoTables() {
  currentTables = [
    { id: 'demo-ve', name: 'Demo VE', description: 'Demo volumetric efficiency', rows: 8, cols: 8, addr: '0x4000', data_type: 'UBYTE', math: 'x*0.5', units: '%', category: 'Fuel', row_major: true, msb: true },
    { id: 'demo-spark', name: 'Demo Spark', description: 'Demo timing', rows: 6, cols: 8, addr: '0x5000', data_type: 'UBYTE', math: '(x-40)/2', units: 'deg', category: 'Ignition', row_major: true, msb: true }
  ];
  if (!currentBin) currentBin = new Uint8Array(524288);
  renderTableList();
  document.getElementById('tables-status').textContent = 'Demo tables loaded';
  document.getElementById('btn-save-patched').disabled = false;
}

function renderTableList() {
  const list = document.getElementById('tables-list');
  if (!list) return;
  list.innerHTML = '';
  currentTables.forEach((t, idx) => {
    const div = document.createElement('div');
    div.className = 'table-item';
    div.style.cssText = 'padding:6px 8px; cursor:pointer; border-bottom:1px solid #222; font-size:12px;';
    div.innerHTML = '<strong>' + (t.name || t.id) + '</strong><br><span style="color:#888;font-size:10px;">' + (t.rows||1) + 'x' + (t.cols||1) + ' @ ' + (t.addr||'?') + ' • ' + (t.units||'') + '</span>';
    div.onclick = () => selectTable(idx);
    list.appendChild(div);
  });
}

async function selectTable(idx) {
  currentTable = currentTables[idx];
  if (!currentTable || !currentBin) return;
  const st = document.getElementById('tables-status');
  try {
    const extracted = await invokeCmd('extract_table_from_bin', { bin_bytes: Array.from(currentBin), table: currentTable });
    currentValues = extracted.values || extracted;
    renderCurrentEditor();
    updateSidePanel();
    if (st) st.textContent = 'Selected: ' + currentTable.name;
  } catch (e) {
    if (st) st.textContent = 'Extract error: ' + e;
    // Fallback synthetic
    const r = currentTable.rows || 4, c = currentTable.cols || 4;
    currentValues = Array.from({length:r}, () => Array.from({length:c}, () => 50));
    renderCurrentEditor();
  }
}

function renderCurrentEditor() {
  const el = document.getElementById('editor-content');
  if (!el || !currentValues) return;
  if (currentEditorTab === 'grid') {
    let html = '<table style="border-collapse:collapse;font-size:11px;"><tbody>';
    currentValues.forEach((row, ri) => {
      html += '<tr>';
      row.forEach((v, ci) => {
        html += '<td style="border:1px solid #333;padding:2px 4px;min-width:36px;text-align:center;" contenteditable="true" data-r="'+ri+'" data-c="'+ci+'">' + (typeof v === 'number' ? v.toFixed(1) : v) + '</td>';
      });
      html += '</tr>';
    });
    html += '</tbody></table><div style="margin-top:8px;"><button id="btn-apply-patch" class="btn btn-primary">Apply Patch + Auto Checksum</button></div>';
    el.innerHTML = html;
    document.getElementById('btn-apply-patch')?.addEventListener('click', applyCurrentPatch);
    el.querySelectorAll('td[contenteditable]').forEach(td => {
      td.onblur = () => {
        const r = +td.dataset.r, c = +td.dataset.c;
        const num = parseFloat(td.textContent);
        if (!isNaN(num) && currentValues[r]) currentValues[r][c] = num;
      };
    });
  } else if (currentEditorTab === '3d') {
    el.innerHTML = '<canvas id="viz3d" width="480" height="320" style="background:#111;border:1px solid #333;"></canvas><p style="font-size:11px;color:#888;">3D color map of current table values (higher = brighter).</p>';
    const canvas = document.getElementById('viz3d');
    if (canvas && currentValues.length) {
      const ctx = canvas.getContext('2d');
      const rows = currentValues.length, cols = currentValues[0].length;
      let min = Infinity, max = -Infinity;
      currentValues.forEach(row => row.forEach(v => { if (v < min) min = v; if (v > max) max = v; }));
      const cellW = 480 / cols, cellH = 320 / rows;
      for (let r = 0; r < rows; r++) {
        for (let c = 0; c < cols; c++) {
          const t = max > min ? (currentValues[r][c] - min) / (max - min) : 0.5;
          ctx.fillStyle = 'hsl(' + (120 - t * 120) + ',80%,40%)';
          ctx.fillRect(c * cellW, r * cellH, cellW + 1, cellH + 1);
        }
      }
    }
  } else if (currentEditorTab === 'hex') {
    if (!currentBin) { el.innerHTML = 'No BIN loaded'; return; }
    let html = '<pre style="font-size:10px;line-height:1.3;">';
    const start = 0x20000; // typical cal
    const len = Math.min(512, currentBin.length - start);
    for (let i = 0; i < len; i += 16) {
      const addr = (start + i).toString(16).padStart(6, '0');
      let hex = '', ascii = '';
      for (let j = 0; j < 16; j++) {
        if (start + i + j < currentBin.length) {
          const b = currentBin[start + i + j];
          hex += b.toString(16).padStart(2, '0') + ' ';
          ascii += (b >= 32 && b < 127) ? String.fromCharCode(b) : '.';
        }
      }
      html += addr + ': ' + hex + ' | ' + ascii + '\n';
    }
    html += '</pre>';
    el.innerHTML = html;
  }
}

function updateSidePanel() {
  if (!currentTable) return;
  const meta = document.getElementById('side-meta');
  if (meta) {
    meta.innerHTML = '<div><b>' + currentTable.name + '</b></div>' +
      '<div>ID: ' + currentTable.id + '</div>' +
      '<div>Size: ' + (currentTable.rows||'?') + ' × ' + (currentTable.cols||'?') + '</div>' +
      '<div>Addr: ' + (currentTable.addr||'?') + '</div>' +
      '<div>Type: ' + (currentTable.data_type||'?') + '  Math: ' + (currentTable.math||'X') + '</div>' +
      '<div>Units: ' + (currentTable.units||'') + '</div>' +
      '<div style="margin-top:6px;color:#aaa;">' + (currentTable.description||'') + '</div>';
  }
  // Advisor
  invokeCmd('get_tuning_advice', { table_id: currentTable.id || '', sample_value: 50, ecu_family: currentBin && currentBin.length === 2097152 ? 'EDC16C41' : 'P01_0411' })
    .then(adv => { const a = document.getElementById('side-advice'); if (a) a.textContent = adv; })
    .catch(() => {});
}

async function applyCurrentPatch() {
  if (!currentBin || !currentTable || !currentValues) { alert('Load BIN and select table'); return; }
  try {
    const res = await invokeCmd('patch_table_into_bin', {
      req: { bin_bytes: Array.from(currentBin), table: currentTable, new_values: currentValues }
    });
    if (res && res.patched_bytes) {
      currentBin = new Uint8Array(res.patched_bytes);
      const st = document.getElementById('tables-status');
      if (st) st.textContent = (res.message || 'Patched') + ' — auto-correcting checksums...';
      try {
        const corrected = await invokeCmd('correct_bin_checksums', { data: Array.from(currentBin) });
        if (corrected && corrected.length) {
          currentBin = new Uint8Array(corrected);
          if (st) st.textContent += ' ✅ CS auto-corrected';
        }
      } catch (cs) {
        if (st) st.textContent += ' (CS note: ' + cs + ')';
      }
      renderCurrentEditor();
    }
  } catch (e) { alert('Patch error: ' + e); }
}

async function validateCurrentBinChecksums() {
  if (!currentBin || !currentBin.length) {
    alert('Load a .bin first');
    return;
  }
  const st = document.getElementById('tables-status');
  if (st) st.textContent = 'Validating checksums...';
  try {
    const summary = await invokeCmd('validate_bin_checksums_summary_cmd', { data: Array.from(currentBin) });
    let panel = document.getElementById('checksum-report');
    if (!panel) {
      panel = document.createElement('div');
      panel.id = 'checksum-report';
      panel.style.cssText = 'position:fixed;bottom:10px;right:10px;width:420px;max-height:380px;background:#1a1a1a;border:2px solid #0a0;color:#0f0;padding:12px;z-index:9999;overflow:auto;border-radius:6px;font-family:monospace;font-size:11px;';
      document.body.appendChild(panel);
    }
    panel.style.display = 'block';
    panel.innerHTML = '<div style="display:flex;justify-content:space-between;"><strong>🔒 Checksum Report</strong><button onclick="this.parentElement.parentElement.style.display=\'none\'">✕</button></div><pre style="white-space:pre-wrap;">' + summary + '</pre>';
    if (st) st.textContent = '✅ Checksum validation complete';
  } catch (e) {
    if (st) st.textContent = 'CS error: ' + e;
  }
}

function savePatchedBin() {
  if (!currentBin) return;
  const blob = new Blob([currentBin], { type: 'application/octet-stream' });
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = 'tuned_' + Date.now() + '.bin';
  a.click();
}

function filterTableList(filter) {
  // Simple filter by size
  const list = document.getElementById('tables-list');
  if (!list) return;
  Array.from(list.children).forEach((div, i) => {
    const t = currentTables[i];
    if (!t) return;
    const is1d = (t.rows || 1) === 1 || (t.cols || 1) === 1;
    const is3d = (t.rows || 1) > 1 && (t.cols || 1) > 1 && (t.rows * t.cols > 64);
    let show = true;
    if (filter === '1d') show = is1d;
    else if (filter === '2d') show = !is1d && !is3d;
    else if (filter === '3d') show = is3d;
    div.style.display = show ? '' : 'none';
  });
}

function setupTablesUI() {
  document.getElementById('btn-load-bin')?.addEventListener('click', loadBinFile);
  document.getElementById('btn-load-xdf')?.addEventListener('click', loadXdfFile);
  document.getElementById('btn-demo-tables')?.addEventListener('click', loadDemoTables);
  document.getElementById('btn-save-patched')?.addEventListener('click', savePatchedBin);

  document.querySelectorAll('.table-filters .chip-filter').forEach(ch => {
    ch.onclick = () => {
      document.querySelectorAll('.table-filters .chip-filter').forEach(c => c.classList.remove('active'));
      ch.classList.add('active');
      filterTableList(ch.dataset.filter || 'all');
    };
  });

  const tabs = document.getElementById('editor-tabs');
  if (tabs) {
    tabs.onclick = e => {
      const b = e.target.closest('.editor-tab');
      if (!b) return;
      document.querySelectorAll('#editor-tabs .editor-tab').forEach(t => t.classList.remove('active'));
      b.classList.add('active');
      currentEditorTab = b.dataset.tab;
      renderCurrentEditor();
    };
  }
}

// ==================== FLASH ====================
function setupFlash() {
  document.getElementById('btn-compare-bin')?.addEventListener('click', async () => {
    const pre = document.getElementById('compare-result');
    if (pre) { pre.style.display = 'block'; pre.textContent = 'Comparing...'; }
    if (!currentBin || !currentBin.length) {
      if (pre) pre.textContent = 'Load a .BIN in Tables first.';
      return;
    }
    try {
      const res = await invokeCmd('compare_bin_to_ecu', { file_bytes: Array.from(currentBin) });
      if (pre) pre.textContent = typeof res === 'string' ? res : JSON.stringify(res, null, 2);
    } catch (e) {
      if (pre) pre.textContent = 'Compare error: ' + e;
    }
  });
  document.getElementById('btn-verify-write')?.addEventListener('click', async () => {
    const pre = document.getElementById('compare-result');
    if (pre) { pre.style.display = 'block'; pre.textContent = 'Verifying...'; }
    try {
      const args = currentBin && currentBin.length
        ? { expected_bytes: Array.from(currentBin) }
        : {};
      const res = await invokeCmd('verify_after_write', args);
      if (pre) pre.textContent = typeof res === 'string' ? res : JSON.stringify(res, null, 2);
    } catch (e) {
      if (pre) pre.textContent = 'Verify error: ' + e;
    }
  });
  const showRisk = document.getElementById('btn-show-risk');
  if (showRisk) showRisk.onclick = () => {
    const sec = document.getElementById('risk-section');
    if (sec) sec.style.display = 'block';
  };
  // Enable proceed when all checked
  ['risk-backup','risk-power','risk-ground','risk-understand'].forEach(id => {
    const cb = document.getElementById(id);
    if (cb) cb.onchange = () => {
      const all = ['risk-backup','risk-power','risk-ground','risk-understand'].every(i => document.getElementById(i)?.checked);
      const btn = document.getElementById('btn-run-flash');
      if (btn) btn.disabled = !all;
    };
  });
  document.getElementById('btn-run-flash')?.addEventListener('click', async () => {
    const log = document.getElementById('flash-log');
    const prog = document.getElementById('flash-progress');
    if (log) log.textContent = 'Starting guided flash pipeline...\n';
    try {
      const req = {
        ecu_family: currentBin && currentBin.length === 2097152 ? 'EDC16C41' : 'P01_0411',
        bin_bytes: currentBin ? Array.from(currentBin) : [],
        do_backup: true,
        do_kernel: true,
        do_write: true
      };
      const res = await invokeCmd('guided_flash_pipeline', { request_json: JSON.stringify(req) });
      try {
        const parsed = typeof res === 'string' ? JSON.parse(res) : res;
        if (parsed && parsed.recovery_prompt) {
          const rm = document.getElementById('recovery-modal');
          const body = document.getElementById('recovery-body');
          if (body) {
            const p = parsed.recovery_prompt;
            body.textContent = [
              p.message || '',
              '',
              ...(p.steps || []).map((s, i) => `${i + 1}. ${s}`),
              '',
              p.grounding_required ? 'Grounding assist may be required for locked P01.' : '',
              p.kernel_to_upload ? `Kernel: ${p.kernel_to_upload}` : '',
              p.reference_notes || ''
            ].filter(Boolean).join('\n');
          }
          rm?.classList.remove('hidden');
        }
      } catch (_) { /* non-JSON result */ }
      if (log) log.textContent += (typeof res === 'string' ? res : JSON.stringify(res, null, 2)) + '\n';
      if (prog) prog.textContent = '100%';
    } catch (e) {
      if (log) log.textContent += 'ERROR: ' + e + '\n';
    }
  });

  // Modal too
  const modal = document.getElementById('risk-modal');
  document.getElementById('rm-cancel')?.addEventListener('click', () => modal?.classList.add('hidden'));
  document.getElementById('recovery-close')?.addEventListener('click', () => {
    document.getElementById('recovery-modal')?.classList.add('hidden');
  });
  ['rm-risk1','rm-risk2','rm-risk3','rm-risk4'].forEach(id => {
    document.getElementById(id)?.addEventListener('change', () => {
      const all = ['rm-risk1','rm-risk2','rm-risk3','rm-risk4'].every(i => document.getElementById(i)?.checked);
      const btn = document.getElementById('rm-proceed');
      if (btn) btn.disabled = !all;
    });
  });
}

// ==================== SCRIPTS ====================
async function setupScripts() {
  document.getElementById('btn-refresh-scripts')?.addEventListener('click', async () => {
    try {
      const t = await invokeCmd('get_logging_templates');
      const list = document.getElementById('custom-scripts-list');
      if (list) list.innerHTML = '<pre style="font-size:11px;">' + (typeof t === 'string' ? t : JSON.stringify(t, null, 2)) + '</pre>';
    } catch (e) {
      console.error(e);
    }
  });
}

// ==================== BOOT ====================
function setupAll() {
  setupNav();
  setupConnect();
  setupLive();
  setupDiagnostics();
  setupTablesUI();
  setupFlash();
  setupScripts();
  showView('dashboard');
  const st = document.getElementById('tables-status');
  if (st) st.textContent = 'Load your .BIN — auto XDF/tables + full checksum validation (P01 & EDC16/EDC17/MED17) ready. Edit safely! v0.8.0 fully operational.';
  console.log('TuneItVerse UI fully wired v0.8.0');
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', setupAll);
} else {
  setupAll();
}
