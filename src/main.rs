mod app;
mod diffview;
mod dictation;
mod find;
mod goal;
mod history;
mod names;
mod queue;
mod refine;
mod runner;
mod search;
mod markdown;
mod session;
mod settings;
mod state;
mod status;
mod theme;
mod workspaces;

fn main() -> eframe::Result {
    if std::env::args().nth(1).as_deref() == Some("statusline") {
        status::run();
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: viewport(),
        // Window size and position, panel widths and scroll positions (egui's own memory).
        persistence_path: Some(settings::config_dir().join("window.ron")),
        ..Default::default()
    };
    eframe::run_native(
        "sessman",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(&cc.egui_ctx)))),
    )
}

/// The window: content runs under a transparent title bar, so the app's own top bar holds the
/// macOS window buttons instead of them covering the panels.
fn viewport() -> eframe::egui::ViewportBuilder {
    eframe::egui::ViewportBuilder::default()
        .with_inner_size([1400.0, 850.0])
        .with_title("sessman")
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false)
}

#[cfg(test)]
mod tests {
    #[test]
    fn window_content_runs_under_the_title_bar() {
        let v = super::viewport();
        assert_eq!(v.fullsize_content_view, Some(true));
        assert_eq!(v.title_shown, Some(false));
        assert_eq!(v.titlebar_shown, Some(false));
        assert!(super::app::TITLE_BAR_HEIGHT >= 28.0, "room for the macOS window buttons");
    }
}
