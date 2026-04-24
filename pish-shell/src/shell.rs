use std::time::Duration;
use chrono::{DateTime, Local};
use pish_core::Services;

pub struct App {
    services: Services,
    home: crate::home::HomeScreen,
}

impl App {
    pub fn new() -> Self {
        Self {
            services: Services::default(),
            home: crate::home::HomeScreen::new(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Local::now();
        self.services.clock.time = clock_time(&now);
        self.services.clock.date = clock_date(&now);

        egui::CentralPanel::default().show(ctx, |ui| {
            self.home.show(ui, &self.services);
        });

        // Repaint often enough that the minute flip is never visibly late.
        ctx.request_repaint_after(Duration::from_millis(500));
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
