  if (state.visibleCharts.size === 0) {
    ctx.fillStyle = isDark ? "#3a5050" : "#9bb6b6";
    ctx.font = "14px Inter, system-ui, sans-serif";
    ctx.textAlign = "center";
    ctx.fillText("Select sensors from the panel to display live traces", W / 2, H / 2 + 10);
    return;
  }

  // Draw traces in user-defined order (supports drag-to-reorder)
  const drawOrder = state.chartOrder.filter((key) => state.visibleCharts.has(key));
  drawOrder.forEach((key) => {
    const data = state.chartData[key] || [];
    if (data.length < 2) return;

    const [minV, maxV] = ranges[key] || [0, 100];
    const range = maxV - minV;

    ctx.beginPath();
    data.forEach((v, i) => {
      const x = i * stepX;
      const y = H - ((v - minV) / range) * H * 0.85 - H * 0.05;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.strokeStyle = colors[key] || "#00c4b4";
    ctx.lineWidth = 2.5;
    ctx.lineJoin = "round";
    ctx.lineCap = "round";
    ctx.stroke();

    ctx.shadowColor = colors[key] || "#00c4b4";
    ctx.shadowBlur = 6;
    ctx.stroke();
    ctx.shadowBlur = 0;
  });