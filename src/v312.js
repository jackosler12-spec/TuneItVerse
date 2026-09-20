// TuneItVerse v3.25.0 overlay — catalog write flags, checksums, flash helpers, scripts.
(function () {
  const PREF_KEY = 'tiv_conn';

  function parseMaybe(raw) {
    if (raw == null) return null;
    if (typeof raw === 'string') { try { return JSON.parse(raw); } catch (_) { return raw; } }
    return raw;
  }
  async function cmd(name, args) {
    if (typeof window.invokeCmd === 'function') return window.invokeCmd(name, args || {});
    throw new Error('backend unavailable');
  }

  function loadPrefs() {
    try { return JSON.parse(localStorage.getItem(PREF_KEY) || '{}') || {}; } catch (_) { return {}; }
  }
  function savePrefs(extra) {
    try {
      const cur = loadPrefs();
      const port = document.getElementById('port-select')?.value || cur.port || '';
      const baud = document.getElementById('baud-select')?.value || cur.baud || '115200';
      const proto = document.querySelector('input[name="proto"]:checked')?.value || cur.proto || 'auto';
      localStorage.setItem(PREF_KEY, JSON.stringify(Object.assign(cur, { port, baud, proto }, extra || {})));
    } catch (_) {}
  }
  function restorePrefs() {
    const p = loadPrefs();
    const baud = document.getElementById('baud-select');
    if (baud && p.baud) baud.value = String(p.baud);
    if (p.proto) {
      const r = document.querySelector('input[name="proto"][value="' + p.proto + '"]');
      if (r) r.checked = true;
    }
    const sel = document.getElementById('port-select');
    if (sel && p.port) {
      const hit = Array.from(sel.options).find((o) => o.value === p.port);
      if (hit) sel.value = p.port;
    }
  }

  async function loadCatalog() {
    const body = document.getElementById('catalog-tbody');
    if (!body) return;
    try {
      const rows = parseMaybe(await cmd('list_ecu_catalog')) || [];
      const list = Array.isArray(rows) ? rows : [];
      if (!list.length) { body.innerHTML = '<tr><td colspan="7">Catalog empty.</td></tr>'; return; }
      body.innerHTML = list.map((e) =>
        '<tr><td>' + (e.ecu_family || '') + '</td><td>' + (e.hardware || '') + '</td><td>' + (e.protocol || '') +
        '</td><td>' + (e.bin_size_bytes || '') + '</td><td>' + (e.checksum || '') + '</td><td>' + (e.security || '') +
        '</td><td>' + (e.write_allowed ? 'LIVE' : 'blocked') + '</td></tr>'
      ).join('');
    } catch (e) {
      body.innerHTML = '<tr><td colspan="7">' + e + '</td></tr>';
    }
  }

  async function refreshDashLive() {
    const vinEl = document.getElementById('dash-vin-calid');
    const hint = document.getElementById('dash-conn-hint');
    try {
      const snap = parseMaybe(await cmd('session_snapshot'));
      if (hint && snap) hint.textContent = (snap.health || 'Disconnected') + (snap.last_family ? (' · ' + snap.last_family) : '');
      const props = parseMaybe(await cmd('read_properties'));
      if (vinEl && props) {
        vinEl.textContent = 'VIN: ' + (props.vin || 'UNREAD') + '  CALID: ' + (props.calid || 'UNREAD');
      }
    } catch (_) {
      if (vinEl) vinEl.textContent = 'VIN / CALID: offline';
    }
  }

  async function validateCs() {
    const bin = window.currentBin;
    if (!bin) { alert('Load a BIN first'); return; }
    const cs = document.getElementById('side-checksum');
    try {
      const summary = await cmd('validate_bin_checksums_summary_cmd', { data: Array.from(bin) });
      if (cs) cs.textContent = typeof summary === 'string' ? summary : JSON.stringify(summary, null, 2);
    } catch (e) { if (cs) cs.textContent = String(e); }
  }

  async function correctCs() {
    const bin = window.currentBin;
    if (!bin) { alert('Load a BIN first'); return; }
    if (!confirm('Correct checksums on the loaded image? Honda / P59 / unknown sizes stay fail-closed.')) return;
    const cs = document.getElementById('side-checksum');
    try {
      const probe = parseMaybe(await cmd('correct_bin_checksums_report', { data: Array.from(bin) }));
      if (!probe || probe.success === false) {
        const msg = (probe && probe.error) || 'correction refused';
        if (cs) cs.textContent = msg;
        return;
      }
      const out = await cmd('correct_bin_checksums', { data: Array.from(bin) });
      window.currentBin = out instanceof Uint8Array ? out : new Uint8Array(out);
      const summary = await cmd('validate_bin_checksums_summary_cmd', { data: Array.from(window.currentBin) });
      if (cs) cs.textContent = JSON.stringify(probe.report || {}, null, 2) + '\n\n' + (typeof summary === 'string' ? summary : '');
      const save = document.getElementById('btn-save-patched');
      if (save) save.disabled = false;
    } catch (e) { if (cs) cs.textContent = String(e); }
  }

  async function flashCmd(name, args, label) {
    const log = document.getElementById('flash-log');
    const pre = document.getElementById('compare-result');
    if (pre) pre.hidden = false;
    try {
      const raw = await cmd(name, args || {});
      const text = typeof raw === 'string' ? raw : JSON.stringify(parseMaybe(raw), null, 2);
      if (log) log.textContent = (log.textContent || '') + (label || name) + ': ' + text + '\n';
      if (pre) pre.textContent = text;
    } catch (e) {
      if (log) log.textContent = (log.textContent || '') + (label || name) + ' error: ' + e + '\n';
    }
  }

  function bind(id, fn) {
    const el = document.getElementById(id);
    if (!el || el.dataset.v312) return;
    el.dataset.v312 = '1';
    el.addEventListener('click', function (ev) { ev.preventDefault(); Promise.resolve(fn()).catch(function (e) { console.error(id, e); }); });
  }

  function listenFlashProgress() {
    try {
      const ev = window.__TAURI__ && window.__TAURI__.event;
      if (!ev || typeof ev.listen !== 'function') return;
      ev.listen('flash-progress', function (event) {
        const p = event && event.payload ? event.payload : event;
        if (!p) return;
        const pct = typeof p.percent === 'number' ? p.percent : 0;
        const bar = document.getElementById('flash-bar');
        if (bar) bar.style.width = pct + '%';
        const prog = document.getElementById('flash-progress');
        if (prog) {
          const warn = p.voltage_warn != null ? '  Vbatt ' + Number(p.voltage_warn).toFixed(2) + ' V' : '';
          prog.textContent = pct + '%  ' + (p.bytes_done || 0) + '/' + (p.bytes_total || 0) + warn;
        }
      });
    } catch (_) {}
  }

  function ensureScriptEditor() {
    const view = document.getElementById('view-scripts');
    if (!view || document.getElementById('script-source')) return;
    const body = view.querySelector('.view-body') || view;
    const wrap = document.createElement('div');
    wrap.innerHTML = '<textarea id="script-source" class="mono-block" rows="10">identify\nchecksum\nhelp\n</textarea><p><button id="btn-run-script" class="btn btn-primary" type="button">Run</button></p><pre id="script-output" class="mono-block">Load a BIN on Maps, then Run.</pre>';
    body.insertBefore(wrap, body.firstChild);
  }

  async function runBenchScript() {
    const src = document.getElementById('script-source');
    const out = document.getElementById('script-output');
    const source = src ? src.value : 'help';
    if (out) out.textContent = 'Running…';
    try {
      const bin = window.currentBin;
      const raw = await cmd('run_bench_script', {
        source: source,
        working_bin: bin ? Array.from(bin) : null,
        compare_bin: window.compareBin ? Array.from(window.compareBin) : null
      });
      const res = parseMaybe(raw) || {};
      if (out) out.textContent = JSON.stringify(res, null, 2);
      if (res.mutated && Array.isArray(res.bin) && res.bin.length) {
        window.currentBin = new Uint8Array(res.bin);
        const save = document.getElementById('btn-save-patched');
        if (save) save.disabled = false;
      }
    } catch (e) {
      if (out) out.textContent = String(e);
    }
  }

  function boot() {
    restorePrefs();
    setTimeout(restorePrefs, 400);
    loadCatalog();
    refreshDashLive();
    listenFlashProgress();
    ensureScriptEditor();
    bind('btn-validate-cs', validateCs);
    bind('btn-correct-cs', correctCs);
    bind('btn-check-voltage', function () { return flashCmd('read_battery_voltage_cmd', {}, 'Voltage'); });
    bind('btn-unlock-l1', function () { return flashCmd('unlock_level1', {}, 'Unlock L1'); });
    bind('btn-unlock-l2', function () { return flashCmd('unlock_level2', {}, 'Unlock L2'); });
    bind('btn-bosch-unlock', function () {
      const fam = (window.lastIdentify && window.lastIdentify.family) || document.getElementById('seed-family')?.value || 'EDC16C41';
      return flashCmd('bosch_uds_unlock', { family: fam, level: 'programming' }, 'Bosch UDS unlock');
    });
    bind('btn-run-script', runBenchScript);
    const connectBtn = document.getElementById('btn-do-connect');
    if (connectBtn && !connectBtn.dataset.v313pref) {
      connectBtn.dataset.v313pref = '1';
      connectBtn.addEventListener('click', function () { savePrefs(); });
    }
    try {
      cmd('app_info').then(function (raw) {
        const info = parseMaybe(raw) || {};
        const sl = document.getElementById('status-left');
        if (sl && info.version) sl.textContent = 'TuneItVerse ' + info.version;
        const badge = document.querySelector('.sidebar-logo-text .version');
        if (badge && info.version) badge.textContent = 'v' + info.version;
      }).catch(function () {
        const sl = document.getElementById('status-left');
        if (sl) sl.textContent = 'TuneItVerse 3.25.0';
      });
    } catch (_) {}
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
  else boot();
})();
