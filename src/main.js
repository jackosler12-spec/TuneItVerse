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
      // Mock for auto XDF load based on BIN size/family
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

// ==================== NAV + VIEWS ====================
function switchView(view) {
  document.querySelectorAll('.content').forEach(c => c.classList.add('content--hidden'));
  const t = document.getElementById('view-' + view);
  if (t) t.classList.remove('content--hidden');
  document.querySelectorAll('.nav-item').forEach(a => a.classList.toggle('active', a.dataset.view === view));
}

function setupNavigation() {
  document.querySelectorAll('.nav-item[data-view]').forEach(link => {
    link.addEventListener('click', e => { e.preventDefault(); switchView(link.dataset.view); });
  });
  const top = document.getElementById('btn-connect-top');
  if (top) top.onclick = () => switchView('connect');

  // dashboard quick buttons
  const d1 = document.getElementById('dash-tables-btn'); if (d1) d1.onclick = () => switchView('tables');
  const d2 = document.getElementById('dash-flash-btn'); if (d2) d2.onclick = () => switchView('flash');
  const d3 = document.getElementById('dash-connect-btn'); if (d3) d3.onclick = () => switchView('connect');

  switchView('dashboard');
}

// ==================== STATUS ====================
let statusInterval = null;
async function updateStatus() {
  const el = document.getElementById('connection-status');
  const dash = document.getElementById('dash-conn-hint');
  try {
    const h = await invokeCmd('get_connection_health', {});
    const txt = (typeof h === 'string') ? h : JSON.stringify(h);
    if (el) { el.textContent = txt.includes('Connected') ? txt : 'Disconnected'; el.style.color = txt.includes('Connected') ? '#0f0' : '#f66'; }
    if (dash) dash.textContent = txt;
  } catch {
    if (el) { el.textContent = 'Disconnected'; el.style.color = '#f66'; }
    if (dash) dash.textContent = 'Disconnected';
  }
}
function startStatus() {
  if (statusInterval) clearInterval(statusInterval);
  updateStatus();
  statusInterval = setInterval(updateStatus, 1800);
}

// ==================== CONNECT (full wired) ====================
async function loadPorts() {
  const sel = document.getElementById('port-select');
  if (!sel) return;
  sel.innerHTML = '';
  try {
    const ports = await invokeCmd('list_serial_ports', {});
    portsCache = (typeof ports === 'string') ? JSON.parse(ports) : ports;
    portsCache.forEach(p => {
      const o = document.createElement('option'); o.value = p; o.textContent = p; sel.appendChild(o);
    });
    if (portsCache.length) sel.value = portsCache[0];
  } catch (e) {
    ['COM3','COM4','COM5','COM10'].forEach(p => { const o = document.createElement('option'); o.value=p; o.text=p; sel.appendChild(o); });
  }
}

async function doConnect() {
  const sel = document.getElementById('port-select');
  const baud = parseInt(document.getElementById('baud-select').value, 10) || 115200;
  const protoRadios = document.querySelectorAll('input[name="proto"]');
  let protocol = 'auto';
  protoRadios.forEach(r => { if (r.checked) protocol = r.value; });
  const hwRadios = document.querySelectorAll('input[name="hw"]');
  let hw = 'elm';
  hwRadios.forEach(r => { if (r.checked) hw = r.value; });
  const jpath = document.getElementById('j2534-path') ? document.getElementById('j2534-path').value : '';

  const log = document.getElementById('connect-log');
  if (log) log.textContent = `Connecting ${sel ? sel.value : ''} @${baud} proto=${protocol} hw=${hw}...\n`;

  try {
    let res;
    if (hw === 'j2534') {
      res = await invokeCmd('connect_ecu', { port_name: sel ? sel.value : 'J2534', baud: 500000, protocol: 'j2534' });
    } else {
      res = await invokeCmd('connect_ecu', { port_name: sel ? sel.value : 'COM3', baud, protocol });
    }
    if (log) log.textContent += 'Result: ' + res + '\n';
    await updateStatus();
  } catch (e) {
    if (log) log.textContent += 'ERROR: ' + e + '\n';
  }
}

async function doDisconnect() {
  const log = document.getElementById('connect-log');
  try {
    const r = await invokeCmd('disconnect_ecu', {});
    if (log) log.textContent = 'Disconnected: ' + r;
    await updateStatus();
  } catch (e) { if (log) log.textContent = 'Disconnect err: ' + e; }
}

async function autoDetect() {
  const sel = document.getElementById('port-select');
  const log = document.getElementById('connect-log');
  const p = sel ? sel.value : 'COM3';
  if (log) log.textContent = 'Auto detecting on ' + p + '...\n';
  try {
    const r = await invokeCmd('auto_detect_protocol', { port_name: p });
    if (log) log.textContent += r + '\n';
  } catch (e) { if (log) log.textContent += 'Detect err: ' + e + '\n'; }
}

function setupConnect() {
  const refresh = document.getElementById('btn-refresh-ports');
  if (refresh) refresh.onclick = loadPorts;

  const conn = document.getElementById('btn-do-connect');
  if (conn) conn.onclick = doConnect;

  const dis = document.getElementById('btn-do-disconnect');
  if (dis) dis.onclick = doDisconnect;

  const ad = document.getElementById('btn-auto-detect');
  if (ad) ad.onclick = autoDetect;

  const hwEls = document.querySelectorAll('input[name="hw"]');
  hwEls.forEach(r => r.addEventListener('change', () => {
    const g = document.getElementById('j2534-group');
    if (g) g.style.display = (document.querySelector('input[name="hw"]:checked').value === 'j2534') ? 'block' : 'none';
  }));

  loadPorts();
}

// ==================== LIVE DATA (full wired) ====================
function updateLiveUI(data) {
  let obj = {};
  try { obj = (typeof data === 'string') ? JSON.parse(data) : data; } catch { obj = { rpm: 1200, map: 50 }; }
  const set = (id, v) => { const el = document.getElementById(id); if (el) el.textContent = v; };
  set('kpi-rpm', obj.rpm || '--');
  set('kpi-map', obj.map || '--');
  set('kpi-ect', obj.ect || '--');
  set('kpi-tps', obj.tps || '--');

  const pids = document.getElementById('live-pids');
  if (pids) pids.innerHTML = Object.entries(obj).map(([k,v]) => `${k}: ${v}`).join(' | ');

  const c = document.getElementById('live-chart');
  if (c) {
    const ctx = c.getContext('2d');
    ctx.fillStyle = '#111'; ctx.fillRect(0,0,c.width,c.height);
    ctx.strokeStyle = '#0f0'; ctx.beginPath();
    const rpm = (obj.rpm || 1200) / 8000 * c.height;
    ctx.moveTo(10, c.height - rpm);
    ctx.lineTo(c.width-10, c.height - rpm);
    ctx.stroke();
  }
}

async function readOnce() {
  try {
    const d = await invokeCmd('read_ecu_data', {});
    updateLiveUI(d);
  } catch (e) { console.error(e); }
}

function startLive() {
  if (liveTimer) clearInterval(liveTimer);
  const st = document.getElementById('live-status'); if (st) st.textContent = 'LIVE';
  liveTimer = setInterval(readOnce, 450);
  readOnce();
}

function stopLive() {
  if (liveTimer) { clearInterval(liveTimer); liveTimer = null; }
  const st = document.getElementById('live-status'); if (st) st.textContent = 'stopped';
}

function setupLive() {
  const s = document.getElementById('btn-start-log'); if (s) s.onclick = startLive;
  const p = document.getElementById('btn-stop-log'); if (p) s.onclick = stopLive;
  const o = document.getElementById('btn-read-once'); if (o) o.onclick = readOnce;
}

// ==================== TABLES (complete & wired) ====================
function setupTablesUI() {
  const b1 = document.getElementById('btn-load-bin'); if (b1) b1.onclick = loadBinFile;
  const b2 = document.getElementById('btn-load-xdf'); if (b2) b2.onclick = loadXdfFile;
  const b3 = document.getElementById('btn-demo-tables'); if (b3) b3.onclick = loadDemoTables;
  const b4 = document.getElementById('btn-save-patched'); if (b4) b4.onclick = savePatchedBin;

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

function filterTableList(type) {
  const list = document.getElementById('tables-list');
  if (!list) return;
  list.querySelectorAll('.table-item').forEach(it => {
    it.style.display = (type === 'all' || it.dataset.type === type) ? '' : 'none';
  });
}

async function loadBinFile() {
  const inp = document.createElement('input'); inp.type = 'file'; inp.accept = '.bin';
  inp.onchange = async e => {
    const f = e.target.files[0]; if (!f) return;
    currentBin = new Uint8Array(await f.arrayBuffer());
    const st = document.getElementById('tables-status');
    if (st) st.textContent = `BIN loaded: ${f.name} (${currentBin.length} bytes) - detecting ECU and loading matching XDF...`;
    try { 
      await invokeCmd('discover_maps_from_bin', { bin_bytes: Array.from(currentBin), family: currentBin.length === 524288 ? 'P01_0411' : 'unknown' }); 
    } catch {}
    // AUTO LOAD CORRESPONDING XDF / TABLES for the BIN (key fix for functionality)
    try {
      const tablesRes = await invokeCmd('auto_load_tables_for_bin', { bin_bytes: Array.from(currentBin) });
      if (tablesRes) {
        currentTables = typeof tablesRes === 'string' ? JSON.parse(tablesRes) : tablesRes;
        renderTablesList();
        if (st) st.textContent = `BIN loaded + auto XDF tables loaded (${currentTables.length} maps/tables matched to parameters)`;
      }
    } catch (err) {
      // Fallback to demo if auto fails
      loadDemoTables();
      if (st) st.textContent = `BIN loaded (${currentBin.length} bytes). Auto XDF not available for this ECU - using demo tables. Load custom XDF if needed.`;
    }
  };
  inp.click();
}

async function loadXdfFile() {
  const inp = document.createElement('input'); inp.type = 'file'; inp.accept = '.xml,.xdf';
  inp.onchange = async e => {
    const f = e.target.files[0]; if (!f) return;
    const xml = await f.text();
    try {
      const res = await invokeCmd('parse_xdf_definitions', { xml });
      currentTables = typeof res === 'string' ? JSON.parse(res) : res;
      renderTablesList();
      const st = document.getElementById('tables-status');
      if (st) st.textContent = `XDF loaded (${currentTables.length} tables) - maps now represent real ECU parameters`;
    } catch (err) { alert('XDF parse: ' + err); }
  };
  inp.click();
}

function loadDemoTables() {
  currentTables = [
    { id:'ve', name:'Main VE', description:'Volumetric efficiency main', rows:8, cols:8, addr:'0x4000', data_type:'UBYTE', math:'x*0.5', units:'%' },
    { id:'spark', name:'Spark', description:'Base spark', rows:6, cols:8, addr:'0x6000', data_type:'UBYTE', math:'(x-40)/2', units:'°' },
    { id:'idle', name:'Idle RPM', description:'Target idle', rows:1, cols:6, addr:'0x1A00', data_type:'UWORD', math:'x', units:'RPM' }
  ];
  renderTablesList();
  const st = document.getElementById('tables-status'); if (st) st.textContent = 'Demo tables loaded (use for testing; load real XDF for your ECU)';
}

function renderTablesList() {
  const listEl = document.getElementById('tables-list'); if (!listEl) return;
  listEl.innerHTML = '';
  currentTables.forEach((t, i) => {
    const d = document.createElement('div');
    d.className = 'table-item';
    const typ = t.rows > 1 && t.cols > 1 ? (t.rows > 4 ? 't3d' : 't2d') : 't1d';
    d.dataset.type = typ.slice(1);
    d.innerHTML = `<span class="tbl-type ${typ}">${typ.slice(1).toUpperCase()}</span><span class="tbl-name">${t.name}</span>`;
    d.onclick = () => selectTable(i, t);
    listEl.appendChild(d);
  });
}

function selectTable(i, def) {
  currentTable = def;
  document.querySelectorAll('#tables-list .table-item').forEach((el, idx) => el.classList.toggle('active', idx === i));
  currentValues = makeDefaultValues(def);
  if (currentBin && currentBin.length) {
    invokeCmd('extract_table_from_bin', { bin_bytes: Array.from(currentBin), table: def })
      .then(ex => { if (ex && ex.values) currentValues = ex.values; renderCurrentEditor(); })
      .catch(() => {});
  }
  renderCurrentEditor();
  updateSidePanel(def);
}

function makeDefaultValues(def) {
  const r = def.rows || 4, c = def.cols || 4;
  return Array.from({length:r}, (_,i) => Array.from({length:c}, (_,j) => 70 + ((i+j)%40)));
}

function renderCurrentEditor() {
  const c = document.getElementById('editor-content'); if (!c || !currentTable) return;
  c.innerHTML = '';
  if (currentEditorTab === 'grid') renderGrid(c);
  else if (currentEditorTab === '3d') render3D(c);
  else renderHex(c);
}

function renderGrid(cont) {
  const tbl = document.createElement('table');
  tbl.style.cssText = 'width:100%;border-collapse:collapse;font-size:11px';
  (currentValues || []).forEach((row, ri) => {
    const tr = tbl.insertRow();
    row.forEach((v, ci) => {
      const td = tr.insertCell();
      td.style.cssText = 'border:1px solid #444;padding:1px';
      const inp = document.createElement('input');
      inp.type = 'number'; inp.value = v; inp.style.cssText = 'width:46px;background:#111;color:#ddd;border:1px solid #333';
      inp.onchange = () => { currentValues[ri][ci] = parseFloat(inp.value) || 0; };
      td.appendChild(inp);
    });
  });
  cont.appendChild(tbl);
  const b = document.createElement('button'); b.textContent = 'Apply Patch'; b.className = 'btn btn-primary'; b.style.marginTop = '6px';
  b.onclick = applyCurrentPatch; cont.appendChild(b);
}

function render3D(cont) {
  const cv = document.createElement('canvas'); cv.width=480; cv.height=280; cv.style.border='1px solid #333';
  cont.appendChild(cv);
  const ctx = cv.getContext('2d');
  const vals = currentValues || []; const r = vals.length, cc = r ? vals[0].length : 0;
  const cw = Math.max(8, cv.width / Math.max(1,cc)), ch = Math.max(8, cv.height / Math.max(1,r));
  let mx = 1; vals.forEach(row => row.forEach(v => mx = Math.max(mx, v||0)));
  for (let i=0; i<r; i++) for (let j=0; j<cc; j++) {
    const norm = mx ? (vals[i][j]||0)/mx : 0;
    ctx.fillStyle = `hsl(${220 - norm*80}, 70%, 55%)`;
    ctx.fillRect(j*cw, i*ch, cw-1, ch-1);
  }
}

function renderHex(cont) {
  const pre = document.createElement('pre'); pre.style.cssText = 'font-size:10px;background:#111;padding:6px;max-height:240px;overflow:auto';
  if (!currentBin || !currentTable) { pre.textContent = 'Load BIN + select table'; }
  else {
    let s = ''; const base = 0x4000;
    for (let i=0; i<Math.min(128, currentBin.length-base); i+=16) {
      s += (base+i).toString(16).padStart(4,'0') + ': ' + Array.from(currentBin.slice(base+i, base+i+16)).map(b=>b.toString(16).padStart(2,'0')).join(' ') + '\n';
    }
    pre.textContent = s;
  }
  cont.appendChild(pre);
}

async function applyCurrentPatch() {
  if (!currentBin || !currentTable || !currentValues) { alert('Load BIN and select table'); return; }
  try {
    const res = await invokeCmd('patch_table_into_bin', {
      req: { bin_bytes: Array.from(currentBin), table: currentTable, new_values: currentValues }
    });
    if (res && res.patched_bytes) {
      currentBin = new Uint8Array(res.patched_bytes);
      const st = document.getElementById('tables-status'); if (st) st.textContent = res.message || 'Patched';
      renderCurrentEditor();
    }
  } catch (e) { alert('Patch error: ' + e); }
}

async function updateSidePanel(def) {
  const m = document.getElementById('side-meta'); const a = document.getElementById('side-advice'); const cs = document.getElementById('side-checksum');
  if (m) m.innerHTML = `<b>${def.name}</b><br>${def.description||''}<br>Addr ${def.addr} ${def.rows}x${def.cols} ${def.data_type}`;
  if (a) {
    try { a.textContent = await invokeCmd('get_tuning_advice', { table_id: def.id||def.name, sample_value: 100, ecu_family: 'P01' }); } catch { a.textContent = 'Advice unavailable'; }
  }
  if (cs) cs.textContent = 'Checksums handled on flash.';
}

async function savePatchedBin() {
  if (!currentBin) { alert('No data'); return; }
  const blob = new Blob([currentBin.buffer], {type:'application/octet-stream'});
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a'); a.href = url; a.download = 'patched.bin'; a.click(); URL.revokeObjectURL(url);
}

// ==================== FLASH (risk + full pipeline wired) ====================
function showRiskModal() {
  const m = document.getElementById('risk-modal');
  if (!m) return;
  m.classList.remove('hidden');
  const proceed = document.getElementById('rm-proceed');
  const checks = ['rm-risk1','rm-risk2','rm-risk3','rm-risk4'].map(id => document.getElementById(id));
  const update = () => { if (proceed) proceed.disabled = !checks.every(ch => ch && ch.checked); };
  checks.forEach(ch => { if (ch) ch.onchange = update; });
  if (proceed) proceed.onclick = async () => {
    m.classList.add('hidden');
    const req = {
      ecu_family: 'P01',
      tuned_bin: currentBin ? Array.from(currentBin) : [],
      perform_backup: true,
      auto_correct_checksum: true,
      enable_recovery_prompts: true,
      user_confirmed_risks: true
    };
    const log = document.getElementById('flash-log');
    if (log) log.textContent = 'Starting guided flash...\n';
    try {
      const result = await invokeCmd('guided_flash_pipeline', { request_json: JSON.stringify(req) });
      if (log) log.textContent += (typeof result === 'string' ? result : JSON.stringify(result, null, 2));
    } catch (e) {
      if (log) log.textContent += 'ERROR: ' + e;
    }
  };
  const cancel = document.getElementById('rm-cancel'); if (cancel) cancel.onclick = () => m.classList.add('hidden');
}

function setupFlash() {
  const show = document.getElementById('btn-show-risk');
  if (show) show.onclick = showRiskModal;
  const direct = document.querySelector('#view-flash button');
  if (direct && direct.id !== 'btn-show-risk') direct.onclick = showRiskModal;
}

// ==================== SCRIPTS ====================
async function refreshScripts() {
  const el = document.getElementById('custom-scripts-list');
  if (!el) return;
  try {
    const t = await invokeCmd('get_logging_templates', {});
    const arr = typeof t === 'string' ? JSON.parse(t) : (t || []);
    el.innerHTML = arr.map(x => `<div class="panel" style="padding:4px 8px;margin:2px 0;">${x.name || x.id} — ${ (x.pids||[]).join(', ') }</div>`).join('');
  } catch (e) { el.textContent = 'Templates unavailable: ' + e; }
}

function setupScripts() {
  const b = document.getElementById('btn-refresh-scripts');
  if (b) b.onclick = refreshScripts;
}

// ==================== BOOT ====================
function setupAll() {
  setupNavigation();
  setupConnect();
  setupLive();
  setupTablesUI();
  setupFlash();
  setupScripts();

  loadPorts();
  startStatus();
  setTimeout(refreshScripts, 800);

  setTimeout(() => {
    const st = document.getElementById('tables-status');
    if (st && currentTables.length === 0) st.textContent = 'Load your .BIN file - matching XDF/tables will auto-load for supported ECUs (P01, EDC16, etc.).';
  }, 1500);
}

setTimeout(() => {
  startStatus();
  setupAll();
}, 900);

// expose for any inline
window.switchView = switchView;