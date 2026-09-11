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

function binBytes(bin) {
  if (!bin) return [];
  return Array.from(bin);
}

function setTablesStatus(msg) {
  const st = document.getElementById('tables-status');
  if (st) st.textContent = msg;
  setStatus(msg);
}

function enableSave(on) {
  const save = document.getElementById('btn-save-patched');
  if (save) save.disabled = !on;
}

async function openNativeFile(kind) {
  const raw = await invokeCmd('dialog_open_file', { kind: kind || 'bin' });
  const res = parseMaybe(raw);
  if (!res) throw new Error('File dialog returned nothing');
  return res;
}

async function saveNativeBytes(data, defaultName) {
  const bytes = typeof data === 'string' ? Array.from(new TextEncoder().encode(data)) : binBytes(data);
  const raw = await invokeCmd('dialog_save_bytes', { data: bytes, default_name: defaultName || 'tuned.bin' });
  return parseMaybe(raw);
}

async function saveNativeText(text, defaultName) {
  const raw = await invokeCmd('dialog_save_text', { text: String(text), default_name: defaultName || 'export.txt' });
  return parseMaybe(raw);
}

async function requireConnected(what) {
  if (isConnected) return true;
  try {
    const h = String(await invokeCmd('get_connection_health') || '');
    if (/connected/i.test(h) && !/disconnected/i.test(h)) return true;
  } catch (_) {}
  const msg = 'Not connected — ' + (what || 'this needs an adapter') + '. Use Tables to load a BIN offline.';
  setStatus(msg);
  alert(msg);
  return false;
}

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
  if (!(await requireConnected('live logging'))) return;
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
    const res = await saveNativeText(csv, 'tuneitverse_log.csv');
    if (res && res.cancelled) { setStatus('Save cancelled.'); return; }
    setStatus('CSV saved' + (res && res.path ? ': ' + res.path : ''));
  } catch (e) {
    alert('Export failed: ' + e);
  }
}

async function importCsv() {
  try {
    const file = await openNativeFile('csv');
    if (file.cancelled) { setStatus('Open cancelled.'); return; }
    const raw = await invokeCmd('log_import_csv', { csv: file.text || '' });
    await refreshLogStatus();
    const samples = parseMaybe(await invokeCmd('log_get_samples', { limit: 200 }));
    const list = Array.isArray(samples) ? samples : [];
    sparkHistory = list.map((s) => s.values && (s.values.rpm ?? Object.values(s.values)[0])).filter((v) => typeof v === 'number');
    drawSpark();
    const st = document.getElementById('log-status');
    if (st) st.textContent = 'Imported CSV: ' + (typeof raw === 'string' ? raw : JSON.stringify(raw));
  } catch (e) { alert('CSV import failed: ' + e); }
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
  if (!(await requireConnected('DTC read'))) return;
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
  if (!(await requireConnected('freeze frame'))) return;
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
  if (!(await requireConnected('DTC clear'))) return;
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

function setupDiagnostics() {}

// ---------- Tables ----------
async function identifyCurrentBin() {
  if (!currentBin) { alert('Load a .BIN first'); return; }
  const st = document.getElementById('tables-status');
  try {
    const info = parseMaybe(await invokeCmd('identify_bin_cmd', { data: binBytes(currentBin) }));
    applyIdentify(info);
    if (st) st.textContent = 'Identified: ' + (info.family || info.family_by_size || 'unknown') + ' (' + info.bin_size_bytes + ' bytes)';
    const meta = document.getElementById('side-meta');
    if (meta) meta.innerHTML = '<pre class="mono-block">' + JSON.stringify(info, null, 2) + '</pre>';
  } catch (e) {
    if (st) st.textContent = 'Identify error: ' + e;
  }
}
window.identifyCurrentBin = identifyCurrentBin;

async function ingestBinBytes(bytes, label) {
  currentBin = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  syncGlobals();
  enableSave(true);
  try {
    const raw = await invokeCmd('auto_load_tables_for_bin', { bin_bytes: binBytes(currentBin) });
    const parsed = parseMaybe(raw);
    if (Array.isArray(parsed)) {
      currentTables = parsed;
      setTablesStatus('Loaded ' + (label || 'BIN') + ' (' + currentBin.length + ' bytes). Tables: ' + currentTables.length);
    } else {
      currentTables = (parsed && parsed.tables) || [];
      const note = (parsed && parsed.note) ? parsed.note : '';
      setTablesStatus('Loaded ' + (label || 'BIN') + ' (' + currentBin.length + ' bytes). ' + note);
    }
    rebuildCategoryChips();
    renderTableList();
  } catch (e) {
    currentTables = [];
    renderTableList();
    setTablesStatus('BIN loaded (' + currentBin.length + ' bytes). Catalog tables unavailable: ' + e);
  }
  await identifyCurrentBin();
  await validateCurrentBinChecksums();
}

async function loadBinFile() {
  setTablesStatus('Opening BIN…');
  try {
    const res = await openNativeFile('bin');
    if (res.cancelled) { setTablesStatus('Open cancelled.'); return; }
    if (!res.bytes || !res.bytes.length) throw new Error('File was empty');
    await ingestBinBytes(res.bytes, res.path || 'BIN');
  } catch (e) {
    setTablesStatus('Load BIN failed: ' + e);
  }
}

async function applyDefinitionText(text, source) {
  let defs = await invokeCmd('parse_xdf_definitions', { xml: text });
  let list = Array.isArray(defs) ? defs : parseMaybe(defs);
  if ((!list || !list.length) && /BEGIN CHARACTERISTIC/i.test(text)) {
    defs = await invokeCmd('parse_a2l_definitions', { text });
    list = Array.isArray(defs) ? defs : parseMaybe(defs);
  }
  if (!list || !list.length) {
    setTablesStatus((source || 'Definition') + ' parsed but no tables found');
    return;
  }
  currentTables = list;
  rebuildCategoryChips();
  renderTableList();
  setTablesStatus((source || 'Definitions') + ' loaded: ' + list.length + ' tables. Select one to extract from the BIN.');
}

async function loadXdfFile() {
  setTablesStatus('Opening XDF / XML…');
  try {
    const res = await openNativeFile('xdf');
    if (res.cancelled) { setTablesStatus('Open cancelled.'); return; }
    await applyDefinitionText(res.text || '', res.path || 'XDF');
  } catch (e) {
    setTablesStatus('Load XDF failed: ' + e);
  }
}

async function loadA2l() {
  setTablesStatus('Opening A2L…');
  try {
    const res = await openNativeFile('a2l');
    if (res.cancelled) { setTablesStatus('Open cancelled.'); return; }
    const defs = await invokeCmd('parse_a2l_definitions', { text: res.text || '' });
    const list = Array.isArray(defs) ? defs : parseMaybe(defs);
    if (!list || !list.length) {
      setTablesStatus('A2L parsed but no CHARACTERISTIC found');
      return;
    }
    currentTables = list;
    renderTableList();
    setTablesStatus('A2L loaded: ' + list.length + ' characteristics. Confirm addresses on your dump.');
  } catch (e) {
    setTablesStatus('A2L parse error: ' + e);
  }
}

let tableFilter = 'all';
let tableCat = 'all';
let tableSearch = '';

function rebuildCategoryChips() {
  const host = document.getElementById('table-cats');
  if (!host) return;
  const cats = [];
  currentTables.forEach((t) => {
    const c = (t.category || '').trim();
    if (c && cats.indexOf(c) < 0) cats.push(c);
  });
  cats.sort();
  host.innerHTML = '<button class="chip-filter' + (tableCat === 'all' ? ' active' : '') + '" data-cat="all" type="button">All cats</button>' +
    cats.slice(0, 48).map((c) => '<button class="chip-filter' + (tableCat === c ? ' active' : '') + '" data-cat="' + c.replace(/"/g, '') + '" type="button">' + c + '</button>').join('');
  host.querySelectorAll('.chip-filter').forEach((ch) => {
    ch.onclick = () => {
      tableCat = ch.getAttribute('data-cat') || 'all';
      rebuildCategoryChips();
      renderTableList();
    };
  });
}

function tablePassesFilter(t) {
  const q = tableSearch.trim().toLowerCase();
  if (q && !(t.name || '').toLowerCase().includes(q) && !(t.id || '').toLowerCase().includes(q) && !(t.category || '').toLowerCase().includes(q)) return false;
  if (tableCat !== 'all' && (t.category || '') !== tableCat) return false;
  const r = t.rows || 1, c = t.cols || 1;
  const is1d = r === 1 || c === 1;
  const is3d = r > 1 && c > 1 && r * c > 64;
  if (tableFilter === '1d') return is1d;
  if (tableFilter === '2d') return !is1d && !is3d;
  if (tableFilter === '3d') return is3d;
  return true;
}

function renderTableList() {
  const list = document.getElementById('tables-list');
  if (!list) return;
  list.innerHTML = '';
  if (!currentTables.length) {
    list.innerHTML = '<div class="muted" style="padding:12px;">No tables located. Load a Holden/GM P01 BIN or an XDF/A2L.</div>';
    return;
  }
  let shown = 0;
  currentTables.forEach((t, idx) => {
    if (!tablePassesFilter(t)) return;
    shown += 1;
    const div = document.createElement('div');
    div.className = 'table-item';
    div.setAttribute('data-idx', String(idx));
    div.innerHTML = '<strong>' + (t.name || t.id) + '</strong><br><span class="muted">' +
      (t.category ? t.category + ' · ' : '') + (t.rows || 1) + '×' + (t.cols || 1) + ' @ ' + (t.addr || '?') +
      (t.units ? ' · ' + t.units : '') + '</span>';
    div.onclick = () => selectTable(idx);
    list.appendChild(div);
  });
  const count = document.createElement('div');
  count.className = 'muted';
  count.style.padding = '8px 12px';
  count.textContent = shown === currentTables.length
    ? currentTables.length + ' parameters'
    : 'Showing ' + shown + ' of ' + currentTables.length;
  list.appendChild(count);
  if (!shown) {
    list.innerHTML = '<div class="muted" style="padding:12px;">No tables match the filter.</div>';
  }
}

async function selectTable(idx) {
  currentTable = currentTables[idx];
  syncGlobals();
  document.querySelectorAll('.table-item').forEach((el) => el.classList.toggle('active', +el.getAttribute('data-idx') === idx));
  const st = document.getElementById('tables-status');
  if (!currentTable || !currentBin) {
    if (st) st.textContent = 'Load a BIN before extracting a table.';
    currentValues = null;
    renderCurrentEditor();
    return;
  }
  try {
    const extracted = await invokeCmd('extract_table_from_bin', { bin_bytes: binBytes(currentBin), table: currentTable });
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
    const dec = (currentTable && currentTable.decimals != null) ? currentTable.decimals : 1;
    const fmt = (v) => (typeof v === 'number' ? v.toFixed(dec) : v);
    const colH = ((currentTable && currentTable.col_headers) || '').split(',').map((s) => s.trim()).filter(Boolean);
    const rowH = ((currentTable && currentTable.row_headers) || '').split(',').map((s) => s.trim()).filter(Boolean);
    const r0 = currentValues.length, c0 = (currentValues[0] || []).length;
    if (r0 === 1 && c0 === 1) {
      const v = currentValues[0][0];
      el.innerHTML = '<div class="one-d-row"><div class="one-d-cell"><label>' + (currentTable && currentTable.units ? currentTable.units : 'value') + '</label>' +
        '<input data-r="0" data-c="0" value="' + fmt(v) + '"></div></div>' +
        '<div class="table-editor-footer"><button id="btn-apply-patch" class="btn btn-primary" type="button">Apply Patch + Auto Checksum</button></div>';
    } else {
      let html = '<table class="map-table"><tbody>';
      if (colH.length) {
        html += '<tr><th class="axis"></th>' + colH.map((h) => '<th class="axis">' + h + '</th>').join('') + '</tr>';
      }
      currentValues.forEach((row, ri) => {
        html += '<tr>';
        if (rowH.length) html += '<th class="axis">' + (rowH[ri] || '') + '</th>';
        row.forEach((v, ci) => {
          const num = typeof v === 'number' ? v : parseFloat(v);
          const bg = Number.isFinite(num) ? heatColor(num, min, max) : 'transparent';
          html += '<td contenteditable="true" data-r="' + ri + '" data-c="' + ci + '" style="background:' + bg + '">' + fmt(v) + '</td>';
        });
        html += '</tr>';
      });
      html += '</tbody></table><div class="table-editor-footer"><button id="btn-apply-patch" class="btn btn-primary" type="button">Apply Patch + Auto Checksum</button><span class="muted">min ' + min.toFixed(dec) + ' → max ' + max.toFixed(dec) + '</span></div>';
      el.innerHTML = html;
    }
    document.getElementById('btn-apply-patch')?.addEventListener('click', applyCurrentPatch);
    el.querySelectorAll('td[contenteditable], input[data-r]').forEach((td) => {
      td.onblur = () => {
        const r = +td.dataset.r, c = +td.dataset.c;
        const num = parseFloat(td.tagName === 'INPUT' ? td.value : td.textContent);
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

async function savePatchedBin() {
  if (!currentBin || !currentBin.length) {
    setTablesStatus('Load a .BIN first.');
    return;
  }
  setTablesStatus('Saving BIN…');
  try {
    const res = await saveNativeBytes(currentBin, 'tuned.bin');
    if (res && res.cancelled) { setTablesStatus('Save cancelled.'); return; }
    setTablesStatus('Saved ' + currentBin.length + ' bytes' + (res && res.path ? ' to ' + res.path : ''));
  } catch (e) {
    setTablesStatus('Save failed: ' + e);
  }
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
  if (!currentBin) { setTablesStatus('Load the first .BIN first'); return; }
  setTablesStatus('Opening second BIN…');
  try {
    const res = await openNativeFile('bin');
    if (res.cancelled) { setTablesStatus('Open cancelled.'); return; }
    const info = parseMaybe(await invokeCmd('compare_bins_cmd', { a: binBytes(currentBin), b: res.bytes || [] }));
    setTablesStatus(info.message || 'Compare done');
    const cs = document.getElementById('side-checksum');
    if (cs) cs.textContent = JSON.stringify(info, null, 2);
  } catch (e) {
    setTablesStatus('Compare error: ' + e);
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
    const raw = await invokeCmd('export_workspace_cmd', { data: currentBin ? binBytes(currentBin) : null });
    const text = typeof raw === 'string' ? raw : JSON.stringify(raw, null, 2);
    const res = await saveNativeText(text, 'tuneitverse-workspace.json');
    if (res && res.cancelled) { setTablesStatus('Save cancelled.'); return; }
    setTablesStatus('Workspace saved' + (res && res.path ? ': ' + res.path : ''));
  } catch (e) { setTablesStatus('Workspace export failed: ' + e); }
}

async function importWorkspace() {
  setTablesStatus('Opening workspace JSON…');
  try {
    const file = await openNativeFile('json');
    if (file.cancelled) { setTablesStatus('Open cancelled.'); return; }
    const res = parseMaybe(await invokeCmd('import_workspace_cmd', { jsonText: file.text || '', json_text: file.text || '' }));
    setTablesStatus('Workspace import: ' + JSON.stringify(res));
    if (res && res.identify) applyIdentify(res.identify);
    if (res && res.map_from_log) {
      lastMapFromLog = res.map_from_log;
      renderHeatmap(res.map_from_log);
    }
  } catch (e) {
    setTablesStatus('Workspace import failed: ' + e);
  }
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
  const search = document.getElementById('table-search');
  if (search && !search.dataset.bound) {
    search.dataset.bound = '1';
    search.addEventListener('input', () => { tableSearch = search.value || ''; renderTableList(); });
  }
  document.querySelectorAll('.table-filters .chip-filter[data-filter]').forEach((ch) => {
    ch.onclick = () => {
      document.querySelectorAll('.table-filters .chip-filter[data-filter]').forEach((c) => c.classList.remove('active'));
      ch.classList.add('active');
      tableFilter = ch.dataset.filter || 'all';
      renderTableList();
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

async function compareBinToEcuUi() {
  const pre = document.getElementById('compare-result');
  if (pre) { pre.hidden = false; pre.textContent = 'Comparing…'; }
  if (!currentBin || !currentBin.length) {
    if (pre) pre.textContent = 'Load a .BIN in Tables first (works offline). Live compare needs an adapter.';
    return;
  }
  if (!(await requireConnected('live BIN compare'))) {
    if (pre) pre.textContent = 'Not connected. Offline: use Compare Another BIN on Tables.';
    return;
  }
  try {
    const res = await invokeCmd('compare_bin_to_ecu', { file_bytes: binBytes(currentBin) });
    if (pre) pre.textContent = typeof res === 'string' ? res : JSON.stringify(res, null, 2);
  } catch (e) {
    if (pre) pre.textContent = 'Compare error: ' + e;
  }
}

async function verifyAfterWriteUi() {
  const pre = document.getElementById('compare-result');
  if (pre) { pre.hidden = false; pre.textContent = 'Verifying…'; }
  if (!(await requireConnected('live verify'))) {
    if (pre) pre.textContent = 'Not connected.';
    return;
  }
  try {
    if (!currentBin || !currentBin.length) throw new Error('Load a .BIN in Tables first.');
    const res = await invokeCmd('verify_after_write', { expected_bytes: binBytes(currentBin) });
    if (pre) pre.textContent = typeof res === 'string' ? res : JSON.stringify(res, null, 2);
  } catch (e) {
    if (pre) pre.textContent = 'Verify error: ' + e;
  }
}

function showRiskUi() {
  const sec = document.getElementById('risk-section');
  if (sec) sec.hidden = false;
}

async function runGuidedFlashUi() {
  const log = document.getElementById('flash-log');
  if (!(await requireConnected('guided flash'))) {
    if (log) log.textContent = 'Not connected — flash needs an adapter. Load/edit/save a BIN on Tables without an ECU.\n';
    return;
  }
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
    if (!identifiedFamily()) await identifyCurrentBin();
    const family = identifiedFamily();
    if (!family) {
      if (log) log.textContent += 'Identify did not resolve a family. Refusing write.\n';
      setFlashStep('identify', 'fail');
      return;
    }
    setFlashStep('identify', 'ok');
    const req = {
      ecu_family: family,
      bin_bytes: binBytes(currentBin),
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
}

function setupFlash() {
  ['risk-backup', 'risk-power', 'risk-ground', 'risk-understand'].forEach((id) => {
    const cb = document.getElementById(id);
    if (cb) cb.onchange = () => {
      const all = ['risk-backup', 'risk-power', 'risk-ground', 'risk-understand'].every((i) => document.getElementById(i)?.checked);
      const btn = document.getElementById('btn-run-flash');
      if (btn) btn.disabled = !all;
    };
  });
}

async function refreshScriptsUi() {
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
}

function setupScripts() {}

function bindAppClicks() {
  if (document.body.dataset.actionsBound) return;
  document.body.dataset.actionsBound = '1';
  const actions = {
    'btn-refresh-ports': refreshPorts,
    'btn-do-connect': doConnect,
    'btn-do-disconnect': doDisconnect,
    'btn-auto-detect': doAutoDetect,
    'btn-compute-key': computeSeedKeyUi,
    'btn-j2534-list': refreshJ2534Devices,
    'btn-log-start': startLogging,
    'btn-log-stop': stopLogging,
    'btn-log-clear': clearLog,
    'btn-log-export': exportCsv,
    'btn-log-import': importCsv,
    'btn-log-apply-ch': applyChannels,
    'btn-log-apply-tmpl': applyTemplate,
    'btn-read-dtcs': readDtcs,
    'btn-read-freeze': readFreezeFrame,
    'btn-clear-dtcs': clearDtcs,
    'btn-load-bin': loadBinFile,
    'btn-load-xdf': loadXdfFile,
    'btn-load-a2l': loadA2l,
    'btn-save-patched': savePatchedBin,
    'btn-identify-bin': identifyCurrentBin,
    'btn-compare-bins': compareAnotherBin,
    'btn-map-from-log': mapFromLog,
    'btn-export-workspace': exportWorkspace,
    'btn-import-workspace': importWorkspace,
    'btn-scan-cs': scanCs,
    'btn-hex-poke': pokeHex,
    'btn-tbl-scale': () => runMath('scale'),
    'btn-tbl-offset': () => runMath('add'),
    'btn-tbl-smooth': () => runMath('smooth'),
    'btn-tbl-stft': applyStft,
    'btn-compare-bin': compareBinToEcuUi,
    'btn-verify-write': verifyAfterWriteUi,
    'btn-show-risk': showRiskUi,
    'btn-run-flash': runGuidedFlashUi,
    'btn-refresh-scripts': refreshScriptsUi
  };
  document.body.addEventListener('click', (e) => {
    const btn = e.target.closest('button[id]');
    if (!btn || !actions[btn.id]) return;
    e.preventDefault();
    Promise.resolve(actions[btn.id]()).catch((err) => {
      console.error(btn.id, err);
      setStatus(btn.id + ': ' + (err && err.message ? err.message : err));
    });
  });
}

function setupAll() {
  setupNav();
  bindAppClicks();
  const steps = [setupConnect, setupLive, setupDiagnostics, setupTablesUI, setupFlash, setupScripts];
  steps.forEach((fn) => {
    try { fn(); } catch (e) { console.error(fn.name, e); setStatus(fn.name + ' failed: ' + e); }
  });
  showView('dashboard');
  pollHealth();
  if (healthTimer) clearInterval(healthTimer);
  healthTimer = setInterval(pollHealth, 2500);
  console.log('TuneItVerse UI v3.11.0');
}

setupNav();
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', setupAll);
} else {
  setupAll();
}
