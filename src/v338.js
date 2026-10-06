// TuneItVerse v3.38.0 — flash preflight, BIN strings, offline DTC lookup.
// None of these enable write.
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
  function show(id, text) {
    const node = document.getElementById(id);
    if (!node) return;
    node.textContent = text == null ? '' : String(text);
  }
  function workingBin() {
    if (!window.currentBin || !window.currentBin.length) return null;
    return Array.from(window.currentBin);
  }
  async function preflight() {
    const data = workingBin();
    if (!data) {
      show('flash-preflight', 'Load a BIN on Maps first. Preflight does not flash.');
      return;
    }
    const batt = document.getElementById('kpi-batt');
    const text = batt ? String(batt.textContent || '') : '';
    const parsed = parseFloat(text);
    const voltage = Number.isFinite(parsed) ? parsed : null;
    const raw = parseMaybe(await cmd('flash_preflight_cmd', { data: data, voltageV: voltage }));
    if (raw && raw.write_allowed && raw.ready && raw.family !== 'P01_0411' && raw.family !== 'EDC16C41') {
      show('flash-preflight', 'Refusing preflight: ready write outside P01_0411 / EDC16C41.');
      return;
    }
    show('flash-preflight', JSON.stringify(raw, null, 2));
    const status = document.getElementById('st-identify');
    if (status && raw) status.textContent = raw.ready ? 'preflight ready' : 'blocked';
  }
  async function strings() {
    const data = workingBin();
    if (!data) {
      show('inspect-output', 'Load a BIN on Maps first.');
      return;
    }
    const raw = parseMaybe(await cmd('bin_strings_cmd', { data: data, minLen: 5 }));
    show('inspect-output', JSON.stringify(raw, null, 2));
    const status = document.getElementById('inspect-status');
    if (status) status.textContent = 'Strings do not enable write.';
  }
  async function lookup() {
    const input = document.getElementById('dtc-lookup-code');
    const code = input ? input.value : '';
    const raw = parseMaybe(await cmd('dtc_lookup_cmd', { code: code || 'P0300' }));
    show('dtc-summary', JSON.stringify(raw, null, 2));
  }
  function boot() {
    document.body.addEventListener('click', function (ev) {
      const btn = ev.target && ev.target.closest ? ev.target.closest('button') : null;
      if (!btn) return;
      const run = {
        'btn-flash-preflight': preflight,
        'btn-inspect-strings': strings,
        'btn-dtc-lookup': lookup
      }[btn.id];
      if (!run) return;
      ev.preventDefault();
      Promise.resolve(run()).catch(function (e) {
        const target = btn.id === 'btn-flash-preflight' ? 'flash-preflight'
          : btn.id === 'btn-inspect-strings' ? 'inspect-output' : 'dtc-summary';
        show(target, String(e));
      });
    });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
  else boot();
})();
