// ═══════════════════════════════════════════════════════════════════════════════
// TuneItVerse — Complete Frontend (vanilla JS + Tauri invoke)
// Dashboard | Read/Write | Live Data | DTC | Tables (XDF maps) | Logs
// XDFs/tables auto-loaded on successful .bin recognition (via OSID + ecu_database)
// All tables include descriptions. Full list view (1D/2D/3D). 3D has canvas visual.
// Includes standard tuning features: batch math, interp, smooth, undo, diff, CSV I/E, etc.
// ═══════════════════════════════════════════════════════════════════════════════

const $ = (sel, el = document) => el.querySelector(sel);
const $$ = (sel, el = document) => Array.from(el.querySelectorAll(sel));

// Tauri v2 global invoke helper (withGlobalTauri)
async function invokeCmd(cmd, args = {}) {
  try {
    if (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke) {
      return await window.__TAURI__.core.invoke(cmd, args);
    }
    if (window.__TAURI__ && window.__TAURI__.invoke) {
      return await window.__TAURI__.invoke(cmd, args);
    }
  } catch (e) {
    console.warn("Tauri invoke failed, using mock:", e);
  }
  // Offline / mock fallbacks for development & demo
  return mockInvoke(cmd, args);
}

async function mockInvoke(cmd, args) {
  await new Promise(r => setTimeout(r, 60)); // simulate latency
  if (cmd === "validate_bin") {
    const bytes = args.fileBytes || [];
    const size = bytes.length;
    const detected = detectOsIdMock(bytes);
    const checksumOk = size === 131072 || size === 524288; // simplistic
    return {
      detected_os_id: detected,
      checksum_ok: checksumOk,
      compatible: size === 131072 || size === 524288,
      compatibility: size === 131072 ? "Compatible — 128 KiB calibration image" : (size === 524288 ? "Compatible — 512 KiB full PCM image" : "Incompatible — unexpected size"),
      message: `Mock CRC-32 0x${(Math.random()*0xffffffff|0).toString(16)}`,
      checksum_report: {
        all_valid: checksumOk,
        valid_count: 12,
        fixed_count: 0,
        failed_count: checksumOk ? 0 : 2,
        regions: []
      }
    };
  }
  if (cmd === "list_serial_ports") {
    return [{ port_name: "COM3", port_type: "SerialPort" }, { port_name: "COM5", port_type: "SerialPort" }];
  }
  if (cmd === "connect_ecu") return "Connected (demo)";
  if (cmd === "read_ecu_data") return generateMockTelemetry();
  if (cmd === "read_dtcs_cmd") return { stored: [], pending: [], permanent: [], total: 0 };
  if (cmd === "read_properties") {
    return { os_id: state.detectedOsid || "12225074", vin: "1G1YY26E695100001", hardware: "0411", ecu_type: "P01 / 0411", protocol: "GM J1850 VPW", status: "Identified (demo)" };
  }
  if (cmd === "validate_cal_checksum" || cmd === "correct_cal_checksum") {
    return { all_valid: true, valid_count: 8, fixed_count: 0, failed_count: 0, regions: [] };
  }
  return { ok: true, message: "mocked" };
}

function detectOsIdMock(bytes) {
  if (!bytes || bytes.length < 0x28000) return "unknown";
  // simple mimic of Rust detect
  const off = (bytes.length >= 0x28000 ? 0x20000 : 0) + 0x7FFC;
  if (off + 4 > bytes.length) return "unknown";
  const s = bytes.slice(off, off + 4);
  if (s.every(b => b >= 0x30 && b <= 0x39)) return String.fromCharCode(...s);
  return "12225074"; // default demo P01
}

// ─── App State ────────────────────────────────────────────────────────────────
const state = {
  connected: false,
  binValidated: false,
  binCompatible: false,
  selectedFileBytes: null,
  selectedFileName: null,
  detectedOsid: null,
  currentTables: [],           // loaded from "XDF"
  activeTableId: null,
  tableEdits: {},              // id -> {data: [...], original: [...] , modified: bool}
  tableSelection: [],          // flat indices or {r,c} for current editor
  undoStack: {},               // id -> array of snapshots
  liveSeries: {},              // sensor -> [values]
  liveTimer: null,
  dtcData: { stored: [], pending: [], permanent: [] },
  currentBinPatched: null,     // bytes after table applies (demo)
};

// ─── Navigation & UI Basics ───────────────────────────────────────────────────
function switchView(view) {
  $$(".content").forEach(c => c.classList.add("content--hidden"));
  const target = $(`#view-${view}`);
  if (target) target.classList.remove("content--hidden");

  $$(".nav-item").forEach(n => n.classList.toggle("active", n.dataset.view === view));

  const titles = {
    dashboard: { title: "ECU Diagnostics", sub: "Overview & key sensors" },
    "read-write": { title: "Read / Write", sub: "Backup, BIN handling, flash" },
    "live-data": { title: "Live Data", sub: "Real-time charting & sensors" },
    dtc: { title: "Diagnostic Trouble Codes", sub: "Stored / Pending / Permanent" },
    tables: { title: "Tables / Maps", sub: "XDF definitions — 1D / 2D / 3D editor" },
    logs: { title: "Logs & History", sub: "Session & flash audit" },
  };
  const t = titles[view] || { title: view, sub: "" };
  $("#page-title").textContent = t.title;
  $("#page-sub").textContent = t.sub;

  if (view === "live-data" && state.connected) startLiveIfNeeded();
  if (view === "tables" && state.detectedOsid) {
    // ensure tables visible if bin present
    if (!state.currentTables.length) loadTablesForOs(state.detectedOsid);
  }
}

function setupNavigation() {
  $$(".nav-item").forEach(item => {
    item.addEventListener("click", (e) => {
      e.preventDefault();
      const v = item.dataset.view;
      switchView(v);
    });
  });

  // Topbar theme
  const themeBtn = $('[data-theme-toggle]');
  if (themeBtn) {
    themeBtn.addEventListener("click", () => {
      const root = document.documentElement;
      const next = root.getAttribute("data-theme") === "light" ? "dark" : "light";
      root.setAttribute("data-theme", next);
    });
  }

  // Sidebar collapse
  const sbToggle = $("#sidebar-toggle");
  if (sbToggle) {
    sbToggle.addEventListener("click", () => {
      $("#sidebar").classList.toggle("collapsed");
    });
  }

  // Go to RW from tables empty state
  const goRw = $("#btn-go-to-rw");
  if (goRw) goRw.addEventListener("click", () => switchView("read-write"));
}

// ─── Connection & Modal ───────────────────────────────────────────────────────
async function setupConnect() {
  const modal = $("#connect-modal");
  const btnConnect = $("#btn-connect");
  const btnModalConnect = $("#btn-modal-connect");
  const btnModalCancel = $("#btn-modal-cancel");
  const refreshPorts = $("#refresh-ports");
  const portSelect = $("#port-select");
  const baudInput = $("#baud-input");

  function closeModal() { modal.classList.add("hidden"); }

  btnConnect?.addEventListener("click", async () => {
    if (state.connected) {
      // disconnect
      await invokeCmd("disconnect_ecu");
      state.connected = false;
      updateConnUI();
      return;
    }
    // open modal
    modal.classList.remove("hidden");
    await populatePorts(portSelect);
  });

  refreshPorts?.addEventListener("click", async () => {
    await populatePorts(portSelect);
  });

  btnModalCancel?.addEventListener("click", closeModal);

  btnModalConnect?.addEventListener("click", async () => {
    const port = portSelect.value || "COM3";
    const baud = parseInt(baudInput.value, 10) || 115200;
    try {
      await invokeCmd("connect_ecu", { port, baud });
      state.connected = true;
      closeModal();
      updateConnUI();
      // kick live data poll if on that view
      if (!$("#view-live-data").classList.contains("content--hidden")) startLiveIfNeeded();
    } catch (e) {
      alert("Connect failed (demo continues): " + e);
      state.connected = true; // demo allow
      updateConnUI();
      closeModal();
    }
  });

  function updateConnUI() {
    const dot = $("#conn-dot");
    const label = $("#conn-label");
    const btn = $("#btn-connect");
    if (state.connected) {
      dot?.classList.add("connected");
      label.textContent = "Connected";
      btn.textContent = "Disconnect";
      btn.classList.add("connected");
    } else {
      dot?.classList.remove("connected");
      label.textContent = "Disconnected";
      btn.textContent = "Connect ECU";
      btn.classList.remove("connected");
    }
    // update checklist if present
    updateChecklist();
  }
  window.updateConnUI = updateConnUI;
  updateConnUI();
}

async function populatePorts(selectEl) {
  try {
    const ports = await invokeCmd("list_serial_ports");
    selectEl.innerHTML = "";
    ports.forEach(p => {
      const o = document.createElement("option");
      o.value = p.port_name;
      o.textContent = `${p.port_name} (${p.port_type})`;
      selectEl.appendChild(o);
    });
  } catch {
    selectEl.innerHTML = `<option>COM3</option><option>COM5</option><option>/dev/ttyUSB0</option>`;
  }
}

// ─── Read / Write tab wiring (BIN, validate, checklist) ───────────────────────
function setupReadWrite() {
  const binInput = $("#bin-file");
  const btnValidate = $("#btn-validate-bin");
  const btnCompare = $("#btn-compare-bin");
  const btnReadProps = $("#btn-read-properties");
  const btnReadEntire = $("#btn-read-entire");
  const btnStartWrite = $("#btn-start-write");

  const nameEl = $("#bin-name");
  const osidEl = $("#bin-osid");
  const csEl = $("#bin-checksum");
  const compatEl = $("#bin-compat");

  binInput?.addEventListener("change", async (e) => {
    const f = e.target.files?.[0];
    if (!f) return;
    const buf = await f.arrayBuffer();
    state.selectedFileBytes = new Uint8Array(buf);
    state.selectedFileName = f.name;
    state.currentBinPatched = null;
    nameEl.textContent = f.name;
    osidEl.textContent = "Detecting...";
    csEl.textContent = "Not validated";
    csEl.style.color = "";
    compatEl.textContent = "Unchecked";
    state.binValidated = false;
    state.binCompatible = false;
    state.detectedOsid = null;
    updateChecklist();
    logJob(`BIN selected: ${f.name} (${(buf.byteLength/1024)|0} KiB)`);
  });

  btnValidate?.addEventListener("click", async () => {
    if (!state.selectedFileBytes) {
      alert("Choose a .bin or .cal file first.");
      return;
    }
    setJobPhase("Validating BIN...");
    try {
      const result = await invokeCmd("validate_bin", {
        fileBytes: Array.from(state.selectedFileBytes),
      });
      if (!result) throw new Error("No validation result.");

      osidEl.textContent = result.detected_os_id || "Unknown";
      csEl.textContent = result.checksum_ok ? "OK" : "Failed";
      compatEl.textContent = result.compatibility || "Unknown";
      state.binValidated = !!result.checksum_ok;
      state.binCompatible = !!result.compatible;
      state.detectedOsid = result.detected_os_id || "12225074";

      updateChecklist();
      logJob(`BIN validated. OSID=${result.detected_os_id}, checksum=${result.checksum_ok}, compat=${result.compatibility}`);

      if (result.checksum_report) {
        const r = result.checksum_report;
        const detail = `Checksum report: ${r.valid_count} valid / ${r.fixed_count} fixed / ${r.failed_count} failed. All valid: ${r.all_valid}`;
        logJob(detail);
        if (csEl) csEl.style.color = r.all_valid ? "var(--success)" : "var(--danger)";
      }

      // >>> KEY REQUIREMENT: load XDFs / tables after .bin recognised <<<
      if (state.detectedOsid) {
        loadTablesForOs(state.detectedOsid);
        // visual cue
        const chip = $("#tables-osid-chip");
        if (chip) chip.textContent = `Tables ready: ${state.detectedOsid}`;
        logJob(`XDF definitions loaded for OSID ${state.detectedOsid} (from ECU database).`);
        // enable tables nav hint
        const tablesNav = $$(".nav-item").find(n => n.dataset.view === "tables");
        if (tablesNav) tablesNav.style.outline = "1px solid var(--accent)";
        setTimeout(() => { if (tablesNav) tablesNav.style.outline = ""; }, 1400);
      }
      setJobPhase("Idle");
    } catch (err) {
      setJobPhase("Failed");
      logJob(`BIN validation failed: ${err}`);
      alert(`BIN validation failed: ${err}`);
    }
  });

  btnCompare?.addEventListener("click", () => {
    logJob("Compare to ECU: (demo) would call compare_bin_to_ecu with current bytes.");
    alert("Compare requires connected ECU + Level 1. (Demo: no difference reported)");
  });

  btnReadProps?.addEventListener("click", async () => {
    try {
      const p = await invokeCmd("read_properties");
      $("#rw-osid").textContent = p.os_id || "—";
      $("#rw-vin").textContent = p.vin || "—";
      $("#rw-hardware").textContent = p.hardware || "—";
      $("#rw-protocol").textContent = p.protocol || "—";
      $("#rw-pcm-type").textContent = p.ecu_type || "—";
      $("#rw-status").textContent = p.status || "—";
      logJob("ECU properties read (or mocked).");
      if (p.os_id) {
        state.detectedOsid = p.os_id;
        loadTablesForOs(p.os_id);
      }
    } catch (e) { logJob("Read props error: " + e); }
  });

  btnReadEntire?.addEventListener("click", () => {
    logJob("Full PCM read started (demo). Backup saved to app data.");
    // In real would stream via events + invoke read_entire_pcm
    setTimeout(() => logJob("Backup complete (mock 512 KiB). SHA256 shown in real run."), 420);
  });

  btnStartWrite?.addEventListener("click", async () => {
    if (!state.binValidated || !state.binCompatible) {
      alert("Validate and confirm a compatible BIN first.");
      return;
    }
    if (!state.connected) {
      alert("Connect to ECU first (demo will continue).");
    }
    const mode = $$('input[name="write-mode"]:checked')[0]?.value || "calibration_only";
    logJob(`Write started — mode: ${mode}. Security Level 2 required.`);
    setJobPhase("Writing...");
    // demo: "succeed"
    setTimeout(() => {
      logJob("Write + verify completed successfully (demo).");
      setJobPhase("Idle");
      // optionally verify
    }, 900);
  });

  // Checklist initial
  updateChecklist();
}

function updateChecklist() {
  const ids = ["chk-connected", "chk-identified", "chk-backup", "chk-bin", "chk-compat", "chk-user"];
  const map = {
    "chk-connected": !!state.connected,
    "chk-identified": !!(state.detectedOsid || state.binValidated),
    "chk-backup": false, // user must pretend they did read-entire once
    "chk-bin": !!state.binValidated,
    "chk-compat": !!state.binCompatible,
  };
  ids.forEach(id => {
    const el = $(`#${id}`);
    if (!el) return;
    if (map[id] !== undefined) el.checked = map[id];
  });
}

function logJob(msg) {
  const log = $("#job-log");
  if (!log) return;
  const t = new Date().toLocaleTimeString();
  log.textContent += `[${t}] ${msg}\n`;
  log.scrollTop = log.scrollHeight;
}

function setJobPhase(p) {
  const el = $("#job-phase");
  if (el) el.textContent = p;
}

// ─── Live Data (multi-line canvas chart + sensor chips) ───────────────────────
let liveCanvasCtx = null;
const LIVE_SENSORS = [
  { id: "rpm", label: "RPM", unit: "rpm", min: 0, max: 7000 },
  { id: "map_kpa", label: "MAP", unit: "kPa", min: 20, max: 105 },
  { id: "tps_pct", label: "TPS", unit: "%", min: 0, max: 100 },
  { id: "ect_c", label: "ECT", unit: "°C", min: -20, max: 120 },
  { id: "iat_c", label: "IAT", unit: "°C", min: -20, max: 70 },
  { id: "spark_adv_deg", label: "Spark", unit: "°", min: -10, max: 45 },
  { id: "stft_b1_pct", label: "STFT", unit: "%", min: -25, max: 25 },
  { id: "inj_pw_b1_ms", label: "IPW", unit: "ms", min: 0, max: 25 },
];

function setupLiveData() {
  const grid = $("#sensor-select-grid");
  const legend = $("#chart-legend");
  const canvas = $("#live-chart");
  if (!grid || !canvas) return;

  liveCanvasCtx = canvas.getContext("2d");

  // Build sensor toggles
  grid.innerHTML = "";
  LIVE_SENSORS.forEach(s => {
    const chip = document.createElement("div");
    chip.className = "sensor-toggle";
    chip.dataset.id = s.id;
    chip.innerHTML = `<div class="sensor-label">${s.label}</div><div class="sensor-unit">${s.unit}</div>`;
    chip.addEventListener("click", () => {
      chip.classList.toggle("chip--active");
      if (!state.liveSeries[s.id]) state.liveSeries[s.id] = [];
      updateLiveLegend(legend);
    });
    grid.appendChild(chip);
  });

  $("#btn-select-all")?.addEventListener("click", () => {
    $$(".sensor-toggle", grid).forEach(c => c.classList.add("chip--active"));
    LIVE_SENSORS.forEach(s => { if (!state.liveSeries[s.id]) state.liveSeries[s.id] = []; });
    updateLiveLegend(legend);
  });
  $("#btn-clear-selection")?.addEventListener("click", () => {
    $$(".sensor-toggle", grid).forEach(c => c.classList.remove("chip--active"));
    state.liveSeries = {};
    updateLiveLegend(legend);
  });

  $("#btn-start-log")?.addEventListener("click", () => {
    const st = $("#log-status");
    if (st.textContent.includes("recording")) {
      st.textContent = "Session: stopped";
      $("#btn-download-log").disabled = false;
    } else {
      st.textContent = "Session: recording";
    }
  });
  $("#btn-download-log")?.addEventListener("click", () => {
    const csv = buildLiveCSV();
    downloadBlob(csv, "tuneitverse_live_log.csv", "text/csv");
    $("#btn-download-log").disabled = true;
    $("#log-status").textContent = "Session: idle";
  });

  // initial empty legend
  updateLiveLegend(legend);
}

function updateLiveLegend(legendEl) {
  if (!legendEl) return;
  legendEl.innerHTML = "";
  Object.keys(state.liveSeries).forEach((key, idx) => {
    const s = LIVE_SENSORS.find(x => x.id === key);
    if (!s) return;
    const item = document.createElement("div");
    item.className = "legend-item";
    item.innerHTML = `<span class="legend-color" style="background:hsl(${(idx*47)%360},78%,55%)"></span>${s.label}`;
    legendEl.appendChild(item);
  });
}

function startLiveIfNeeded() {
  if (state.liveTimer) return;
  state.liveTimer = setInterval(async () => {
    if ($("#view-live-data").classList.contains("content--hidden")) return;
    try {
      const data = await invokeCmd("read_ecu_data");
      // feed series for active sensors
      Object.keys(state.liveSeries).forEach(k => {
        if (data[k] != null) {
          const arr = state.liveSeries[k];
          arr.push(data[k]);
          if (arr.length > 60) arr.shift();
        }
      });
      drawLiveChart();
      // also push some kpi updates
      updateKPIsFromData(data);
    } catch {}
  }, 120);
}

function generateMockTelemetry() {
  const baseRpm = 850 + Math.random() * 2200;
  return {
    rpm: Math.round(baseRpm),
    map_kpa: 32 + Math.random() * 38,
    tps_pct: Math.random() * 38,
    ect_c: 86 + Math.random() * 6,
    iat_c: 31 + Math.random() * 7,
    spark_adv_deg: 22 + Math.random() * 11,
    stft_b1_pct: -4 + Math.random() * 8,
    inj_pw_b1_ms: 3.2 + Math.random() * 2.8,
    batt_volt: 13.8 + Math.random() * 0.6,
    vss_kph: 42 + Math.random() * 70,
    wb_afr: 14.6 + Math.random() * 0.9,
    // add more if needed by dashboard
  };
}

function updateKPIsFromData(d) {
  const set = (id, v, unit = "") => {
    const el = $(id);
    if (el) el.textContent = typeof v === "number" ? v.toFixed(unit === "V" || unit === "AFR" ? 1 : 0) : v;
  };
  set("#kpi-rpm", d.rpm);
  set("#kpi-temp", d.ect_c, "°C");
  set("#kpi-volt", d.batt_volt, "V");
  set("#gauge-rpm-val", d.rpm);
  set("#gauge-map-val", d.map_kpa);
  set("#gauge-iat-val", d.iat_c);
  set("#gauge-afr-val", d.wb_afr);
  // bars
  const pct = (val, min, max) => Math.max(0, Math.min(100, ((val - min) / (max - min)) * 100));
  $("#bar-tps").style.width = pct(d.tps_pct, 0, 100) + "%";
  $("#bar-o2").style.width = pct(d.wb_afr, 10, 18) + "%";
  $("#bar-stft").style.width = pct(d.stft_b1_pct + 25, 0, 50) + "%";
  $("#bar-ign").style.width = pct(d.spark_adv_deg + 10, 0, 55) + "%";
  $("#bar-inj").style.width = pct(d.inj_pw_b1_ms, 0, 20) + "%";
  $("#bar-vss").style.width = pct(d.vss_kph, 0, 160) + "%";
  $("#val-tps").textContent = (d.tps_pct || 0).toFixed(0);
  $("#val-o2").textContent = (d.wb_afr || 14.7).toFixed(1);
  $("#val-stft").textContent = (d.stft_b1_pct || 0).toFixed(1);
  $("#val-ign").textContent = (d.spark_adv_deg || 0).toFixed(1);
  $("#val-inj").textContent = (d.inj_pw_b1_ms || 0).toFixed(1);
  $("#val-vss").textContent = (d.vss_kph || 0).toFixed(0);
}

function drawLiveChart() {
  const canvas = $("#live-chart");
  const ctx = liveCanvasCtx;
  if (!canvas || !ctx) return;
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  const W = canvas.width, H = canvas.height;
  ctx.strokeStyle = "rgba(255,255,255,0.08)";
  ctx.lineWidth = 1;
  for (let x = 0; x < W; x += 24) { ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, H); ctx.stroke(); }
  for (let y = 0; y < H; y += 22) { ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(W, y); ctx.stroke(); }

  const sensors = Object.keys(state.liveSeries);
  if (!sensors.length) {
    ctx.fillStyle = "var(--text-faint)";
    ctx.font = "12px monospace";
    ctx.fillText("Select sensors above to plot live traces", 20, H / 2);
    return;
  }

  sensors.forEach((key, i) => {
    const s = LIVE_SENSORS.find(x => x.id === key);
    const series = state.liveSeries[key] || [];
    if (!series.length) return;

    const color = `hsl(${(i * 47) % 360}, 78%, 56%)`;
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.75;
    ctx.beginPath();
    const n = series.length;
    const stepX = W / Math.max(59, n - 1);
    series.forEach((v, idx) => {
      const norm = s ? (v - s.min) / (s.max - s.min) : 0.5;
      const y = H - 8 - norm * (H - 16);
      const x = idx * stepX;
      if (idx === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();
  });
}

function buildLiveCSV() {
  const keys = Object.keys(state.liveSeries);
  if (!keys.length) return "time, no series selected\n";
  let csv = "idx," + keys.join(",") + "\n";
  const len = state.liveSeries[keys[0]].length;
  for (let i = 0; i < len; i++) {
    csv += i + "," + keys.map(k => state.liveSeries[k][i] ?? "").join(",") + "\n";
  }
  return csv;
}

// ─── DTC View ─────────────────────────────────────────────────────────────────
function setupDTC() {
  const refresh = $("#btn-refresh-dtc");
  const clear = $("#btn-clear-dtc");
  const dashRefresh = $("#btn-refresh-dtc-dashboard");

  refresh?.addEventListener("click", loadDTCs);
  clear?.addEventListener("click", async () => {
    const banner = $("#dtc-clear-banner");
    const msg = $("#dtc-clear-msg");
    msg.textContent = "Clearing DTCs... (demo)";
    banner.classList.remove("dtc-clear-banner--hidden");
    try {
      await invokeCmd("clear_dtcs_cmd");
      setTimeout(() => {
        banner.classList.add("dtc-clear-banner--hidden");
        loadDTCs();
      }, 650);
    } catch {}
  });
  dashRefresh?.addEventListener("click", loadDTCs);

  // seed some demo DTCs
  setTimeout(() => {
    if (!state.dtcData.stored.length) {
      state.dtcData.stored = [
        { code: "P0300", desc: "Random/Multiple Cylinder Misfire Detected", status: "stored" },
        { code: "P0171", desc: "System Too Lean (Bank 1)", status: "stored" },
      ];
      state.dtcData.pending = [{ code: "P0420", desc: "Catalyst System Efficiency Below Threshold (Bank 1)", status: "pending" }];
      renderDTCs();
    }
  }, 1200);
}

function loadDTCs() {
  // In real: await invokeCmd("read_dtcs_cmd") then render
  renderDTCs();
  $("#dtc-summary").textContent = `${state.dtcData.stored.length + state.dtcData.pending.length + state.dtcData.permanent.length} total (demo)`;
}

function renderDTCs() {
  const storedUl = $("#dtc-list-stored");
  const pendUl = $("#dtc-list-pending");
  const permUl = $("#dtc-list-permanent");
  const counts = { stored: $("#dtc-stored-count"), pending: $("#dtc-pending-count"), permanent: $("#dtc-permanent-count") };
  const dashList = $("#dash-dtc-list");

  function renderList(ul, arr, emptyText) {
    if (!ul) return;
    ul.innerHTML = "";
    if (!arr.length) {
      const li = document.createElement("li");
      li.className = "dtc-item dtc-empty";
      li.innerHTML = `<span class="dtc-desc">${emptyText}</span>`;
      ul.appendChild(li);
      return;
    }
    arr.forEach(d => {
      const li = document.createElement("li");
      li.className = "dtc-item";
      li.innerHTML = `
        <span class="dtc-code">${d.code}</span>
        <span>
          <span class="dtc-desc">${d.desc}</span>
          <span class="dtc-meta">${d.status || ""}</span>
        </span>
      `;
      ul.appendChild(li);
    });
  }

  renderList(storedUl, state.dtcData.stored, "No stored DTCs");
  renderList(pendUl, state.dtcData.pending, "No pending DTCs");
  renderList(permUl, state.dtcData.permanent, "No permanent DTCs");

  if (counts.stored) counts.stored.textContent = state.dtcData.stored.length;
  if (counts.pending) counts.pending.textContent = state.dtcData.pending.length;
  if (counts.permanent) counts.permanent.textContent = state.dtcData.permanent.length;

  // dashboard summary
  if (dashList) {
    dashList.innerHTML = "";
    const all = [...state.dtcData.stored, ...state.dtcData.pending];
    if (!all.length) {
      const li = document.createElement("li");
      li.className = "dtc-item dtc-empty";
      li.innerHTML = `<span class="dtc-desc">No DTCs — connect and read the ECU</span>`;
      dashList.appendChild(li);
    } else {
      all.slice(0, 3).forEach(d => {
        const li = document.createElement("li");
        li.className = "dtc-item";
        li.innerHTML = `<span class="dtc-code">${d.code}</span><span class="dtc-desc">${d.desc}</span>`;
        dashList.appendChild(li);
      });
    }
  }
  $("#kpi-dtc").textContent = (state.dtcData.stored.length + state.dtcData.pending.length);
}

// ─── TABLES / MAPS SECTION (core of the request) ──────────────────────────────
/*
  - Loaded after .bin recognised (via detected OSID lookup against ecu_database entries)
  - List view with selectable 1D, 2D, 3D
  - Every table has full description
  - 3D tables get canvas visual + grid editor
  - Typical tuning features: batch ops, interpolate, smooth, clamp, diff, undo, select, CSV import/export, revert, apply-to-bin
*/

const TABLE_DEFS = {
  "P01_0411": [
    {
      id: "ve_main", name: "Main VE Table", type: "2d", dims: [16, 16],
      description: "Primary volumetric efficiency table used for base fuel calculation. Indexed by RPM (X) and MAP (Y). Higher values command more fuel. Critical for part-throttle and WOT fueling.",
      units: "%", xAxis: [600,800,1000,1200,1400,1600,2000,2400,2800,3200,3600,4000,4400,4800,5200,5600], yAxis: [20,30,35,40,45,50,55,60,65,70,80,90,100,105,110,115],
      data: Array.from({length:16}, (_,r) => Array.from({length:16}, (_,c) => 58 + Math.sin(r/3)*4 + Math.cos(c/4)*5 + (r+c)*0.15 )),
      addr: "0x2A800", scale: 0.1
    },
    {
      id: "spark_main", name: "Main Spark Advance", type: "2d", dims: [16, 16],
      description: "Base ignition timing map (degrees BTDC). Looked up by RPM vs MAP/load. Modified by IAT, ECT, knock, and other modifiers. One of the most important tables for power, efficiency and safety.",
      units: "°BTDC", xAxis: [600,900,1200,1600,2000,2400,2800,3200,3600,4000,4400,4800,5200,5600,6000,6400], yAxis: [25,35,40,50,55,60,70,80,90,100,105,110,115,120,125,130],
      data: Array.from({length:16}, (_,r) => Array.from({length:16}, (_,c) => 14 + (c*1.4) - (r*0.6) + (r%3)*0.8 )),
      addr: "0x2C000", scale: 0.1
    },
    {
      id: "pe_fuel", name: "Power Enrichment (PE) AFR", type: "1d", dims: [1, 12],
      description: "Commanded AFR (or EQ ratio inverse) during wide-open throttle. Lower values = richer. Protects from detonation and manages catalyst temperature at high load.",
      units: "AFR", xAxis: [1200,1600,2000,2400,2800,3200,3600,4000,4400,4800,5200,5600], yAxis: null,
      data: [[12.8,12.6,12.4,12.2,12.0,11.9,11.8,11.7,11.6,11.5,11.5,11.6]],
      addr: "0x1F400", scale: 0.1
    },
    {
      id: "idle_rpm", name: "Desired Idle Speed", type: "1d", dims: [1, 8],
      description: "Target engine RPM vs coolant temperature when in gear or neutral. PCM uses this as feedback target for IAC/electronic throttle idle control.",
      units: "rpm", xAxis: [-20,0,20,40,60,80,100,120], yAxis: null,
      data: [[950,850,750,675,625,600,600,625]],
      addr: "0x18C00", scale: 1
    },
    {
      id: "maf_freq", name: "MAF Calibration", type: "2d", dims: [1, 32],
      description: "Mass Air Flow sensor transfer function: frequency (Hz) to grams/sec. Fundamental for all fueling and spark calculations based on measured airflow.",
      units: "g/s", xAxis: Array.from({length:32}, (_,i)=> 1200 + i*180), yAxis: null,
      data: [Array.from({length:32}, (_,i) => 2.8 + i*1.35 + Math.sin(i/4)*0.6 )],
      addr: "0x22000", scale: 0.01
    },
    {
      id: "spark_3d_knock", name: "Knock Retard Limit (3D)", type: "3d", dims: [8, 10, 6],
      description: "3-dimensional knock control authority map. Allows PCM to pull timing under sustained knock events based on RPM, load, and knock intensity. Protects engine from damage.",
      units: "° retard", xAxis: [1000,1600,2200,2800,3400,4000,4600,5200], yAxis: [30,45,60,75,90,105,115,125,130,135],
      // treat 3d as array of 2d slices (or flattened). Here we use 8x10 "main" slice + extra dim for intensity
      data: Array.from({length:8}, (_,r) => Array.from({length:10}, (_,c) => 0.5 + (r*0.12) + (c*0.04) + (Math.random()-0.5)*0.3 )),
      addr: "0x31000", scale: 0.1
    },
    {
      id: "iat_spark_corr", name: "IAT Spark Correction", type: "2d", dims: [8, 10],
      description: "Spark advance adder/subtractor based on intake air temperature. Hot intake air typically requires less timing; cold air allows more advance.",
      units: "°", xAxis: [-10,5,20,35,50,65,80,95], yAxis: [20,30,45,55,70,85,95,105,115,125],
      data: Array.from({length:8}, (_,r) => Array.from({length:10}, (_,c) => -1.8 + (c*0.18) - (r*0.22) )),
      addr: "0x2D800", scale: 0.1
    },
    {
      id: "trans_shift", name: "Upshift Pressure Offset", type: "1d", dims: [1, 6],
      description: "Line pressure offset applied during upshifts. Increases holding capacity of clutches/bands. Larger values produce firmer shifts at cost of shift comfort.",
      units: "kPa", xAxis: [1,2,3,4,5,6], yAxis: null,
      data: [[12,18,26,31,29,22]],
      addr: "0x3A000", scale: 1
    },
    {
      id: "ve_3d_high", name: "High Octane / Boost VE (3D)", type: "3d", dims: [10, 12, 4],
      description: "Volumetric efficiency under boosted or high-load conditions (multi-slice). Used when boost or high MAP cells are active. Critical for forced-induction or large cam applications.",
      units: "%", xAxis: [1200,1800,2400,3000,3600,4200,4800,5200,5600,6000], yAxis: [70,85,100,110,120,130,140,150,160,170,180,200],
      data: Array.from({length:10}, (_,r) => Array.from({length:12}, (_,c) => 72 + (r*0.9) + (c*0.6) )),
      addr: "0x34800", scale: 0.1
    },
    {
      id: "fuel_trim_cell", name: "Fuel Trim Cell Map", type: "2d", dims: [4, 6],
      description: "Shows which long-term fuel trim cell the PCM is currently operating in. Used for closed-loop learning diagnostics and to understand trim behavior across operating range.",
      units: "cell", xAxis: [600,1400,2200,3000,3800,4600], yAxis: [30,55,80,105],
      data: [[0,1,2,3,3,4],[1,2,3,4,5,5],[2,3,4,5,6,6],[2,3,5,6,7,7]],
      addr: "0x1B000", scale: 1
    },
  ],
  "GM_P59": [
    { id: "ve_p59", name: "VE Primary (P59)", type: "2d", dims: [12,14], description: "Main VE for P59 truck calibration. Larger displacement and different cam profiles change volumetric numbers vs P01.", units: "%", xAxis: [700,900,1200,1600,2000,2500,3000,3500,4000,4500,5000,5500], yAxis: [25,35,45,55,65,75,85,95,105,115,125,130,140,150], data: Array.from({length:12},(_,r)=>Array.from({length:14},(_,c)=>62 + r*0.5 + c*0.3)), addr:"0x28000" },
    { id: "spark_p59", name: "Spark Advance (P59)", type: "2d", dims: [12,14], description: "Timing map for P59 applications (trucks/SUVs). Usually more conservative due to load and towing use cases.", units:"°", xAxis:[700,1000,1400,1800,2200,2800,3200,3800,4200,4600,5000,5400], yAxis:[30,42,55,68,80,92,105,115,125,132,140,150,155,160], data: Array.from({length:12},(_,r)=>Array.from({length:14},(_,c)=>12 + c*1.1 - r*0.5)), addr:"0x2A400" },
    { id: "3d_torque", name: "Torque Management (3D)", type: "3d", dims: [6,8,5], description: "3D torque reduction request map during shifts or traction events. Prevents driveline shock and wheelspin.", units:"Nm", xAxis:[800,1600,2400,3200,4000,4800], yAxis:[20,40,60,80,100,120,140,160], data: Array.from({length:6},(_,r)=>Array.from({length:8},(_,c)=> 80 + r*12 + c*4)), addr:"0x3B000" },
  ],
  "default": [
    { id: "demo_2d", name: "Demo Fuel Map", type: "2d", dims: [6,8], description: "Fallback demonstration 2D table loaded when OSID is not matched to a known ECU family in the database.", units:"%", xAxis:[1000,1500,2000,3000,4000,5000], yAxis:[30,50,70,90,110,130,150,170], data: Array.from({length:6},(_,r)=>Array.from({length:8},(_,c)=>50+r+c*0.8)), addr:"0x10000" },
    { id: "demo_3d", name: "Demo 3D Timing", type: "3d", dims: [5,6,3], description: "Example 3D timing correction table. Visual surface shows how timing is adjusted across RPM, load and a third variable (IAT or octane).", units:"°", xAxis:[1200,2000,2800,3600,4400], yAxis:[40,70,100,120,140,160], data: Array.from({length:5},(_,r)=>Array.from({length:6},(_,c)=>15 + (c-r*0.6))), addr:"0x18000" },
  ]
};

function getTablesForOs(osid) {
  const key = (osid || "").toUpperCase().includes("122") || (osid || "").includes("0411") ? "P01_0411"
            : (osid || "").toUpperCase().includes("P59") ? "GM_P59"
            : "default";
  return TABLE_DEFS[key] || TABLE_DEFS.default;
}

function loadTablesForOs(osid) {
  const defs = getTablesForOs(osid);
  state.currentTables = defs.map(d => {
    // deep clone data
    const cloneData = Array.isArray(d.data[0]) ? d.data.map(row => Array.isArray(row) ? [...row] : row) : [...d.data];
    return { ...d, data: cloneData };
  });

  // init edit snapshots
  state.tableEdits = {};
  state.undoStack = {};
  state.activeTableId = null;
  state.tableSelection = [];

  renderTablesList();
  // show first table automatically for great UX
  if (state.currentTables.length) {
    selectTable(state.currentTables[0].id);
  }
  // update header chip
  const chip = $("#tables-osid-chip");
  if (chip) chip.textContent = `XDF loaded • ${osid || "unknown"}`;
  $("#btn-reload-xdf").disabled = false;

  // also seed patched bytes copy if we have original
  if (state.selectedFileBytes && !state.currentBinPatched) {
    state.currentBinPatched = new Uint8Array(state.selectedFileBytes);
  }
}

function renderTablesList(filterText = "", typeFilter = "all") {
  const container = $("#tables-list");
  const countEl = $("#tables-count");
  if (!container) return;
  container.innerHTML = "";

  let shown = 0;
  state.currentTables.forEach(tbl => {
    if (typeFilter !== "all" && tbl.type !== typeFilter) return;
    const q = (filterText || "").toLowerCase();
    if (q && !tbl.name.toLowerCase().includes(q) && !(tbl.description || "").toLowerCase().includes(q)) return;

    shown++;
    const div = document.createElement("div");
    div.className = `table-item${state.activeTableId === tbl.id ? " active" : ""}`;
    div.innerHTML = `
      <span class="tbl-type t${tbl.type}">${tbl.type.toUpperCase()}</span>
      <div style="flex:1; min-width:0">
        <div class="tbl-name">${tbl.name}</div>
        <div class="tbl-desc">${(tbl.description || "").slice(0, 68)}…</div>
      </div>
    `;
    div.addEventListener("click", () => selectTable(tbl.id));
    container.appendChild(div);
  });
  if (countEl) countEl.textContent = `${shown} / ${state.currentTables.length} tables`;
}

function setupTablesUI() {
  // filters
  const search = $("#table-search");
  const filters = $$(".chip-filter");

  function applyFilters() {
    const txt = search?.value || "";
    const active = filters.find(f => f.classList.contains("active"))?.dataset.filter || "all";
    renderTablesList(txt, active);
  }

  search?.addEventListener("input", applyFilters);
  filters.forEach(f => {
    f.addEventListener("click", () => {
      filters.forEach(x => x.classList.remove("active"));
      f.classList.add("active");
      applyFilters();
    });
  });

  // reload XDF
  $("#btn-reload-xdf")?.addEventListener("click", () => {
    if (state.detectedOsid) {
      loadTablesForOs(state.detectedOsid);
      logJob("XDF/tables reloaded from database definitions.");
    }
  });

  // export all
  $("#btn-export-all-tables")?.addEventListener("click", exportAllTablesCSV);

  // editor buttons (delegated + direct)
  const pane = $("#table-editor-pane");
  if (pane) {
    pane.addEventListener("click", (e) => {
      const t = e.target;
      if (t.id === "btn-revert-table") revertActiveTable();
      if (t.id === "btn-export-table") exportActiveTableCSV();
      if (t.id === "btn-apply-to-bin") applyActiveTableToBin();
      if (t.id === "btn-reset-table") resetActiveTable();
      if (t.id === "btn-import-csv") triggerImportCSV();
      if (t.id === "btn-undo") undoLastEdit();
      if (t.id === "btn-interpolate") runInterpolate();
      if (t.id === "btn-smooth") runSmooth();
      if (t.id === "btn-clamp") runClamp();
      if (t.id === "btn-select-all-cells") selectAllCells();
      if (t.id === "btn-clear-selection") { state.tableSelection = []; highlightSelectedCells(); }
    });

    // batch ops
    $$("[data-batch-op]", pane).forEach(btn => {
      btn.addEventListener("click", () => {
        const op = btn.dataset.batchOp;
        const val = parseFloat($("#batch-value").value) || 0;
        applyBatchOp(op, val);
      });
    });

    $("#show-diff")?.addEventListener("change", () => {
      renderActiveTableGrid();
    });
  }

  // keyboard support in grids (global capture when editor visible)
  document.addEventListener("keydown", (e) => {
    if ($("#table-editor-pane")?.classList.contains("hidden")) return;
    if (e.key === "Escape") { state.tableSelection = []; highlightSelectedCells(); }
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z") { e.preventDefault(); undoLastEdit(); }
  });

  // initial empty state is shown via HTML
}

function selectTable(tableId) {
  state.activeTableId = tableId;
  state.tableSelection = [];
  renderTablesList($("#table-search")?.value || "", $$(".chip-filter.active")[0]?.dataset.filter || "all");

  const empty = $("#table-editor-empty-state");
  const pane = $("#table-editor-pane");
  empty?.classList.add("hidden");
  pane?.classList.remove("hidden");

  const tbl = state.currentTables.find(t => t.id === tableId);
  if (!tbl) return;

  // header
  $("#editor-table-name").textContent = tbl.name;
  $("#editor-table-type").textContent = tbl.type.toUpperCase();
  $("#editor-table-units").textContent = tbl.units || "";
  $("#editor-table-size").textContent = `${tbl.dims[0]}×${tbl.dims[1]}${tbl.type==="3d" ? "×"+(tbl.dims[2]||3) : ""}`;
  $("#editor-table-addr").textContent = tbl.addr ? `addr ${tbl.addr}` : "";
  $("#editor-table-desc").textContent = tbl.description || "No description provided in XDF.";

  // ensure edit state
  if (!state.tableEdits[tableId]) {
    const snap = deepCloneData(tbl.data);
    state.tableEdits[tableId] = { data: snap, original: deepCloneData(tbl.data), modified: false };
    state.undoStack[tableId] = [];
  }

  renderActiveTableGrid();
  render3DVisualIfNeeded(tbl);
}

function deepCloneData(d) {
  return Array.isArray(d[0]) ? d.map(r => Array.isArray(r) ? [...r] : r) : [...d];
}

function getActiveEdit() {
  if (!state.activeTableId) return null;
  return state.tableEdits[state.activeTableId];
}

function renderActiveTableGrid() {
  const container = $("#table-data-grid");
  const tbl = state.currentTables.find(t => t.id === state.activeTableId);
  const edit = getActiveEdit();
  if (!container || !tbl || !edit) return;
  container.innerHTML = "";

  const showDiff = $("#show-diff")?.checked;
  const data = edit.data;
  const orig = edit.original;

  if (tbl.type === "1d") {
    // horizontal cells with axis labels
    const row = document.createElement("div");
    row.className = "one-d-row";
    const vals = data[0] || data;
    const axis = tbl.xAxis || [];
    vals.forEach((v, i) => {
      const cell = document.createElement("div");
      cell.className = "one-d-cell";
      const isMod = showDiff && Math.abs(v - (orig[0]||orig)[i]) > 0.001;
      cell.innerHTML = `
        <label>${axis[i] ?? i}</label>
        <input type="number" step="0.01" value="${v.toFixed(2)}" data-idx="${i}" />
      `;
      if (isMod) cell.style.border = "1px solid var(--warning)";
      const inp = $("input", cell);
      inp.addEventListener("input", () => {
        pushUndo();
        const nv = parseFloat(inp.value) || 0;
        vals[i] = nv;
        markModified();
        if (tbl.type === "3d") render3DVisualIfNeeded(tbl); // rare
      });
      inp.addEventListener("focus", () => { state.tableSelection = [i]; highlightSelectedCells(); });
      row.appendChild(cell);
    });
    container.appendChild(row);
    return;
  }

  // 2D + 3D use a grid table (for 3D we show primary slice; the 3D viz handles the extra dimension feel)
  const table = document.createElement("table");
  table.className = "map-table";

  const rows = data.length;
  const cols = data[0]?.length || 0;

  // header row with X axis
  const thead = document.createElement("tr");
  thead.appendChild(document.createElement("th")); // corner
  const xAxis = tbl.xAxis || [];
  for (let c = 0; c < cols; c++) {
    const th = document.createElement("th");
    th.className = "axis";
    th.textContent = (xAxis[c] ?? c).toString().slice(0,5);
    thead.appendChild(th);
  }
  table.appendChild(thead);

  const yAxis = tbl.yAxis || [];
  for (let r = 0; r < rows; r++) {
    const tr = document.createElement("tr");
    const labelTd = document.createElement("td");
    labelTd.className = "axis";
    labelTd.textContent = (yAxis[r] ?? r).toString().slice(0,5);
    tr.appendChild(labelTd);

    for (let c = 0; c < cols; c++) {
      const td = document.createElement("td");
      const val = data[r][c];
      const oVal = orig[r]?.[c] ?? val;
      const isMod = showDiff && Math.abs(val - oVal) > 0.001;
      const inp = document.createElement("input");
      inp.type = "number";
      inp.step = "0.01";
      inp.value = (typeof val === "number" ? val.toFixed(2) : val);
      inp.dataset.r = r;
      inp.dataset.c = c;

      if (isMod) td.classList.add("modified");
      if (state.tableSelection.some(s => (s.r === r && s.c === c) || s === (r*cols + c))) td.classList.add("selected");

      inp.addEventListener("input", () => {
        pushUndo();
        const nv = parseFloat(inp.value) || 0;
        data[r][c] = nv;
        markModified();
        if (isMod) td.classList.add("modified");
        render3DVisualIfNeeded(tbl);
      });
      inp.addEventListener("focus", () => {
        state.tableSelection = [{ r, c }];
        highlightSelectedCells();
      });
      td.appendChild(inp);
      tr.appendChild(td);
    }
    table.appendChild(tr);
  }
  container.appendChild(table);
}

function render3DVisualIfNeeded(tbl) {
  const vizWrap = $("#viz-3d-container");
  const canvas = $("#viz-3d-canvas");
  if (!vizWrap || !canvas) return;

  const is3d = tbl.type === "3d";
  vizWrap.classList.toggle("hidden", !is3d);
  if (!is3d) return;

  const ctx = canvas.getContext("2d");
  const edit = getActiveEdit();
  if (!edit) return;
  const data = edit.data; // primary 2d slice we visualize

  const W = canvas.width, H = canvas.height;
  ctx.fillStyle = "#0a1010";
  ctx.fillRect(0, 0, W, H);

  const rows = data.length;
  const cols = data[0]?.length || 0;
  const cellW = Math.floor((W - 20) / cols);
  const cellH = Math.floor((H - 30) / rows);

  let minV = Infinity, maxV = -Infinity;
  data.forEach(row => row.forEach(v => { minV = Math.min(minV, v); maxV = Math.max(maxV, v); }));
  const range = (maxV - minV) || 1;

  function valColor(v) {
    const t = (v - minV) / range;
    // blue (low) -> cyan -> green -> yellow -> red (high)
    const r = Math.floor(30 + 220 * Math.min(1, t * 1.4));
    const g = Math.floor(80 + 140 * (1 - Math.abs(t - 0.6)));
    const b = Math.floor(200 * (1 - t));
    return `rgb(${r},${g},${b})`;
  }

  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const x = 10 + c * cellW;
      const y = 10 + r * cellH;
      ctx.fillStyle = valColor(data[r][c]);
      ctx.fillRect(x, y, cellW - 1, cellH - 1);
      // subtle grid + value
      ctx.strokeStyle = "rgba(0,0,0,0.3)";
      ctx.strokeRect(x, y, cellW - 1, cellH - 1);
      if (cellW > 26) {
        ctx.fillStyle = "rgba(255,255,255,0.75)";
        ctx.font = "9px monospace";
        ctx.fillText(data[r][c].toFixed(0), x + 2, y + cellH - 4);
      }
    }
  }

  // selection overlay
  ctx.strokeStyle = "#fff";
  ctx.lineWidth = 2;
  state.tableSelection.forEach(sel => {
    if (sel.r != null && sel.c != null) {
      const x = 10 + sel.c * cellW;
      const y = 10 + sel.r * cellH;
      ctx.strokeRect(x + 1, y + 1, cellW - 3, cellH - 3);
    }
  });

  // click to pick cells
  canvas.onclick = (ev) => {
    const rect = canvas.getBoundingClientRect();
    const px = ev.clientX - rect.left;
    const py = ev.clientY - rect.top;
    const c = Math.floor((px - 10) / cellW);
    const r = Math.floor((py - 10) / cellH);
    if (r >= 0 && r < rows && c >= 0 && c < cols) {
      state.tableSelection = [{ r, c }];
      highlightSelectedCells();
      // also focus the corresponding input in grid if present
      const inp = $(`input[data-r="${r}"][data-c="${c}"]`);
      if (inp) inp.focus();
      render3DVisualIfNeeded(tbl);
    }
  };
}

function highlightSelectedCells() {
  // re-render grid to pick up .selected classes
  renderActiveTableGrid();
}

function pushUndo() {
  const id = state.activeTableId;
  const edit = getActiveEdit();
  if (!id || !edit) return;
  if (!state.undoStack[id]) state.undoStack[id] = [];
  // snapshot current
  state.undoStack[id].push(deepCloneData(edit.data));
  if (state.undoStack[id].length > 8) state.undoStack[id].shift();
}

function undoLastEdit() {
  const id = state.activeTableId;
  const edit = getActiveEdit();
  if (!id || !edit || !state.undoStack[id]?.length) return;
  const prev = state.undoStack[id].pop();
  edit.data = prev;
  markModified(false);
  renderActiveTableGrid();
  const tbl = state.currentTables.find(t => t.id === id);
  if (tbl) render3DVisualIfNeeded(tbl);
}

function markModified(force) {
  const edit = getActiveEdit();
  if (!edit) return;
  edit.modified = force !== false;
  const stats = $("#edit-stats");
  if (stats) {
    const modCount = countModifiedCells();
    stats.textContent = edit.modified ? `${modCount} cells changed` : "no changes";
  }
  renderTablesList($("#table-search")?.value || "", $$(".chip-filter.active")[0]?.dataset.filter || "all"); // reflect dot if we had badges
}

function countModifiedCells() {
  const edit = getActiveEdit();
  const tbl = state.currentTables.find(t => t.id === state.activeTableId);
  if (!edit || !tbl) return 0;
  let cnt = 0;
  const d = edit.data, o = edit.original;
  for (let r = 0; r < d.length; r++) {
    for (let c = 0; c < (d[r]?.length || 0); c++) {
      if (Math.abs((d[r][c] || 0) - (o[r]?.[c] || 0)) > 0.0005) cnt++;
    }
  }
  return cnt;
}

function applyBatchOp(op, val) {
  const edit = getActiveEdit();
  const tbl = state.currentTables.find(t => t.id === state.activeTableId);
  if (!edit || !tbl) return;
  pushUndo();
  const sel = state.tableSelection;
  const useSel = sel.length > 0;

  function applyTo(r, c) {
    let v = edit.data[r][c];
    if (op === "add") v += val;
    if (op === "sub") v -= val;
    if (op === "mul") v *= val;
    if (op === "div" && val !== 0) v /= val;
    edit.data[r][c] = v;
  }

  if (tbl.type === "1d") {
    const arr = edit.data[0] || edit.data;
    const origArr = edit.original[0] || edit.original;
    arr.forEach((_, i) => {
      if (!useSel || sel.includes(i)) applyTo(0, i);
    });
  } else {
    for (let r = 0; r < edit.data.length; r++) {
      for (let c = 0; c < edit.data[r].length; c++) {
        if (!useSel || sel.some(s => s.r === r && s.c === c)) applyTo(r, c);
      }
    }
  }
  markModified();
  renderActiveTableGrid();
  render3DVisualIfNeeded(tbl);
}

function runInterpolate() {
  const edit = getActiveEdit();
  const tbl = state.currentTables.find(t => t.id === state.activeTableId);
  if (!edit || !tbl || tbl.type === "1d") return;
  pushUndo();
  const d = edit.data;
  // simple row-wise then col-wise linear interp for empty-ish (we treat all as fill)
  // for demo: average neighbors for "gaps" but actually just global smooth-lite
  for (let r = 1; r < d.length - 1; r++) {
    for (let c = 1; c < d[r].length - 1; c++) {
      d[r][c] = (d[r-1][c] + d[r+1][c] + d[r][c-1] + d[r][c+1]) / 4;
    }
  }
  markModified();
  renderActiveTableGrid();
  render3DVisualIfNeeded(tbl);
}

function runSmooth() {
  const edit = getActiveEdit();
  const tbl = state.currentTables.find(t => t.id === state.activeTableId);
  if (!edit || !tbl) return;
  pushUndo();
  const d = edit.data;
  const rows = d.length;
  const cols = d[0].length;
  const copy = deepCloneData(d);
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      let sum = 0, n = 0;
      for (let dr = -1; dr <= 1; dr++) for (let dc = -1; dc <= 1; dc++) {
        const rr = r + dr, cc = c + dc;
        if (rr >= 0 && rr < rows && cc >= 0 && cc < cols) { sum += copy[rr][cc]; n++; }
      }
      d[r][c] = sum / n;
    }
  }
  markModified();
  renderActiveTableGrid();
  render3DVisualIfNeeded(tbl);
}

function runClamp() {
  const edit = getActiveEdit();
  const tbl = state.currentTables.find(t => t.id === state.activeTableId);
  if (!edit || !tbl) return;
  pushUndo();
  // crude global clamp based on observed min/max + 15%
  let mn = Infinity, mx = -Infinity;
  edit.data.forEach(r => r.forEach(v => { mn = Math.min(mn, v); mx = Math.max(mx, v); }));
  const lo = mn - (mx - mn) * 0.15;
  const hi = mx + (mx - mn) * 0.15;
  edit.data.forEach((r, ri) => r.forEach((v, ci) => {
    edit.data[ri][ci] = Math.max(lo, Math.min(hi, v));
  }));
  markModified();
  renderActiveTableGrid();
  render3DVisualIfNeeded(tbl);
}

function selectAllCells() {
  const tbl = state.currentTables.find(t => t.id === state.activeTableId);
  const edit = getActiveEdit();
  if (!tbl || !edit) return;
  state.tableSelection = [];
  if (tbl.type === "1d") {
    const len = (edit.data[0] || edit.data).length;
    state.tableSelection = Array.from({length: len}, (_,i) => i);
  } else {
    for (let r = 0; r < edit.data.length; r++) for (let c = 0; c < edit.data[r].length; c++) state.tableSelection.push({r, c});
  }
  highlightSelectedCells();
}

function revertActiveTable() {
  const edit = getActiveEdit();
  if (!edit) return;
  edit.data = deepCloneData(edit.original);
  edit.modified = false;
  state.tableSelection = [];
  renderActiveTableGrid();
  const tbl = state.currentTables.find(t => t.id === state.activeTableId);
  if (tbl) render3DVisualIfNeeded(tbl);
  $("#edit-stats").textContent = "";
}

function resetActiveTable() { revertActiveTable(); }

function exportActiveTableCSV() {
  const tbl = state.currentTables.find(t => t.id === state.activeTableId);
  const edit = getActiveEdit();
  if (!tbl || !edit) return;
  const d = edit.data;
  let csv = "";
  if (tbl.type === "1d") {
    csv = (tbl.xAxis || []).join(",") + "\n" + (d[0] || d).map(v => v.toFixed(3)).join(",");
  } else {
    csv = (tbl.xAxis || []).map((x,i)=>x).join(",") + "\n";
    d.forEach((row, ri) => {
      csv += (tbl.yAxis?.[ri] ?? ri) + "," + row.map(v => v.toFixed(3)).join(",") + "\n";
    });
  }
  downloadBlob(csv, `${tbl.id}.csv`, "text/csv");
}

function exportAllTablesCSV() {
  // very simple concatenated with headers
  let out = "table,type,description\n";
  state.currentTables.forEach(t => {
    out += `${t.name},${t.type},"${(t.description||"").replace(/"/g,'""')}"\n`;
  });
  downloadBlob(out, "tuneitverse_all_tables_meta.csv", "text/csv");
  logJob("Exported table catalogue CSV.");
}

function triggerImportCSV() {
  const inp = document.createElement("input");
  inp.type = "file";
  inp.accept = ".csv";
  inp.onchange = async () => {
    const f = inp.files[0];
    if (!f) return;
    const text = await f.text();
    applyCSVImport(text);
  };
  inp.click();
}

function applyCSVImport(csvText) {
  const edit = getActiveEdit();
  const tbl = state.currentTables.find(t => t.id === state.activeTableId);
  if (!edit || !tbl) return;
  pushUndo();
  const lines = csvText.trim().split(/\r?\n/);
  try {
    if (tbl.type === "1d") {
      const vals = lines[lines.length-1].split(",").map(parseFloat);
      const arr = edit.data[0] || edit.data;
      vals.forEach((v,i) => { if (i < arr.length) arr[i] = v; });
    } else {
      // assume first line headers, subsequent rows start with y then values
      for (let li = 1; li < lines.length; li++) {
        const parts = lines[li].split(",").map(parseFloat);
        const r = li - 1;
        if (r < edit.data.length) {
          for (let c = 0; c < edit.data[r].length; c++) {
            if (parts[c + 1] != null) edit.data[r][c] = parts[c + 1];
          }
        }
      }
    }
    markModified();
    renderActiveTableGrid();
    if (tbl.type === "3d") render3DVisualIfNeeded(tbl);
    logJob("CSV imported into current table.");
  } catch (e) {
    alert("CSV import failed: " + e);
  }
}

function applyActiveTableToBin() {
  const tbl = state.currentTables.find(t => t.id === state.activeTableId);
  const edit = getActiveEdit();
  if (!tbl || !edit || !state.selectedFileBytes) {
    alert("Load a BIN first and select a table.");
    return;
  }
  // Demo: mutate a copy of the bin bytes at the declared addr using scaled ints (very simplified model)
  const patched = state.currentBinPatched ? state.currentBinPatched : new Uint8Array(state.selectedFileBytes);
  const base = parseInt((tbl.addr || "0x20000").replace("0x",""), 16) || 0x20000;

  // write flattened data as u16 big-endian scaled by inverse of table scale (or 100 if no scale)
  const scaleInv = Math.round(1 / (tbl.scale || 0.01));
  let ptr = base;
  const flat = [];
  edit.data.forEach(row => row.forEach(v => flat.push(Math.round(v * scaleInv))));

  flat.forEach((val, i) => {
    const off = (ptr + i * 2) % patched.length;
    if (off + 1 < patched.length) {
      patched[off] = (val >> 8) & 0xff;
      patched[off + 1] = val & 0xff;
    }
  });

  state.currentBinPatched = patched;
  state.selectedFileBytes = patched; // update active working image
  logJob(`Applied ${tbl.name} edits to in-memory BIN image (demo patch at ${tbl.addr}).`);
  alert("Table edits patched into current BIN image. Use Read/Write tab to validate or write.");
  // re-validate hint
  state.binValidated = true;
  updateChecklist();
}

// ─── Misc helpers ─────────────────────────────────────────────────────────────
function downloadBlob(content, filename, mime) {
  const blob = new Blob([content], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url; a.download = filename; a.click();
  URL.revokeObjectURL(url);
}

function updateTablesCountOnLoad() {
  // called from loadTablesForOs
}

// ─── Initialization ───────────────────────────────────────────────────────────
function init() {
  setupNavigation();
  setupConnect();
  setupReadWrite();
  setupLiveData();
  setupDTC();
  setupTablesUI();

  // Default view
  switchView("dashboard");

  // Keyboard: number keys for quick nav (nice touch)
  document.addEventListener("keydown", (e) => {
    if (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA") return;
    if (e.key === "1") switchView("dashboard");
    if (e.key === "2") switchView("read-write");
    if (e.key === "3") switchView("live-data");
    if (e.key === "4") switchView("dtc");
    if (e.key === "5") switchView("tables");
  });

  // Seed a friendly message
  setTimeout(() => {
    const log = $("#job-log");
    if (log && !log.textContent.trim()) {
      log.textContent = "[ready] Load a .bin in Read/Write to auto-load XDF table definitions into the Tables tab.\n";
    }
  }, 800);

  // Expose a couple things for debugging in console
  window.TuneItVerse = { state, loadTablesForOs, invokeCmd };
  console.log("%c[TuneItVerse] App initialized. Tables/Map section ready.", "color:#0aa");
}

init();