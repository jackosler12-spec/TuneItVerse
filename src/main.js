// Global disconnect helpers

window.disconnectAll = async function() {
  try {
    const result = await invokeCmd('disconnect_all', {});
    showToast(result, 'info');
    // Optionally reload or update UI state
  } catch (e) {
    showToast('Disconnect failed: ' + e, 'error');
  }
};

window.resetConnectionState = async function() {
  try {
    const result = await invokeCmd('reset_connection_state', {});
    showToast(result, 'info');
  } catch (e) {
    showToast('Reset failed: ' + e, 'error');
  }
};