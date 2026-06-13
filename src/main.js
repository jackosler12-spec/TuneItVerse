  updateChecklist();
  logJob("TuneItVerse ready.");

  // Auto-open Connect & Backup wizard as the default starting window
  setTimeout(() => {
    const modal = $("#connect-modal");
    if (modal && !state.connected) {
      modal.classList.remove("hidden");
      // Optionally populate ports here if needed
    }
  }, 800);

  drawGauge(gaugeRpmCanvas, 0, 0, 7000, { start: 0.78, end: 1.0 });
  drawGauge(gaugeMapCanvas, 20, 20, 105, null, "#6cb8e0");
  drawGauge(gaugeIatCanvas, 0, -10, 80, { start: 0.85, end: 1.0 }, "#e0a030");
  drawGauge(gaugeAfrCanvas, 14.7, 10, 18, { start: 0.0, end: 0.35 }, "#4ac990");
  drawLiveChart();