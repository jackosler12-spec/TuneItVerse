// Standalone wiring for Adapter Status and short bus monitor.
// Include after main.js so invokeCmd / parseMaybe are available.

(function () {
  function ready() {
    const log = document.getElementById('connect-log');

    const statusBtn = document.getElementById('btn-adapter-status');
    if (statusBtn) {
      statusBtn.addEventListener('click', async function () {
        try {
          const raw = await window.invokeCmd('st_adapter_status');
          const st = (typeof parseMaybe === 'function') ? parseMaybe(raw) : (typeof raw === 'string' ? JSON.parse(raw) : raw);
          if (st && log) {
            const v = (st.voltage != null) ? Number(st.voltage).toFixed(1) + 'V' : '—';
            log.textContent += 'Adapter: ' + (st.identity || '—') + '  ' + v + '  proto=' + (st.protocol || '—') + '\n';
          }
        } catch (e) {
          if (log) log.textContent += 'Status error: ' + e + '\n';
        }
      });
    }

    const monBtn = document.getElementById('btn-monitor');
    if (monBtn) {
      monBtn.addEventListener('click', async function () {
        try {
          if (log) log.textContent += 'Capturing bus (500 ms)…\n';
          const dump = await window.invokeCmd('st_capture_monitor', { ms: 500 });
          if (log) log.textContent += (dump || '(empty)') + '\n';
        } catch (e) {
          if (log) log.textContent += 'Monitor error: ' + e + '\n';
        }
      });
    }

    const sessBtn = document.getElementById('btn-elm-session');
    if (sessBtn) {
      sessBtn.addEventListener('click', async function () {
        try {
          const raw = await window.invokeCmd('elm_session_info');
          const st = typeof raw === 'string' ? JSON.parse(raw) : raw;
          const pids = (st && st.supported_pids) ? st.supported_pids.join(' ') : 'not read yet (read live data first)';
          if (log) log.textContent += 'Supported Mode 01 PIDs: ' + pids + '  NO DATA streak: ' + (st ? st.no_data_streak : '—') + '\n';
        } catch (e) {
          if (log) log.textContent += 'Session error: ' + e + '\n';
        }
      });
    }

    const nisBtn = document.getElementById('btn-nissan-scan');
    if (nisBtn) {
      nisBtn.addEventListener('click', async function () {
        try {
          if (log) log.textContent += 'Nissan CAN scan (read-only)…\n';
          const raw = await window.invokeCmd('nissan_can_scan', { readValues: true });
          const sc = typeof raw === 'string' ? JSON.parse(raw) : raw;
          if (log && sc) {
            log.textContent += 'Supported IDs (meaning unknown): ' + (sc.supported_ids || []).join(' ') + '\n';
            (sc.reads || []).forEach(function (r) {
              log.textContent += '  ' + r.id + ' = ' + (r.raw_hex != null ? r.raw_hex : ('error: ' + r.error)) + (r.via ? ' (' + r.via + ')' : '') + '  [meaning unknown]\n';
            });
            (sc.identity || []).forEach(function (p) { log.textContent += '  ' + p.id + ': ' + p.outcome + '\n'; });
            (sc.mask_errors || []).forEach(function (m) { log.textContent += '  mask: ' + m + '\n'; });
            log.textContent += 'Session exit: ' + sc.session_exit + '\n';
          }
        } catch (e) {
          if (log) log.textContent += 'Nissan scan error: ' + e + '\n';
        }
      });
    }
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', ready);
  } else {
    ready();
  }
})();
