fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = pish_core::Config::load().unwrap_or_else(|e| {
        log::error!("{e} — continuing with defaults (no MQTT)");
        pish_core::Config::default()
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 600.0])
            .with_decorations(false)
            .with_resizable(false)
            .with_title("Pish"),
        ..Default::default()
    };
    eframe::run_native(
        "Pish",
        options,
        Box::new(|_cc| Ok(Box::new(pish_shell::App::new(&config)))),
    )
}
