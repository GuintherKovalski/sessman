mod app;
mod history;
mod names;
mod runner;
mod session;
mod workspaces;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1400.0, 850.0]),
        ..Default::default()
    };
    eframe::run_native("sessman", options, Box::new(|_cc| Ok(Box::new(app::App::new()))))
}
