const state = {
  connected: false,
  pollInterval: null,
  chartData: { rpm: [], map: [], iat: [], afr: [], tps: [], ect: [] },
  maxPoints: 60,
  visibleCharts: new Set(),
  chartOrder: [], // ordered list for drawing and drag-reorder
  backupDone: false,
  binValidated: false,
  binCompatible: false,
  selectedFile: null,
  selectedFileBytes: null,
  identified: false,
  // CSV Logging
  isLogging: false,
  sessionData: [],
  logStartTime: null,
};