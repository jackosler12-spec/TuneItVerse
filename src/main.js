function initChartControls() {
  // Legacy - kept for compatibility but dashboard chart removed; live data uses dynamic below
}

// Sensor metadata for Live Data view charting and legend
const sensorMeta = {
  rpm: { label: "Engine RPM", unit: "rpm", color: "#00c4b4" },
  map: { label: "Manifold Pressure", unit: "kPa", color: "#6cb8e0" },
  iat: { label: "Intake Air Temp", unit: "°C", color: "#e0a030" },
  afr: { label: "Air / Fuel Ratio", unit: "", color: "#4ac990" },
  tps: { label: "Throttle Position", unit: "%", color: "#00c4b4" },
  ect: { label: "Coolant Temp", unit: "°C", color: "#e05555" },
};

function updateLegend() {
  const legendEl = $("#chart-legend");
  if (!legendEl) return;
  legendEl.innerHTML = "";

  if (state.visibleCharts.size === 0) {
    const hint = document.createElement("span");
    hint.textContent = "No sensors selected";
    hint.style.color = "var(--text-faint)";
    hint.style.fontSize = "var(--text-xs)";
    legendEl.appendChild(hint);
    return;
  }

  state.visibleCharts.forEach((key) => {
    const meta = sensorMeta[key] || { label: key.toUpperCase(), unit: "", color: "#00c4b4" };
    const item = document.createElement("div");
    item.className = "legend-item";
    item.innerHTML = `
      <span class="legend-color" style="background: ${meta.color}"></span>
      <span>${meta.label}${meta.unit ? " (« + meta.unit + ")" : ""}</span>
    `;
    legendEl.appendChild(item);
  });
}

function populateSensorGrid() {
  const grid = $("#sensor-select-grid");
  if (!grid) return;
  grid.innerHTML = "";

  Object.keys(sensorMeta).forEach((key) => {
    const meta = sensorMeta[key];
    const btn = document.createElement("button");
    btn.className = "chip sensor-toggle";
    btn.dataset.key = key;
    btn.innerHTML = `
      <span class="sensor-label">${meta.label}</span>
      <span class="sensor-unit">${meta.unit}</span>
    `;

    // Initial state
    if (state.visibleCharts.has(key)) {
      btn.classList.add("chip--active");
    }

    btn.addEventListener("click", () => {
      if (state.visibleCharts.has(key)) {
        state.visibleCharts.delete(key);
        btn.classList.remove("chip--active");
      } else {
        state.visibleCharts.add(key);
        btn.classList.add("chip--active");
      }
      drawLiveChart();
      updateLegend();
    });

    grid.appendChild(btn);
  });
}

function initLiveDataControls() {
  populateSensorGrid();

  // Select All
  $("#btn-select-all")?.addEventListener("click", () => {
    Object.keys(sensorMeta).forEach((key) => state.visibleCharts.add(key));
    document.querySelectorAll("#sensor-select-grid .chip").forEach((b) => b.classList.add("chip--active"));
    drawLiveChart();
    updateLegend();
  });

  // Clear
  $("#btn-clear-selection")?.addEventListener("click", () => {
    state.visibleCharts.clear();
    document.querySelectorAll("#sensor-select-grid .chip").forEach((b) => b.classList.remove("chip--active"));
    drawLiveChart();
    updateLegend();
  });

  // Initial legend
  updateLegend();
}