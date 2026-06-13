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

  const drawOrder = state.chartOrder.filter((key) => state.visibleCharts.has(key));
  drawOrder.forEach((key) => {
    const meta = sensorMeta[key] || { label: key.toUpperCase(), unit: "", color: "#00c4b4" };
    const item = document.createElement("div");
    item.className = "legend-item";
    item.draggable = true;
    item.dataset.key = key;
    item.innerHTML = `
      <span class="legend-color" style="background: ${meta.color}"></span>
      <span>${meta.label}${meta.unit ? " (" + meta.unit + ")" : ""}</span>
      <span class="drag-handle" style="margin-left:auto; cursor:grab; opacity:0.6;">⠿</span>
    `;

    // Drag events for reordering
    item.addEventListener("dragstart", (e) => {
      e.dataTransfer.setData("text/plain", key);
      item.style.opacity = "0.5";
    });
    item.addEventListener("dragend", () => {
      item.style.opacity = "1";
    });
    item.addEventListener("dragover", (e) => {
      e.preventDefault();
      item.style.border = "1px dashed var(--accent)";
    });
    item.addEventListener("dragleave", () => {
      item.style.border = "";
    });
    item.addEventListener("drop", (e) => {
      e.preventDefault();
      item.style.border = "";
      const draggedKey = e.dataTransfer.getData("text/plain");
      if (draggedKey === key) return;

      const currentOrder = state.chartOrder.filter((k) => state.visibleCharts.has(k));
      const fromIndex = currentOrder.indexOf(draggedKey);
      const toIndex = currentOrder.indexOf(key);

      if (fromIndex > -1 && toIndex > -1) {
        const [moved] = currentOrder.splice(fromIndex, 1);
        currentOrder.splice(toIndex, 0, moved);
        // Rebuild chartOrder preserving non-visible if any, but mainly the active order
        state.chartOrder = currentOrder;
        drawLiveChart();
        updateLegend();
      }
    });

    legendEl.appendChild(item);
  });
}