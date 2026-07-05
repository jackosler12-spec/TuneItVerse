// main.js — Enhanced J2534 frontend integration

// ... existing state, invokeCmd, mock, helpers ...

// ==================== IMPROVED CONNECT MODAL + J2534 ====================

function setupConnect() {
  const btn = $('#btn-connect');
  const modal = $('#connect-modal');
  const connectBtn = $('#btn-modal-connect');
  const cancelBtn = $('#btn-modal-cancel');

  if (btn) btn.addEventListener('click', () => modal?.classList.remove('hidden'));
  if (cancelBtn) cancelBtn.addEventListener('click', () => modal?.classList.add('hidden'));

  if (connectBtn) {
    connectBtn.addEventListener('click', async () => {
      await applyAdvancedCanSettings();

      const hwType = document.querySelector('input[name="hw-type"]:checked')?.value || 'elm';
      const dllPathInput = $('#j2534-dll-path');
      const dllPath = dllPathInput ? dllPathInput.value.trim() : null;

      try {
        let result;
        if (hwType === 'j2534') {
          result = await invokeCmd('j2534_connect_cmd', { dll_path: dllPath || null });
        } else {
          result = await invokeCmd('connect_elm', { port: 'COM3' });
        }

        // Handle both object and string responses
        const success = result && (result.success || typeof result === 'string' || result.includes('connected'));
        if (success) {
          state.connected = true;
          state.protocol = hwType;
          state.detectedOsid = result.osid || '12225074';
          showToast('Connected successfully via ' + hwType.toUpperCase(), 'success');
          modal?.classList.add('hidden');
          updateConnectionUI();
          if (typeof triggerDynamicParsingOnLoad === 'function') triggerDynamicParsingOnLoad();
        }
      } catch (e) {
        showToast('Connection failed: ' + e, 'error');
      }
    });
  }

  setupJ2534UI();
  setupIsoTpStatsToggle();
}

function setupJ2534UI() {
  // Show DLL path input only when J2534 is selected
  const radios = document.querySelectorAll('input[name="hw-type"]');
  const dllGroup = $('#j2534-dll-group'); // assume a wrapper div in HTML

  radios.forEach(radio => {
    radio.addEventListener('change', () => {
      if (dllGroup) {
        dllGroup.style.display = (radio.value === 'j2534') ? 'block' : 'none';
      }
    });
  });
}

// Quick test helpers exposed for development
window.j2534TestWrite = async function(data = [0x22, 0xF1, 0x90]) {
  return await invokeCmd('j2534_write_uds', { data, timeout_ms: 1500 });
};

window.j2534TestRead = async function() {
  return await invokeCmd('j2534_read_msgs', { timeout_ms: 800, max_msgs: 8 });
};

window.j2534Disconnect = async function() {
  return await invokeCmd('j2534_disconnect', {});
};

window.j2534Reconnect = async function() {
  return await invokeCmd('j2534_reconnect', {});
};

// Expose everything for console debugging
window.TuneItVerse = { 
  state, invokeCmd, switchView, startRealLoggingLoop, runGuidedPipeline,
  j2534TestWrite, j2534TestRead, j2534Disconnect, j2534Reconnect 
};