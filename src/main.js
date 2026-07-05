// main.js — Professional 2D Table Editor

function renderTableEditor(table) {
  const editor = document.getElementById('table-editor');
  if (!editor) return;

  editor.innerHTML = '';
  editor.style.padding = '12px';

  const title = document.createElement('h4');
  title.textContent = `Editing: ${table.name} (${table.size ? table.size.join(' × ') : 'N/A'})`;
  editor.appendChild(title);

  if (!table.size || table.size.length !== 2) {
    editor.innerHTML += '<p>Table size not available for editing.</p>';
    return;
  }

  const [rows, cols] = table.size;
  const grid = document.createElement('div');
  grid.style.display = 'grid';
  grid.style.gridTemplateColumns = `repeat(${cols}, 58px)`;
  grid.style.gap = '2px';
  grid.style.marginTop = '12px';

  // Create editable cells
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const cell = document.createElement('input');
      cell.type = 'number';
      cell.step = '0.1';
      cell.style.width = '56px';
      cell.style.padding = '4px';
      cell.style.textAlign = 'center';
      cell.style.fontSize = '12px';
      cell.style.border = '1px solid #444';
      cell.style.background = '#1a1a1a';
      cell.style.color = '#ddd';

      // Placeholder value (in real version this would come from loaded bin/XDF)
      const baseValue = 50 + (r * 3) + (c * 1.2);
      cell.value = baseValue.toFixed(1);

      // Live edit handler (future: send patch to ECU)
      cell.addEventListener('change', () => {
        console.log(`[TuneItVerse] Cell [${r},${c}] changed to ${cell.value}`);
        // TODO: Send live patch via Tauri when connected
      });

      grid.appendChild(cell);
    }
  }

  editor.appendChild(grid);

  // Add action buttons
  const actions = document.createElement('div');
  actions.style.marginTop = '16px';
  actions.innerHTML = `
    <button class="btn">Apply to ECU</button>
    <button class="btn">Save as .bin patch</button>
    <button class="btn">Reset to Original</button>
  `;
  editor.appendChild(actions);
}