const state = {
  connected: false,
  pollInterval: null,
  chartData: { rpm: [], map: [], iat: [], afr: [], tps: [], ect: [] },
  maxPoints: 60,
  visibleCharts: new Set(),  // default empty until user selects sensors in Live Data view
  backupDone: false,
  binValidated: false,
  binCompatible: false,
  selectedFile: null,
  selectedFileBytes: null,   // Uint8Array — populated on file select
  identified: false,
};