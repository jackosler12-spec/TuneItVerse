// main.js — XDF Axis Integration

async function loadTablesForOs(osid) {
  try {
    // Try to load real XDF data
    const xdfData = await invokeCmd('load_xdf_for_os', { osid });
    const parsed = JSON.parse(xdfData);

    if (parsed.tables && parsed.tables.length > 0) {
      state.currentTables = parsed.tables.map(t => ({
        id: t.id,
        name: t.name,
        size: [t.rows, t.cols],
        xAxisLabel: t.x_label + ' →',
        yAxisLabel: t.y_label,
        xAxis: t.x_axis || [],
        yAxis: t.y_axis || []
      }));
      renderTablesList();
      showToast(`Loaded tables with real axes from XDF for ${osid}`);
      return;
    }
  } catch (e) {
    console.warn('Could not load XDF, using defaults', e);
  }

  // Fallback to default tables
  state.currentTables = [
    { id: 'main_ve', name: 'Main VE Table', size: [16, 16], xAxisLabel: 'RPM →', yAxisLabel: 'MAP (kPa)' },
    { id: 'spark', name: 'Spark Advance', size: [16, 16], xAxisLabel: 'RPM →', yAxisLabel: 'MAP (kPa)' },
    { id: 'boost', name: 'Boost Target', size: [8, 8], xAxisLabel: 'RPM →', yAxisLabel: 'Desired Boost (kPa)' }
  ];
  renderTablesList();
  showToast(`Loaded default tables for ${osid}`);
}

// Update renderTableEditor to show real axis values as headers
function renderTableEditor(table) {
  const editor = document.getElementById('table-editor');
  if (!editor) return;

  editor.innerHTML = '';
  editor.style.padding = '16px';
  editor.style.background = '#111';
  editor.style.borderRadius = '8px';

  const title = document.createElement('div');
  title.style.fontSize = '15px';
  title.style.fontWeight = '600';
  title.style.marginBottom = '12px';
  title.textContent = `Editing: ${table.name}`;
  editor.appendChild(title);

  const hasAxes = table.xAxis && table.xAxis.length > 0 && table.yAxis && table.yAxis.length > 0;
  const [rows, cols] = table.size || [16, 16];

  const wrapper = document.createElement('div');
  wrapper.style.display = 'flex';
  wrapper.style.gap = '8px';
  wrapper.style.alignItems = 'flex-start';

  // Y Axis values (left)
  const yCol = document.createElement('div');
  yCol.style.display = 'flex';
  yCol.style.flexDirection = 'column';
  yCol.style.gap = '3px';
  yCol.style.marginTop = hasAxes ? '28px' : '0';

  for (let r = 0; r < rows; r++) {
    const label = document.createElement('div');
    label.style.width = '70px';
    label.style.height = '28px';
    label.style.fontSize = '10px';
    label.style.color = '#888';
    label.style.display = 'flex';
    label.style.alignItems = 'center';
    label.style.justifyContent = 'flex-end';
    label.style.paddingRight = '6px';
    label.textContent = hasAxes ? (table.yAxis[r] || r) : r;
    yCol.appendChild(label);
  }
  wrapper.appendChild(yCol);

  // Main grid area
  const main = document.createElement('div');

  // X Axis values (top)
  const xRow = document.createElement('div');
  xRow.style.display = 'grid';
  xRow.style.gridTemplateColumns = `repeat(${cols}, 62px)`;
  xRow.style.gap = '3px';
  xRow.style.marginBottom = '4px';

  for (let c = 0; c < cols; c++) {
    const label = document.createElement('div');
    label.style.fontSize = '10px';
    label.style.color = '#888';
    label.style.textAlign = 'center';
    label.textContent = hasAxes ? (table.xAxis[c] || c) : c;
    xRow.appendChild(label);
  }
  main.appendChild(xRow);

  // The actual editable grid
  const grid = document.createElement('div');
  grid.style.display = 'grid';
  grid.style.gridTemplateColumns = `repeat(${cols}, 62px)`;
  grid.style.gap = '3px';
  grid.style.background = '#222';
  grid.style.padding = '6px';
  grid.style.borderRadius = '4px';

  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const cell = document.createElement('input');
      cell.type = 'number';
      cell.step = '0.01';
      cell.style.width = '60px';
      cell.style.height = '28px';
      cell.style.padding = '2px';
      cell.style.textAlign = 'center';
      cell.style.fontSize = '12px';
      cell.style.border = '1px solid #444';
      cell.style.background = (r + c) % 2 === 0 ? '#1a1a1a' : '#161616';
      cell.style.color = '#ddd';
      cell.style.borderRadius = '3px';

      cell.value = (50 + r * 3 + c * 2).toFixed(1);

      cell.onchange = () => {
        console.log(`Changed [${r},${c}] to ${cell.value}`);
      };

      grid.appendChild(cell);
    }
  }

  main.appendChild(grid);
  wrapper.appendChild(main);
  editor.appendChild(wrapper);
}