// Update the Apply button logic to pass auto_unlock

// In the loop where we call apply_live_patch:
await invokeCmd('apply_live_patch', {
    table_id: table.id,
    row: change.row,
    col: change.col,
    new_value: change.value,
    auto_unlock: true,           // Enable automatic security access
    family: "P01"                // Change to "EDC16" or "ZD30" as needed
});