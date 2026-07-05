// Update loadTablesForOs with axis labels
function loadTablesForOs(osid) {
  state.currentTables = [
    { 
      id: 'main_ve', 
      name: 'Main VE Table', 
      size: [16, 16],
      xAxisLabel: 'RPM →',
      yAxisLabel: 'MAP (kPa)'
    },
    { 
      id: 'spark', 
      name: 'Spark Advance', 
      size: [16, 16],
      xAxisLabel: 'RPM →',
      yAxisLabel: 'MAP (kPa)'
    },
    { 
      id: 'boost', 
      name: 'Boost Target', 
      size: [8, 8],
      xAxisLabel: 'RPM →',
      yAxisLabel: 'Desired Boost (kPa)'
    }
  ];
  renderTablesList();
  showToast(`Loaded tables for ${osid}`);
}