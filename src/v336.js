// TuneItVerse v3.36.0 — Inspect page: hex window, entropy profile, tune report.
(function () {
  function parseMaybe(raw) {
    if (raw == null) return null;
    if (typeof raw === 'string') { try { return JSON.parse(raw); } catch (_) { return raw; } }
    return raw;
  }
  function bytes() {
    const bin = window.currentBin;
    if (!bin || !bin.length) return null;
    return Array.from(bin);
  }
  function setStatus(text) {
    const el = document.getElementById('inspect-status');
    if (el) el.textContent = text;
  }
  function show(text) {
    const pre = document.getElementById('inspect-output');
    if (!pre) return;
    pre.textContent = text;
  }
  function parseOff(id, fallback) {
    const raw = (document.getElementById(id) && document.getElementById(id).value) || '';
    const t = String(raw).trim();
    if (!t) return fallback;
    const n = t.toLowerCase().indexOf('0x') === 0 ? parseInt(t, 16) : parseInt(t, 10);
    return Number.isFinite(n) && n >= 0 ? n : fallback;
  }
  async function cmd(name, args) {
    if (typeof window.invokeCmd !== 'function') throw new Error('backend unavailable');
    return window.invokeCmd(name, args || {});
  }
  async function profile() {
    const data = bytes();
    if (!data) { setStatus('Load a BIN on Maps first.'); return; }
    const raw = parseMaybe(await cmd('bin_profile', { data: data }));
    show(JSON.stringify(raw, null, 2));
    setStatus('Profile is not an identification. Write path unchanged.');
  }
  async function hex() {
    const data = bytes();
    if (!data) { setStatus('Load a BIN on Maps first.'); return; }
    const offset = parseOff('inspect-offset', 0);
    const length = parseOff('inspect-len', 256);
    const raw = parseMaybe(await cmd('bin_hex_window', { data: data, offset: offset, length: length }));
    show((raw && raw.text) || JSON.stringify(raw, null, 2));
    setStatus('Hex window at 0x' + offset.toString(16) + ' · ' + ((raw && raw.length) || 0) + ' bytes.');
  }
  async function report() {
    const data = bytes();
    if (!data) { setStatus('Load a BIN on Maps first.'); return; }
    const text = await cmd('bin_tune_report', { data: data });
    show(typeof text === 'string' ? text : JSON.stringify(text, null, 2));
    setStatus('Report does not enable write.');
  }
  function boot() {
    document.body.addEventListener('click', function (ev) {
      const btn = ev.target && ev.target.closest ? ev.target.closest('button') : null;
      if (!btn) return;
      const run = btn.id === 'btn-inspect-profile' ? profile
        : btn.id === 'btn-inspect-hex' ? hex
        : btn.id === 'btn-inspect-report' ? report
        : null;
      if (!run) return;
      ev.preventDefault();
      Promise.resolve(run()).catch(function (e) { setStatus(String(e)); show(String(e)); });
    }, true);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
  else boot();
})();
