/// Shared system services injected into every widget on each frame.
#[derive(Default, Clone)]
pub struct Services {
    pub wifi: WifiStatus,
    pub clock: ClockInfo,
    pub brightness: f32, // 0.0–1.0
}

#[derive(Default, Clone)]
pub struct WifiStatus {
    pub connected: bool,
    pub ssid: Option<String>,
    pub signal_dbm: Option<i32>,
}

#[derive(Default, Clone)]
pub struct ClockInfo {
    pub time: String,   // "HH:MM"
    pub date: String,   // "Fri 24 Apr"
}
