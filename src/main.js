// Connection Status Indicator

let connectionStatusInterval = null;

function updateConnectionStatus() {
  const statusEl = document.getElementById('connection-status');
  if (!statusEl) return;

  invokeCmd('get_connection_state', {}).then(data => {
    try {
      const state = JSON.parse(data);
      if (state.connected) {
        let text = 'Connected';
        if (state.protocol) text += ` (${state.protocol})`;
        if (state.current_session) text += ` | Session 0x${state.current_session.toString(16)}`;
        if (state.security_unlocked) text += ' | Unlocked';
        statusEl.textContent = text;
        statusEl.style.color = '#0f0';
      } else {
        statusEl.textContent = 'Disconnected';
        statusEl.style.color = '#f66';
      }
    } catch {
      statusEl.textContent = 'Unknown';
      statusEl.style.color = '#fa0';
    }
  }).catch(() => {
    statusEl.textContent = 'Disconnected';
    statusEl.style.color = '#f66';
  });
}

function startConnectionStatusPolling() {
  if (connectionStatusInterval) clearInterval(connectionStatusInterval);
  updateConnectionStatus();
  connectionStatusInterval = setInterval(updateConnectionStatus, 2000); // Poll every 2s
}

// Auto-start polling when app loads
setTimeout(() => {
  startConnectionStatusPolling();
}, 1500);