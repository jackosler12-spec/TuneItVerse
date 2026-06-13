  // Select All
  $("#btn-select-all")?.addEventListener("click", () => {
    Object.keys(sensorMeta).forEach((key) => {
      state.visibleCharts.add(key);
      if (!state.chartOrder.includes(key)) state.chartOrder.push(key);
    });
    document.querySelectorAll("#sensor-select-grid .chip").forEach((b) => b.classList.add("chip--active"));
    drawLiveChart();
    updateLegend();
  });

  // Clear
  $("#btn-clear-selection")?.addEventListener("click", () => {
    state.visibleCharts.clear();
    state.chartOrder = [];
    document.querySelectorAll("#sensor-select-grid .chip").forEach((b) => b.classList.remove("chip--active"));
    drawLiveChart();
    updateLegend();
  });