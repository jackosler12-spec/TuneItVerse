window.addEventListener("DOMContentLoaded", () => {
  initTheme();
  initSidebar();
  initChartControls();
  initLiveDataControls();
  initDtcView();
  initNav();
  initBinFile();
  initReadWriteActions();
  btnConnect?.addEventListener("click", connectEcu);

  updateChecklist();
  logJob("TuneItVerse ready.");

  drawGauge(gaugeRpmCanvas, 0, 0, 7000, { start: 0.78, end: 1.0 });
  drawGauge(gaugeMapCanvas, 20, 20, 105, null, "#6cb8e0");
  drawGauge(gaugeIatCanvas, 0, -10, 80, { start: 0.85, end: 1.0 }, "#e0a030");
  drawGauge(gaugeAfrCanvas, 14.7, 10, 18, { start: 0.0, end: 0.35 }, "#4ac990");
  drawLiveChart();
});