// v3.6.0 overlay — hex poke + OS-ID identify label + checksum honesty
(function () {
  function parseMaybe(raw) {
    if (raw == null) return null;
    if (typeof raw === 'string') {
      try { return JSON.parse(raw); } catch (e) { return raw; }
    }
    return raw;
  }

  async function pokeHex() {
    const bin = window.currentBin;
    if (!bin) { alert('Load a .BIN first'); return; }
    const offEl = document.getElementById('hex-poke-off');
    const valEl = document.getElementById('hex-poke-val');
    const off = parseInt((offEl && offEl.value) || '0', 16);
    const hex = ((valEl && valEl.value) || '').replace(/[^0-9a-fA-F]/g, '');
    if (!hex || hex.length % 2) { alert('Value must be even-length hex'); return; }
    const bytes = [];
    for (let i = 0; i < hex.length; i += 2) bytes.push(parseInt(hex.substr(i, 2), 16));
    try {
      const out = await window.invokeCmd('patch_bin_bytes_cmd', {
        data: Array.from(bin),
        offset: off >>> 0,
        bytes: bytes
      });
      const arr = out instanceof Uint8Array ? out : new Uint8Array(out);
      window.currentBin = arr;
      try { currentBin = arr; } catch (e) {}
      const st = document.getElementById('tables-status');
      if (st) st.textContent = 'Poked ' + bytes.length + ' byte(s) at 0x' + off.toString(16).toUpperCase();
      if (typeof window.renderEditor === 'function') window.renderEditor();
    } catch (e) {
      alert('Poke failed: ' + e);
    }
  }

  const prevIdentify = window.identifyCurrentBin;
  window.identifyCurrentBin = async function () {
    if (typeof prevIdentify === 'function') {
      try { await prevIdentify(); } catch (e) { console.warn(e); }
    }
    try {
      const bin = window.currentBin || (typeof currentBin !== 'undefined' ? currentBin : null);
      if (!bin) return;
      const raw = await window.invokeCmd('identify_bin_cmd', { data: Array.from(bin) });
      const info = parseMaybe(raw);
      const dash = document.getElementById('dash-identify');
      if (dash && info) {
        const fam = info.family || info.family_by_os || info.family_by_size || 'unknown';
        dash.textContent = fam + '  ' + (info.bin_size_bytes || '?') + ' bytes\n' + (info.notes || '');
      }
    } catch (e) { console.warn('identify overlay', e); }
  };

  window.addEventListener('DOMContentLoaded', function () {
    const btn = document.getElementById('btn-hex-poke');
    if (btn) btn.addEventListener('click', pokeHex);
  });
})();
