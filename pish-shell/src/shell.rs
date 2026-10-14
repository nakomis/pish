use std::sync::Arc;
use std::time::Duration;
use chrono::{DateTime, Local};
use pish_core::mqtt::RumqttcPublisher;
use pish_core::{Config, MqttPublisher, Services};

pub struct App {
    services: Services,
    /// Keeps the sensor reader thread alive for the life of the app.
    _sensor: pish_core::sensor::SensorFeed,
    home: crate::home::HomeScreen,
}

impl App {
    pub fn new(config: &Config) -> Self {
        let sensor = pish_core::sensor::SensorFeed::spawn_unix(&pish_core::SensorConfig::from_env());
        let services = Services {
            mqtt: connect_mqtt(config),
            sensor: sensor.handle(),
            ..Services::default()
        };
        Self {
            services,
            _sensor: sensor,
            home: crate::home::HomeScreen::new(config),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Local::now();
        self.services.clock.time = clock_time(&now);
        self.services.clock.date = clock_date(&now);

        ctx.set_cursor_icon(egui::CursorIcon::None);

        egui::CentralPanel::default().show(ctx, |ui| {
            self.home.show(ui, &self.services);
        });

        // Repaint often enough that the minute flip is never visibly late.
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

/// Start the MQTT connection if configured. Failure is logged, not fatal: the
/// shell is still useful without a broker.
fn connect_mqtt(config: &Config) -> Option<Arc<dyn MqttPublisher>> {
    let Some(mqtt) = config.mqtt.as_ref() else {
        log::info!("No [mqtt] config — running without MQTT");
        return None;
    };
    match RumqttcPublisher::connect(mqtt) {
        Ok(p) => {
            log::info!("MQTT connecting to {}:{} as {}", mqtt.endpoint, mqtt.port, mqtt.client_id);
            Some(Arc::new(p))
        }
        Err(e) => {
            log::error!("MQTT disabled: {e}");
            None
        }
    }
}

fn clock_time(now: &DateTime<Local>) -> String {
    now.format("%H:%M").to_string()
}

fn clock_date(now: &DateTime<Local>) -> String {
    now.format("%a %-d %b").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 4, 24, 16, 45, 0).unwrap()
    }

    #[test]
    fn connect_mqtt_without_config_is_none() {
        assert!(connect_mqtt(&Config::default()).is_none());
    }

    #[test]
    fn connect_mqtt_with_bad_cert_is_none() {
        let config = Config {
            mqtt: Some(pish_core::config::MqttConfig {
                endpoint: "localhost".into(),
                port: 8883,
                client_id: "test".into(),
                cert_path: "/nonexistent/cert.pem".into(),
                key_path: "/nonexistent/key.pem".into(),
            }),
            ..Config::default()
        };
        assert!(connect_mqtt(&config).is_none());
    }

    #[test]
    fn clock_time_format() {
        assert_eq!(clock_time(&fixed()), "16:45");
    }

    #[test]
    fn clock_date_format() {
        assert_eq!(clock_date(&fixed()), "Fri 24 Apr");
    }

    #[test]
    fn clock_time_pads_hours() {
        let t = Local.with_ymd_and_hms(2026, 4, 24, 9, 5, 0).unwrap();
        assert_eq!(clock_time(&t), "09:05");
    }

    #[test]
    fn clock_date_single_digit_day() {
        let t = Local.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap();
        assert_eq!(clock_date(&t), "Wed 1 Apr");
    }
}
