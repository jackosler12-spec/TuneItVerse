const state = {
  connected: false,
  pollInterval: null,
  chartData: { rpm: [], map: [], iat: [], afr: [] },
  maxPoints: 60,
  visibleCharts: new Set(["rpm", "map"]),  // multi-line support: toggle which traces are shown
  backupDone: false,
  binValidated: false,
  binCompatible: false,
  selectedFile: null,
  selectedFileBytes: null,   // Uint8Array — populated on file select
  identified: false,
};