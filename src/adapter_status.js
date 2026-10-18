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
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', ready);
  } else {
    ready();
  }
})();
