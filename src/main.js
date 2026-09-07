// TuneItVerse v3.10.1 — sidebar buttons + dual-case Tauri IPC.

function parseMaybe(raw) {
  if (raw == null) return null;
  if (typeof raw === 'string') {
    try { return JSON.parse(raw); } catch (_) { return raw; }
  }
  return raw;
}

function dualCaseArgs(args) {
  const out = Object.assign({}, args);
  Object.keys(args || {}).forEach((k) => {
    if (k.indexOf('_') !== -1) {
      const camel = k.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
      if (out[camel] === undefined) out[camel] = args[k];
    }
  });
  return out;
}

async function invokeCmd(cmd, args = {}) {
  const payload = dualCaseArgs(args || {});
  const t = window.__TAURI__;
  if (t && t.core && typeof t.core.invoke === 'function') {
    return await t.core.invoke(cmd, payload);
  }
  if (t && typeof t.invoke === 'function') {
    return await t.invoke(cmd, payload);
  }
  throw new Error('TuneItVerse must run as the desktop app (Tauri). Browser file-open has no ECU backend.');
}
window.invokeCmd = invokeCmd;

const PAGE_TITLES = {
  dashboard: ['Dashboard', 'Honest live data. No invented PIDs.'],
  connect: ['Connect', 'Serial, ELM, or J2534. Fail-closed on silence.'],
  live: ['Data Logging', 'Mode 01 samples or imported CSV only.'],
  diagnostics: ['Diagnostics', 'Stored / pending / permanent DTCs from the adapter.'],
  tables: ['Tables / Maps', 'Your BIN + XDF/A2L. Identify before patch or flash.'],
  flash: ['Flash', 'Identify → voltage → backup → write → live verify.'],
  scripts: ['Scripts', 'Bench CLI helpers. Not an embedded interpreter.']
};

let currentBin = null;
let currentTables = [];
let currentTable = null;
let currentEditorTab = 'grid';
let currentValues = null;
let lastIdentify = null;
let lastMapFromLog = null;
let isConnected = false;
let logPollTimer = null;
let logRunning = false;
let healthTimer = null;
let sparkHistory = [];

window.currentBin = null;
window.currentTables = [];
window.currentTable = null;
window.currentValues = null;
window.lastMapFromLog = null;

function syncGlobals() {
  window.currentBin = currentBin;
  window.currentTables = currentTables;
  window.currentTable = currentTable;
  window.currentValues = currentValues;
  window.lastMapFromLog = lastMapFromLog;
}

function setStatus(msg) {
  const el = document.getElementById('status-center');
  if (el) el.textContent = msg;
}

function banner(id, text) {
  let el = document.getElementById(id);
  if (!el) {
    el = document.createElement('div');
    el.id = id;
    el.className = 'banner-warn';
    const host = document.getElementById('tables-status') || document.getElementById('dash-identify');
    if (host && host.parentNode) host.parentNode.insertBefore(el, host.nextSibling);
    else document.body.appendChild(el);
  }
  el.textContent = text;
}

function showView(name) {
  if (!name) return;
  document.querySelectorAll('[data-view-panel], .content').forEach((el) => {
    const isTarget = el.id === 'view-' + name || el.getAttribute('data-view-panel') === name;
    el.classList.toggle('content--hidden', !isTarget);
    if (isTarget) el.removeAttribute('hidden');
    else el.setAttribute('hidden', '');
  });
  document.querySelectorAll('.nav-item[data-view]').forEach((btn) => {
    btn.classList.toggle('active', btn.getAttribute('data-view') === name);
  });
  const titles = PAGE_TITLES[name] || [name, ''];
  const pt = document.getElementById('page-title');
  const ps = document.getElementById('page-sub');
  if (pt) pt.textContent = titles[0];
  if (ps) ps.textContent = titles[1];
  setStatus('View: ' + titles[0]);
}
window.showView = showView;

function onSidebarClick(e) {
  const nav = e.target.closest('[data-view]');
  if (nav && nav.classList.contains('nav-item')) {
    e.preventDefault();
    e.stopPropagation();
    showView(nav.getAttribute('data-view'));
    return;
  }
  if (e.target.closest('#btn-connect-top')) {
    e.preventDefault();
    e.stopPropagation();
    if (isConnected) doDisconnect();
    else {
      showView('connect');
      refreshPorts();
    }
  }
}

function setupNav() {
  const sidebar = document.getElementById('sidebar');
  if (sidebar && !sidebar.dataset.navBound) {
    sidebar.dataset.navBound = '1';
    sidebar.addEventListener('click', onSidebarClick);
  }
  document.querySelectorAll('.workflow-card[data-go]').forEach((card) => {
    if (card.dataset.bound) return;
    card.dataset.bound = '1';
    card.addEventListener('click', () => showView(card.getAttribute('data-go')));
  });
}

function updateConnStatus(txt, connected) {
  isConnected = !!connected;
  const el = document.getElementById('connection-status');
  if (el) el.textContent = txt;
  const hint = document.getElementById('dash-conn-hint');
  if (hint) hint.textContent = txt;
  const dot = document.getElementById('conn-dot');
  if (dot) dot.classList.toggle('connected', !!connected);
  const btn = document.getElementById('btn-connect-top');
  if (btn) {
    btn.textContent = connected ? 'Disconnect' : 'Connect ECU';
    btn.classList.toggle('connected', !!connected);
  }
  const right = document.getElementById('status-right');
  if (right) right.textContent = connected ? txt : 'Disconnected';
}

async function pollHealth() {
  try {
    const h = await invokeCmd('get_connection_health');
    const text = typeof h === 'string' ? h : String(h);
    const on = /connected/i.test(text) && !/disconnected/i.test(text);
    updateConnStatus(text, on);
    if (on) {
      try {
        const raw = await invokeCmd('read_ecu_data');
        const data = parseMaybe(raw) || {};
        setKpi('kpi-rpm', data.rpm);
        setKpi('kpi-map', data.map);
        setKpi('kpi-ect', data.ect);
        setKpi('kpi-batt', data.batt);
      } catch (_) { /* stay at last honest value */ }
    } else {
      ['kpi-rpm', 'kpi-map', 'kpi-ect', 'kpi-batt'].forEach((id) => setKpi(id, undefined));
    }
  } catch (e) {
    updateConnStatus('Desktop backend unavailable', false);
    setStatus(String(e.message || e));
  }
}

function setKpi(id, value) {
  const el = document.getElementById(id);
  if (!el) return;
  if (value == null || value === '' || Number.isNaN(value)) el.textContent = '—';
  else el.textContent = typeof value === 'number' ? (Number.isInteger(value) ? String(value) : value.toFixed(1)) : String(value);
}

function applyIdentify(info) {
  lastIdentify = info;
  const dash = document.getElementById('dash-identify');
  const chip = document.getElementById('vehicle-chip');
  if (dash && info) {
    dash.textContent = JSON.stringify({
      size: info.bin_size_bytes,
      family: info.family,
      collision: info.size_collision,
      honda_os: info.honda_os,
      gm_p01_os: info.gm_p01_os,
      correction_safe: info.correction_safe,
      notes: info.notes
    }, null, 2);
  }
  if (chip) {
    const fam = (info && (info.family || info.family_by_os)) || 'UNREAD';
    chip.textContent = 'OS: ' + fam;
  }
  if (info && info.size_collision && !info.family) {
    banner('tiv-collision', 'Size collides across catalog families. Confirm OS string before any corrector.');
  }
  if (info && info.honda_os && !info.gm_p01_os) {
    banner('tiv-honda-guard', 'Honda OS string. P01 additive correction is blocked.');
  }
}

function identifiedFamily() {
  if (lastIdentify && lastIdentify.family) return lastIdentify.family;
  return null;
}

// ---------- Connect ----------
async function refreshPorts() {
  const sel = document.getElementById('port-select');
  if (!sel) return;
  sel.innerHTML = '';
  try {
    const ports = await invokeCmd('list_serial_ports');
    const list = Array.isArray(ports) ? ports : [];
    if (!list.length) {
      const o = document.createElement('option');
      o.textContent = 'No ports found';
      o.value = '';
      sel.appendChild(o);
      return;
    }
    list.forEach((p) => {
      const o = document.createElement('option');
      o.value = p;
      o.textContent = p;
      sel.appendChild(o);
    });
  } catch (e) {
    const o = document.createElement('option');
    o.textContent = 'Port list failed';
    o.value = '';
    sel.appendChild(o);
    const log = document.getElementById('connect-log');
    if (log) log.textContent = String(e);
  }
}

async function doConnect() {
  const hw = document.querySelector('input[name="hw"]:checked')?.value || 'elm';
  const log = document.getElementById('connect-log');
  if (hw === 'j2534') {
    const dll = document.getElementById('j2534-path')?.value || '';
    if (log) log.textContent = 'Opening J2534…\n';
    try {
      const msg = await invokeCmd('j2534_connect', dll ? { dll_path: dll } : {});
      if (log) log.textContent += (typeof msg === 'string' ? msg : JSON.stringify(msg)) + '\n';
      await pollHealth();
    } catch (e) {
      if (log) log.textContent += 'ERROR: ' + e + '\n';
    }
    return;
  }
  const port = document.getElementById('port-select')?.value;
  if (!port) {
    if (log) log.textContent = 'Select a serial port first.\n';
    return;
  }
  const baud = parseInt(document.getElementById('baud-select')?.value || '115200', 10);
  const proto = document.querySelector('input[name="proto"]:checked')?.value || 'auto';
  if (log) log.textContent = 'Connecting to ' + port + ' @ ' + baud + ' (' + proto + ')…\n';
  try {
    const msg = await invokeCmd('connect_ecu', { port_name: port, baud, protocol: proto });
    if (log) log.textContent += msg + '\n';
    try {
      const props = await invokeCmd('read_properties');
      const obj = parseMaybe(props);
      if (log) log.textContent += 'Properties: ' + JSON.stringify(obj, null, 2) + '\n';
      if (obj && obj.os_id && obj.os_id !== 'UNREAD') {
        document.getElementById('vehicle-chip').textContent = 'OS: ' + obj.os_id;
      }
    } catch (_) {}
    await pollHealth();
  } catch (e) {
    if (log) log.textContent += 'ERROR: ' + e + '\n';
    updateConnStatus('Error', false);
  }
}

async function doDisconnect() {
  try {
    await invokeCmd('disconnect_ecu');
  } catch (_) {}
  updateConnStatus('Disconnected', false);
  const log = document.getElementById('connect-log');
  if (log) log.textContent += 'Disconnected.\n';
}

async function doAutoDetect() {
  const port = document.getElementById('port-select')?.value;
  const log = document.getElementById('connect-log');
  if (!port) {
    if (log) log.textContent = 'Select a serial port first.\n';
    return;
  }
  if (log) log.textContent = 'Auto-detecting on ' + port + '…\n';
  try {
    const res = await invokeCmd('auto_detect_protocol', { port_name: port });
    if (log) log.textContent += res + '\n';
    await pollHealth();
  } catch (e) {
    if (log) log.textContent += 'Detect error: ' + e + '\n';
    updateConnStatus('Disconnected', false);
  }
}

async function computeSeedKeyUi() {
  const seed = document.getElementById('seed-hex')?.value || '';
  const family = document.getElementById('seed-family')?.value || 'P01_0411';
  const level = document.getElementById('seed-level')?.value || '1';
  const out = document.getElementById('seed-result');
  try {
    const raw = await invokeCmd('compute_seed_key', { seed_hex: seed, family, level });
    const obj = parseMaybe(raw);
    if (out) out.textContent = typeof obj === 'string' ? obj : JSON.stringify(obj, null, 2);
  } catch (e) {
    if (out) out.textContent = 'Error: ' + e;
  }
}

async function refreshJ2534Devices() {
  const box = document.getElementById('j2534-list');
  const log = document.getElementById('connect-log');
  try {
    const list = await invokeCmd('j2534_list_devices');
    const items = Array.isArray(list) ? list : parseMaybe(list) || [];
    if (box) box.textContent = items.length ? items.join('\n') : 'No J2534 FunctionLibrary in the registry.';
    if (log) log.textContent = (log.textContent || '') + 'J2534 devices:\n' + (items.join ? items.join('\n') : String(items)) + '\n';
  } catch (e) {
    if (log) log.textContent += 'J2534 list error: ' + e + '\n';
  }
}

function setupConnect() {
  document.getElementById('btn-refresh-ports')?.addEventListener('click', refreshPorts);
  document.getElementById('btn-do-connect')?.addEventListener('click', doConnect);
  document.getElementById('btn-do-disconnect')?.addEventListener('click', doDisconnect);
  document.getElementById('btn-auto-detect')?.addEventListener('click', doAutoDetect);
  document.getElementById('btn-compute-key')?.addEventListener('click', computeSeedKeyUi);
  document.getElementById('btn-j2534-list')?.addEventListener('click', refreshJ2534Devices);
  document.querySelectorAll('input[name="hw"]').forEach((r) => {
    r.onchange = () => {
      const g = document.getElementById('j2534-group');
      if (g) g.hidden = r.value !== 'j2534';
    };
  });
  refreshPorts();
}

// ---------- Logging ----------
async function refreshLogStatus() {
  try {
    const raw = await invokeCmd('log_get_status');
    const st = parseMaybe(raw);
    logRunning = !!st.running;
    const el = document.getElementById('log-status');
    if (el) el.textContent = st.running
      ? `LOGGING @ ${st.rate_hz} Hz — ${st.sample_count} samples`
      : `Idle — ${st.sample_count || 0} samples buffered`;
    const meta = document.getElementById('log-session-meta');
    if (meta) meta.textContent = `Session: ${st.session_name || '—'} | Rate: ${st.rate_hz} Hz | Enabled: ${(st.channels || []).filter((c) => c.enabled).length}`;
    renderChannelList(st.channels || []);
    if (st.last_sample) updateLogKpis(st.last_sample);
    return st;
  } catch (e) {
    const el = document.getElementById('log-status');
    if (el) el.textContent = String(e.message || e);
  }
}

function renderChannelList(channels) {
  const box = document.getElementById('log-channels');
  if (!box) return;
  box.innerHTML = channels.map((c) => `
    <label class="row" style="padding:4px 0;border-bottom:1px solid var(--border-subtle);">
      <input type="checkbox" data-ch="${c.id}" ${c.enabled ? 'checked' : ''}>
      <span style="flex:1;">${c.name}</span>
      <span class="muted">${c.unit || ''}</span>
    </label>`).join('');
}

function updateLogKpis(sample) {
  const box = document.getElementById('log-kpis');
  if (!box || !sample || !sample.values) return;
  const entries = Object.entries(sample.values);
  if (!entries.length) {
    box.innerHTML = '<span class="muted">No live PIDs in this sample.</span>';
    return;
  }
  box.innerHTML = entries.map(([k, v]) =>
    `<div class="kpi-chip"><span class="k">${k}</span><span class="v">${typeof v === 'number' ? v.toFixed(1) : v}</span></div>`
  ).join('');
}

function appendLogRow(sample) {
  const thead = document.getElementById('log-thead');
  const tbody = document.getElementById('log-tbody');
  if (!thead || !tbody || !sample) return;
  const keys = Object.keys(sample.values || {});
  if (!thead.innerHTML) {
    thead.innerHTML = '<tr><th>t(ms)</th>' + keys.map((k) => `<th>${k}</th>`).join('') + '</tr>';
  }
  const tr = document.createElement('tr');
  tr.innerHTML = `<td>${sample.timestamp_ms}</td>` +
    keys.map((k) => `<td>${typeof sample.values[k] === 'number' ? sample.values[k].toFixed(1) : (sample.values[k] ?? '')}</td>`).join('');
  tbody.insertBefore(tr, tbody.firstChild);
  while (tbody.children.length > 40) tbody.removeChild(tbody.lastChild);
  if (sample.values && sample.values.rpm != null) sparkHistory.push(sample.values.rpm);
  else if (keys[0] && sample.values[keys[0]] != null) sparkHistory.push(sample.values[keys[0]]);
  if (sparkHistory.length > 240) sparkHistory.shift();
  drawSpark();
}

function drawSpark() {
  const canvas = document.getElementById('log-spark');
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  const w = canvas.width, h = canvas.height;
  ctx.clearRect(0, 0, w, h);
  const hint = document.getElementById('log-spark-hint');
  if (sparkHistory.length < 2) {
    if (hint) hint.textContent = 'Chart stays empty until a live or imported sample exists.';
    return;
  }
  if (hint) hint.textContent = sparkHistory.length + ' points (first enabled numeric channel).';
  const min = Math.min(...sparkHistory);
  const max = Math.max(...sparkHistory);
  const span = max - min || 1;
  ctx.strokeStyle = '#00c4b4';
  ctx.lineWidth = 2;
  ctx.beginPath();
  sparkHistory.forEach((v, i) => {
    const x = (i / (sparkHistory.length - 1)) * w;
    const y = h - ((v - min) / span) * (h - 8) - 4;
    if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
  });
  ctx.stroke();
}

async function logPollTick() {
  if (!logRunning) return;
  try {
    const raw = await invokeCmd('log_capture_sample');
    const sample = parseMaybe(raw);
    updateLogKpis(sample);
    appendLogRow(sample);
    await refreshLogStatus();
  } catch (e) {
    const el = document.getElementById('log-status');
    if (el) el.textContent = String(e.message || e);
  }
}

async function startLogging() {
  const rate = parseFloat(document.getElementById('log-rate')?.value || '10');
  try {
    await invokeCmd('log_start', { rate_hz: rate, session_name: 'session_' + Date.now() });
    logRunning = true;
    sparkHistory = [];
    drawSpark();
    const interval = Math.max(40, Math.floor(1000 / rate));
    if (logPollTimer) clearInterval(logPollTimer);
    logPollTimer = setInterval(logPollTick, interval);
    document.getElementById('log-thead').innerHTML = '';
    document.getElementById('log-tbody').innerHTML = '';
    await refreshLogStatus();
  } catch (e) {
    alert('Start failed: ' + e);
  }
}

async function stopLogging() {
  try {
    if (logPollTimer) { clearInterval(logPollTimer); logPollTimer = null; }
    await invokeCmd('log_stop');
    logRunning = false;
    await refreshLogStatus();
  } catch (e) {
    alert('Stop failed: ' + e);
  }
}

async function applyChannels() {
  const ids = Array.from(document.querySelectorAll('#log-channels input[type=checkbox]:checked')).map((cb) => cb.dataset.ch);
  try {
    await invokeCmd('log_set_channels', { enabled_ids: ids });
    await refreshLogStatus();
  } catch (e) { alert(e); }
}

async function applyTemplate() {
  const id = document.getElementById('log-template')?.value;
  if (!id) return;
  try {
    await invokeCmd('log_apply_template', { template_id: id });
    await refreshLogStatus();
  } catch (e) { alert(e); }
}

async function clearLog() {
  try {
    await invokeCmd('log_clear');
    sparkHistory = [];
    drawSpark();
    document.getElementById('log-thead').innerHTML = '';
    document.getElementById('log-tbody').innerHTML = '';
    document.getElementById('log-kpis').innerHTML = '';
    await refreshLogStatus();
  } catch (e) { alert(e); }
}

async function exportCsv() {
  try {
    const csv = await invokeCmd('log_export_csv');
    const blob = new Blob([csv], { type: 'text/csv' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'tuneitverse_log_' + Date.now() + '.csv';
    a.click();
  } catch (e) {
    alert('Export failed: ' + e);
  }
}

async function importCsv() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.csv,text/csv';
  input.onchange = async (ev) => {
    const file = ev.target.files[0];
    if (!file) return;
    try {
      const text = await file.text();
      const raw = await invokeCmd('log_import_csv', { csv: text });
      await refreshLogStatus();
      const samples = parseMaybe(await invokeCmd('log_get_samples', { limit: 200 }));
      const list = Array.isArray(samples) ? samples : [];
      sparkHistory = list.map((s) => s.values && (s.values.rpm ?? Object.values(s.values)[0])).filter((v) => typeof v === 'number');
      drawSpark();
      const st = document.getElementById('log-status');
      if (st) st.textContent = 'Imported CSV: ' + (typeof raw === 'string' ? raw : JSON.stringify(raw));
    } catch (e) { alert('CSV import failed: ' + e); }
  };
  input.click();
}

async function loadTemplates() {
  try {
    const raw = await invokeCmd('get_logging_templates');
    const list = parseMaybe(raw) || [];
    const sel = document.getElementById('log-template');
    if (!sel) return;
    sel.innerHTML = list.map((t) => `<option value="${t.id}">${t.name} (${t.rate_hz} Hz)</option>`).join('');
  } catch (e) {
    const sel = document.getElementById('log-template');
    if (sel) sel.innerHTML = '';
    setStatus(String(e.message || e));
  }
}

function setupLive() {
  document.getElementById('btn-log-start')?.addEventListener('click', startLogging);
  document.getElementById('btn-log-stop')?.addEventListener('click', stopLogging);
  document.getElementById('btn-log-clear')?.addEventListener('click', clearLog);
  document.getElementById('btn-log-export')?.addEventListener('click', exportCsv);
  document.getElementById('btn-log-import')?.addEventListener('click', importCsv);
  document.getElementById('btn-log-apply-ch')?.addEventListener('click', applyChannels);
  document.getElementById('btn-log-apply-tmpl')?.addEventListener('click', applyTemplate);
  loadTemplates();
  refreshLogStatus();
}

// ---------- Diagnostics ----------
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
  (result.stored || []).forEach((r) => rows.push(r));
  (result.pending || []).forEach((r) => rows.push(r));
  (result.permanent || []).forEach((r) => rows.push(r));
  if (summary) {
    summary.textContent = `Total ${result.total ?? rows.length} — stored ${result.stored?.length ?? 0}, pending ${result.pending?.length ?? 0}, permanent ${result.permanent?.length ?? 0}`;
  }
  if (!rows.length) {
    tbody.innerHTML = '<tr><td colspan="3" class="muted">No DTCs reported.</td></tr>';
    return;
  }
  tbody.innerHTML = rows.map((rec) => {
    const code = rec.code || '????';
    const desc = String(rec.description || '').replace(/</g, '&lt;');
    return `<tr><td class="dtc-code">${code}</td><td>${dtcTypeLabel(rec)}</td><td>${desc}</td></tr>`;
  }).join('');
}

async function readDtcs() {
  const st = document.getElementById('dtc-status');
  if (st) st.textContent = 'Reading DTCs…';
  try {
    const result = parseMaybe(await invokeCmd('read_dtcs_cmd')) || { stored: [], pending: [], permanent: [], total: 0 };
    renderDtcRows(result);
    if (st) st.textContent = 'Read complete';
  } catch (e) {
    if (st) st.textContent = 'Error: ' + e;
  }
}

async function readFreezeFrame() {
  const st = document.getElementById('dtc-status');
  const pre = document.getElementById('freeze-frame');
  if (st) st.textContent = 'Reading freeze frame…';
  try {
    const result = parseMaybe(await invokeCmd('read_freeze_frame_cmd'));
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
  if (st) st.textContent = 'Clearing DTCs…';
  try {
    const result = parseMaybe(await invokeCmd('clear_dtcs_cmd'));
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

// ---------- Tables ----------
async function identifyCurrentBin() {
  if (!currentBin) { alert('Load a .BIN first'); return; }
  const st = document.getElementById('tables-status');
  try {
    const info = parseMaybe(await invokeCmd('identify_bin_cmd', { data: Array.from(currentBin) }));
    applyIdentify(info);
    if (st) st.textContent = 'Identified: ' + (info.family || info.family_by_size || 'unknown') + ' (' + info.bin_size_bytes + ' bytes)';
    const meta = document.getElementById('side-meta');
    if (meta) meta.innerHTML = '<pre class="mono-block">' + JSON.stringify(info, null, 2) + '</pre>';
  } catch (e) {
    if (st) st.textContent = 'Identify error: ' + e;
  }
}
window.identifyCurrentBin = identifyCurrentBin;

async function loadBinFile() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.bin,.BIN';
  input.onchange = async (ev) => {
    const file = ev.target.files[0];
    if (!file) return;
    const buf = await file.arrayBuffer();
    currentBin = new Uint8Array(buf);
    syncGlobals();
    const st = document.getElementById('tables-status');
    const save = document.getElementById('btn-save-patched');
    if (save) save.disabled = false;
    try {
      const raw = await invokeCmd('auto_load_tables_for_bin', { bin_bytes: Array.from(currentBin) });
      const list = parseMaybe(raw);
      currentTables = Array.isArray(list) ? list : [];
      renderTableList();
      if (st) st.textContent = 'Loaded ' + file.name + ' (' + currentBin.length + ' bytes). Tables from catalog: ' + currentTables.length;
    } catch (e) {
      currentTables = [];
      renderTableList();
      if (st) st.textContent = 'BIN loaded (' + currentBin.length + ' bytes). Catalog tables unavailable: ' + e;
    }
    await identifyCurrentBin();
    await validateCurrentBinChecksums();
  };
  input.click();
}

async function loadXdfFile() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.xdf,.xml,.XDF,.XML,.a2l,.A2L';
  input.onchange = async (ev) => {
    const file = ev.target.files[0];
    if (!file) return;
    const text = await file.text();
    const st = document.getElementById('tables-status');
    try {
      let defs = await invokeCmd('parse_xdf_definitions', { xml: text });
      let list = Array.isArray(defs) ? defs : parseMaybe(defs);
      if ((!list || !list.length) && /BEGIN CHARACTERISTIC/i.test(text)) {
        defs = await invokeCmd('parse_a2l_definitions', { text });
        list = Array.isArray(defs) ? defs : parseMaybe(defs);
      }
      if (!list || !list.length) {
        if (st) st.textContent = 'Definition parsed but no tables found';
        return;
      }
      currentTables = list;
      renderTableList();
      if (st) st.textContent = 'Definitions loaded: ' + list.length;
    } catch (e) {
      if (st) st.textContent = 'Parse error: ' + e;
    }
  };
  input.click();
}

async function loadA2l() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.a2l,.A2L,.txt';
  input.onchange = async () => {
    const file = input.files && input.files[0];
    if (!file) return;
    const text = await file.text();
    const st = document.getElementById('tables-status');
    try {
      const defs = await invokeCmd('parse_a2l_definitions', { text });
      const list = Array.isArray(defs) ? defs : parseMaybe(defs);
      if (!list || !list.length) {
        if (st) st.textContent = 'A2L parsed but no CHARACTERISTIC found';
        return;
      }
      currentTables = list;
      renderTableList();
      if (st) st.textContent = 'A2L loaded: ' + list.length + ' characteristics. Confirm addresses on your dump.';
    } catch (e) {
      if (st) st.textContent = 'A2L parse error: ' + e;
    }
  };
  input.click();
}

function renderTableList() {
  const list = document.getElementById('tables-list');
  if (!list) return;
  list.innerHTML = '';
  if (!currentTables.length) {
    list.innerHTML = '<div class="muted" style="padding:12px;">No tables. Load a BIN with catalog maps, or an XDF/A2L.</div>';
    return;
  }
  currentTables.forEach((t, idx) => {
    const div = document.createElement('div');
    div.className = 'table-item';
    div.innerHTML = '<strong>' + (t.name || t.id) + '</strong><br><span class="muted">' + (t.rows || 1) + '×' + (t.cols || 1) + ' @ ' + (t.addr || '?') + ' • ' + (t.units || '') + '</span>';
    div.onclick = () => selectTable(idx);
    list.appendChild(div);
  });
}

async function selectTable(idx) {
  currentTable = currentTables[idx];
  syncGlobals();
  document.querySelectorAll('.table-item').forEach((el, i) => el.classList.toggle('active', i === idx));
  const st = document.getElementById('tables-status');
  if (!currentTable || !currentBin) {
    if (st) st.textContent = 'Load a BIN before extracting a table.';
    currentValues = null;
    renderCurrentEditor();
    return;
  }
  try {
    const extracted = await invokeCmd('extract_table_from_bin', { bin_bytes: Array.from(currentBin), table: currentTable });
    currentValues = extracted.values || extracted;
    if (!Array.isArray(currentValues)) currentValues = null;
    renderCurrentEditor();
    updateSidePanel();
    if (st) st.textContent = 'Selected: ' + currentTable.name;
  } catch (e) {
    currentValues = null;
    renderCurrentEditor();
    if (st) st.textContent = 'Extract error: ' + e;
  }
}

function tableMinMax(values) {
  let min = Infinity, max = -Infinity;
  values.forEach((row) => row.forEach((v) => {
    if (typeof v === 'number') { if (v < min) min = v; if (v > max) max = v; }
  }));
  if (!Number.isFinite(min)) { min = 0; max = 1; }
  return { min, max };
}

function heatColor(v, min, max) {
  const t = max > min ? (v - min) / (max - min) : 0.5;
  return 'hsl(' + (120 - t * 120) + ',70%,28%)';
}

function renderCurrentEditor() {
  const el = document.getElementById('editor-content');
  if (!el) return;
  if (!currentValues) {
    el.innerHTML = '<p class="muted">No table values. Load a BIN and select a definition whose address lands in the image.</p>';
    return;
  }
  if (currentEditorTab === 'grid') {
    const { min, max } = tableMinMax(currentValues);
    let html = '<table class="map-table"><tbody>';
    currentValues.forEach((row, ri) => {
      html += '<tr>';
      row.forEach((v, ci) => {
        const num = typeof v === 'number' ? v : parseFloat(v);
        const bg = Number.isFinite(num) ? heatColor(num, min, max) : 'transparent';
        html += '<td contenteditable="true" data-r="' + ri + '" data-c="' + ci + '" style="background:' + bg + '">' +
          (typeof v === 'number' ? v.toFixed(1) : v) + '</td>';
      });
      html += '</tr>';
    });
    html += '</tbody></table><div class="table-editor-footer"><button id="btn-apply-patch" class="btn btn-primary" type="button">Apply Patch + Auto Checksum</button><span class="muted">min ' + min.toFixed(1) + ' → max ' + max.toFixed(1) + '</span></div>';
    el.innerHTML = html;
    document.getElementById('btn-apply-patch')?.addEventListener('click', applyCurrentPatch);
    el.querySelectorAll('td[contenteditable]').forEach((td) => {
      td.onblur = () => {
        const r = +td.dataset.r, c = +td.dataset.c;
        const num = parseFloat(td.textContent);
        if (!isNaN(num) && currentValues[r]) currentValues[r][c] = num;
      };
    });
  } else if (currentEditorTab === '3d') {
    const { min, max } = tableMinMax(currentValues);
    el.innerHTML = '<canvas id="viz3d" width="560" height="360"></canvas><div class="heatmap-legend"><span>' + min.toFixed(1) + '</span><div class="heatmap-scale"></div><span>' + max.toFixed(1) + '</span><span id="heat-hover" class="muted">hover a cell</span></div>';
    const canvas = document.getElementById('viz3d');
    const ctx = canvas.getContext('2d');
    const rows = currentValues.length, cols = currentValues[0].length;
    const cellW = 560 / cols, cellH = 360 / rows;
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        ctx.fillStyle = heatColor(currentValues[r][c], min, max);
        ctx.fillRect(c * cellW, r * cellH, cellW + 1, cellH + 1);
      }
    }
    canvas.onmousemove = (ev) => {
      const rect = canvas.getBoundingClientRect();
      const c = Math.min(cols - 1, Math.max(0, Math.floor((ev.clientX - rect.left) / rect.width * cols)));
      const r = Math.min(rows - 1, Math.max(0, Math.floor((ev.clientY - rect.top) / rect.height * rows)));
      const hover = document.getElementById('heat-hover');
      if (hover) hover.textContent = 'r' + r + ' c' + c + ' = ' + (typeof currentValues[r][c] === 'number' ? currentValues[r][c].toFixed(2) : currentValues[r][c]);
    };
  } else if (currentEditorTab === 'hex') {
    if (!currentBin) { el.innerHTML = 'No BIN loaded'; return; }
    let start = 0;
    const parsed = parseInt(String(currentTable && currentTable.addr || '0'), 16);
    if (!isNaN(parsed) && parsed >= 0 && parsed < currentBin.length) start = parsed;
    const len = Math.min(512, Math.max(0, currentBin.length - start));
    let html = '<pre class="mono-block">';
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
      html += '<span class="hex-row">' + addr + ': ' + hex + ' | ' + ascii + '</span>\n';
    }
    html += '</pre>';
    el.innerHTML = html;
  }
}
window.renderCurrentEditor = renderCurrentEditor;

function updateSidePanel() {
  if (!currentTable) return;
  const meta = document.getElementById('side-meta');
  if (meta) {
    meta.innerHTML = '<div><b>' + currentTable.name + '</b></div>' +
      '<div>ID: ' + currentTable.id + '</div>' +
      '<div>Size: ' + (currentTable.rows || '?') + ' × ' + (currentTable.cols || '?') + '</div>' +
      '<div>Addr: ' + (currentTable.addr || '?') + '</div>' +
      '<div>Type: ' + (currentTable.data_type || '?') + '  Math: ' + (currentTable.math || 'X') + '</div>' +
      '<div>Units: ' + (currentTable.units || '') + '</div>';
  }
  const fam = identifiedFamily() || 'unresolved';
  const sample = (currentValues && currentValues[0] && typeof currentValues[0][0] === 'number') ? currentValues[0][0] : 0;
  invokeCmd('get_tuning_advice', { table_id: currentTable.id || '', sample_value: sample, ecu_family: fam })
    .then((adv) => { const a = document.getElementById('side-advice'); if (a) a.textContent = adv; })
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
      syncGlobals();
      const st = document.getElementById('tables-status');
      if (st) st.textContent = (res.message || 'Patched') + ' — auto-correcting checksums…';
      try {
        const corrected = await invokeCmd('correct_bin_checksums', { data: Array.from(currentBin) });
        if (corrected && corrected.length) {
          currentBin = new Uint8Array(corrected);
          syncGlobals();
          if (st) st.textContent += ' checksums rewritten';
        }
      } catch (cs) {
        if (st) st.textContent += ' (CS note: ' + cs + ')';
      }
      renderCurrentEditor();
    }
  } catch (e) { alert('Patch error: ' + e); }
}

async function validateCurrentBinChecksums() {
  if (!currentBin || !currentBin.length) return;
  const st = document.getElementById('tables-status');
  const cs = document.getElementById('side-checksum');
  try {
    const summary = await invokeCmd('validate_bin_checksums_summary_cmd', { data: Array.from(currentBin) });
    const text = typeof summary === 'string' ? summary : JSON.stringify(summary, null, 2);
    if (st) st.textContent = (st.textContent || '') + ' | checksum report ready';
    if (cs) cs.textContent = text;
  } catch (e) {
    if (cs) cs.textContent = 'CS error: ' + e;
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

async function compareAnotherBin() {
  if (!currentBin) { alert('Load the first .BIN first'); return; }
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.bin,.BIN';
  input.onchange = async (ev) => {
    const file = ev.target.files[0];
    if (!file) return;
    const other = Array.from(new Uint8Array(await file.arrayBuffer()));
    const st = document.getElementById('tables-status');
    try {
      const info = parseMaybe(await invokeCmd('compare_bins_cmd', { a: Array.from(currentBin), b: other }));
      if (st) st.textContent = info.message || 'Compare done';
      const cs = document.getElementById('side-checksum');
      if (cs) cs.textContent = JSON.stringify(info, null, 2);
    } catch (e) {
      if (st) st.textContent = 'Compare error: ' + e;
    }
  };
  input.click();
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
  let html = '<div class="muted">' + (info.advice || '') + '</div>';
  html += '<div style="display:grid;grid-template-columns:repeat(16,12px);gap:1px;margin-top:8px;">';
  for (let r = 15; r >= 0; r--) {
    for (let c = 0; c < 16; c++) {
      const v = grid[r][c];
      const t = v / max;
      html += '<div title="r' + r + ' c' + c + ' hits=' + v + '" style="width:12px;height:12px;background:rgba(0,196,180,' + (0.08 + t * 0.92) + ');"></div>';
    }
  }
  html += '</div><div class="muted">rows RPM↑  cols MAP→</div>';
  adv.innerHTML = html;
}

async function mapFromLog() {
  const st = document.getElementById('tables-status');
  try {
    const info = parseMaybe(await invokeCmd('map_from_log_cmd'));
    lastMapFromLog = info;
    window.lastMapFromLog = info;
    if (st) st.textContent = info.advice || 'Map-from-log ready';
    renderHeatmap(info);
  } catch (e) {
    if (st) st.textContent = 'Map-from-log: ' + e;
  }
}

async function exportWorkspace() {
  try {
    const raw = await invokeCmd('export_workspace_cmd', { data: currentBin ? Array.from(currentBin) : null });
    const text = typeof raw === 'string' ? raw : JSON.stringify(raw, null, 2);
    const blob = new Blob([text], { type: 'application/json' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'tuneitverse-workspace.json';
    a.click();
  } catch (e) { alert('Workspace export failed: ' + e); }
}

async function importWorkspace() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.json,.txt';
  input.onchange = async () => {
    const file = input.files && input.files[0];
    if (!file) return;
    const text = await file.text();
    const st = document.getElementById('tables-status');
    try {
      const res = parseMaybe(await invokeCmd('import_workspace_cmd', { jsonText: text }));
      if (st) st.textContent = 'Workspace import: ' + JSON.stringify(res);
      if (res && res.identify) applyIdentify(res.identify);
      if (res && res.map_from_log) {
        lastMapFromLog = res.map_from_log;
        renderHeatmap(res.map_from_log);
      }
    } catch (e) {
      if (st) st.textContent = 'Workspace import failed: ' + e;
    }
  };
  input.click();
}

async function scanCs() {
  if (!currentBin) { alert('Load a BIN first'); return; }
  const st = document.getElementById('tables-status');
  try {
    const res = parseMaybe(await invokeCmd('scan_checksum_candidates_cmd', { data: Array.from(currentBin) }));
    const cs = document.getElementById('side-checksum');
    if (cs) cs.textContent = JSON.stringify(res, null, 2);
    if (st) st.textContent = 'CS scan: ' + (res && res.note ? res.note : 'done');
    if (res && res.honda_os && !res.gm_p01_os) banner('tiv-honda-guard', 'Honda OS string on this image. P01 additive correction is blocked.');
  } catch (e) {
    if (st) st.textContent = 'CS scan failed: ' + e;
  }
}

async function pokeHex() {
  if (!currentBin) { alert('Load a .BIN first'); return; }
  const off = parseInt((document.getElementById('hex-poke-off')?.value) || '0', 16);
  const hex = ((document.getElementById('hex-poke-val')?.value) || '').replace(/[^0-9a-fA-F]/g, '');
  if (!hex || hex.length % 2) { alert('Value must be even-length hex'); return; }
  const bytes = [];
  for (let i = 0; i < hex.length; i += 2) bytes.push(parseInt(hex.substr(i, 2), 16));
  try {
    const out = await invokeCmd('patch_bin_bytes_cmd', { data: Array.from(currentBin), offset: off >>> 0, bytes });
    currentBin = out instanceof Uint8Array ? out : new Uint8Array(out);
    syncGlobals();
    const st = document.getElementById('tables-status');
    if (st) st.textContent = 'Poked ' + bytes.length + ' byte(s) at 0x' + off.toString(16).toUpperCase();
    renderCurrentEditor();
  } catch (e) { alert('Poke failed: ' + e); }
}

async function runMath(op) {
  if (!currentValues) { alert('Select a table first'); return; }
  const req = { values: currentValues, op };
  if (op === 'scale') {
    req.factor = parseFloat(document.getElementById('tbl-scale')?.value || '1');
    if (!isFinite(req.factor)) { alert('Scale factor must be a number'); return; }
  }
  if (op === 'add') {
    req.offset = parseFloat(document.getElementById('tbl-offset')?.value || '0');
    if (!isFinite(req.offset)) { alert('Offset must be a number'); return; }
  }
  try {
    const res = parseMaybe(await invokeCmd('table_math_cmd', { req }));
    if (!res || !res.values) throw new Error('no values');
    currentValues = res.values;
    syncGlobals();
    renderCurrentEditor();
    const st = document.getElementById('tables-status');
    if (st) st.textContent = (res.message || op) + ' — ' + res.cells_changed + ' cells. Apply Patch to write BIN.';
  } catch (e) { alert('Table math failed: ' + e); }
}

async function applyStft() {
  if (!currentValues) { alert('Select a table first'); return; }
  if (!lastMapFromLog || !lastMapFromLog.occupancy_16x16) { alert('Run Map from Log first so STFT occupancy exists'); return; }
  try {
    const res = parseMaybe(await invokeCmd('apply_stft_preview_cmd', {
      req: { values: currentValues, occupancy: lastMapFromLog.occupancy_16x16, stft_avg: lastMapFromLog.stft_avg_16x16, gain: 0.25, min_hits: 3 }
    }));
    if (!res || !res.values) throw new Error('no preview');
    currentValues = res.values;
    syncGlobals();
    renderCurrentEditor();
    const st = document.getElementById('tables-status');
    if (st) st.textContent = res.message + ' (' + res.cells_changed + ' cells). Apply Patch to write BIN.';
  } catch (e) { alert('STFT preview failed: ' + e); }
}

function setupTablesUI() {
  document.getElementById('btn-load-bin')?.addEventListener('click', loadBinFile);
  document.getElementById('btn-load-xdf')?.addEventListener('click', loadXdfFile);
  document.getElementById('btn-load-a2l')?.addEventListener('click', loadA2l);
  document.getElementById('btn-save-patched')?.addEventListener('click', savePatchedBin);
  document.getElementById('btn-identify-bin')?.addEventListener('click', identifyCurrentBin);
  document.getElementById('btn-compare-bins')?.addEventListener('click', compareAnotherBin);
  document.getElementById('btn-map-from-log')?.addEventListener('click', mapFromLog);
  document.getElementById('btn-export-workspace')?.addEventListener('click', exportWorkspace);
  document.getElementById('btn-import-workspace')?.addEventListener('click', importWorkspace);
  document.getElementById('btn-scan-cs')?.addEventListener('click', scanCs);
  document.getElementById('btn-hex-poke')?.addEventListener('click', pokeHex);
  document.getElementById('btn-tbl-scale')?.addEventListener('click', () => runMath('scale'));
  document.getElementById('btn-tbl-offset')?.addEventListener('click', () => runMath('add'));
  document.getElementById('btn-tbl-smooth')?.addEventListener('click', () => runMath('smooth'));
  document.getElementById('btn-tbl-stft')?.addEventListener('click', applyStft);
  document.querySelectorAll('.table-filters .chip-filter').forEach((ch) => {
    ch.onclick = () => {
      document.querySelectorAll('.table-filters .chip-filter').forEach((c) => c.classList.remove('active'));
      ch.classList.add('active');
      filterTableList(ch.dataset.filter || 'all');
    };
  });
  const tabs = document.getElementById('editor-tabs');
  if (tabs) {
    tabs.onclick = (e) => {
      const b = e.target.closest('.editor-tab');
      if (!b) return;
      document.querySelectorAll('#editor-tabs .editor-tab').forEach((t) => t.classList.remove('active'));
      b.classList.add('active');
      currentEditorTab = b.dataset.tab;
      renderCurrentEditor();
    };
  }
}

// ---------- Flash ----------
function setFlashStep(id, state) {
  const el = document.getElementById('st-' + id);
  if (!el) return;
  el.textContent = state;
  el.classList.remove('done', 'active');
  if (state === 'ok' || state === 'done') el.classList.add('done');
  if (state === 'running' || state === 'active') el.classList.add('active');
}

function resetFlashSteps() {
  ['identify', 'voltage', 'backup', 'unlock', 'write', 'verify'].forEach((s) => setFlashStep(s, 'pending'));
  const bar = document.getElementById('flash-bar');
  if (bar) bar.style.width = '0%';
  const prog = document.getElementById('flash-progress');
  if (prog) prog.textContent = '0%';
}

function applyFlashResult(res) {
  const steps = (res.steps_completed || []).join(' ').toLowerCase();
  const logs = (res.logs || []).join('\n').toLowerCase();
  const blob = steps + ' ' + logs;
  if (identifiedFamily() || /identify|family/.test(blob)) setFlashStep('identify', res.error && /identify|family|honda/i.test(res.error) ? 'fail' : 'ok');
  if (/voltage/.test(blob)) setFlashStep('voltage', /voltage gate failed/.test(blob) ? 'fail' : 'ok');
  if (/backup/.test(blob)) setFlashStep('backup', /backup failed/.test(blob) ? 'fail' : 'ok');
  if (/unlock/.test(blob)) setFlashStep('unlock', /unlock failed/.test(blob) ? 'fail' : 'ok');
  if (res.flash_write_result) setFlashStep('write', 'ok');
  if (res.verified_live) setFlashStep('verify', 'ok');
  else if (res.error && /verify/i.test(res.error)) setFlashStep('verify', 'fail');
  const n = (res.steps_completed || []).length;
  const pct = res.success ? 100 : Math.min(90, n * 16);
  const bar = document.getElementById('flash-bar');
  if (bar) bar.style.width = pct + '%';
  const prog = document.getElementById('flash-progress');
  if (prog) prog.textContent = pct + '%';
}

function setupFlash() {
  document.getElementById('btn-compare-bin')?.addEventListener('click', async () => {
    const pre = document.getElementById('compare-result');
    if (pre) { pre.hidden = false; pre.textContent = 'Comparing…'; }
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
    if (pre) { pre.hidden = false; pre.textContent = 'Verifying…'; }
    try {
      if (!currentBin || !currentBin.length) throw new Error('Load a .BIN in Tables first.');
      const res = await invokeCmd('verify_after_write', { expected_bytes: Array.from(currentBin) });
      if (pre) pre.textContent = typeof res === 'string' ? res : JSON.stringify(res, null, 2);
    } catch (e) {
      if (pre) pre.textContent = 'Verify error: ' + e;
    }
  });
  document.getElementById('btn-show-risk')?.onclick = () => {
    const sec = document.getElementById('risk-section');
    if (sec) sec.hidden = false;
  };
  ['risk-backup', 'risk-power', 'risk-ground', 'risk-understand'].forEach((id) => {
    const cb = document.getElementById(id);
    if (cb) cb.onchange = () => {
      const all = ['risk-backup', 'risk-power', 'risk-ground', 'risk-understand'].every((i) => document.getElementById(i)?.checked);
      const btn = document.getElementById('btn-run-flash');
      if (btn) btn.disabled = !all;
    };
  });
  document.getElementById('btn-run-flash')?.addEventListener('click', async () => {
    const log = document.getElementById('flash-log');
    const risksOk = ['risk-backup', 'risk-power', 'risk-ground', 'risk-understand'].every((i) => document.getElementById(i)?.checked);
    if (!risksOk) {
      if (log) log.textContent = 'Fail-closed: tick every risk checkbox before flashing.\n';
      return;
    }
    if (!currentBin || !currentBin.length) {
      if (log) log.textContent = 'Load a .BIN in Tables first.\n';
      return;
    }
    resetFlashSteps();
    setFlashStep('identify', 'running');
    if (log) log.textContent = 'Starting guided flash pipeline…\n';
    try {
      const fam = identifiedFamily();
      if (!fam) {
        await identifyCurrentBin();
      }
      const family = identifiedFamily();
      if (!family) {
        if (log) log.textContent += 'Identify did not resolve a family. Refusing write.\n';
        setFlashStep('identify', 'fail');
        return;
      }
      setFlashStep('identify', 'ok');
      const req = {
        ecu_family: family,
        bin_bytes: Array.from(currentBin),
        do_backup: true,
        do_kernel: true,
        do_write: true,
        user_confirmed_risks: true,
        accept_unverified_write: !!(document.getElementById('risk-unverified') && document.getElementById('risk-unverified').checked)
      };
      const raw = await invokeCmd('guided_flash_pipeline', { request_json: JSON.stringify(req) });
      const res = parseMaybe(raw) || {};
      if (log) log.textContent += (typeof raw === 'string' ? raw : JSON.stringify(res, null, 2)) + '\n';
      applyFlashResult(res);
    } catch (e) {
      if (log) log.textContent += 'ERROR: ' + e + '\n';
    }
  });
}

// ---------- Scripts ----------
async function setupScripts() {
  document.getElementById('btn-refresh-scripts')?.addEventListener('click', async () => {
    const list = document.getElementById('custom-scripts-list');
    try {
      const raw = await invokeCmd('list_script_helpers');
      const helpers = parseMaybe(raw) || [];
      if (!list) return;
      if (!helpers.length) { list.innerHTML = '<p class="muted">No helpers returned.</p>'; return; }
      list.innerHTML = helpers.map((h) =>
        `<div class="script-card"><strong>${h.name || h.id}</strong><br><code>${h.command || ''}</code></div>`
      ).join('');
    } catch (e) {
      if (list) list.innerHTML = '<p class="muted">' + e + '</p>';
    }
  });
}

function setupAll() {
  setupNav();
  const steps = [setupConnect, setupLive, setupDiagnostics, setupTablesUI, setupFlash, setupScripts];
  steps.forEach((fn) => {
    try { fn(); } catch (e) { console.error(fn.name, e); setStatus(fn.name + ' failed: ' + e); }
  });
  showView('dashboard');
  pollHealth();
  if (healthTimer) clearInterval(healthTimer);
  healthTimer = setInterval(pollHealth, 2500);
  console.log('TuneItVerse UI v3.10.1');
}

setupNav();
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', setupAll);
} else {
  setupAll();
}
