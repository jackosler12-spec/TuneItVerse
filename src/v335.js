// TuneItVerse v3.35.0 — definition packs, offline self-check, supply-watch chrome.
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
  async function cmd(name, args) {
    if (typeof window.invokeCmd !== 'function') throw new Error('backend unavailable');
    return window.invokeCmd(name, args || {});
  }

  function ensureButtons() {
    const maps = document.querySelector('#view-maps .toolbar-group, #view-tables .toolbar-group');
    const host = maps || document.querySelector('#view-maps .page-toolbar') || document.getElementById('view-scripts');
    if (host && !document.getElementById('btn-import-pack')) {
      host.appendChild(el('button', { id: 'btn-import-pack', class: 'btn', type: 'button' }, 'Import definition pack'));
    }
    const scripts = document.querySelector('#view-scripts .toolbar-group');
    if (scripts && !document.getElementById('btn-self-check')) {
      scripts.appendChild(el('button', { id: 'btn-self-check', class: 'btn', type: 'button' }, 'Self-check'));
    }
    if (!document.getElementById('ops-output')) {
      const foot = document.querySelector('.status-bar');
      const pre = el('pre', { id: 'ops-output', class: 'mono-block' }, '');
      pre.hidden = true;
      if (foot && foot.parentNode) foot.parentNode.insertBefore(pre, foot);
    }
  }

  function show(obj) {
    const pre = document.getElementById('ops-output');
    if (!pre) return;
    pre.hidden = false;
    pre.textContent = typeof obj === 'string' ? obj : JSON.stringify(obj, null, 2);
  }

  async function importPack() {
    const opened = parseMaybe(await cmd('dialog_open_file', { kind: 'json' }));
    if (!opened || opened.cancelled) return;
    const text = opened.text || '';
    const raw = await cmd('import_definition_pack', { json_text: text });
    const pack = parseMaybe(raw) || raw;
    show(pack);
    if (pack && pack.write_allowed) show('Refusing pack: write_allowed must stay false.');
  }

  async function selfCheck() {
    show(parseMaybe(await cmd('operational_self_check')));
  }

  function boot() {
    ensureButtons();
    document.body.addEventListener('click', function (ev) {
      const btn = ev.target && ev.target.closest ? ev.target.closest('button') : null;
      if (!btn) return;
      if (btn.id === 'btn-import-pack') {
        ev.preventDefault();
        Promise.resolve(importPack()).catch(function (e) { show(String(e)); });
      }
      if (btn.id === 'btn-self-check') {
        ev.preventDefault();
        Promise.resolve(selfCheck()).catch(function (e) { show(String(e)); });
      }
    }, true);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
  else boot();
})();
