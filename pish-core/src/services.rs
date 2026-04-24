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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn services_default_brightness() {
        let s = Services::default();
        assert_eq!(s.brightness, 0.0);
    }

    #[test]
    fn services_default_clock_empty() {
        let s = Services::default();
        assert!(s.clock.time.is_empty());
        assert!(s.clock.date.is_empty());
    }

    #[test]
    fn services_default_wifi_disconnected() {
        let s = Services::default();
        assert!(!s.wifi.connected);
        assert!(s.wifi.ssid.is_none());
        assert!(s.wifi.signal_dbm.is_none());
    }

    #[test]
    fn services_clone() {
        let mut s = Services::default();
        s.clock.time = "12:00".to_string();
        s.brightness = 0.8;
        let s2 = s.clone();
        assert_eq!(s2.clock.time, "12:00");
        assert_eq!(s2.brightness, 0.8);
    }
}
