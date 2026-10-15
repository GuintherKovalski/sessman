mod app;
mod diffview;
mod goal;
mod history;
mod names;
mod runner;
mod session;
mod status;
mod workspaces;

fn main() -> eframe::Result {
    if std::env::args().nth(1).as_deref() == Some("statusline") {
        status::run();
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1400.0, 850.0]),
        ..Default::default()
    };
    eframe::run_native("sessman", options, Box::new(|_cc| Ok(Box::new(app::App::new()))))
}
