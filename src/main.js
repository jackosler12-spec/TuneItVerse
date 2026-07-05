// main.js — Professional-grade J2534 UI integration + Diagnostics Panel

// ... existing code ...

// ==================== PROFESSIONAL J2534 DIAGNOSTICS PANEL ====================

function createJ2534DiagnosticsPanel() {
  const container = document.createElement('div');
  container.id = 'j2534-diagnostics';
  container.style.cssText = 'margin-top: 20px; padding: 16px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface);';

  container.innerHTML = `
    <h3 style="margin:0 0 12px 0; font-size:15px;">🔧 J2534 Diagnostics</h3>
    
    <div style="display:flex; gap:8px; margin-bottom:12px; flex-wrap:wrap;">
      <button id="btn-j2534-status" class="btn">Check Status</button>
      <button id="btn-j2534-reconnect" class="btn">Reconnect</button>
      <button id="btn-j2534-disconnect" class="btn btn-danger">Disconnect</button>
    </div>

    <div style="display:grid; grid-template-columns:1fr 1fr; gap:12px; margin-bottom:12px;">
      <div>
        <label>Send UDS (hex, space separated)</label>
        <input id="j2534-send-data" type="text" value="22 F1 90" style="width:100%; padding:6px;" />
        <button id="btn-j2534-send" class="btn" style="margin-top:6px; width:100%;">Send UDS</button>
      </div>
      <div>
        <label>Read Messages</label>
        <button id="btn-j2534-read" class="btn" style="width:100%;">Read (last 5 msgs)</button>
        <pre id="j2534-read-output" style="margin-top:6px; background:#111; padding:8px; font-size:11px; max-height:120px; overflow:auto;"></pre>
      </div>
    </div>

    <div style="font-size:11px; color:var(--text-faint);">
      Status: <span id="j2534-status">Not connected</span>
    </div>
  `;

  // Append to Flash view or a tools section if exists, otherwise to body for now
  const flashView = document.getElementById('view-flash');
  if (flashView) {
    flashView.appendChild(container);
  } else {
    document.body.appendChild(container);
  }

  // Bind buttons
  bindJ2534Buttons(container);
}

function bindJ2534Buttons(panel) {
  const statusEl = panel.querySelector('#j2534-status');

  panel.querySelector('#btn-j2534-status').onclick = async () => {
    const connected = state.connected && state.protocol === 'j2534';
    statusEl.textContent = connected ? 'Connected (J2534)' : 'Not connected';
    statusEl.style.color = connected ? '#0f0' : '#f66';
  };

  panel.querySelector('#btn-j2534-reconnect').onclick = async () => {
    try {
      const res = await invokeCmd('j2534_reconnect', {});
      showToast(res, 'success');
    } catch (e) { showToast('Reconnect failed: ' + e, 'error'); }
  };

  panel.querySelector('#btn-j2534-disconnect').onclick = async () => {
    try {
      const res = await invokeCmd('j2534_disconnect', {});
      state.connected = false;
      statusEl.textContent = 'Disconnected';
      showToast(res, 'info');
    } catch (e) { showToast(e, 'error'); }
  };

  panel.querySelector('#btn-j2534-send').onclick = async () => {
    const input = panel.querySelector('#j2534-send-data').value.trim();
    const bytes = input.split(/\s+/).map(h => parseInt(h, 16)).filter(n => !isNaN(n));
    if (bytes.length === 0) return showToast('Invalid hex data', 'error');

    try {
      const res = await invokeCmd('j2534_write_uds', { data: bytes });
      showToast(res, 'success');
    } catch (e) { showToast('Send failed: ' + e, 'error'); }
  };

  panel.querySelector('#btn-j2534-read').onclick = async () => {
    const output = panel.querySelector('#j2534-read-output');
    try {
      const msgs = await invokeCmd('j2534_read_msgs', { timeout_ms: 800, max_msgs: 5 });
      output.textContent = Array.isArray(msgs) ? msgs.join('\n') : JSON.stringify(msgs, null, 2);
    } catch (e) {
      output.textContent = 'Error: ' + e;
    }
  };
}

// Auto-create diagnostics panel on Flash view load
setTimeout(() => {
  if (document.getElementById('view-flash') && !document.getElementById('j2534-diagnostics')) {
    createJ2534DiagnosticsPanel();
  }
}, 1200);

// Also expose in Connect modal for quick access
// (existing setupConnect already improved)