// TuneItVerse v3.34.0 — Scripts page runner + compare-bin memory.
(function () {
  function el(tag, attrs, text) {
    const n = document.createElement(tag);
    if (attrs) Object.keys(attrs).forEach(function (k) { n.setAttribute(k, attrs[k]); });
    if (text != null) n.textContent = text;
    return n;
  }

  function parseMaybe(raw) {
    if (raw == null) return null;
    if (typeof raw === 'string') { try { return JSON.parse(raw); } catch (_) { return raw; } }
    return raw;
  }

  function ensureScriptUi() {
    const list = document.getElementById('custom-scripts-list');
    if (!list || document.getElementById('script-source')) return;
    const parent = list.parentNode;
    if (!parent) return;
    const hint = el('p', { class: 'muted' }, 'Runs against the BIN loaded on Maps. Compare BIN is remembered from Maps → Compare.');
    const ta = el('textarea', { id: 'script-source', class: 'mono-block', rows: '8' });
    ta.style.width = '100%';
    ta.style.minHeight = '8rem';
    ta.value = 'identify\nchecksum\nhelp\n';
    const out = el('pre', { id: 'script-output', class: 'mono-block log-box' }, '—');
    parent.insertBefore(hint, list);
    parent.insertBefore(ta, list);
    parent.insertBefore(out, list);
    const bar = document.querySelector('#view-scripts .toolbar-group');
    if (bar && !document.getElementById('btn-run-script')) {
      const b = el('button', { id: 'btn-run-script', class: 'btn', type: 'button' }, 'Run against loaded BIN');
      bar.appendChild(b);
    }
    const st = document.querySelector('#view-scripts .toolbar-status');
    if (st) st.textContent = 'Deterministic commands only — no eval, no shell.';
  }

  async function runScript() {
    const src = document.getElementById('script-source') ? document.getElementById('script-source').value : '';
    const out = document.getElementById('script-output');
    if (out) out.textContent = 'Running…';
    const bin = (typeof window.binBytes === 'function' && window.currentBin) ? window.binBytes(window.currentBin) : (window.currentBin || null);
    try {
      if (typeof window.invokeCmd !== 'function') throw new Error('backend unavailable');
      const raw = await window.invokeCmd('run_bench_script', {
        source: src,
        working_bin: bin,
        compare_bin: window.__compareBin || null
      });
      const obj = parseMaybe(raw);
      if (obj && obj.mutated && Array.isArray(obj.bin) && window.currentBin) {
        window.currentBin = new Uint8Array(obj.bin);
        if (typeof window.enableSave === 'function') window.enableSave(true);
      }
      if (out) out.textContent = typeof obj === 'string' ? obj : JSON.stringify(obj, null, 2);
    } catch (e) {
      if (out) out.textContent = 'Error: ' + e;
    }
  }

  function rememberCompare() {
    document.body.addEventListener('click', function (ev) {
      const btn = ev.target && ev.target.closest ? ev.target.closest('button') : null;
      if (!btn) return;
      if (btn.id === 'btn-run-script') {
        ev.preventDefault();
        Promise.resolve(runScript()).catch(function (e) { console.error(e); });
      }
    }, true);
  }

  function boot() {
    ensureScriptUi();
    rememberCompare();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
  else boot();
})();
