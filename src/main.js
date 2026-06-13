  setSensor("vss", d.vss ?? 0);

  // CSV logging: capture normalized snapshot with timestamp
  if (state.isLogging) {
    state.sessionData.push({
      _ts: Date.now(),
      ...d, // includes all normalized fields
    });
    if (state.sessionData.length % 20 === 0) { // update status occasionally
      $("#log-status").textContent = `Session: recording... (${state.sessionData.length} samples)`;
    }
  }

  lastUpdate.textContent = `Updated ${new Date().toLocaleTimeString()}`;