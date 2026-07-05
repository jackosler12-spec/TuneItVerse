// Connection State Tracking

#[derive(Default, Debug, Clone)]
pub struct ConnectionState {
    pub connected: bool,
    pub protocol: Option<String>,           // "j2534" or "elm"
    pub current_session: Option<u8>,
    pub security_unlocked: bool,
    pub security_level: Option<u8>,
    pub ecu_family: Option<String>,
}

// Extend AppState
pub struct AppState {
    pub j2534_device: Mutex<Option<crate::j2534::J2534Device>>,
    pub elm_port: Mutex<Option<Box<dyn serialport::SerialPort + Send>>>,
    pub connection_state: Mutex<ConnectionState>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            j2534_device: Mutex::new(None),
            elm_port: Mutex::new(None),
            connection_state: Mutex::new(ConnectionState::default()),
        }
    }
}

// Helper to update state
fn update_connection_state(
    state: &State<'_, AppState>,
    session: Option<u8>,
    unlocked: Option<bool>,
    level: Option<u8>,
    family: Option<String>,
) {
    if let Ok(mut guard) = state.connection_state.lock() {
        if let Some(s) = session { guard.current_session = Some(s); }
        if let Some(u) = unlocked { guard.security_unlocked = u; }
        if let Some(l) = level { guard.security_level = Some(l); }
        if let Some(f) = family { guard.ecu_family = Some(f); }
        guard.connected = true;
    }
}