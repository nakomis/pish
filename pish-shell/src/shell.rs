use pish_core::Services;

/// Top-level eframe application. Owns the home screen and all widget instances.
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
        // TODO: update services (wifi, clock, brightness) from system
        egui::CentralPanel::default().show(ctx, |ui| {
            self.home.show(ui, &self.services);
        });
    }
}
