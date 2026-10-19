//! The heating strip: is the boiler firing, is an override active, is anything
//! off-nominal. Derived purely from the daemon's state so it can be unit-tested.

use std::time::{Duration, Instant};

use chrono::{DateTime, FixedOffset, Utc};
use egui::{Color32, RichText};
use pish_core::{DaemonState, SensorStatus};

use crate::sync::state_setpoint;

const MUTED: Color32 = Color32::from_rgb(0x88, 0x99, 0xaa);
const FIRING: Color32 = Color32::from_rgb(0xff, 0x8c, 0x32);
const CAUTION: Color32 = Color32::from_rgb(0xe5, 0xa5, 0x4b);

/// The daemon sends state every 60 s; three minutes of silence means stale.
pub(crate) const STALE_AFTER: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct HeatingView {
    /// Mode is `"off"`.
    pub off: bool,
    /// The boiler is firing (heatlink relay, falling back to demand).
    pub heating: bool,
    /// e.g. `"until 21:30"`.
    pub override_hint: Option<String>,
    /// Subtle warning: heatlink fallback / forced, or stale state.
    pub warning: Option<String>,
}

impl HeatingView {
    /// `None` when there is no state to show. `offset` is the local UTC offset
    /// used to render the override's end time.
    pub(crate) fn from_status(
        status: &SensorStatus,
        now: Instant,
        offset: FixedOffset,
    ) -> Option<Self> {
        let snap = status.state.as_ref()?;
        let stale = now.saturating_duration_since(snap.received_at) > STALE_AFTER;
        let mut view = Self::from_state(&snap.state, offset);
        if stale {
            view.warning = Some("State out of date".into());
        }
        Some(view)
    }

    pub(crate) fn from_state(state: &DaemonState, offset: FixedOffset) -> Self {
        let off = state.mode == "off";
        let relay = state.heatlink.as_ref().and_then(|h| h.relay);
        let heating = !off && relay.unwrap_or(state.demand);
        let override_hint = if off {
            None
        } else {
            state
                .override_
                .as_ref()
                .map(|o| format!("until {}", format_until(o.until, offset)))
        };
        let warning = match state.heatlink.as_ref().and_then(|h| h.mode.as_deref()) {
            Some("fallback") => Some("Boiler controller in fallback".to_owned()),
            Some("force_on") => Some("Boiler forced on at switch".to_owned()),
            Some("force_off") => Some("Boiler forced off at switch".to_owned()),
            _ => None,
        };
        Self {
            off,
            heating,
            override_hint,
            warning,
        }
    }

    pub(crate) fn show(&self, ui: &mut egui::Ui, height: f32) {
        let size = (height * 0.6).max(10.0);
        ui.vertical_centered(|ui| {
            ui.horizontal(|ui| {
                let mut parts: Vec<(String, Color32)> = vec![];
                if self.off {
                    parts.push(("Off".into(), MUTED));
                } else if self.heating {
                    parts.push(("● Heating".into(), FIRING));
                } else {
                    parts.push(("Idle".into(), MUTED));
                }
                if let Some(h) = &self.override_hint {
                    parts.push((h.clone(), MUTED));
                }
                let width: f32 = parts
                    .iter()
                    .map(|(t, _)| t.chars().count() as f32 * size * 0.55 + size)
                    .sum();
                ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                for (text, colour) in parts {
                    ui.label(RichText::new(text).size(size).color(colour));
                }
            });
            if let Some(w) = &self.warning {
                ui.label(RichText::new(w).size(size * 0.85).color(CAUTION));
            }
        });
    }
}

/// `HH:MM` in the given offset.
pub(crate) fn format_until(until: DateTime<Utc>, offset: FixedOffset) -> String {
    until.with_timezone(&offset).format("%H:%M").to_string()
}

/// Feed-derived input for the dial reconciliation: `(seq, effective setpoint)`.
pub(crate) fn state_input(status: &SensorStatus) -> Option<(u64, f32)> {
    let snap = status.state.as_ref()?;
    Some((snap.seq, state_setpoint(&snap.state)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pish_core::sensor::{parse_message, Message};
    use pish_core::StateSnapshot;

    fn utc() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    fn state(json: &str) -> DaemonState {
        match parse_message(json) {
            Some(Message::State(s)) => *s,
            other => panic!("not a state: {other:?}"),
        }
    }

    fn status(state: DaemonState, received_at: Instant) -> SensorStatus {
        SensorStatus {
            state: Some(StateSnapshot {
                state,
                received_at,
                seq: 1,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn relay_true_means_heating() {
        let s = state(
            r#"{"type":"state","mode":"heat","setpoint":20,"demand":false,"heatlink":{"relay":true}}"#,
        );
        assert!(HeatingView::from_state(&s, utc()).heating);
    }

    #[test]
    fn relay_false_beats_demand() {
        let s = state(
            r#"{"type":"state","mode":"heat","setpoint":20,"demand":true,"heatlink":{"relay":false}}"#,
        );
        assert!(!HeatingView::from_state(&s, utc()).heating);
    }

    #[test]
    fn falls_back_to_demand_without_heatlink() {
        let on = state(r#"{"type":"state","mode":"heat","setpoint":20,"demand":true}"#);
        assert!(HeatingView::from_state(&on, utc()).heating);
        let unknown = state(
            r#"{"type":"state","mode":"heat","setpoint":20,"demand":true,"heatlink":{"mode":"auto"}}"#,
        );
        assert!(HeatingView::from_state(&unknown, utc()).heating);
    }

    #[test]
    fn off_mode_never_shows_heating_or_override() {
        let s = state(
            r#"{"type":"state","mode":"off","setpoint":20,"demand":true,"heatlink":{"relay":true},"override":{"setpoint":21,"until":"2026-10-19T09:30:00Z"}}"#,
        );
        let v = HeatingView::from_state(&s, utc());
        assert!(v.off && !v.heating);
        assert_eq!(v.override_hint, None);
    }

    #[test]
    fn override_hint_uses_local_offset() {
        let s = state(
            r#"{"type":"state","mode":"heat","setpoint":21,"override":{"setpoint":21,"until":"2026-10-19T20:30:00Z"}}"#,
        );
        let bst = FixedOffset::east_opt(3600).unwrap();
        assert_eq!(
            HeatingView::from_state(&s, bst).override_hint.as_deref(),
            Some("until 21:30")
        );
        assert_eq!(
            HeatingView::from_state(&s, utc()).override_hint.as_deref(),
            Some("until 20:30")
        );
    }

    #[test]
    fn no_override_no_hint() {
        let s = state(r#"{"type":"state","mode":"heat","setpoint":20,"override":null}"#);
        assert_eq!(HeatingView::from_state(&s, utc()).override_hint, None);
    }

    #[test]
    fn heatlink_modes_warn() {
        for (mode, expect) in [
            ("auto", None),
            ("fallback", Some("fallback")),
            ("force_on", Some("on")),
            ("force_off", Some("off")),
        ] {
            let s = state(&format!(
                r#"{{"type":"state","mode":"heat","setpoint":20,"heatlink":{{"mode":"{mode}"}}}}"#
            ));
            let w = HeatingView::from_state(&s, utc()).warning;
            match expect {
                None => assert_eq!(w, None, "{mode}"),
                Some(word) => assert!(w.unwrap().contains(word), "{mode}"),
            }
        }
    }

    #[test]
    fn stale_after_three_minutes() {
        let t0 = Instant::now();
        let s = state(r#"{"type":"state","mode":"heat","setpoint":20}"#);
        let st = status(s, t0);
        let fresh = HeatingView::from_status(&st, t0 + Duration::from_secs(180), utc()).unwrap();
        assert_eq!(fresh.warning, None);
        let stale = HeatingView::from_status(&st, t0 + Duration::from_secs(181), utc()).unwrap();
        assert!(stale.warning.unwrap().contains("out of date"));
    }

    #[test]
    fn no_state_no_view() {
        assert_eq!(
            HeatingView::from_status(&SensorStatus::default(), Instant::now(), utc()),
            None
        );
    }

    #[test]
    fn state_input_gives_seq_and_setpoint() {
        let s = state(r#"{"type":"state","mode":"heat","setpoint":20.5}"#);
        assert_eq!(state_input(&status(s, Instant::now())), Some((1, 20.5)));
        assert_eq!(state_input(&SensorStatus::default()), None);
        let no_sp = state(r#"{"type":"state","mode":"heat"}"#);
        assert_eq!(state_input(&status(no_sp, Instant::now())), None);
    }
}
