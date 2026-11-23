mod app;
mod model;
mod scores;
mod settings;

fn main() -> eframe::Result<()> {
    let cheat = std::env::args().any(|a| a == "--cheat");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Bombicat")
            .with_min_inner_size([320.0, 280.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Bombicat",
        options,
        Box::new(move |cc| Ok(Box::new(app::BombicatApp::new(cc, cheat)))),
    )
}
