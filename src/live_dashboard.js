// Live dashboard: gauges, multi-channel chart, X/Y scatter and hits per map cell.
// Read-only view of the logger buffer via dashboard_snapshot. Never writes.
// v3.52.0: user-saved gauge layouts + per-channel alarm thresholds (localStorage).
(function () {
  'use strict';
  const COLORS = ['#00c4b4', '#f5a623', '#e74c3c', '#9b59b6', '#3498db', '#2ecc71', '#f1c40f', '#ecf0f1'];
  const LS_LAYOUT = 'tiv_ld_layout_v1';
  const LS_ALARMS = 'tiv_ld_alarms_v1';
  let timer = null, snap = null, busy = false;
  function $(id) { return document.getElementById(id); }
  function esc(s) { return String(s == null ? '' : s).replace(/[&<>\"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '\"': '&quot;' }[c])); }
  function fmt(v) { return typeof v === 'number' && isFinite(v) ? (Math.abs(v) >= 100 ? v.toFixed(0) : v.toFixed(1)) : '—'; }
  async function cmd(name, args) {
    if (!window.invokeCmd) throw new Error('Desktop app required');
    const r = await window.invokeCmd(name, args || {});
    return typeof r === 'string' ? JSON.parse(r) : r;
  }
  function visible() { const v = $('view-livedash'); return v && !v.hidden; }
  function guess(names, pats) { for (const p of pats) { const f = names.find((n) => p.test(n)); if (f) return f; } return ''; }

  function loadLayout() {
    try { return JSON.parse(localStorage.getItem(LS_LAYOUT) || '[]'); } catch { return []; }
  }
  function saveLayout(names) {
    localStorage.setItem(LS_LAYOUT, JSON.stringify(names));
  }
  function loadAlarms() {
    try { return JSON.parse(localStorage.getItem(LS_ALARMS) || '{}'); } catch { return {}; }
  }
  function saveAlarms(a) {
    localStorage.setItem(LS_ALARMS, JSON.stringify(a));
  }

  function fillSelects(names) {
    [['ld-x', [/rpm/i, /engine.?speed/i]], ['ld-y', [/^map$/i, /map|load|maf/i]], ['ld-z', [/stft|trim/i, /afr|lambda|o2/i]]].forEach(([id, pats]) => {
      const sel = $(id); if (!sel) return;
      const cur = sel.value;
      const key = names.join('|');
      if (sel.dataset.key === key) return;
      sel.dataset.key = key;
      sel.innerHTML = '<option value="">—</option>' + names.map((n) => `<option>${esc(n)}</option>`).join('');
      sel.value = names.includes(cur) ? cur : guess(names, pats);
    });
  }

  function drawGauge(c, val, mn, mx, alarm) {
    const ctx = c.getContext('2d'), w = c.width, h = c.height, cx = w / 2, cy = h - 8, r = Math.min(w / 2, h) - 10;
    ctx.clearRect(0, 0, w, h);
    ctx.lineWidth = 10; ctx.lineCap = 'round';
    ctx.strokeStyle = '#2a2f36'; ctx.beginPath(); ctx.arc(cx, cy, r, Math.PI, 2 * Math.PI); ctx.stroke();
    const span = mx - mn, f = span > 0 ? Math.max(0, Math.min(1, (val - mn) / span)) : 0.5;
    let color = f > 0.9 ? '#e74c3c' : f > 0.75 ? '#f5a623' : '#00c4b4';
    if (alarm) color = '#e74c3c';
    ctx.strokeStyle = color;
    ctx.beginPath(); ctx.arc(cx, cy, r, Math.PI, Math.PI + f * Math.PI); ctx.stroke();
    ctx.fillStyle = alarm ? '#e74c3c' : '#eee'; ctx.font = 'bold 18px sans-serif'; ctx.textAlign = 'center'; ctx.fillText(fmt(val), cx, cy - 6);
  }

  function renderGauges(stats) {
    const box = $('ld-gauges'); if (!box) return;
    const allNames = Object.keys(stats);
    const layout = loadLayout();
    const show = layout.length ? allNames.filter(n => layout.includes(n)).slice(0, 12) : allNames.slice(0, 12);
    if (!allNames.length) { box.innerHTML = '<p class="muted">No samples yet. Start a log, import a CSV, or use demo mode on Data Logging.</p>'; return; }
    const key = show.join('|');
    if (box.dataset.key !== key) {
      box.dataset.key = key;
      box.innerHTML = show.map((n, i) => `<div class="ld-gauge" data-ch="${esc(n)}"><div class="n">${esc(n)}</div><canvas id="ld-g-${i}" width="140" height="80"></canvas><div class="mm" id="ld-gm-${i}"></div><div class="alarm-row"><input type="number" class="alarm-lo" placeholder="lo" step="any" style="width:50px"> <input type="number" class="alarm-hi" placeholder="hi" step="any" style="width:50px"></div></div>`).join('');
      // restore alarm inputs
      const alarms = loadAlarms();
      show.forEach((n, i) => {
        const g = box.querySelectorAll('.ld-gauge')[i];
        if (!g || !alarms[n]) return;
        const lo = g.querySelector('.alarm-lo'), hi = g.querySelector('.alarm-hi');
        if (lo && alarms[n].lo != null) lo.value = alarms[n].lo;
        if (hi && alarms[n].hi != null) hi.value = alarms[n].hi;
      });
    }
    const alarms = loadAlarms();
    show.forEach((n, i) => {
      const s = stats[n], c = $('ld-g-' + i); if (!c) return;
      const a = alarms[n] || {};
      const alarm = (a.lo != null && s.last < a.lo) || (a.hi != null && s.last > a.hi);
      drawGauge(c, s.last, s.min, s.max, alarm);
      const mm = $('ld-gm-' + i); if (mm) mm.textContent = `min ${fmt(s.min)} · max ${fmt(s.max)} · n ${s.count}`;
    });
  }

  function renderChart(recent, stats) {
    const c = $('ld-chart'); if (!c) return;
    const ctx = c.getContext('2d'), w = c.width, h = c.height;
    ctx.clearRect(0, 0, w, h);
    const names = Object.keys(stats).slice(0, COLORS.length);
    if (recent.length < 2) { $('ld-legend').textContent = 'Need at least 2 samples.'; return; }
    names.forEach((n, ci) => {
      const s = stats[n], span = (s.max - s.min) || 1;
      ctx.strokeStyle = COLORS[ci]; ctx.lineWidth = 1.5; ctx.beginPath();
      let pen = false;
      recent.forEach((p, i) => {
        const v = p.v[n];
        if (typeof v !== 'number') { pen = false; return; }
        const x = (i / (recent.length - 1)) * w, y = h - ((v - s.min) / span) * (h - 10) - 5;
        if (pen) ctx.lineTo(x, y); else { ctx.moveTo(x, y); pen = true; }
      });
      ctx.stroke();
    });
    $('ld-legend').innerHTML = names.map((n, i) => `<span style="color:${COLORS[i]}">■ ${esc(n)} ${fmt(stats[n].last)}</span>`).join(' &nbsp; ');
  }

  function tableAxes() {
    const t = window.currentTable;
    if (!t || !$('ld-use-table')?.checked) return null;
    return { name: t.extra_name || t.name, x: t.col_headers || '', y: t.row_headers || '' };
  }

  function renderScatter(d) {
    const c = $('ld-scatter'); if (!c) return;
    const ctx = c.getContext('2d'), w = c.width, h = c.height;
    ctx.clearRect(0, 0, w, h);
    const pts = d.scatter || [];
    if (!pts.length) { ctx.fillStyle = '#888'; ctx.fillText('Pick X and Y channels present in the log.', 10, 20); return; }
    let xs = pts.map((p) => p[0]), ys = pts.map((p) => p[1]);
    let x0 = Math.min(...xs), x1 = Math.max(...xs), y0 = Math.min(...ys), y1 = Math.max(...ys);
    const ax = parseAxis(tableAxes()?.x), ay = parseAxis(tableAxes()?.y);
    if (ax.length) { x0 = Math.min(x0, ...ax); x1 = Math.max(x1, ...ax); }
    if (ay.length) { y0 = Math.min(y0, ...ay); y1 = Math.max(y1, ...ay); }
    const sx = (v) => 30 + ((v - x0) / ((x1 - x0) || 1)) * (w - 40), sy = (v) => h - 20 - ((v - y0) / ((y1 - y0) || 1)) * (h - 30);
    ctx.strokeStyle = '#2a2f36'; ctx.lineWidth = 1;
    ax.forEach((v) => { ctx.beginPath(); ctx.moveTo(sx(v), 10); ctx.lineTo(sx(v), h - 20); ctx.stroke(); });
    ay.forEach((v) => { ctx.beginPath(); ctx.moveTo(30, sy(v)); ctx.lineTo(w - 10, sy(v)); ctx.stroke(); });
    ctx.fillStyle = 'rgba(0,196,180,0.6)';
    pts.forEach((p) => ctx.fillRect(sx(p[0]) - 1.5, sy(p[1]) - 1.5, 3, 3));
    const last = pts[pts.length - 1]; ctx.fillStyle = '#f5a623'; ctx.beginPath(); ctx.arc(sx(last[0]), sy(last[1]), 5, 0, 2 * Math.PI); ctx.fill();
    ctx.fillStyle = '#aaa'; ctx.font = '11px sans-serif';
    ctx.fillText(`${fmt(x0)}–${fmt(x1)} ${d.x_channel || ''}`, 30, h - 5);
    ctx.save(); ctx.translate(10, h - 20); ctx.rotate(-Math.PI / 2); ctx.fillText(`${fmt(y0)}–${fmt(y1)} ${d.y_channel || ''}`, 0, 0); ctx.restore();
  }
  function parseAxis(s) {
    if (!s) return [];
    const p = String(s).split(/[\s,]+/).filter(Boolean).map(Number);
    return p.every((v) => isFinite(v)) ? p : [];
  }

  function renderCells(d) {
    const box = $('ld-cells'); if (!box) return;
    const c = d.cells, ta = tableAxes();
    if (!c || !c.cols) {
      box.innerHTML = `<p class="muted">${esc(c && c.note ? c.note : 'Open a Maps table with numeric axis headers to bin samples to its cells.')}</p>` +
        (c && c.missing_channel ? `<p class="muted">${c.missing_channel} samples lacked X or Y.</p>` : '');
      return;
    }
    const ax = parseAxis(ta && ta.x), ay = parseAxis(ta && ta.y);
    let maxHit = 1; c.hits.forEach((r) => r.forEach((v) => { if (v > maxHit) maxHit = v; }));
    let html = `<p class="muted">${esc(ta ? ta.name : '')}: cell text = ${d.z_channel ? 'mean ' + esc(d.z_channel) : 'hit count'}; shade = hits. Outside axis: ${c.outside_axis}, missing channel: ${c.missing_channel}.</p>`;
    html += '<table class="data-table ld-cells"><thead><tr><th></th>' + (ax.length ? ax : c.hits[0].map((_, i) => i)).map((v) => `<th>${fmt(v)}</th>`).join('') + '</tr></thead><tbody>';
    c.hits.forEach((row, r) => {
      html += `<tr><th>${ay.length ? fmt(ay[r]) : r}</th>` + row.map((n, ci) => {
        const a = n ? 0.15 + 0.75 * (n / maxHit) : 0;
        const z = c.z_mean[r][ci];
        const txt = n ? (d.z_channel ? (z == null ? '·' : fmt(z)) : n) : '';
        return `<td data-r="${r}" data-c="${ci}" style="background:rgba(0,196,180,${a.toFixed(2)})" title="${n} hits. Click to select this Maps cell.">${txt}</td>`;
      }).join('') + '</tr>';
    });
    box.innerHTML = html + '</tbody></table>';
  }

  function bindAlarmInputs() {
    const box = $('ld-gauges');
    if (!box) return;
    box.querySelectorAll('.ld-gauge').forEach(g => {
      const ch = g.dataset.ch;
      const lo = g.querySelector('.alarm-lo'), hi = g.querySelector('.alarm-hi');
      const save = () => {
        const a = loadAlarms();
        a[ch] = {
          lo: lo && lo.value !== '' ? parseFloat(lo.value) : null,
          hi: hi && hi.value !== '' ? parseFloat(hi.value) : null
        };
        if (a[ch].lo == null && a[ch].hi == null) delete a[ch];
        saveAlarms(a);
      };
      lo?.addEventListener('change', save);
      hi?.addEventListener('change', save);
    });
  }

  async function refresh() {
    if (busy) return; busy = true;
    try {
      const ta = tableAxes();
      const d = await cmd('dashboard_snapshot', {
        series_len: 300,
        x_channel: $('ld-x')?.value || null, y_channel: $('ld-y')?.value || null, z_channel: $('ld-z')?.value || null,
        x_headers: ta ? ta.x : null, y_headers: ta ? ta.y : null
      });
      snap = d;
      fillSelects(Object.keys(d.stats || {}));
      renderGauges(d.stats || {});
      bindAlarmInputs();
      renderChart(d.recent || [], d.stats || {});
      renderScatter(d); renderCells(d);
      const st = $('ld-status');
      if (st) st.textContent = `${d.count} samples${d.demo ? ' · DEMO samples present (simulated, not real data)' : ''} · read-only · not hardware-tested · layouts/alarms local`;
    } catch (e) {
      const st = $('ld-status'); if (st) st.textContent = 'Dashboard: ' + (e.message || e);
    } finally { busy = false; }
  }
  window.liveDashboardSnapshot = () => snap;

  function setupLayoutControls() {
    const btn = $('btn-ld-save-layout');
    if (btn) btn.addEventListener('click', () => {
      const names = Array.from(document.querySelectorAll('#ld-gauges .ld-gauge')).map(g => g.dataset.ch).filter(Boolean);
      saveLayout(names);
      alert('Gauge layout saved (local only).');
    });
    const reset = $('btn-ld-reset-layout');
    if (reset) reset.addEventListener('click', () => {
      localStorage.removeItem(LS_LAYOUT);
      refresh();
    });
  }

  function tick() { if (visible() && $('ld-auto')?.checked) refresh(); }
  function bindHitClicks() {
    const box = $('ld-cells');
    if (!box || box.dataset.bound) return;
    box.dataset.bound = '1';
    box.addEventListener('click', (e) => {
      const td = e.target.closest('td[data-r]');
      if (!td || !box.contains(td)) return;
      if (typeof window.selectMapCell === 'function') window.selectMapCell(+td.dataset.r, +td.dataset.c);
    });
  }

  function init() {
    bindHitClicks();
    $('btn-ld-refresh')?.addEventListener('click', refresh);
    ['ld-x', 'ld-y', 'ld-z', 'ld-use-table'].forEach((id) => $(id)?.addEventListener('change', refresh));
    document.addEventListener('click', (e) => { if (e.target.closest('.nav-item[data-view="livedash"]')) setTimeout(refresh, 50); });
    setupLayoutControls();
    if (!timer) timer = setInterval(tick, 500);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
