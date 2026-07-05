// j2534.rs — Enhanced with UDS helpers and reconnection logic

// ... (keep all previous code)

impl J2534Device {
    // ... existing methods ...

    /// Higher-level UDS write (ISO15765)
    pub unsafe fn write_uds(&self, data: &[u8], timeout_ms: u32) -> Result<(), String> {
        self.write_msg(data, timeout_ms)
    }

    /// Higher-level read (returns raw PASSTHRU_MSGs)
    pub unsafe fn read_uds(&self, timeout_ms: u32, max_msgs: usize) -> Result<Vec<PASSTHRU_MSG>, String> {
        self.read_msgs(timeout_ms, max_msgs)
    }

    /// Simple health check (device_id > 0 means we have opened a device)
    pub fn is_connected(&self) -> bool {
        self.device_id != 0 && self.channel_id != 0
    }

    /// Attempt to reconnect using the stored dll_path
    pub unsafe fn reconnect(&mut self) -> Result<(), String> {
        // Close existing if any
        let _ = self.disconnect();

        // Re-open
        self.open()?;
        self.connect_can_500k()?;
        let _ = self.start_filter()?;
        Ok(())
    }
}

// Update the old placeholder functions to note they are legacy
// The real logic now lives in AppState + commands in lib.rs