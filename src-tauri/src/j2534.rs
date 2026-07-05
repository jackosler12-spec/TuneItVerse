// j2534.rs — Advanced features: Proper filtering + 29-bit CAN

// ... existing constants and structs ...

// Additional constants for advanced use
pub const J2534_FLAG_CAN_29BIT_ID: c_ulong = 0x00000100;
pub const J2534_FLAG_ISO15765_FRAME_PAD: c_ulong = 0x00000040;

impl J2534Device {
    // ... existing methods ...

    /// Start a proper ISO15765 flow control filter (recommended for real ECUs)
    pub unsafe fn start_iso15765_filter(&self, can_id: u32, mask: u32, is_29bit: bool) -> Result<c_ulong, String> {
        let start_filter: Symbol<PassThruStartMsgFilter> = self.get_symbol(b"PassThruStartMsgFilter\0")?;

        let mut mask_msg = PASSTHRU_MSG {
            ProtocolID: J2534_PROTOCOL_ISO15765,
            TxFlags: if is_29bit { J2534_FLAG_CAN_29BIT_ID } else { 0 },
            DataSize: 4,
            ..Default::default()
        };
        mask_msg.Data[0..4].copy_from_slice(&mask.to_be_bytes());

        let mut pattern_msg = PASSTHRU_MSG {
            ProtocolID: J2534_PROTOCOL_ISO15765,
            TxFlags: if is_29bit { J2534_FLAG_CAN_29BIT_ID } else { 0 },
            DataSize: 4,
            ..Default::default()
        };
        pattern_msg.Data[0..4].copy_from_slice(&can_id.to_be_bytes());

        let mut filter_id: c_ulong = 0;
        let status = start_filter(
            self.channel_id,
            0x00000001, // PASS_FILTER
            &mask_msg,
            &pattern_msg,
            std::ptr::null(),
            &mut filter_id,
        );

        if status != 0 {
            return Err(format!("StartMsgFilter failed with status {}", status));
        }
        Ok(filter_id)
    }

    /// Connect with explicit 29-bit CAN support
    pub unsafe fn connect_can_29bit(&mut self) -> Result<(), String> {
        let connect: Symbol<PassThruConnect> = self.get_symbol(b"PassThruConnect\0")?;
        let mut ch_id: c_ulong = 0;
        let flags = J2534_FLAG_CAN_29BIT_ID | J2534_FLAG_ISO15765_FRAME_PAD;

        let status = connect(self.device_id, J2534_PROTOCOL_ISO15765, flags, 500000, &mut ch_id);
        if status != 0 { return Err(format!("29-bit Connect failed: {}", status)); }
        self.channel_id = ch_id;
        self.set_config().ok();
        Ok(())
    }
}