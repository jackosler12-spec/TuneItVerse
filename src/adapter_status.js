// Standalone wiring for the Adapter Status button.
// Include after main.js so invokeCmd / parseMaybe are available.

(function () {
  function ready() {
    const btn = document.getElementById('btn-adapter-status');
    if (!btn) return;
    btn.addEventListener('click', async function () {
      const log = document.getElementById('connect-log');
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
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', ready);
  } else {
    ready();
  }
})();
