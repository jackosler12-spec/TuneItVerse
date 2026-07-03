// Add near the end of init() or in a setup function

function setupIsoTpStatsToggle() {
  const toggle = document.getElementById('toggle-iso-stats');
  const panel = document.getElementById('iso-stats-panel');
  const resetBtn = document.getElementById('btn-reset-iso-stats');

  let statsInterval = null;

  if (toggle && panel) {
    toggle.addEventListener('change', async () => {
      if (toggle.checked) {
        panel.style.display = 'block';
        await updateIsoTpStats();
        if (!statsInterval) {
          statsInterval = setInterval(updateIsoTpStats, 1500); // live update every 1.5s
        }
      } else {
        panel.style.display = 'none';
        if (statsInterval) {
          clearInterval(statsInterval);
          statsInterval = null;
        }
      }
    });
  }

  if (resetBtn) {
    resetBtn.addEventListener('click', async () => {
      await invokeCmd('reset_iso_tp_statistics');
      await updateIsoTpStats();
    });
  }
}

async function updateIsoTpStats() {
  try {
    const json = await invokeCmd('get_iso_tp_statistics');
    const stats = JSON.parse(json);

    const set = (id, val) => {
      const el = document.getElementById(id);
      if (el) el.textContent = val;
    };

    set('stat-ff-sent', stats.ff_sent || 0);
    set('stat-cf-sent', stats.cf_sent || 0);
    set('stat-fc-rcv', stats.fc_received || 0);
    set('stat-bytes-sent', stats.bytes_sent || 0);
    set('stat-bytes-rcv', stats.bytes_received || 0);
    set('stat-errors', stats.errors || 0);

    const errEl = document.getElementById('stat-last-error');
    if (errEl) {
      errEl.textContent = stats.last_error ? `Last: ${stats.last_error}` : '';
    }
  } catch (e) {
    console.warn('Failed to fetch ISO-TP stats:', e);
  }
}

// Call in init()
// setupIsoTpStatsToggle();