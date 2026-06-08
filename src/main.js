const tauri = window.__TAURI__?.core ?? null;

async function invokeCmd(cmd, args = {}) {
  if (!tauri) return null;
  return tauri.invoke(cmd, args);
}

const state = {
  connected: false,
  pollInterval: null,
  chartData: { rpm: [], map: [], iat: [] },
  maxPoints: 60,
  activeChart: "rpm",
  backupDone: false,
  binValidated: false,
  binCompatible: false,
  selectedFile: null,
  selectedFileBytes: null,   // Uint8Array — populated on file select
  identified: false,
};

const $ = (s) => document.querySelector(s);

const connDot = $("#conn-dot");
const connLabel = $("#conn-label");
const btnConnect = $("#btn-connect");
const lastUpdate = $("#last-update");

const kpiRpm = $("#kpi-rpm");
const kpiTemp = $("#kpi-temp");
const kpiVolt = $("#kpi-volt");
const kpiDtc = $("#kpi-dtc");

const gaugeRpmCanvas = $("#gauge-rpm");
const gaugeMapCanvas = $("#gauge-map");
const gaugeIatCanvas = $("#gauge-iat");
const gaugeAfrCanvas = $("#gauge-afr");

const gaugeRpmVal = $("#gauge-rpm-val");
const gaugeMapVal = $("#gauge-map-val");
const gaugeIatVal = $("#gauge-iat-val");
const gaugeAfrVal = $("#gauge-afr-val");

const sensors = {
  tps: { bar: $("#bar-tps"), val: $("#val-tps"), max: 100, unit: "%" },
  o2: { bar: $("#bar-o2"), val: $("#val-o2"), max: 1.1, unit: "V" },
  stft: { bar: $("#bar-stft"), val: $("#val-stft"), max: 50, unit: "%", isTrim: true },
  ign: { bar: $("#bar-ign"), val: $("#val-ign"), max: 50, unit: "°BTDC" },
  inj: { bar: $("#bar-inj"), val: $("#val-inj"), max: 15, unit: "ms" },
  vss: { bar: $("#bar-vss"), val: $("#val-vss"), max: 200, unit: "km/h" },
};

function logJob(message) {
  const log = $("#job-log");
  const time = new Date().toLocaleTimeString();
  if (log) {
    log.textContent += `[${time}] ${message}\n`;
    log.scrollTop = log.scrollHeight;
  }
}

function setJobPhase(phase) {
  const el = $("#job-phase");
  if (el) el.textContent = phase;
}

function updateChecklist() {
  $("#chk-connected").checked = state.connected;
  $("#chk-identified").checked = state.identified;
  $("#chk-backup").checked = state.backupDone;
  $("#chk-bin").checked = state.binValidated;
  $("#chk-compat").checked = state.binCompatible;
}

function drawGauge(canvas, value, min, max, dangerZone = null, color = null) {
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  const W = canvas.width;
  const H = canvas.height;
  ctx.clearRect(0, 0, W, H);

  const cx = W / 2;
  const cy = H * 0.95;
  const r = Math.min(W, H * 2) * 0.42;
  const startAngle = Math.PI;
  const fraction = Math.min(Math.max((value - min) / (max - min), 0), 1);
  const valueAngle = startAngle + fraction * Math.PI;
  const isDark = document.documentElement.getAttribute("data-theme") !== "light";
  const baseColor = color || (isDark ? "#00c4b4" : "#008c80");

  ctx.beginPath();
  ctx.arc(cx, cy, r, Math.PI, 2 * Math.PI);
  ctx.strokeStyle = isDark ? "#1c2929" : "#d8e4e4";
  ctx.lineWidth = 10;
  ctx.lineCap = "round";
  ctx.stroke();

  if (dangerZone) {
    const dStart = startAngle + dangerZone.start * Math.PI;
    const dEnd = startAngle + dangerZone.end * Math.PI;
    ctx.beginPath();
    ctx.arc(cx, cy, r, dStart, dEnd);
    ctx.strokeStyle = "rgba(224,85,85,0.35)";
    ctx.lineWidth = 10;
    ctx.stroke();
  }

  ctx.beginPath();
  ctx.arc(cx, cy, r, startAngle, valueAngle);
  ctx.strokeStyle = baseColor;
  ctx.lineWidth = 10;
  ctx.lineCap = "round";
  ctx.stroke();

  const nx = cx + r * Math.cos(valueAngle);
  const ny = cy + r * Math.sin(valueAngle);
  ctx.beginPath();
  ctx.arc(nx, ny, 5, 0, Math.PI * 2);
  ctx.fillStyle = baseColor;
  ctx.fill();

  for (let i = 0; i <= 10; i++) {
    const a = startAngle + (i / 10) * Math.PI;
    const inner = r - 14;
    const outer = r - (i % 5 === 0 ? 6 : 10);
    ctx.beginPath();
    ctx.moveTo(cx + inner * Math.cos(a), cy + inner * Math.sin(a));
    ctx.lineTo(cx + outer * Math.cos(a), cy + outer * Math.sin(a));
    ctx.strokeStyle = isDark ? "#3a5050" : "#9bb6b6";
    ctx.lineWidth = i % 5 === 0 ? 2 : 1;
    ctx.stroke();
  }
}

function updateGauges(data) {
  drawGauge(gaugeRpmCanvas, data.rpm, 0, 7000, { start: 0.78, end: 1.0 });
  drawGauge(gaugeMapCanvas, data.map, 20, 105, null, "#6cb8e0");
  drawGauge(gaugeIatCanvas, data.iat, -10, 80, { start: 0.85, end: 1.0 }, "#e0a030");
  drawGauge(
    gaugeAfrCanvas,
    data.afr,
    10,
    18,
    { start: 0.0, end: 0.35 },
    data.afr < 13.5 ? "#e05555" : data.afr > 15.5 ? "#e0a030" : "#4ac990"
  );

  gaugeRpmVal.textContent = Math.round(data.rpm).toLocaleString();
  gaugeMapVal.textContent = Math.round(data.map);
  gaugeIatVal.textContent = Math.round(data.iat);
  gaugeAfrVal.textContent = data.afr.toFixed(1);
}

function drawLiveChart() {
  const canvas = $("#live-chart");
  if (!canvas) return;

  const ctx = canvas.getContext("2d");
  const W = canvas.offsetWidth || 600;
  const H = 160;
  canvas.width = W;
  canvas.height = H;

  const data = state.chartData[state.activeChart];
  const isDark = document.documentElement.getAttribute("data-theme") !== "light";

  ctx.clearRect(0, 0, W, H);

  ctx.strokeStyle = isDark ? "rgba(255,255,255,0.04)" : "rgba(0,0,0,0.05)";
  ctx.lineWidth = 1;
  for (let i = 0; i <= 4; i++) {
    const y = (i / 4) * H;
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(W, y);
    ctx.stroke();
  }

  if (data.length < 2) return;

  const ranges = { rpm: [0, 7000], map: [20, 105], iat: [-10, 80] };
  const colors = {
    rpm: isDark ? "#00c4b4" : "#008c80",
    map: "#6cb8e0",
    iat: "#e0a030",
  };

  const [minV, maxV] = ranges[state.activeChart];
  const range = maxV - minV;
  const stepX = W / Math.max(state.maxPoints - 1, 1);

  ctx.beginPath();
  data.forEach((v, i) => {
    const x = i * stepX;
    const y = H - ((v - minV) / range) * H * 0.85 - H * 0.05;
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.strokeStyle = colors[state.activeChart];
  ctx.lineWidth = 2;
  ctx.lineJoin = "round";
  ctx.lineCap = "round";
  ctx.stroke();
}

// Zeroed telemetry frame — shown when no live ECU data is available.
// TuneItVerse never displays synthetic/simulated engine values as if real.
function emptyEcuData() {
  return {
    rpm: 0, map: 0, iat: 0, ect: 0, afr: 14.7, batt_volt: 0,
    tps: 0, o2_b1s1: 0, stft_b1: 0, spark_adv: 0, inj_pw: 0,
    vss: 0, dtc_count: 0,
  };
}

// Normalize the Rust EcuTelemetry struct (snake_case, unit-suffixed fields)
// into the flat shape applyData/updateGauges expect.
function normalizeTelemetry(d) {
  return {
    rpm:       d.rpm ?? 0,
    map:       d.map_kpa ?? 0,
    iat:       d.iat_c ?? 0,
    ect:       d.ect_c ?? 0,
    afr:       d.target_afr ?? d.wb_afr ?? 14.7,
    batt_volt: d.batt_volt ?? 0,
    tps:       d.tps_pct ?? 0,
    o2_b1s1:   d.o2_left_up_v ?? 0,
    stft_b1:   d.stft_b1_pct ?? 0,
    spark_adv: d.spark_adv_deg ?? 0,
    inj_pw:    d.inj_pw_b1_ms ?? 0,
    vss:       d.vss_kph ?? 0,
    dtc_count: d.dtc_count ?? 0,
  };
}

function applyData(d) {
  const temp = d.ect ?? 0;
  const volt = d.batt_volt ?? 0;
  const stft = d.stft_b1 ?? 0;
  const ign = d.spark_adv ?? 0;
  const inj = d.inj_pw ?? 0;
  const o2 = d.o2_b1s1 ?? 0;
  const dtcCount = d.dtc_count ?? 0;

  kpiRpm.textContent = Math.round(d.rpm).toLocaleString();
  kpiTemp.textContent = Math.round(temp);
  kpiVolt.textContent = volt.toFixed(1);
  kpiDtc.textContent = String(dtcCount);

  $("#kpi-dtc-card")?.classList.toggle("kpi-card--alert", dtcCount > 0);
  $("#dtc-count-badge").textContent = String(dtcCount);

  updateGauges({
    rpm: d.rpm ?? 0,
    map: d.map ?? 0,
    iat: d.iat ?? 0,
    afr: d.afr ?? 14.7,
  });

  const key = state.activeChart;
  if (typeof d[key] === "number") {
    state.chartData[key].push(d[key]);
    if (state.chartData[key].length > state.maxPoints) state.chartData[key].shift();
  }
  drawLiveChart();

  const setSensor = (name, value) => {
    const s = sensors[name];
    if (!s?.bar || !s?.val) return;
    let pct;
    if (s.isTrim) {
      const clamped = Math.max(-s.max, Math.min(s.max, value));
      pct = 50 + (clamped / s.max) * 50;
    } else {
      pct = Math.min(Math.max((value / s.max) * 100, 0), 100);
    }
    s.bar.style.width = `${pct}%`;
    s.val.textContent = Number.isInteger(value) ? String(value) : value.toFixed(1);
  };

  setSensor("tps", d.tps ?? 0);
  setSensor("o2", o2);
  setSensor("stft", stft);
  setSensor("ign", ign);
  setSensor("inj", inj);
  setSensor("vss", d.vss ?? 0);

  lastUpdate.textContent = `Updated ${new Date().toLocaleTimeString()}`;
}

async function pollEcuData() {
  if (!state.connected) { applyData(emptyEcuData()); return; }
  try {
    const data = await invokeCmd("read_ecu_data");
    if (data) { applyData(normalizeTelemetry(data)); return; }
  } catch (err) {
    logJob(`Telemetry read error: ${err}`);
  }
  // Connected but no valid frame — show zeros, never synthetic values.
  applyData(emptyEcuData());
}

async function connectEcu() {
  if (state.connected) {
    clearInterval(state.pollInterval);
    state.pollInterval = null;
    try { await invokeCmd("disconnect_ecu"); } catch (_) {}
    state.connected = false;
    state.identified = false;
    connDot.classList.remove("connected");
    connLabel.textContent = "Disconnected";
    btnConnect.textContent = "Connect ECU";
    btnConnect.classList.remove("connected");
    lastUpdate.textContent = "Disconnected";
    $("#vehicle-osid-chip") && ($("#vehicle-osid-chip").textContent = "No ECU identified");
    $("#page-sub") && ($("#page-sub").textContent = "No vehicle loaded");
    applyData(emptyEcuData());
    updateChecklist();
    logJob("Disconnected from ECU.");
    return;
  }

  try {
    const ports = await invokeCmd("list_serial_ports");
    if (!ports || ports.length === 0) { alert("No serial ports found."); return; }

    const portList = ports.map((p, i) => `${i + 1}: ${p.port_name} (${p.port_type})`).join("\n");
    const selection = prompt(`Select serial port:\n${portList}\n\nEnter port number:`);
    if (!selection) return;

    const index = Number(selection) - 1;
    if (Number.isNaN(index) || index < 0 || index >= ports.length) { alert("Invalid port selection."); return; }

    const selectedPort = ports[index].port_name;
    const baudInput = prompt("Enter baud rate:", "115200");
    if (!baudInput) return;

    const baud = Number(baudInput);
    if (Number.isNaN(baud)) { alert("Invalid baud rate."); return; }

    await invokeCmd("connect_ecu", { port: selectedPort, baud });
    state.connected = true;
    connDot.classList.add("connected");
    connLabel.textContent = "ECU Connected";
    btnConnect.textContent = "Disconnect";
    btnConnect.classList.add("connected");
    lastUpdate.textContent = `Connected to ${selectedPort}`;
    updateChecklist();
    logJob(`Connected to ${selectedPort} at ${baud} baud.`);
    state.pollInterval = setInterval(pollEcuData, 250);
    await pollEcuData();
  } catch (err) {
    alert(`Connect failed: ${err}`);
    logJob(`Connect failed: ${err}`);
  }
}

function switchView(viewName) {
  document.querySelectorAll(".content").forEach((el) => el.classList.add("content--hidden"));
  document.querySelectorAll(".nav-item").forEach((el) => el.classList.remove("active"));
  const target = document.getElementById(`view-${viewName}`);
  if (target) target.classList.remove("content--hidden");
  const navItem = document.querySelector(`[data-view="${viewName}"]`);
  if (navItem) navItem.classList.add("active");

  const titles = {
    dashboard: ["ECU Diagnostics", "2001 LS1 Nissan Patrol GU"],
    "read-write": ["Read / Write Pipeline", "Identify, backup, validate, write, verify"],
    "live-data": ["Live Data Stream", "Real-time parameter feed"],
    dtc: ["DTC Fault Codes", "Diagnostic trouble codes"],
    calibration: ["ECU Calibration", "Definition-backed editing comes next"],
    logs: ["Session Logs", "Backups, write jobs, and audit history"],
  };

  const [title, sub] = titles[viewName] || ["TuneItVerse", ""];
  $("#page-title").textContent = title;
  $("#page-sub").textContent = sub;
}

function initTheme() {
  const html = document.documentElement;
  const btn = document.querySelector("[data-theme-toggle]");
  let theme = "dark";
  html.setAttribute("data-theme", theme);
  btn?.addEventListener("click", () => {
    theme = theme === "dark" ? "light" : "dark";
    html.setAttribute("data-theme", theme);
    drawLiveChart();
    updateGauges({ rpm: 0, map: 0, iat: 0, afr: 14.7 });
  });
}

function initSidebar() {
  const sidebar = $("#sidebar");
  $("#sidebar-toggle")?.addEventListener("click", () => sidebar.classList.toggle("collapsed"));
}

function initChartControls() {
  document.querySelectorAll("[data-chart]").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll("[data-chart]").forEach((b) => b.classList.remove("chip--active"));
      btn.classList.add("chip--active");
      state.activeChart = btn.dataset.chart;
      state.chartData[state.activeChart] = [];
      drawLiveChart();
    });
  });
}

// ─── DTC subsystem — backed by real Tauri commands (read_dtcs_cmd / clear_dtcs_cmd) ───

function dtcItemHtml(rec) {
  const sev = rec.is_permanent ? "sev--high" : rec.is_pending ? "sev--med" : "sev--low";
  const sevLabel = rec.is_permanent ? "Permanent" : rec.is_pending ? "Pending" : "Stored";
  const desc = rec.description || "Unknown code";
  return `<li class="dtc-item dtc-item--active">
      <span class="dtc-code">${rec.code}</span>
      <div><span class="dtc-desc">${desc}</span><span class="dtc-meta">${sevLabel}</span></div>
      <span class="dtc-sev ${sev}">${sevLabel}</span>
    </li>`;
}

function renderDtcList(ulId, records, emptyText) {
  const ul = document.getElementById(ulId);
  if (!ul) return;
  if (!records || records.length === 0) {
    ul.innerHTML = `<li class="dtc-item dtc-empty"><span class="dtc-desc">${emptyText}</span></li>`;
    return;
  }
  ul.innerHTML = records.map(dtcItemHtml).join("");
}

function applyDtcResult(result) {
  const stored = result?.stored ?? [];
  const pending = result?.pending ?? [];
  const permanent = result?.permanent ?? [];
  const total = result?.total ?? stored.length + pending.length + permanent.length;

  renderDtcList("dtc-list-stored", stored, "No stored DTCs");
  renderDtcList("dtc-list-pending", pending, "No pending DTCs");
  renderDtcList("dtc-list-permanent", permanent, "No permanent DTCs");

  $("#dtc-stored-count") && ($("#dtc-stored-count").textContent = String(stored.length));
  $("#dtc-pending-count") && ($("#dtc-pending-count").textContent = String(pending.length));
  $("#dtc-permanent-count") && ($("#dtc-permanent-count").textContent = String(permanent.length));
  $("#dtc-summary") && ($("#dtc-summary").textContent = `${total} code(s) read from PCM`);

  // Dashboard mirror
  const dash = stored.concat(pending, permanent);
  renderDtcList("dash-dtc-list", dash, "No DTCs — connect and read the ECU");

  $("#kpi-dtc") && ($("#kpi-dtc").textContent = String(total));
  $("#dtc-count-badge") && ($("#dtc-count-badge").textContent = String(total));
  $("#kpi-dtc-card")?.classList.toggle("kpi-card--alert", total > 0);
}

async function refreshDtcs() {
  if (!state.connected) { alert("Connect to the ECU first."); return; }
  $("#dtc-summary") && ($("#dtc-summary").textContent = "Reading…");
  logJob("Reading DTCs from PCM…");
  try {
    const result = await invokeCmd("read_dtcs_cmd");
    if (!result) throw new Error("No DTC data returned.");
    applyDtcResult(result);
    logJob(`DTC read complete: ${result.total} code(s).`);
  } catch (err) {
    $("#dtc-summary") && ($("#dtc-summary").textContent = "Read failed");
    logJob(`DTC read failed: ${err}`);
    alert(`DTC read failed: ${err}`);
  }
}

function showDtcClearBanner(message, ok) {
  const banner = $("#dtc-clear-banner");
  const msg = $("#dtc-clear-msg");
  if (!banner || !msg) return;
  msg.textContent = message;
  banner.classList.remove("dtc-clear-banner--hidden");
  banner.style.borderColor = ok ? "" : "var(--danger, #e05555)";
  setTimeout(() => banner.classList.add("dtc-clear-banner--hidden"), 6000);
}

async function clearDtcs() {
  if (!state.connected) { alert("Connect to the ECU first."); return; }
  if (!confirm("Clear all stored and pending DTCs? Permanent codes are not erasable.")) return;
  logJob("Clearing DTCs (Mode 04)…");
  try {
    const result = await invokeCmd("clear_dtcs_cmd");
    if (!result) throw new Error("No clear result returned.");
    showDtcClearBanner(result.message || "DTCs cleared.", !!result.success);
    logJob(result.message || `Cleared ${result.cleared_count} DTC(s).`);
    await refreshDtcs();
  } catch (err) {
    showDtcClearBanner(`Clear failed: ${err}`, false);
    logJob(`DTC clear failed: ${err}`);
  }
}

function initDtcView() {
  $("#btn-refresh-dtc")?.addEventListener("click", refreshDtcs);
  $("#btn-refresh-dtc-dashboard")?.addEventListener("click", refreshDtcs);
  $("#btn-clear-dtc")?.addEventListener("click", clearDtcs);
}

function initNav() {
  document.querySelectorAll(".nav-item").forEach((item) => {
    item.addEventListener("click", (e) => {
      e.preventDefault();
      switchView(item.dataset.view);
    });
  });
}

async function readProperties() {
  if (!state.connected) { alert("Connect to the ECU first."); return; }
  setJobPhase("Identifying");
  logJob("Reading ECU properties...");
  try {
    const result = await invokeCmd("read_properties");
    if (!result) throw new Error("No property data returned.");
    $("#rw-osid").textContent = result.os_id || "Unknown";
    $("#rw-vin").textContent = result.vin || "Unknown";
    $("#rw-hardware").textContent = result.hardware || "Unknown";
    $("#rw-status").textContent = result.status || "Identified";
    $("#rw-pcm-type").textContent = result.ecu_type || "P01 / 0411";
    $("#rw-protocol").textContent = result.protocol || "GM J1850 VPW";
    const osid = result.os_id || "Unknown";
    $("#vehicle-osid-chip") && ($("#vehicle-osid-chip").textContent = `${result.ecu_type || "P01"} / OS ${osid}`);
    $("#page-sub") && ($("#page-sub").textContent = `OSID ${osid}`);
    state.identified = true;
    updateChecklist();
    logJob(`ECU identified. OSID=${result.os_id}, VIN=${result.vin}`);
    setJobPhase("Idle");
  } catch (err) {
    setJobPhase("Failed");
    logJob(`Read properties failed: ${err}`);
    alert(`Read properties failed: ${err}`);
  }
}

async function readEntirePcm() {
  if (!state.connected || !state.identified) { alert("Connect and identify the ECU first."); return; }
  setJobPhase("Reading");
  logJob("Starting full PCM backup...");
  try {
    const result = await invokeCmd("read_entire_pcm");
    if (!result) throw new Error("No backup result returned.");
    $("#backup-file").textContent = result.file_name || "backup.bin";
    $("#backup-size").textContent = result.size_bytes ? `${result.size_bytes} bytes` : "Unknown";
    $("#backup-hash").textContent = result.sha256 || "Unavailable";
    $("#backup-required").textContent = "Completed";
    state.backupDone = true;
    updateChecklist();
    logJob(`Backup complete: ${result.file_name}`);
    setJobPhase("Idle");
  } catch (err) {
    setJobPhase("Failed");
    logJob(`Backup failed: ${err}`);
    alert(`Backup failed: ${err}`);
  }
}

// ─── BIN file selection: read bytes immediately so they're ready for Tauri ───
function readFileBytes(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = (e) => resolve(new Uint8Array(e.target.result));
    reader.onerror = () => reject(new Error("FileReader error"));
    reader.readAsArrayBuffer(file);
  });
}

function initBinFile() {
  $("#bin-file")?.addEventListener("change", async (e) => {
    const file = e.target.files?.[0] || null;
    state.selectedFile = file;
    state.selectedFileBytes = null;
    state.binValidated = false;
    state.binCompatible = false;
    $("#bin-name").textContent = file ? file.name : "None";
    $("#bin-osid").textContent = "Unknown";
    $("#bin-checksum").textContent = "Not validated";
    $("#bin-compat").textContent = "Unchecked";
    updateChecklist();

    if (file) {
      try {
        state.selectedFileBytes = await readFileBytes(file);
        logJob(`Selected BIN: ${file.name} (${state.selectedFileBytes.length} bytes loaded into memory)`);
      } catch (err) {
        logJob(`Failed to read BIN bytes: ${err}`);
      }
    } else {
      logJob("BIN selection cleared.");
    }
  });
}

async function validateBin() {
  if (!state.selectedFileBytes) { alert("Select a BIN file first."); return; }
  setJobPhase("Preflight");
  logJob(`Validating BIN ${state.selectedFile.name}...`);
  try {
    // Pass raw bytes — Rust: validate_bin(file_bytes: Vec<u8>)
    const result = await invokeCmd("validate_bin", {
      fileBytes: Array.from(state.selectedFileBytes),
    });
    if (!result) throw new Error("No validation result returned.");
    $("#bin-osid").textContent = result.detected_os_id || "Unknown";
    $("#bin-checksum").textContent = result.checksum_ok ? "OK" : "Failed";
    $("#bin-compat").textContent = result.compatibility || "Unknown";
    state.binValidated = !!result.checksum_ok;
    state.binCompatible = !!result.compatible;
    updateChecklist();
    logJob(`BIN validated. OSID=${result.detected_os_id}, checksum=${result.checksum_ok}, compat=${result.compatibility}`);
    setJobPhase("Idle");
  } catch (err) {
    setJobPhase("Failed");
    logJob(`BIN validation failed: ${err}`);
    alert(`BIN validation failed: ${err}`);
  }
}

async function compareBinToEcu() {
  if (!state.selectedFileBytes) { alert("Select a BIN file first."); return; }
  if (!state.connected || !state.identified) { alert("Connect and identify the ECU first."); return; }
  setJobPhase("Compare");
  logJob("Comparing selected BIN to ECU...");
  try {
    // Pass raw bytes — Rust: compare_bin_to_ecu(file_bytes: Vec<u8>)
    const result = await invokeCmd("compare_bin_to_ecu", {
      fileBytes: Array.from(state.selectedFileBytes),
    });
    if (!result) throw new Error("No compare result returned.");
    $("#bin-compat").textContent = result.compatibility || "Unknown";
    state.binCompatible = !!result.compatible;
    updateChecklist();
    logJob(`Compare complete: ${result.summary}`);
    setJobPhase("Idle");
  } catch (err) {
    setJobPhase("Failed");
    logJob(`Compare failed: ${err}`);
    alert(`Compare failed: ${err}`);
  }
}

async function startWrite() {
  const confirmUser = $("#chk-user")?.checked;
  const mode = document.querySelector('input[name="write-mode"]:checked')?.value || "calibration_only";

  if (!state.connected || !state.identified || !state.backupDone || !state.binValidated || !state.binCompatible || !confirmUser) {
    alert("Preflight checklist not complete.");
    return;
  }
  if (!state.selectedFileBytes) { alert("BIN file bytes not loaded."); return; }

  const CAL_SIZE = 131072; // 128 KiB calibration image
  if (state.selectedFileBytes.length !== CAL_SIZE) {
    alert(`Both write modes flash the 128 KiB calibration region. Loaded file is ${state.selectedFileBytes.length} bytes.`);
    return;
  }

  const proceed = confirm(`Start ${mode} write? Do not interrupt vehicle power.`);
  if (!proceed) return;

  setJobPhase("Writing");
  logJob(`Starting write job: ${mode}`);

  // Single conversion reused for the write call.
  const bytesArg = Array.from(state.selectedFileBytes);

  try {
    let result;
    if (mode === "calibration_only") {
      result = await invokeCmd("write_calibration_cmd", { fileBytes: bytesArg });
    } else {
      result = await invokeCmd("write_os_calibration", { fileBytes: bytesArg });
    }

    logJob(result?.message || "Write completed.");
    setJobPhase("Verifying");

    const verify = await invokeCmd("verify_after_write");
    logJob(verify?.message || "Verification complete.");
    setJobPhase("Completed");
  } catch (err) {
    setJobPhase("Failed");
    logJob(`Write failed: ${err}`);
    alert(`Write failed: ${err}`);
  }
}

function initReadWriteActions() {
  $("#btn-read-properties")?.addEventListener("click", readProperties);
  $("#btn-read-entire")?.addEventListener("click", readEntirePcm);
  $("#btn-validate-bin")?.addEventListener("click", validateBin);
  $("#btn-compare-bin")?.addEventListener("click", compareBinToEcu);
  $("#btn-start-write")?.addEventListener("click", startWrite);
}

window.addEventListener("DOMContentLoaded", () => {
  initTheme();
  initSidebar();
  initChartControls();
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
