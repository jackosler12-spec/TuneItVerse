    btn.addEventListener("click", () => {
      if (state.visibleCharts.has(key)) {
        state.visibleCharts.delete(key);
        btn.classList.remove("chip--active");
        // remove from order
        const idx = state.chartOrder.indexOf(key);
        if (idx > -1) state.chartOrder.splice(idx, 1);
      } else {
        state.visibleCharts.add(key);
        btn.classList.add("chip--active");
        if (!state.chartOrder.includes(key)) {
          state.chartOrder.push(key); // append to end for new traces
        }
      }
      drawLiveChart();
      updateLegend();
    });