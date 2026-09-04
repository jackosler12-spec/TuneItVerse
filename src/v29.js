// v2.9.0 UI helpers — identify BIN, compare BINs, map-from-log
async function identifyCurrentBin() {
  if (!currentBin) { alert('Load a .BIN first'); return; }
  const st = document.getElementById('tables-status');
  const dash = document.getElementById('dash-identify');
  try {
    const raw = await invokeCmd('identify_bin_cmd', { data: Array.from(currentBin) });
    const info = typeof raw === 'string' ? JSON.parse(raw) : raw;
    const text = JSON.stringify(info, null, 2);
    if (st) st.textContent = 'Identified: ' + (info.family_by_size || 'unknown') + ' (' + info.bin_size_bytes + ' bytes)';
    if (dash) dash.textContent = text;
    const meta = document.getElementById('side-meta');
    if (meta) meta.innerHTML = '<pre style="font-size:11px;white-space:pre-wrap;">' + text + '</pre>';
  } catch (e) {
    if (st) st.textContent = 'Identify error: ' + e;
  }
}

async function compareAnotherBin() {
  if (!currentBin) { alert('Load the first .BIN first'); return; }
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.bin,.BIN';
  input.onchange = async (ev) => {
    const file = ev.target.files[0];
    if (!file) return;
    const buf = await file.arrayBuffer();
    const other = Array.from(new Uint8Array(buf));
    const st = document.getElementById('tables-status');
    try {
      const raw = await invokeCmd('compare_bins_cmd', { a: Array.from(currentBin), b: other });
      const info = typeof raw === 'string' ? JSON.parse(raw) : raw;
      if (st) st.textContent = info.message || 'Compare done';
      const cs = document.getElementById('side-checksum');
      if (cs) cs.textContent = JSON.stringify(info, null, 2);
      alert((info.message || 'Compare') + '\nDiff bytes: ' + (info.diff_bytes ?? '?') + ' (' + (info.diff_percent ?? '?') + '%)');
    } catch (e) {
      if (st) st.textContent = 'Compare error: ' + e;
    }
  };
  input.click();
}

async function mapFromLog() {
  const st = document.getElementById('tables-status');
  try {
    const raw = await invokeCmd('map_from_log_cmd');
    const info = typeof raw === 'string' ? JSON.parse(raw) : raw;
    window.lastMapFromLog = info;
    if (st) st.textContent = info.advice || 'Map-from-log ready';
    const adv = document.getElementById('side-advice');
    if (adv) adv.textContent = info.advice || JSON.stringify(info);
    alert(info.advice || JSON.stringify(info, null, 2));
  } catch (e) {
    if (st) st.textContent = 'Map-from-log: ' + e;
    alert('Map-from-log: ' + e);
  }
}

window.identifyCurrentBin = identifyCurrentBin;
window.compareAnotherBin = compareAnotherBin;
window.mapFromLog = mapFromLog;

window.addEventListener('DOMContentLoaded', () => {
  document.getElementById('btn-identify-bin')?.addEventListener('click', identifyCurrentBin);
  document.getElementById('btn-compare-bins')?.addEventListener('click', compareAnotherBin);
  document.getElementById('btn-map-from-log')?.addEventListener('click', mapFromLog);
});
