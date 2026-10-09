// TuneItVerse v3.39.0 — load the v3.33/v3.34 desks, plus readiness, seed coverage, table CSV.
// Readiness never authorizes a flash. Write path stays P01_0411 and EDC16C41.
(function () {
  function parseMaybe(raw) {
    if (raw == null) return null;
    if (typeof raw === 'string') {
      try { return JSON.parse(raw); } catch (_) { return raw; }
    }
    return raw;
  }
  async function cmd(name, args) {
    if (typeof window.invokeCmd !== 'function') throw new Error('backend unavailable');
    return window.invokeCmd(name, args || {});
  }
  function flashLog(text) {
    const node = document.getElementById('flash-log');
    if (node) node.textContent = text == null ? '' : String(text);
  }
  function tableValues() {
    const values = window.currentValues;
    if (Array.isArray(values) && values.length && Array.isArray(values[0])) return values;
    return null;
  }
  function labels(raw) {
    return String(raw || '').split(',').map(function (s) { return s.trim(); }).filter(Boolean);
  }

  async function readiness() {
    const bin = window.currentBin;
    if (!bin || !bin.length) {
      flashLog('Load a BIN on Maps first. Readiness does not talk to the ECU.');
      return;
    }
    const raw = parseMaybe(await cmd('flash_readiness_cmd', { data: Array.from(bin) }));
    if (raw && raw.flash_allowed_now) {
      flashLog('Refusing readiness: flash_allowed_now must stay false.');
      return;
    }
    flashLog(JSON.stringify(raw, null, 2));
  }
  async function coverage() {
    const raw = parseMaybe(await cmd('seed_coverage_cmd'));
    flashLog(JSON.stringify(raw, null, 2));
  }
  async function csv() {
    const values = tableValues();
    if (!values) {
      alert('Select a table first');
      return;
    }
    const table = window.currentTable || {};
    const raw = parseMaybe(await cmd('table_csv_cmd', {
      req: {
        values: values,
        row_labels: labels(table.row_headers),
        col_labels: labels(table.col_headers)
      }
    }));
    const text = raw && raw.csv ? raw.csv : JSON.stringify(raw, null, 2);
    const pre = document.getElementById('inspect-output');
    if (pre) pre.textContent = text;
    if (navigator.clipboard && raw && raw.csv) {
      try { await navigator.clipboard.writeText(raw.csv); } catch (_) {}
    }
    const st = document.getElementById('tables-status');
    if (st) st.textContent = 'CSV exported (' + (raw.rows || '?') + 'x' + (raw.cols || '?') + '). Copied if the clipboard allowed it. Write stays off.';
  }

  document.getElementById('btn-flash-readiness')?.addEventListener('click', function () {
    readiness().catch(function (e) { flashLog(String(e)); });
  });
  document.getElementById('btn-seed-coverage')?.addEventListener('click', function () {
    coverage().catch(function (e) { flashLog(String(e)); });
  });
  document.getElementById('btn-tbl-csv')?.addEventListener('click', function () {
    csv().catch(function (e) { alert(String(e)); });
  });
})();
