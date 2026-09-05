// v3.8.0 overlay — honest live source badge, workspace import, CS window scan
(function () {
  function parseMaybe(raw) {
    if (raw == null) return null;
    if (typeof raw === 'string') {
      try { return JSON.parse(raw); } catch (e) { return raw; }
    }
    return raw;
  }

  function banner(id, text, color) {
    let el = document.getElementById(id);
    if (!el) {
      el = document.createElement('div');
      el.id = id;
      el.style.cssText = 'margin:8px 0;padding:8px;font-size:12px;border:1px solid ' + color + ';color:' + color + ';';
      const host = document.getElementById('tables-status') || document.getElementById('dash-identify');
      if (host && host.parentNode) host.parentNode.insertBefore(el, host.nextSibling);
      else document.body.appendChild(el);
    }
    el.textContent = text;
  }

  async function importWorkspace() {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.json,.txt';
    input.onchange = async function () {
      const file = input.files && input.files[0];
      if (!file) return;
      const text = await file.text();
      const st = document.getElementById('tables-status');
      try {
        const raw = await window.invokeCmd('import_workspace_cmd', { jsonText: text });
        const res = parseMaybe(raw);
        if (st) st.textContent = 'Workspace import: ' + JSON.stringify(res);
      } catch (e) {
        if (st) st.textContent = 'Workspace import failed: ' + e;
      }
    };
    input.click();
  }

  async function scanCs() {
    const bin = window.currentBin;
    const st = document.getElementById('tables-status');
    if (!bin) { alert('Load a BIN first'); return; }
    const bytes = bin instanceof Uint8Array ? Array.from(bin) : bin;
    try {
      const raw = await window.invokeCmd('scan_checksum_candidates_cmd', { data: bytes });
      const res = parseMaybe(raw);
      if (st) st.textContent = 'CS scan: ' + (res && res.note ? res.note : JSON.stringify(res));
      if (res && res.honda_os && !res.gm_p01_os) {
        banner('tiv-honda-guard', 'Honda OS string on this image. P01 additive correction is blocked.', '#c80');
      }
    } catch (e) {
      if (st) st.textContent = 'CS scan failed: ' + e;
    }
  }

  const prevInvoke = window.invokeCmd;
  window.invokeCmd = async function (cmd, args) {
    const out = typeof prevInvoke === 'function' ? await prevInvoke(cmd, args) : null;
    if (cmd === 'read_ecu_data') {
      const data = parseMaybe(out);
      const badge = document.getElementById('connection-status');
      if (badge && data && data.source) {
        badge.dataset.liveSource = data.source;
        if (data.source === 'offline') badge.title = 'No invented live PIDs';
        else if (data.source === 'live-no-pids') badge.title = 'Connected but no Mode 01 PIDs decoded';
        else badge.title = (data.pids_decoded || 0) + ' PIDs decoded';
      }
    }
    if (cmd === 'identify_bin_cmd') {
      const data = parseMaybe(out);
      const dash = document.getElementById('dash-identify');
      if (dash && data) {
        dash.textContent = JSON.stringify({
          size: data.bin_size_bytes,
          family: data.family,
          collision: data.size_collision,
          honda_os: data.honda_os,
          gm_p01_os: data.gm_p01_os,
          correction_safe: data.correction_safe,
          notes: data.notes
        }, null, 2);
      }
      if (data && data.size_collision && !data.family) {
        banner('tiv-collision', 'Size collides across catalog families. Confirm OS string before any corrector.', '#c80');
      }
      if (data && data.honda_os && !data.gm_p01_os) {
        banner('tiv-honda-guard', 'Honda OS string. P01 additive correction is blocked.', '#c80');
      }
    }
    return out;
  };

  window.addEventListener('DOMContentLoaded', function () {
    const ver = document.querySelector('.version');
    if (ver) ver.textContent = 'v3.8.0';
    document.getElementById('btn-import-workspace') && document.getElementById('btn-import-workspace').addEventListener('click', importWorkspace);
    document.getElementById('btn-scan-cs') && document.getElementById('btn-scan-cs').addEventListener('click', scanCs);
  });
})();
