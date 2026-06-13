function startSessionLog() {
  if (!state.connected) {
    alert("Connect to the ECU first to start logging.");
    return;
  }
  state.isLogging = true;
  state.sessionData = [];
  state.logStartTime = Date.now();
  $("#btn-start-log").textContent = "Stop Logging";
  $("#btn-start-log").classList.add("btn-danger");
  $("#btn-download-log").disabled = true;
  $("#log-status").textContent = "Session: recording...";
  logJob("Session logging started.");
}

function stopSessionLog() {
  state.isLogging = false;
  $("#btn-start-log").textContent = "Start Session Log";
  $("#btn-start-log").classList.remove("btn-danger");
  $("#btn-download-log").disabled = state.sessionData.length === 0;
  $("#log-status").textContent = `Session: stopped (${state.sessionData.length} samples)`;
  logJob(`Session logging stopped. ${state.sessionData.length} samples recorded.`);
}

function downloadCSVLog() {
  if (state.sessionData.length === 0) {
    alert("No data logged yet.");
    return;
  }

  const headers = ["timestamp", "rpm", "map_kpa", "iat_c", "afr", "tps_pct", "ect_c", "o2_left_up_v", "stft_b1_pct", "spark_adv_deg", "inj_pw_b1_ms", "vss_kph", "dtc_count"];
  let csv = headers.join(",") + "\n";

  state.sessionData.forEach((row) => {
    const ts = new Date(row._ts).toISOString();
    const line = [
      ts,
      row.rpm ?? "",
      row.map ?? "",
      row.iat ?? "",
      row.afr ?? "",
      row.tps ?? "",
      row.ect ?? "",
      row.o2_b1s1 ?? "",
      row.stft_b1 ?? "",
      row.spark_adv ?? "",
      row.inj_pw ?? "",
      row.vss ?? "",
      row.dtc_count ?? ""
    ].join(",");
    csv += line + "\n";
  });

  const blob = new Blob([csv], { type: "text/csv;charset=utf-8;" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  const ts = new Date(state.logStartTime || Date.now()).toISOString().slice(0,19).replace(/[:T]/g, "-");
  a.download = `tuneitverse_log_${ts}.csv`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);

  logJob(`CSV log downloaded: ${state.sessionData.length} samples.`);
}

// Wire logging buttons (called from initLiveDataControls)
function initLogging() {
  const startBtn = $("#btn-start-log");
  const downloadBtn = $("#btn-download-log");

  startBtn?.addEventListener("click", () => {
    if (state.isLogging) {
      stopSessionLog();
    } else {
      startSessionLog();
    }
  });

  downloadBtn?.addEventListener("click", downloadCSVLog);
}