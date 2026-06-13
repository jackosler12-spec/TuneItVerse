function initChartControls() {
  document.querySelectorAll("[data-chart]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const key = btn.dataset.chart;
      if (state.visibleCharts.has(key)) {
        // Toggle off (but keep at least one visible)
        if (state.visibleCharts.size > 1) {
          state.visibleCharts.delete(key);
          btn.classList.remove("chip--active");
        }
      } else {
        state.visibleCharts.add(key);
        btn.classList.add("chip--active");
      }
      drawLiveChart();
    });
  });

  // Initialize chip active states from visibleCharts
  document.querySelectorAll("[data-chart]").forEach((btn) => {
    if (state.visibleCharts.has(btn.dataset.chart)) {
      btn.classList.add("chip--active");
    }
  });
}