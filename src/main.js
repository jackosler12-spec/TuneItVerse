// main.js — Polished Table Editor with Axis Labels

function renderTableEditor(table) {
  const editor = document.getElementById('table-editor');
  if (!editor) return;

  editor.innerHTML = '';
  editor.style.padding = '16px';
  editor.style.background = '#111';
  editor.style.borderRadius = '8px';

  // Title
  const title = document.createElement('div');
  title.style.fontSize = '15px';
  title.style.fontWeight = '600';
  title.style.marginBottom = '12px';
  title.textContent = `Editing: ${table.name}`;
  editor.appendChild(title);

  if (!table.size || table.size.length !== 2) {
    const msg = document.createElement('p');
    msg.textContent = 'Table dimensions not available.';
    editor.appendChild(msg);
    return;
  }

  const [rows, cols] = table.size;

  // Create wrapper for axis + grid
  const wrapper = document.createElement('div');
  wrapper.style.display = 'flex';
  wrapper.style.gap = '8px';
  wrapper.style.alignItems = 'flex-start';

  // Y-Axis Label (left side)
  const yAxis = document.createElement('div');
  yAxis.style.writingMode = 'vertical-rl';
  yAxis.style.transform = 'rotate(180deg)';
  yAxis.style.fontSize = '12px';
  yAxis.style.color = '#888';
  yAxis.style.padding = '4px 0';
  yAxis.style.whiteSpace = 'nowrap';
  yAxis.textContent = table.yAxisLabel || 'Y Axis (MAP / Boost kPa)';
  wrapper.appendChild(yAxis);

  // Main content area (X labels + grid)
  const mainArea = document.createElement('div');

  // X-Axis Label (top)
  const xLabel = document.createElement('div');
  xLabel.style.fontSize = '12px';
  xLabel.style.color = '#888';
  xLabel.style.textAlign = 'center';
  xLabel.style.marginBottom = '4px';
  xLabel.textContent = table.xAxisLabel || 'X Axis (RPM) →';
  mainArea.appendChild(xLabel);

  // Grid container
  const grid = document.createElement('div');
  grid.style.display = 'grid';
  grid.style.gridTemplateColumns = `repeat(${cols}, 62px)`;
  grid.style.gap = '3px';
  grid.style.background = '#222';
  grid.style.padding = '6px';
  grid.style.borderRadius = '4px';

  // Generate editable cells with better formatting
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const cell = document.createElement('input');
      cell.type = 'number';
      cell.step = '0.01';
      cell.style.width = '60px';
      cell.style.height = '28px';
      cell.style.padding = '2px 4px';
      cell.style.textAlign = 'center';
      cell.style.fontSize = '12px';
      cell.style.border = '1px solid #444';
      cell.style.background = (r + c) % 2 === 0 ? '#1a1a1a' : '#161616';
      cell.style.color = '#ddd';
      cell.style.borderRadius = '3px';

      // Realistic placeholder values
      const value = (45 + (r * 4.5) + (c * 2.8)).toFixed(1);
      cell.value = value;

      // Future: live update + send to ECU
      cell.addEventListener('change', () => {
        console.log(`[Table] ${table.id} [row ${r}, col ${c}] = ${cell.value}`);
        // TODO: Call Tauri command to apply live patch
      });

      grid.appendChild(cell);
    }
  }

  mainArea.appendChild(grid);
  wrapper.appendChild(mainArea);
  editor.appendChild(wrapper);

  // Footer actions
  const footer = document.createElement('div');
  footer.style.marginTop = '16px';
  footer.style.display = 'flex';
  footer.style.gap = '8px';

  const btnApply = document.createElement('button');
  btnApply.className = 'btn';
  btnApply.textContent = 'Apply Changes to ECU';
  btnApply.onclick = () => alert('Live patching coming soon...');

  const btnSave = document.createElement('button');
  btnSave.className = 'btn';
  btnSave.textContent = 'Export Patch (.bin)';

  const btnReset = document.createElement('button');
  btnReset.className = 'btn';
  btnReset.textContent = 'Reset to Stock';

  footer.appendChild(btnApply);
  footer.appendChild(btnSave);
  footer.appendChild(btnReset);
  editor.appendChild(footer);
}