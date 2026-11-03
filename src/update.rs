//! Updates: scope looks for a newer release on GitHub (at start, then twice a day) and, when
//! it runs as an installed scope.app, replaces itself with it and restarts on one click.
//!
//! Releases are made by `.github/workflows/release.yml` from a `v*` tag. Each has
//! `scope-<version>-macos.zip` (the signed, notarized app, which this downloads) and
//! `scope-<version>.dmg` (for people installing by hand).
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

/// `owner/name` of the GitHub repository the releases are published in. Set at build time with
/// `SCOPE_RELEASE_REPO`, so builds of forks look in their own.
pub const REPO: &str = match option_env!("SCOPE_RELEASE_REPO") {
    Some(r) => r,
    None => "GuintherKovalski/sessman",
};

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// How often to look again while scope stays open.
const EVERY: Duration = Duration::from_secs(12 * 60 * 60);

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    /// The `.zip` asset with the app in it.
    pub zip_url: String,
    /// The release's page, for its notes.
    pub page: String,
}

/// `1.2.3` (or `v1.2.3`) as numbers; anything after a `-` or `+` is ignored.
fn parse_version(v: &str) -> Option<Vec<u64>> {
    let v = v.trim().trim_start_matches('v');
    let core = v.split(['-', '+']).next()?;
    core.split('.').map(|p| p.parse().ok()).collect()
}

/// True if `latest` is a higher version than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(mut a), Some(mut b)) => {
            let n = a.len().max(b.len());
            a.resize(n, 0);
            b.resize(n, 0);
            a > b
        }
        _ => false,
    }
}

/// The release in GitHub's `releases/latest` reply, if it has the macOS zip.
pub fn parse_release(json: &str) -> Option<Release> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let version = v["tag_name"].as_str()?.trim_start_matches('v').to_owned();
    let zip_url = v["assets"]
        .as_array()?
        .iter()
        .filter_map(|a| Some((a["name"].as_str()?, a["browser_download_url"].as_str()?)))
        .find(|(name, _)| name.ends_with("-macos.zip"))?
        .1
        .to_owned();
    let page = v["html_url"].as_str().unwrap_or_default().to_owned();
    Some(Release { version, zip_url, page })
}

/// The scope.app this process runs from (`…/scope.app/Contents/MacOS/scope`), if any.
pub fn running_bundle() -> Option<PathBuf> {
    bundle_of(&std::env::current_exe().ok()?.canonicalize().ok()?)
}

fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    let ok = macos.file_name()? == "MacOS" && contents.file_name()? == "Contents" && app.extension()? == "app";
    ok.then(|| app.to_path_buf())
}

fn curl() -> std::process::Command {
    let mut c = std::process::Command::new("curl");
    c.args(["--location", "--fail", "--silent", "--show-error", "--max-time", "600"]);
    c.arg("--user-agent").arg(format!("scope/{CURRENT}"));
    c.stdin(std::process::Stdio::null());
    c
}

/// Asks GitHub for the latest release. `Ok(None)`: there is none (yet) with a macOS build.
fn fetch_latest() -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let out = curl()
        .args(["--max-time", "20", "--header", "Accept: application/vnd.github+json"])
        .arg(&url)
        .output()
        .map_err(|e| format!("can't start curl: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        // 404: no release published yet.
        if err.contains("404") {
            return Ok(None);
        }
        return Err(format!("couldn't reach GitHub: {}", err.trim()));
    }
    Ok(parse_release(&String::from_utf8_lossy(&out.stdout)))
}

/// The Team ID a bundle is signed with, if it has one (ad-hoc signatures don't).
fn team_id(app: &Path) -> Option<String> {
    let out = std::process::Command::new("codesign").arg("-dv").arg(app).output().ok()?;
    // codesign -d writes its report to stderr.
    let text = String::from_utf8_lossy(&out.stderr).into_owned() + &String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find_map(|l| l.strip_prefix("TeamIdentifier="))
        .map(str::trim)
        .filter(|t| !t.is_empty() && *t != "not set")
        .map(str::to_owned)
}

fn run(cmd: &mut std::process::Command, what: &str) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("can't {what}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("can't {what}: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// Downloads `release`, checks its signature and puts it in place of `app`. The running process
/// goes on from the old files (they stay open) until it restarts.
fn install(release: &Release, app: &Path) -> Result<(), String> {
    let parent = app.parent().ok_or("the app has no folder")?;
    // Next to the app, so the final move is a rename on the same volume.
    let work = parent.join(format!(".scope-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| {
        format!("can't write to {} ({e}); download the update from the release page instead", parent.display())
    })?;
    let result = (|| {
        let zip = work.join("scope.zip");
        run(curl().arg("--output").arg(&zip).arg(&release.zip_url), "download the update")?;
        run(std::process::Command::new("ditto").args(["-x", "-k"]).arg(&zip).arg(&work), "unpack the update")?;
        let new = work.join("scope.app");
        if !new.join("Contents/MacOS/scope").is_file() {
            return Err("the download has no scope.app in it".into());
        }
        run(
            std::process::Command::new("codesign").args(["--verify", "--deep", "--strict"]).arg(&new),
            "verify the update's signature",
        )?;
        // A signed app only takes updates signed by the same developer.
        if let Some(team) = team_id(app) {
            if team_id(&new).as_deref() != Some(team.as_str()) {
                return Err(format!("the update isn't signed by the same developer ({team}); not installed"));
            }
        }
        let old = work.join("old.app");
        std::fs::rename(app, &old).map_err(|e| format!("can't move the old app aside: {e}"))?;
        if let Err(e) = std::fs::rename(&new, app) {
            let _ = std::fs::rename(&old, app);
            return Err(format!("can't put the new app in place: {e}"));
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// Opens `app` once this process has exited (so two scopes never run at once).
pub fn relaunch_after_exit(app: &Path) {
    let script = format!("while kill -0 {} 2>/dev/null; do sleep 0.2; done; open \"$0\"", std::process::id());
    let _ = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .arg(app)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

#[derive(Debug, PartialEq)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Installing(Release),
    /// Installed; scope restarts into it.
    Ready(Release),
    Failed(String),
}

/// Checks for and installs updates in the background; the app polls it every frame.
pub struct Updater {
    pub state: State,
    pub auto_check: bool,
    last_check: Option<Instant>,
    check_rx: Option<Receiver<Result<Option<Release>, String>>>,
    install_rx: Option<Receiver<Result<(), String>>>,
    /// Set when an update has been installed and the app should quit to restart into it.
    pub restart: bool,
}

impl Updater {
    pub fn new(auto_check: bool) -> Self {
        Self { state: State::Idle, auto_check, last_check: None, check_rx: None, install_rx: None, restart: false }
    }

    /// The release that can be installed, if there is one.
    pub fn available(&self) -> Option<&Release> {
        match &self.state {
            State::Available(r) => Some(r),
            _ => None,
        }
    }

    pub fn check_now(&mut self, ctx: &eframe::egui::Context) {
        if self.check_rx.is_some() || matches!(self.state, State::Installing(_) | State::Ready(_)) {
            return;
        }
        self.last_check = Some(Instant::now());
        self.state = State::Checking;
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(fetch_latest());
            ctx.request_repaint();
        });
        self.check_rx = Some(rx);
    }

    pub fn install(&mut self, ctx: &eframe::egui::Context) {
        let State::Available(release) = &self.state else { return };
        let Some(app) = running_bundle() else {
            self.state = State::Failed("scope isn't running from scope.app; download the update from the release page".into());
            return;
        };
        let release = release.clone();
        self.state = State::Installing(release.clone());
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(install(&release, &app));
            ctx.request_repaint();
        });
        self.install_rx = Some(rx);
    }

    /// Takes finished checks and installs; starts the periodic check when it is due.
    pub fn poll(&mut self, ctx: &eframe::egui::Context) {
        if let Some(Ok(result)) = self.check_rx.as_ref().map(|rx| rx.try_recv()) {
            self.check_rx = None;
            self.state = match result {
                Ok(Some(r)) if is_newer(&r.version, CURRENT) => State::Available(r),
                Ok(_) => State::UpToDate,
                Err(e) => State::Failed(e),
            };
        }
        if let Some(Ok(result)) = self.install_rx.as_ref().map(|rx| rx.try_recv()) {
            self.install_rx = None;
            let State::Installing(release) = std::mem::replace(&mut self.state, State::Idle) else { return };
            match result {
                Ok(()) => {
                    if let Some(app) = running_bundle() {
                        relaunch_after_exit(&app);
                    }
                    self.state = State::Ready(release);
                    self.restart = true;
                }
                Err(e) => self.state = State::Failed(e),
            }
        }
        // Not in tests, nor for builds run with `cargo run` (they can't install anyway).
        let due = self.last_check.is_none_or(|t| t.elapsed() >= EVERY);
        if self.auto_check && due && !cfg!(test) && running_bundle().is_some() {
            self.check_now(ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("v0.10.0", "0.9.0"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(is_newer("0.1.1", "0.1"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
        assert!(is_newer("0.2.0-beta", "0.1.0"));
    }

    #[test]
    fn latest_release_needs_the_macos_zip() {
        let json = r#"{
            "tag_name": "v0.2.0", "draft": false, "prerelease": false,
            "html_url": "https://github.com/o/r/releases/tag/v0.2.0",
            "assets": [
                {"name": "scope-0.2.0.dmg", "browser_download_url": "https://x/scope-0.2.0.dmg"},
                {"name": "scope-0.2.0-macos.zip", "browser_download_url": "https://x/scope-0.2.0-macos.zip"}
            ]
        }"#;
        let r = parse_release(json).unwrap();
        assert_eq!(r.version, "0.2.0");
        assert_eq!(r.zip_url, "https://x/scope-0.2.0-macos.zip");
        assert_eq!(r.page, "https://github.com/o/r/releases/tag/v0.2.0");
        assert_eq!(parse_release(&json.replace("-macos.zip", ".tar.gz")), None);
        assert_eq!(parse_release(&json.replace(r#""prerelease": false"#, r#""prerelease": true"#)), None);
        assert_eq!(parse_release("not json"), None);
    }

    #[test]
    fn bundle_is_found_from_the_executable() {
        assert_eq!(
            bundle_of(Path::new("/Applications/scope.app/Contents/MacOS/scope")),
            Some(PathBuf::from("/Applications/scope.app"))
        );
        assert_eq!(bundle_of(Path::new("/repo/target/release/scope")), None);
    }

    /// The whole install: download (a file:// URL here), unpack, verify, swap in place.
    #[cfg(target_os = "macos")]
    #[test]
    fn install_swaps_the_app_for_the_downloaded_one() {
        let root = std::env::temp_dir().join(format!("scope-update-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let make_app = |dir: &Path, body: &str| {
            let app = dir.join("scope.app");
            std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
            let exe = app.join("Contents/MacOS/scope");
            std::fs::write(&exe, format!("#!/bin/sh\necho {body}\n")).unwrap();
            std::fs::set_permissions(&exe, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
            app
        };
        let installed = make_app(&root.join("Applications"), "old");
        let new = make_app(&root.join("build"), "new");
        run(std::process::Command::new("codesign").args(["--force", "--sign", "-"]).arg(&new), "sign").unwrap();
        let zip = root.join("scope-9.9.9-macos.zip");
        run(std::process::Command::new("ditto").args(["-c", "-k", "--keepParent"]).arg(&new).arg(&zip), "zip").unwrap();
        let release = Release { version: "9.9.9".into(), zip_url: format!("file://{}", zip.display()), page: String::new() };

        install(&release, &installed).unwrap();
        let exe = std::fs::read_to_string(installed.join("Contents/MacOS/scope")).unwrap();
        assert!(exe.contains("new"), "the new app is in place");
        let leftovers: Vec<_> = std::fs::read_dir(root.join("Applications")).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("scope.app")], "nothing left beside it");

        // A download that isn't an app leaves the installed one alone.
        std::fs::write(&zip, "not a zip").unwrap();
        assert!(install(&release, &installed).is_err());
        assert!(installed.join("Contents/MacOS/scope").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_new_updater_waits_and_offers_nothing() {
        let u = Updater::new(true);
        assert_eq!(u.state, State::Idle);
        assert!(u.available().is_none());
        assert!(!u.restart);
    }
}
