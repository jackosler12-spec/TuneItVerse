// TuneItVerse v3.39.0 — bench planner, driver plugins, fingerprint, coverage.
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
  function titles() {
    const pt = document.getElementById('page-title');
    const ps = document.getElementById('page-sub');
    const view = document.querySelector('.nav-item.active');
    const name = view ? view.getAttribute('data-view') : '';
    if (name === 'bench' && pt) {
      pt.textContent = 'Bench';
      if (ps) ps.textContent = 'Session checklist only. Does not start a write.';
    }
    if (name === 'plugins' && pt) {
      pt.textContent = 'Plugins';
      if (ps) ps.textContent = 'Driver descriptors. Cannot enable write.';
    }
  }
  async function loadFamilies() {
    const sel = document.getElementById('bench-family');
    if (!sel || sel.dataset.loaded) return;
    const raw = parseMaybe(await cmd('list_ecu_catalog'));
    const rows = Array.isArray(raw) ? raw : (raw && raw.families) || [];
    sel.innerHTML = '';
    rows.forEach(function (row) {
      const id = row.ecu_family || row.id || row.family;
      if (!id) return;
      const opt = document.createElement('option');
      opt.value = id;
      opt.textContent = id;
      sel.appendChild(opt);
    });
    if (!sel.options.length) {
      ['P01_0411', 'EDC16C41', 'GM_P59'].forEach(function (id) {
        const opt = document.createElement('option');
        opt.value = id;
        opt.textContent = id;
        sel.appendChild(opt);
      });
    }
    sel.dataset.loaded = '1';
  }
  async function plan() {
    await loadFamilies();
    const family = (document.getElementById('bench-family') || {}).value || 'P01_0411';
    const raw = parseMaybe(await cmd('bench_session_plan', { family: family }));
    if (raw && raw.write_allowed) {
      show('bench-readout', 'Refusing plan: write_allowed must stay false.');
      return;
    }
    show('bench-readout', JSON.stringify(raw, null, 2));
  }
  async function plugins() {
    const raw = parseMaybe(await cmd('list_driver_plugins'));
    if (raw && raw.write_allowed) {
      show('plugin-readout', 'Refusing list: write_allowed must stay false.');
      return;
    }
    show('plugin-readout', JSON.stringify(raw, null, 2));
  }
  async function validate() {
    const text = (document.getElementById('plugin-json') || {}).value || '';
    const raw = parseMaybe(await cmd('validate_driver_plugin', { text: text }));
    if (raw && raw.write_allowed) {
      show('plugin-readout', 'Refusing plugin: write_allowed must stay false.');
      return;
    }
    show('plugin-readout', JSON.stringify(raw, null, 2));
  }
  async function fingerprint() {
    const bin = window.currentBin ? Array.from(window.currentBin) : null;
    if (!bin) {
      show('bench-readout', 'Load a BIN on Maps first.');
      return;
    }
    const raw = parseMaybe(await cmd('fingerprint_bin_cmd', { data: bin }));
    if (raw && raw.write_allowed) {
      show('bench-readout', 'Refusing fingerprint: write_allowed must stay false.');
      return;
    }
    show('bench-readout', JSON.stringify(raw, null, 2));
  }
  async function coverage() {
    const raw = parseMaybe(await cmd('coverage_report_cmd'));
    show('plugin-readout', JSON.stringify(raw, null, 2));
  }
  function boot() {
    document.addEventListener('click', function () { setTimeout(titles, 0); });
    const actions = {
      'btn-bench-plan': plan,
      'btn-bench-fingerprint': fingerprint,
      'btn-plugins-list': plugins,
      'btn-plugins-validate': validate,
      'btn-coverage': coverage
    };
    Object.keys(actions).forEach(function (id) {
      const btn = document.getElementById(id);
      if (!btn) return;
      btn.addEventListener('click', function (ev) {
        ev.preventDefault();
        Promise.resolve(actions[id]()).catch(function (e) {
          show(id.indexOf('plugin') === 0 || id === 'btn-coverage' ? 'plugin-readout' : 'bench-readout', String(e));
        });
      });
    });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
  else boot();
})();
