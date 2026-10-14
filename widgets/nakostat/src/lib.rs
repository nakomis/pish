use std::time::{Duration, Instant};

use chrono::{DateTime, SecondsFormat, Utc};
use egui::Vec2;
use pish_core::{MqttPublisher, PishWidget, Services};
use rotary_dial::RotaryDial;

/// How long the dial must sit still, untouched, before the setpoint is published.
pub const SETTLE_TIME: Duration = Duration::from_millis(1500);

pub struct NakostatWidget {
    dial: RotaryDial,
    /// True from the moment the widget is focused until the opening touch is released.
    /// While set, setpoint changes are suppressed so the tap-to-open doesn't move the dial.
    waiting_for_release: bool,
    debouncer: SetpointDebouncer,
    setpoint_topic: String,
}

impl NakostatWidget {
    pub fn new(setpoint_topic: impl Into<String>) -> Self {
        let dial = RotaryDial::new(20.0, 20.0);
        Self {
            debouncer: SetpointDebouncer::new(dial.setpoint, SETTLE_TIME),
            dial,
            waiting_for_release: false,
            setpoint_topic: setpoint_topic.into(),
        }
    }

    /// Per-frame publishing logic, split out of `update()` so it can be tested
    /// without an egui context. Returns how long until the pending setpoint
    /// settles, so the caller can schedule a repaint.
    fn after_dial(
        &mut self,
        touching: bool,
        now: Instant,
        mqtt: Option<&dyn MqttPublisher>,
    ) -> Option<Duration> {
        if let Some(setpoint) = self.debouncer.observe(self.dial.setpoint, touching, now) {
            self.publish_setpoint(setpoint, mqtt);
        }
        self.debouncer.time_to_settle(now)
    }

    fn publish_setpoint(&self, setpoint: f32, mqtt: Option<&dyn MqttPublisher>) {
        let Some(mqtt) = mqtt else {
            log::info!("Setpoint {setpoint:.1}°C settled, but MQTT is not configured");
            return;
        };
        let payload = setpoint_payload(setpoint, Utc::now(), mqtt.client_id());
        match mqtt.publish(&self.setpoint_topic, payload.as_bytes()) {
            Ok(()) => log::info!("Published setpoint {setpoint:.1}°C to {}", self.setpoint_topic),
            Err(e) => log::warn!("Failed to publish setpoint {setpoint:.1}°C: {e}"),
        }
    }
}

mod readings;
use readings::ReadingsView;

impl PishWidget for NakostatWidget {
    fn id(&self) -> &'static str { "nakostat" }
    fn title(&self) -> &str { "Heating" }
    fn min_size(&self) -> Vec2 { Vec2::new(300.0, 300.0) }

    fn on_focus(&mut self) {
        self.waiting_for_release = true;
    }

    fn update(&mut self, ui: &mut egui::Ui, services: &Services) {
        let available = ui.available_size();
        let readings = ReadingsView::from_status(&services.sensor.status());
        if let Some(t) = readings.temperature_c() {
            self.dial.current_temperature = t;
        }
        // Leave a strip under the dial for the sensor readings.
        let readings_h = (available.y * 0.2).clamp(48.0, 110.0);
        let size = available.x.min(available.y - readings_h).max(100.0);
        let y_pad = ((available.y - size - readings_h) / 2.0).max(0.0);

        // Suppress setpoint changes until the opening touch has been fully released.
        let saved = self.dial.setpoint;
        ui.add_space(y_pad);
        ui.vertical_centered(|ui| {
            self.dial.show(ui);
        });
        readings.show(ui, readings_h);
        if self.waiting_for_release {
            self.dial.setpoint = saved;
            if ui.input(|i| !i.pointer.any_down()) {
                self.waiting_for_release = false;
            }
        }

        let touching = ui.input(|i| i.pointer.any_down());
        if let Some(wait) = self.after_dial(touching, Instant::now(), services.mqtt.as_deref()) {
            ui.ctx().request_repaint_after(wait);
        }
    }
}

/// Turns a stream of per-frame dial values into a single "settled" event.
///
/// A value settles once the finger is off the screen and the value has not
/// changed for `settle` — so a drag publishes once, at the end, not on every
/// half-degree step. A settle back to the last published value publishes nothing.
#[derive(Debug, Clone)]
pub struct SetpointDebouncer {
    settle: Duration,
    last_published: f32,
    /// Candidate value and when it was last changed or touched.
    pending: Option<(f32, Instant)>,
}

impl SetpointDebouncer {
    pub fn new(initial: f32, settle: Duration) -> Self {
        Self { settle, last_published: initial, pending: None }
    }

    /// Feed the current dial value. Returns `Some(value)` exactly once per settle.
    pub fn observe(&mut self, value: f32, touching: bool, now: Instant) -> Option<f32> {
        match self.pending {
            Some((v, _)) if touching || v != value => self.pending = Some((value, now)),
            None if touching || value != self.last_published => {
                self.pending = Some((value, now))
            }
            Some((v, since)) if now.duration_since(since) >= self.settle => {
                self.pending = None;
                if v != self.last_published {
                    self.last_published = v;
                    return Some(v);
                }
            }
            _ => {}
        }
        None
    }

    /// Time left until the pending value settles, if one is pending.
    pub fn time_to_settle(&self, now: Instant) -> Option<Duration> {
        self.pending
            .map(|(_, since)| self.settle.saturating_sub(now.duration_since(since)))
    }
}

/// JSON body for a setpoint change. Matches what nakostat's setpoint API
/// publishes on `nakostat/<env>/api/setpoint`: `temperature` in °C,
/// `timestamp` as ISO 8601 UTC with milliseconds (JavaScript's
/// `toISOString()`), and `userId` identifying who made the change — here the
/// display's MQTT client id rather than a Cognito user.
pub fn setpoint_payload(temperature: f32, timestamp: DateTime<Utc>, user_id: &str) -> String {
    serde_json::json!({
        "temperature": f64::from(temperature),
        "timestamp": timestamp.to_rfc3339_opts(SecondsFormat::Millis, true),
        "userId": user_id,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use pish_core::MqttError;
    use std::sync::Mutex;

    const TOPIC: &str = "nakostat/sandbox/pish/setpoint";

    #[derive(Default)]
    struct MockMqtt {
        published: Mutex<Vec<(String, String)>>,
        fail: bool,
    }

    impl MockMqtt {
        fn published(&self) -> Vec<(String, String)> {
            self.published.lock().unwrap().clone()
        }
    }

    impl MqttPublisher for MockMqtt {
        fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), MqttError> {
            if self.fail {
                return Err(MqttError::Publish("queue full".into()));
            }
            let payload = String::from_utf8(payload.to_vec()).unwrap();
            self.published.lock().unwrap().push((topic.to_owned(), payload));
            Ok(())
        }

        fn client_id(&self) -> &str {
            "nakostat-pish-sandbox"
        }
    }

    fn widget() -> NakostatWidget {
        NakostatWidget::new(TOPIC)
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    // ── Widget basics ─────────────────────────────────────────────────────────

    #[test]
    fn nakostat_widget_id() {
        assert_eq!(widget().id(), "nakostat");
    }

    #[test]
    fn nakostat_widget_title() {
        assert_eq!(widget().title(), "Heating");
    }

    #[test]
    fn nakostat_default_setpoint() {
        assert_eq!(widget().dial.setpoint, 20.0);
    }

    #[test]
    fn nakostat_default_current_temp() {
        assert_eq!(widget().dial.current_temperature, 20.0);
    }

    #[test]
    fn nakostat_on_focus_arms_release_guard() {
        let mut w = widget();
        assert!(!w.waiting_for_release);
        w.on_focus();
        assert!(w.waiting_for_release);
    }

    // ── Debouncer ─────────────────────────────────────────────────────────────

    #[test]
    fn debouncer_idle_at_initial_value_never_fires() {
        let t0 = Instant::now();
        let mut d = SetpointDebouncer::new(20.0, ms(1000));
        assert_eq!(d.observe(20.0, false, t0), None);
        assert_eq!(d.observe(20.0, false, t0 + ms(5000)), None);
        assert_eq!(d.time_to_settle(t0 + ms(5000)), None);
    }

    #[test]
    fn debouncer_fires_once_after_settle() {
        let t0 = Instant::now();
        let mut d = SetpointDebouncer::new(20.0, ms(1000));
        assert_eq!(d.observe(21.0, false, t0), None);
        assert_eq!(d.observe(21.0, false, t0 + ms(999)), None);
        assert_eq!(d.observe(21.0, false, t0 + ms(1000)), Some(21.0));
        assert_eq!(d.observe(21.0, false, t0 + ms(5000)), None, "fires only once");
    }

    #[test]
    fn debouncer_drag_publishes_only_final_value() {
        let t0 = Instant::now();
        let mut d = SetpointDebouncer::new(20.0, ms(1000));
        let mut fired = vec![];
        // Drag through several half-degree steps over two seconds.
        for (i, v) in [20.5, 21.0, 21.5, 22.0, 22.5].iter().enumerate() {
            fired.extend(d.observe(*v, true, t0 + ms(400 * i as u64)));
        }
        // Finger lifted at 22.5; then idle frames.
        for i in 0..10 {
            fired.extend(d.observe(22.5, false, t0 + ms(2000 + 200 * i)));
        }
        assert_eq!(fired, vec![22.5]);
    }

    #[test]
    fn debouncer_does_not_fire_while_finger_held_still() {
        let t0 = Instant::now();
        let mut d = SetpointDebouncer::new(20.0, ms(1000));
        assert_eq!(d.observe(21.0, true, t0), None);
        assert_eq!(d.observe(21.0, true, t0 + ms(3000)), None);
        // Released: the settle clock starts from the last touched frame.
        assert_eq!(d.observe(21.0, false, t0 + ms(3500)), None);
        assert_eq!(d.observe(21.0, false, t0 + ms(4000)), Some(21.0));
    }

    #[test]
    fn debouncer_change_restarts_settle_clock() {
        let t0 = Instant::now();
        let mut d = SetpointDebouncer::new(20.0, ms(1000));
        d.observe(21.0, false, t0);
        assert_eq!(d.observe(21.5, false, t0 + ms(900)), None);
        assert_eq!(d.observe(21.5, false, t0 + ms(1500)), None);
        assert_eq!(d.observe(21.5, false, t0 + ms(1900)), Some(21.5));
    }

    #[test]
    fn debouncer_return_to_published_value_is_silent() {
        let t0 = Instant::now();
        let mut d = SetpointDebouncer::new(20.0, ms(1000));
        d.observe(21.0, true, t0);
        assert_eq!(d.observe(20.0, false, t0 + ms(100)), None);
        assert_eq!(d.observe(20.0, false, t0 + ms(2000)), None);
        assert_eq!(d.time_to_settle(t0 + ms(2000)), None, "pending cleared");
    }

    #[test]
    fn debouncer_successive_settles_each_fire() {
        let t0 = Instant::now();
        let mut d = SetpointDebouncer::new(20.0, ms(1000));
        d.observe(21.0, false, t0);
        assert_eq!(d.observe(21.0, false, t0 + ms(1000)), Some(21.0));
        d.observe(19.5, false, t0 + ms(2000));
        assert_eq!(d.observe(19.5, false, t0 + ms(3000)), Some(19.5));
    }

    #[test]
    fn debouncer_time_to_settle_counts_down() {
        let t0 = Instant::now();
        let mut d = SetpointDebouncer::new(20.0, ms(1000));
        d.observe(21.0, false, t0);
        assert_eq!(d.time_to_settle(t0 + ms(400)), Some(ms(600)));
        assert_eq!(d.time_to_settle(t0 + ms(2000)), Some(Duration::ZERO));
    }

    // ── Payload ───────────────────────────────────────────────────────────────

    #[test]
    fn payload_matches_nakostat_setpoint_schema() {
        let ts = Utc.with_ymd_and_hms(2026, 10, 13, 7, 30, 0).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&setpoint_payload(21.5, ts, "nakostat-pish-prod")).unwrap();
        assert_eq!(json["temperature"], 21.5);
        assert_eq!(json["timestamp"], "2026-10-13T07:30:00.000Z");
        assert_eq!(json["userId"], "nakostat-pish-prod");
        assert_eq!(json.as_object().unwrap().len(), 3, "no extra fields");
    }

    #[test]
    fn payload_temperature_is_a_json_number() {
        let json: serde_json::Value =
            serde_json::from_str(&setpoint_payload(20.0, Utc::now(), "x")).unwrap();
        assert!(json["temperature"].is_number());
        assert_eq!(json["temperature"].as_f64(), Some(20.0));
    }

    // ── Publishing via the mocked trait ───────────────────────────────────────

    #[test]
    fn settled_dial_publishes_once_to_configured_topic() {
        let mqtt = MockMqtt::default();
        let mut w = widget();
        let t0 = Instant::now();

        w.dial.setpoint = 21.0;
        w.after_dial(true, t0, Some(&mqtt));
        w.dial.setpoint = 22.5;
        w.after_dial(true, t0 + ms(200), Some(&mqtt));
        w.after_dial(false, t0 + ms(300), Some(&mqtt));
        assert!(mqtt.published().is_empty(), "nothing before settle");

        w.after_dial(false, t0 + ms(300) + SETTLE_TIME, Some(&mqtt));
        w.after_dial(false, t0 + ms(10_000), Some(&mqtt));

        let published = mqtt.published();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].0, TOPIC);
        let json: serde_json::Value = serde_json::from_str(&published[0].1).unwrap();
        assert_eq!(json["temperature"], 22.5);
        assert_eq!(json["userId"], "nakostat-pish-sandbox");
    }

    #[test]
    fn after_dial_reports_time_to_settle_for_repaint() {
        let mqtt = MockMqtt::default();
        let mut w = widget();
        let t0 = Instant::now();
        assert_eq!(w.after_dial(false, t0, Some(&mqtt)), None);
        w.dial.setpoint = 21.0;
        assert_eq!(w.after_dial(false, t0, Some(&mqtt)), Some(SETTLE_TIME));
    }

    #[test]
    fn untouched_dial_publishes_nothing() {
        let mqtt = MockMqtt::default();
        let mut w = widget();
        let t0 = Instant::now();
        for i in 0..20 {
            w.after_dial(false, t0 + ms(500 * i), Some(&mqtt));
        }
        assert!(mqtt.published().is_empty());
    }

    #[test]
    fn settle_without_mqtt_does_not_panic() {
        let mut w = widget();
        let t0 = Instant::now();
        w.dial.setpoint = 18.0;
        w.after_dial(false, t0, None);
        assert_eq!(w.after_dial(false, t0 + SETTLE_TIME, None), None);
    }

    #[test]
    fn publish_failure_is_logged_not_fatal() {
        let mqtt = MockMqtt { fail: true, ..Default::default() };
        let mut w = widget();
        let t0 = Instant::now();
        w.dial.setpoint = 23.0;
        w.after_dial(false, t0, Some(&mqtt));
        w.after_dial(false, t0 + SETTLE_TIME, Some(&mqtt));
        assert!(mqtt.published().is_empty());
    }
}
