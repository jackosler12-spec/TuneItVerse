// TuneItVerse v3.64.0 — local definitions folder (SHA-256 manifest) and GM
// multi-family checksum check / export. Offline only: nothing here talks to an
// ECU, replaces the open BIN, or changes a write gate.
(function () {
  let folder = null;

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
  function panel() {
    const p = document.getElementById('local-defs-panel');
    if (p) p.hidden = false;
  }
  function status(text) {
    panel();
    const pre = document.getElementById('local-defs-status');
    if (pre) pre.textContent = text;
  }
  function list() { return document.getElementById('local-defs-list'); }
  function hex(n) { return n == null ? '?' : String(n); }

  async function pickFolder() {
    const r = parseMaybe(await cmd('dialog_pick_folder'));
    if (!r || r.cancelled || !r.path) { status('Folder selection cancelled.'); return; }
    folder = r.path;
    await scan();
  }

  async function scan() {
    status('Checking SHA-256 of every .xdf in ' + folder + ' …');
    const res = parseMaybe(await cmd('local_defs_scan', { folder: folder }));
    const box = list();
    if (box) box.textContent = '';
    const files = (res && res.files) || [];
    status(folder + '\n' + res.verified + ' of ' + files.length + ' XDF files match the manifest (' + res.manifest_entries + ' known). Unlisted or changed files are refused here.');
    if (!box) return;
    const table = el('table', { class: 'data-table' });
    const head = el('tr');
    ['Status', 'File', 'Family / OS', 'Licence', ''].forEach(function (h) { head.appendChild(el('th', null, h)); });
    table.appendChild(head);
    files.forEach(function (f) {
      const tr = el('tr');
      tr.appendChild(el('td', null, f.status));
      tr.appendChild(el('td', { title: f.sha256 || '' }, f.rel_path));
      const e = f.entry;
      tr.appendChild(el('td', null, e ? (e.family + ' / ' + e.os_ids.join(',') + ' (' + e.tier + ')') : '—'));
      tr.appendChild(el('td', { title: e ? e.source : f.message }, e ? e.licence : f.message));
      const td = el('td');
      if (f.loadable) {
        const b = el('button', { class: 'btn local-defs-load', type: 'button', 'data-rel': f.rel_path }, 'Load');
        td.appendChild(b);
      }
      tr.appendChild(td);
      table.appendChild(tr);
    });
    box.appendChild(table);
  }

  async function loadOne(rel) {
    if (!folder) return;
    status('Loading ' + rel + ' (hash re-checked) …');
    const r = parseMaybe(await cmd('local_defs_load', { folder: folder, rel_path: rel }));
    const c = r.counts || {};
    const lines = [
      rel + ' — ' + r.entry.family + ' OS ' + r.entry.os_ids.join(','),
      'Tables: ' + r.tables.length + ' (confirmed ' + c.confirmed + ', unconfirmed ' + c.unconfirmed + ', untagged ' + c.untagged + ')',
      'Read-only: ' + c.read_only + (c.inferred_order ? (' (incl. ' + c.inferred_order + ' with inferred row/column order)') : ''),
      'Source: ' + r.entry.source,
      'Licence: ' + r.entry.licence,
      r.entry.notes || '',
    ];
    if (r.skipped && r.skipped.length) lines.push('Parser skipped ' + r.skipped.length + ': ' + r.skipped.slice(0, 5).join('; '));
    status(lines.join('\n'));
    if (typeof window.applyDefinitionList === 'function') {
      await window.applyDefinitionList(r.tables, 'Local defs: ' + rel, r.skipped || []);
    }
  }

  async function verifyBin() {
    const bin = window.currentBin;
    if (!bin || !bin.length) { status('Load a BIN first.'); return; }
    const res = parseMaybe(await cmd('gm_checksum_verify', { bin_bytes: Array.from(bin), family: null }));
    const reps = (res && res.reports) || [];
    if (!reps.length) { status(res.note || 'No supported GM layout.'); return; }
    const out = [];
    reps.forEach(function (r) {
      out.push(r.label + ' [' + r.family + ']  ' + r.passed + '/' + r.total + (r.all_valid ? '  ALL VALID' : '  MISMATCH') + '  OS ' + hex(r.os_id) + '  layout: ' + r.remap);
      r.checksums.forEach(function (c) {
        out.push('   ' + (c.ok ? 'OK ' : 'BAD') + ' ' + c.segment + ' ' + c.which + ' @' + c.addr + ' ' + c.method + ' stored ' + c.stored + ' computed ' + c.computed);
      });
      (r.notes || []).forEach(function (n) { out.push('   note: ' + n); });
      out.push('   ' + r.write_support);
    });
    out.push(res.note || '');
    status(out.join('\n'));
  }

  async function exportCorrected() {
    const bin = window.currentBin;
    if (!bin || !bin.length) { status('Load a BIN first.'); return; }
    // Re-identify every time: the open BIN may have changed since the last check.
    const v = parseMaybe(await cmd('gm_checksum_verify', { bin_bytes: Array.from(bin), family: null }));
    const best = ((v && v.reports) || [])[0];
    if (!best) { status('No supported GM layout resolves on this BIN. Nothing exported.'); return; }
    if (!best.all_valid && best.passed === 0) {
      status('Family is not certain (' + best.family + ' resolves but 0/' + best.total + ' checksums match). Nothing exported.');
      return;
    }
    if (!window.confirm('Write a checksum-corrected COPY as ' + best.label + ' (' + best.passed + '/' + best.total + ' valid now)?\nThe open BIN is not changed and nothing is flashed.')) return;
    const r = parseMaybe(await cmd('gm_checksum_fix_export', { bin_bytes: Array.from(bin), family: best.family }));
    const saved = parseMaybe(await cmd('dialog_save_bytes', { data: r.bytes, default_name: 'checksum-corrected-copy.bin' }));
    status((saved && !saved.cancelled ? ('Saved ' + saved.path + '\n') : 'Save cancelled.\n') + r.words_changed + ' checksum word(s) changed. ' + r.note);
  }

  function run(fn) {
    Promise.resolve(fn()).catch(function (e) { status(String(e)); });
  }

  function boot() {
    document.body.addEventListener('click', function (ev) {
      const btn = ev.target && ev.target.closest ? ev.target.closest('button') : null;
      if (!btn) return;
      if (btn.id === 'btn-local-defs-folder') { ev.preventDefault(); run(pickFolder); }
      else if (btn.id === 'btn-gm-cs-verify') { ev.preventDefault(); run(verifyBin); }
      else if (btn.id === 'btn-gm-cs-export') { ev.preventDefault(); run(exportCorrected); }
      else if (btn.id === 'btn-local-defs-close') { ev.preventDefault(); const p = document.getElementById('local-defs-panel'); if (p) p.hidden = true; }
      else if (btn.classList.contains('local-defs-load')) { ev.preventDefault(); const rel = btn.getAttribute('data-rel'); run(function () { return loadOne(rel); }); }
    }, true);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
  else boot();
})();
