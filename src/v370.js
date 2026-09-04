// v3.7.0 overlay — A2L load, table math, STFT preview apply
(function () {
  function parseMaybe(raw) {
    if (raw == null) return null;
    if (typeof raw === 'string') {
      try { return JSON.parse(raw); } catch (e) { return raw; }
    }
    return raw;
  }

  function syncGlobals() {
    try { if (typeof currentBin !== 'undefined') window.currentBin = currentBin; } catch (e) {}
    try { if (typeof currentValues !== 'undefined') window.currentValues = currentValues; } catch (e) {}
    try { if (typeof currentTables !== 'undefined') window.currentTables = currentTables; } catch (e) {}
    try { if (typeof currentTable !== 'undefined') window.currentTable = currentTable; } catch (e) {}
  }

  function setValues(next) {
    try { currentValues = next; } catch (e) {}
    window.currentValues = next;
    if (typeof window.renderEditor === 'function') window.renderEditor();
    else if (typeof renderCurrentEditor === 'function') renderCurrentEditor();
  }

  async function runMath(op) {
    syncGlobals();
    const values = window.currentValues;
    if (!values) { alert('Select a table first'); return; }
    const req = { values: values, op: op };
    if (op === 'scale') {
      const el = document.getElementById('tbl-scale');
      req.factor = parseFloat((el && el.value) || '1');
      if (!isFinite(req.factor)) { alert('Scale factor must be a number'); return; }
    }
    if (op === 'add') {
      const el = document.getElementById('tbl-offset');
      req.offset = parseFloat((el && el.value) || '0');
      if (!isFinite(req.offset)) { alert('Offset must be a number'); return; }
    }
    try {
      const raw = await window.invokeCmd('table_math_cmd', { req: req });
      const res = parseMaybe(raw);
      if (!res || !res.values) throw new Error('no values');
      setValues(res.values);
      const st = document.getElementById('tables-status');
      if (st) st.textContent = (res.message || op) + ' — ' + res.cells_changed + ' cells. Apply Patch to write BIN.';
    } catch (e) {
      alert('Table math failed: ' + e);
    }
  }

  async function applyStft() {
    syncGlobals();
    const values = window.currentValues;
    const hint = window.lastMapFromLog;
    if (!values) { alert('Select a table first'); return; }
    if (!hint || !hint.occupancy_16x16) { alert('Run Map from Log first so STFT occupancy exists'); return; }
    if (values.length !== hint.occupancy_16x16.length) {
      if (!confirm('Table size ' + values.length + 'x' + (values[0]||[]).length + ' vs occupancy 16x16. Continue anyway?')) return;
    }
    try {
      const raw = await window.invokeCmd('apply_stft_preview_cmd', {
        req: {
          values: values,
          occupancy: hint.occupancy_16x16,
          stft_avg: hint.stft_avg_16x16,
          gain: 0.25,
          min_hits: 3
        }
      });
      const res = parseMaybe(raw);
      if (!res || !res.values) throw new Error('no preview');
      setValues(res.values);
      const st = document.getElementById('tables-status');
      if (st) st.textContent = res.message + ' (' + res.cells_changed + ' cells). Apply Patch to write BIN.';
    } catch (e) {
      alert('STFT preview failed: ' + e);
    }
  }

  function loadA2l() {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.a2l,.A2L,.txt';
    input.onchange = async function () {
      const file = input.files && input.files[0];
      if (!file) return;
      const text = await file.text();
      const st = document.getElementById('tables-status');
      try {
        const defs = await window.invokeCmd('parse_a2l_definitions', { text: text });
        const list = Array.isArray(defs) ? defs : parseMaybe(defs);
        if (!list || !list.length) {
          if (st) st.textContent = 'A2L parsed but no CHARACTERISTIC found';
          return;
        }
        try { currentTables = list; } catch (e) {}
        window.currentTables = list;
        if (typeof renderTableList === 'function') renderTableList();
        if (st) st.textContent = 'A2L loaded: ' + list.length + ' characteristics. Confirm addresses on your dump.';
      } catch (e) {
        if (st) st.textContent = 'A2L parse error: ' + e;
      }
    };
    input.click();
  }

  window.addEventListener('DOMContentLoaded', function () {
    document.getElementById('btn-tbl-scale') && document.getElementById('btn-tbl-scale').addEventListener('click', function () { runMath('scale'); });
    document.getElementById('btn-tbl-offset') && document.getElementById('btn-tbl-offset').addEventListener('click', function () { runMath('add'); });
    document.getElementById('btn-tbl-smooth') && document.getElementById('btn-tbl-smooth').addEventListener('click', function () { runMath('smooth'); });
    document.getElementById('btn-tbl-stft') && document.getElementById('btn-tbl-stft').addEventListener('click', applyStft);
    document.getElementById('btn-load-a2l') && document.getElementById('btn-load-a2l').addEventListener('click', loadA2l);
    syncGlobals();
  });

  const prevInvoke = window.invokeCmd;
  window.invokeCmd = async function (cmd, args) {
    const out = typeof prevInvoke === 'function' ? await prevInvoke(cmd, args) : null;
    if (cmd === 'parse_xdf_definitions') {
      const list = Array.isArray(out) ? out : parseMaybe(out);
      if ((!list || !list.length) && args && args.xml && /BEGIN CHARACTERISTIC/i.test(args.xml)) {
        return prevInvoke('parse_a2l_definitions', { text: args.xml });
      }
    }
    return out;
  };
})();
