//! The readings strip under the dial: live values from the sensor daemon.

use egui::{Color32, RichText};
use pish_core::sensor::LinkState;
use pish_core::SensorStatus;

const MUTED: Color32 = Color32::from_rgb(0x88, 0x99, 0xaa);
const WARNING: Color32 = Color32::from_rgb(0xe0, 0x6c, 0x75);
const WARMING: Color32 = Color32::from_rgb(0xe5, 0xa5, 0x4b);

/// What the readings strip should show. Derived purely from the sensor status
/// so it can be unit-tested without an `egui::Ui`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReadingsView {
    /// First connection attempt still in flight.
    Connecting,
    /// Socket is open but the daemon hasn't sent a reading yet.
    AwaitingReading,
    /// The daemon socket is absent or dropped; retrying after `retry_in_s`.
    NoDaemon { retry_in_s: u64 },
    Live {
        temperature_c: f32,
        temperature: String,
        humidity: String,
        eco2: String,
        tvoc: String,
        warming_up: bool,
    },
}

impl ReadingsView {
    pub(crate) fn from_status(status: &SensorStatus) -> Self {
        match (status.link, &status.latest) {
            (LinkState::Connecting, _) => Self::Connecting,
            // A reading from a dropped connection is stale: show the outage instead.
            (LinkState::Retrying { delay, .. }, _) => Self::NoDaemon {
                retry_in_s: delay.as_secs_f32().ceil() as u64,
            },
            (LinkState::Connected, None) => Self::AwaitingReading,
            (LinkState::Connected, Some(r)) => Self::Live {
                temperature_c: r.temperature_c,
                temperature: format!("{:.1}°C", r.temperature_c),
                humidity: format!("{:.0}%", r.humidity_pct),
                eco2: format!("eCO₂ {} ppm", r.eco2_ppm),
                tvoc: format!("TVOC {} ppb", r.tvoc_ppb),
                warming_up: r.warming_up,
            },
        }
    }

    /// The live room temperature, if there is one.
    pub(crate) fn temperature_c(&self) -> Option<f32> {
        match self {
            Self::Live { temperature_c, .. } => Some(*temperature_c),
            _ => None,
        }
    }

    /// Draw the strip into `height` logical pixels: temperature and humidity
    /// large, eCO₂ and TVOC small, plus a "warming up" badge whilst the ENS160
    /// settles.
    pub(crate) fn show(&self, ui: &mut egui::Ui, height: f32) {
        let big = (height * 0.45).max(14.0);
        let small = (height * 0.2).max(10.0);
        ui.vertical_centered(|ui| match self {
            Self::Connecting => {
                ui.label(
                    RichText::new("Connecting to sensor…")
                        .size(small)
                        .color(MUTED),
                );
            }
            Self::AwaitingReading => {
                ui.label(
                    RichText::new("Waiting for first reading…")
                        .size(small)
                        .color(MUTED),
                );
            }
            Self::NoDaemon { retry_in_s } => {
                ui.label(
                    RichText::new("No sensor daemon")
                        .size(big * 0.7)
                        .color(WARNING),
                );
                ui.label(
                    RichText::new(format!("Retrying in {retry_in_s}s"))
                        .size(small)
                        .color(MUTED),
                );
            }
            Self::Live {
                temperature,
                humidity,
                eco2,
                tvoc,
                warming_up,
                ..
            } => {
                ui.label(
                    RichText::new(format!("{temperature}   {humidity}"))
                        .size(big)
                        .color(Color32::WHITE),
                );
                ui.horizontal(|ui| {
                    let row = format!("{eco2}  ·  {tvoc}");
                    // Centre the small row, leaving room for the badge when shown.
                    let badge_w = if *warming_up { small * 7.0 } else { 0.0 };
                    let text_w = row.chars().count() as f32 * small * 0.5;
                    ui.add_space(((ui.available_width() - text_w - badge_w) / 2.0).max(0.0));
                    ui.label(RichText::new(row).size(small).color(MUTED));
                    if *warming_up {
                        egui::Frame::new()
                            .fill(WARMING)
                            .corner_radius(small * 0.5)
                            .inner_margin(egui::Margin::symmetric(6, 1))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("warming up")
                                        .size(small)
                                        .color(Color32::BLACK),
                                );
                            });
                    }
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pish_core::SensorReading;
    use std::time::Duration;

    /// A reading as the daemon sends it over the socket.
    fn reading(warming_up: bool) -> SensorReading {
        pish_core::sensor::parse_line(&format!(
            r#"{{"temperature_c":21.04,"humidity_pct":49.6,"eco2_ppm":600,"tvoc_ppb":100,"warming_up":{warming_up},"timestamp":"2026-10-13T09:30:00Z"}}"#
        ))
        .unwrap()
    }

    fn connected(r: SensorReading) -> SensorStatus {
        SensorStatus {
            link: LinkState::Connected,
            latest: Some(r),
        }
    }

    fn retrying(delay: Duration, latest: Option<SensorReading>) -> SensorStatus {
        SensorStatus {
            link: LinkState::Retrying { attempt: 3, delay },
            latest,
        }
    }

    #[test]
    fn live_formats_values() {
        assert_eq!(
            ReadingsView::from_status(&connected(reading(false))),
            ReadingsView::Live {
                temperature_c: 21.04,
                temperature: "21.0°C".into(),
                humidity: "50%".into(),
                eco2: "eCO₂ 600 ppm".into(),
                tvoc: "TVOC 100 ppb".into(),
                warming_up: false,
            }
        );
    }

    #[test]
    fn live_exposes_temperature_for_the_dial() {
        let view = ReadingsView::from_status(&connected(reading(false)));
        assert_eq!(view.temperature_c(), Some(21.04));
    }

    #[test]
    fn warming_up_shows_badge() {
        let view = ReadingsView::from_status(&connected(reading(true)));
        assert!(matches!(
            view,
            ReadingsView::Live {
                warming_up: true,
                ..
            }
        ));
    }

    #[test]
    fn no_daemon_when_retrying() {
        let view = ReadingsView::from_status(&retrying(Duration::from_millis(1_500), None));
        assert_eq!(view, ReadingsView::NoDaemon { retry_in_s: 2 });
        assert_eq!(view.temperature_c(), None);
    }

    #[test]
    fn no_daemon_hides_stale_reading() {
        let view =
            ReadingsView::from_status(&retrying(Duration::from_secs(1), Some(reading(false))));
        assert_eq!(view, ReadingsView::NoDaemon { retry_in_s: 1 });
    }

    #[test]
    fn connecting_initially() {
        let view = ReadingsView::from_status(&SensorStatus::default());
        assert_eq!(view, ReadingsView::Connecting);
        assert_eq!(view.temperature_c(), None);
    }

    #[test]
    fn awaiting_first_reading_once_connected() {
        let status = SensorStatus {
            link: LinkState::Connected,
            latest: None,
        };
        assert_eq!(
            ReadingsView::from_status(&status),
            ReadingsView::AwaitingReading
        );
    }
}
