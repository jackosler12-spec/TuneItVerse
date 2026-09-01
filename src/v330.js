// v3.3.0 overlay — seed/key bench, J2534 list, expose globals, honest mocks
(function () {
  const prev = window.invokeCmd;
  window.invokeCmd = async function (cmd, args) {
    args = args || {};
    if (typeof prev === 'function') {
      if (cmd === 'read_properties') {
        try {
          const raw = await prev(cmd, args);
          const obj = typeof raw === 'string' ? JSON.parse(raw) : raw;
          if (obj && obj.os_id === '12225074' && (!window.__TAURI__)) {
            return JSON.stringify({ os_id: 'UNREAD', vin: 'UNREAD', calid: 'UNREAD', hardware: 'UNREAD', ecu_type: 'UNREAD', protocol: 'offline', status: 'Offline (browser mock)' });
          }
          return raw;
        } catch (e) { throw e; }
      }
      return prev(cmd, args);
    }
    if (cmd === 'compute_seed_key') return JSON.stringify({ family: args.family || 'P01_0411', level: args.level || '1', algo: 'mock', seed_hex: args.seed_hex || '', key_hex: '0000', key_len: 2 });
    if (cmd === 'log_import_csv') return JSON.stringify({ running: false, rate_hz: 10, sample_count: 2, channels: [], session_name: 'imported_mock' });
    if (cmd === 'j2534_list_devices') return ['J2534 PassThru is a Windows API. Use Serial/ELM on this OS.'];
    if (cmd === 'bosch_uds_unlock' || cmd === 'guided_flash_pipeline') {
      return JSON.stringify({ success: false, message: 'Refused offline (v3.3 overlay). Connect an adapter.' });
    }
    return null;
  };

  async function computeSeedKeyUi() {
    const seed = document.getElementById('seed-hex') && document.getElementById('seed-hex').value;
    const family = (document.getElementById('seed-family') && document.getElementById('seed-family').value) || 'P01_0411';
    const level = (document.getElementById('seed-level') && document.getElementById('seed-level').value) || '1';
    const out = document.getElementById('seed-result');
    try {
      const raw = await window.invokeCmd('compute_seed_key', { seed_hex: seed || '', family, level });
      if (out) out.textContent = typeof raw === 'string' ? raw : JSON.stringify(raw, null, 2);
    } catch (e) {
      if (out) out.textContent = 'Error: ' + e;
    }
  }

  async function refreshJ2534Devices() {
    const box = document.getElementById('j2534-list');
    const log = document.getElementById('connect-log');
    try {
      const list = await window.invokeCmd('j2534_list_devices');
      const items = Array.isArray(list) ? list : [];
      if (box) box.innerHTML = items.map(function (d) { return '<div style="font-family:monospace;font-size:11px;">' + d + '</div>'; }).join('') || '—';
      if (log) log.textContent = (log.textContent || '') + 'J2534 devices:\n' + items.join('\n') + '\n';
    } catch (e) {
      if (log) log.textContent += 'J2534 list error: ' + e + '\n';
    }
  }

  window.addEventListener('DOMContentLoaded', function () {
    document.getElementById('btn-compute-key') && document.getElementById('btn-compute-key').addEventListener('click', computeSeedKeyUi);
    document.getElementById('btn-j2534-list') && document.getElementById('btn-j2534-list').addEventListener('click', refreshJ2534Devices);
    if (typeof identifyCurrentBin === 'function') window.identifyCurrentBin = identifyCurrentBin;
  });
})();
