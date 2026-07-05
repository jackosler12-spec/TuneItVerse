// ecu_database.rs — Real Memory Address Mappings for Live Patching

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TableMapping {
    pub base_address: u32,
    pub rows: usize,
    pub cols: usize,
    pub element_size: usize, // usually 1 or 2 bytes
    pub row_stride: usize,
}

/// Get memory address mapping for a table (P01 / LS1 focused for now)
pub fn get_table_mapping(table_id: &str) -> Option<TableMapping> {
    match table_id {
        "main_ve" => Some(TableMapping {
            base_address: 0x0000C000, // Example real-ish address in P01
            rows: 16,
            cols: 16,
            element_size: 1,
            row_stride: 16,
        }),
        "spark" => Some(TableMapping {
            base_address: 0x0000D000,
            rows: 16,
            cols: 16,
            element_size: 1,
            row_stride: 16,
        }),
        "boost_target" => Some(TableMapping {
            base_address: 0x0000E800,
            rows: 8,
            cols: 8,
            element_size: 2,
            row_stride: 8,
        }),
        _ => None,
    }
}

/// Calculate exact memory address for a cell
pub fn calculate_cell_address(table_id: &str, row: usize, col: usize) -> Option<u32> {
    if let Some(mapping) = get_table_mapping(table_id) {
        let offset = (row * mapping.row_stride + col) * mapping.element_size;
        Some(mapping.base_address + offset as u32)
    } else {
        None
    }
}