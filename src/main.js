function drawLiveChart() {
  const canvas = $("#live-chart");
  if (!canvas) return;

  const ctx = canvas.getContext("2d");
  const W = canvas.offsetWidth || 600;
  const H = 160;
  canvas.width = W;
  canvas.height = H;

  const isDark = document.documentElement.getAttribute("data-theme") !== "light";

  ctx.clearRect(0, 0, W, H);

  // Grid
  ctx.strokeStyle = isDark ? "rgba(255,255,255,0.04)" : "rgba(0,0,0,0.05)";
  ctx.lineWidth = 1;
  for (let i = 0; i <= 4; i++) {
    const y = (i / 4) * H;
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(W, y);
    ctx.stroke();
  }

  const ranges = { rpm: [0, 7000], map: [20, 105], iat: [-10, 80], afr: [10, 18] };
  const colors = {
    rpm: isDark ? "#00c4b4" : "#008c80",
    map: "#6cb8e0",
    iat: "#e0a030",
    afr: "#4ac990",
  };

  const stepX = W / Math.max(state.maxPoints - 1, 1);

  // Draw each visible trace as its own line (independent scaling for clarity)
  state.visibleCharts.forEach((key) => {
    const data = state.chartData[key] || [];
    if (data.length < 2) return;

    const [minV, maxV] = ranges[key];
    const range = maxV - minV;

    ctx.beginPath();
    data.forEach((v, i) => {
      const x = i * stepX;
      const y = H - ((v - minV) / range) * H * 0.85 - H * 0.05;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.strokeStyle = colors[key];
    ctx.lineWidth = 2.5;
    ctx.lineJoin = "round";
    ctx.lineCap = "round";
    ctx.stroke();

    // Optional subtle glow for visibility
    ctx.shadowColor = colors[key];
    ctx.shadowBlur = 4;
    ctx.stroke();
    ctx.shadowBlur = 0;
  });
}