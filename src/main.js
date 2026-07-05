// Smarter live patching with change tracking

function renderTableEditor(table) {
  // ... (keep existing grid creation code)

  const cells = [];
  const originalValues = new Map(); // row-col -> original value

  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const cell = document.createElement('input');
      // ... styling ...
      const initialValue = (50 + r * 3 + c * 2).toFixed(1);
      cell.value = initialValue;

      const key = `${r}-${c}`;
      originalValues.set(key, parseFloat(initialValue));

      cells.push({ row: r, col: c, input: cell, key });
      grid.appendChild(cell);
    }
  }

  // ... (rest of grid setup)

  // Improved Apply button
  btnApply.onclick = async () => {
    btnApply.disabled = true;
    btnApply.textContent = 'Applying...';

    let changedCells = [];

    for (const cellInfo of cells) {
      const currentValue = parseFloat(cellInfo.input.value);
      const originalValue = originalValues.get(cellInfo.key);

      if (Math.abs(currentValue - originalValue) > 0.01) {
        changedCells.push({
          row: cellInfo.row,
          col: cellInfo.col,
          value: currentValue,
          original: originalValue
        });
      }
    }

    if (changedCells.length === 0) {
      showToast('No changes detected', 'info');
      btnApply.disabled = false;
      btnApply.textContent = 'Apply Changes to ECU';
      return;
    }

    let success = 0;
    let failed = 0;

    for (let i = 0; i < changedCells.length; i++) {
      const change = changedCells[i];
      btnApply.textContent = `Applying ${i+1}/${changedCells.length}...`;

      try {
        await invokeCmd('apply_live_patch', {
          table_id: table.id,
          row: change.row,
          col: change.col,
          new_value: change.value
        });
        success++;
        // Update original value after successful write
        originalValues.set(change.key, change.value);
      } catch (e) {
        failed++;
        console.error('Patch failed:', change, e);
      }
    }

    btnApply.disabled = false;
    btnApply.textContent = 'Apply Changes to ECU';

    showToast(`Applied ${success} changes${failed > 0 ? `, ${failed} failed` : ''}`, failed > 0 ? 'warning' : 'success');
  };
}