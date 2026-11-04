mod app;
mod control;
mod diffview;
#[cfg(test)]
mod e2e;
mod dictation;
mod find;
mod goal;
mod history;
mod images;
mod names;
mod orchestration;
mod overview;
mod processes;
mod queue;
mod refine;
mod runner;
mod search;
mod markdown;
mod session;
mod settings;
mod shared;
mod state;
mod status;
mod theme;
mod update;
mod verdict;
mod workspaces;

fn main() -> eframe::Result {
    if std::env::args().nth(1).as_deref() == Some("statusline") {
        status::run();
        return Ok(());
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| control::is_command(a)) {
        std::process::exit(control::cli(&args));
    }
    settings::adopt_old_config();
    runner::fix_path();
    images::watch_paste();
    let options = eframe::NativeOptions {
        viewport: viewport(),
        // Window size and position, panel widths and scroll positions (egui's own memory).
        persistence_path: Some(settings::config_dir().join("window.ron")),
        ..Default::default()
    };
    eframe::run_native(
        "scope",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(&cc.egui_ctx)))),
    )
}

/// The window: content runs under a transparent title bar, so the app's own top bar holds the
/// macOS window buttons instead of them covering the panels.
fn viewport() -> eframe::egui::ViewportBuilder {
    eframe::egui::ViewportBuilder::default()
        .with_inner_size([1400.0, 850.0])
        .with_title("scope")
        // The same image scripts/bundle.sh puts in scope.app, so running and quit look alike.
        .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).expect("valid icon"))
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
