// Wire Apply button in renderTableEditor

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

  const hasAxes = table.xAxis && table.xAxis.length > 0;
  const [rows, cols] = table.size || [16, 16];

  const wrapper = document.createElement('div');
  wrapper.style.display = 'flex';
  wrapper.style.gap = '8px';
  wrapper.style.alignItems = 'flex-start';

  // Y-axis labels
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

  const main = document.createElement('div');

  // X-axis labels
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

  // Editable grid
  const grid = document.createElement('div');
  grid.style.display = 'grid';
  grid.style.gridTemplateColumns = `repeat(${cols}, 62px)`;
  grid.style.gap = '3px';
  grid.style.background = '#222';
  grid.style.padding = '6px';
  grid.style.borderRadius = '4px';

  const cells = []; // store references to all input elements

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
      cells.push({ row: r, col: c, input: cell });

      grid.appendChild(cell);
    }
  }

  main.appendChild(grid);
  wrapper.appendChild(main);
  editor.appendChild(wrapper);

  // Footer with wired buttons
  const footer = document.createElement('div');
  footer.style.marginTop = '16px';
  footer.style.display = 'flex';
  footer.style.gap = '8px';

  const btnApply = document.createElement('button');
  btnApply.className = 'btn btn-primary';
  btnApply.textContent = 'Apply Changes to ECU';
  btnApply.onclick = async () => {
    btnApply.disabled = true;
    btnApply.textContent = 'Applying...';

    let successCount = 0;
    let failCount = 0;

    for (const cellInfo of cells) {
      const value = parseFloat(cellInfo.input.value);
      try {
        await invokeCmd('apply_live_patch', {
          table_id: table.id,
          row: cellInfo.row,
          col: cellInfo.col,
          new_value: value
        });
        successCount++;
      } catch (e) {
        failCount++;
        console.warn('Patch failed for cell', cellInfo, e);
      }
    }

    btnApply.disabled = false;
    btnApply.textContent = 'Apply Changes to ECU';

    if (failCount === 0) {
      showToast(`Successfully applied ${successCount} changes to ECU`, 'success');
    } else {
      showToast(`${successCount} succeeded, ${failCount} failed`, 'warning');
    }
  };

  const btnSave = document.createElement('button');
  btnSave.className = 'btn';
  btnSave.textContent = 'Export Patch (.bin)';
  btnSave.onclick = () => showToast('Patch export coming soon');

  const btnReset = document.createElement('button');
  btnReset.className = 'btn';
  btnReset.textContent = 'Reset to Stock';
  btnReset.onclick = () => {
    if (confirm('Reset all values to stock?')) {
      location.reload(); // simple reset for now
    }
  };

  footer.appendChild(btnApply);
  footer.appendChild(btnSave);
  footer.appendChild(btnReset);
  editor.appendChild(footer);
}