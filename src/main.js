  updateGauges({
    rpm: d.rpm ?? 0,
    map: d.map ?? 0,
    iat: d.iat ?? 0,
    afr: d.afr ?? 14.7,
  });

  // Always push latest values to ALL series so history is ready when toggled visible
  Object.keys(state.chartData).forEach((key) => {
    if (typeof d[key] === "number") {
      state.chartData[key].push(d[key]);
      if (state.chartData[key].length > state.maxPoints) state.chartData[key].shift();
    }
  });
  drawLiveChart();