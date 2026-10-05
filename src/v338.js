// TuneItVerse v3.38.0 — offline flash preflight. Does not enable write.
(function () {
  function parseMaybe(raw) {
    if (raw == null) return null;
    if (typeof raw === 'string') { try { return JSON.parse(raw); } catch (_) { return raw; } }
    return raw;
  }
  async function cmd(name, args) {
    if (typeof window.invokeCmd !== 'function') throw new Error('backend unavailable');
    return window.invokeCmd(name, args || {});
  }
  function show(text) {
    const log = document.getElementById('flash-log');
    if (log) log.textContent = text;
    const pre = document.getElementById('ops-output');
    if (pre) {
      pre.hidden = false;
      pre.textContent = text;
    }
  }
  function binBytes() {
    if (!window.currentBin || !window.currentBin.length) return [];
    return Array.from(window.currentBin);
  }
  async function run() {
    const bytes = binBytes();
    const raw = parseMaybe(await cmd('flash_preflight_cmd', { data: bytes }));
    window.__flashPreflight = raw;
    const ready = !!(raw && raw.ready_for_guided_flash === true && raw.write_allowed === true);
    window.__flashPreflightReady = ready;
    const lines = [];
    lines.push('Flash preflight ' + (raw && raw.app_version ? raw.app_version : '3.38.0'));
    lines.push('family: ' + (raw && raw.family ? raw.family : 'unresolved'));
    lines.push('bytes: ' + (raw ? raw.bytes : 0) + '  checksum_valid: ' + (raw && raw.checksum_valid));
    lines.push('write_allowed: ' + (raw && raw.write_allowed) + '  ready: ' + ready);
    if (raw && raw.blockers && raw.blockers.length) {
      lines.push('blockers:');
      raw.blockers.forEach(function (b) { lines.push('  - ' + b); });
    }
    if (raw && raw.warnings) raw.warnings.forEach(function (w) { lines.push('note: ' + w); });
    lines.push('Proceed stays disabled until this report is ready. Honda / P59 / catalog-only families cannot pass.');
    show(lines.join('\n'));
    const btn = document.getElementById('btn-run-flash');
    if (btn && !ready) btn.disabled = true;
    return raw;
  }
  function boot() {
    const group = document.querySelector('#view-flash .toolbar-group');
    if (group && !document.getElementById('btn-flash-preflight')) {
      const b = document.createElement('button');
      b.id = 'btn-flash-preflight';
      b.className = 'btn btn-primary';
      b.type = 'button';
      b.textContent = 'Preflight';
      group.appendChild(b);
    }
    document.body.addEventListener('click', function (ev) {
      const btn = ev.target && ev.target.closest ? ev.target.closest('button') : null;
      if (!btn) return;
      if (btn.id === 'btn-flash-preflight') {
        run().catch(function (e) { show(String(e)); });
      }
    }, true);
    document.body.addEventListener('click', function (ev) {
      const btn = ev.target && ev.target.closest ? ev.target.closest('#btn-run-flash') : null;
      if (!btn) return;
      if (!window.__flashPreflightReady) {
        ev.preventDefault();
        ev.stopPropagation();
        show('Guided flash refused. Run Preflight on the loaded Maps BIN first. Ready must be true. Write stays P01_0411 and EDC16C41 only.');
        btn.disabled = true;
      }
    }, true);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
  else boot();
})();
