//! Updates: scope looks for a newer release on GitHub (at start, then twice a day) and, when
//! it runs as an installed scope.app, replaces itself with it and restarts on one click.
//!
//! Releases are made by `.github/workflows/release.yml` from a `v*` tag. Each has
//! `scope-macos.zip` (the signed, notarized app, which this downloads) and
//! `scope-<version>.dmg` (for people installing by hand), and for Windows
//! `scope-windows-x64.zip` (`scope.exe`) with its checksum in `SHA256SUMS`.
//!
//! On Windows the installed scope is the `scope.exe` that install.ps1 put in
//! `%LOCALAPPDATA%\Programs\scope`. A running program can't be overwritten there but can be
//! renamed: the update moves it aside to `scope.exe.old` (removed at the next start) and puts
//! the new one in its place.
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
    /// The release's `SHA256SUMS` asset, when it has one: Windows builds aren't signed, so the
    /// download is checked against it.
    pub sums_url: Option<String>,
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

/// The end of the name of the zip this platform installs from.
pub const ASSET: &str = if cfg!(windows) { "-windows-x64.zip" } else { "-macos.zip" };

/// The release in GitHub's `releases/latest` reply, if it has this platform's zip ([`ASSET`]).
pub fn parse_release(json: &str) -> Option<Release> {
    parse_release_for(json, ASSET)
}

fn parse_release_for(json: &str, asset: &str) -> Option<Release> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let version = v["tag_name"].as_str()?.trim_start_matches('v').to_owned();
    let assets: Vec<(&str, &str)> = v["assets"].as_array()?.iter().filter_map(|a| Some((a["name"].as_str()?, a["browser_download_url"].as_str()?))).collect();
    let zip_url = assets.iter().find(|(name, _)| name.ends_with(asset))?.1.to_owned();
    let sums_url = assets.iter().find(|(name, _)| *name == "SHA256SUMS").map(|(_, url)| (*url).to_owned());
    let page = v["html_url"].as_str().unwrap_or_default().to_owned();
    Some(Release { version, zip_url, sums_url, page })
}

/// The scope.app this process runs from (`…/scope.app/Contents/MacOS/scope`), if any; on
/// Windows the installed `scope.exe` (not one under a cargo `target` folder).
pub fn running_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if cfg!(windows) { installed_exe(&exe) } else { bundle_of(&exe.canonicalize().ok()?) }
}

/// `exe` when it is an installed `scope.exe`: one cargo built (under `target\`) can't update.
fn installed_exe(exe: &Path) -> Option<PathBuf> {
    let named = exe.file_name()?.to_string_lossy().eq_ignore_ascii_case("scope.exe");
    let built = exe.components().any(|c| c.as_os_str().eq_ignore_ascii_case("target"));
    (named && !built).then(|| exe.to_path_buf())
}

/// Removes the `scope.exe.old` a Windows update left beside the new one (it was still running
/// then). Call at start.
pub fn clean_up_old() {
    if let Some(exe) = running_bundle().filter(|_| cfg!(windows)) {
        let _ = std::fs::remove_file(old_exe(&exe));
    }
}

fn old_exe(exe: &Path) -> PathBuf {
    exe.with_extension("exe.old")
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
    crate::platform::hide_window(&mut c);
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
#[cfg(not(windows))]
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

/// The SHA-256 of `bytes`, in hex.
#[cfg(any(windows, test))]
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// The checksum `SHA256SUMS` (`<hex>  <name>` lines) gives for `name`.
#[cfg(any(windows, test))]
fn checksum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut parts = l.split_whitespace();
        let (sum, file) = (parts.next()?, parts.next()?.trim_start_matches('*'));
        (file == name).then(|| sum.to_ascii_lowercase())
    })
}

/// Downloads `release` and, once its checksum matches the release's `SHA256SUMS`, puts its
/// `scope.exe` in place of `exe`, the running one moved aside to `scope.exe.old`.
#[cfg(windows)]
fn install(release: &Release, exe: &Path) -> Result<(), String> {
    let parent = exe.parent().ok_or("scope has no folder")?;
    let work = parent.join(format!(".scope-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| {
        format!("can't write to {} ({e}); download the update from the release page instead", parent.display())
    })?;
    let result = (|| {
        let zip = work.join("scope.zip");
        run(curl().arg("--output").arg(&zip).arg(&release.zip_url), "download the update")?;
        let sums_url = release.sums_url.as_deref().ok_or("the release has no SHA256SUMS to check the download against")?;
        let sums = work.join("SHA256SUMS");
        run(curl().arg("--output").arg(&sums).arg(sums_url), "download the checksums")?;
        let name = release.zip_url.rsplit('/').next().unwrap_or_default();
        let want = checksum_for(&std::fs::read_to_string(&sums).unwrap_or_default(), name).ok_or("the checksums don't list the download")?;
        let have = sha256_hex(&std::fs::read(&zip).map_err(|e| e.to_string())?);
        if want != have {
            return Err("the download doesn't match its checksum; not installed".into());
        }
        // Windows 10 and later have bsdtar, which unpacks zips.
        let mut tar = std::process::Command::new("tar");
        crate::platform::hide_window(&mut tar);
        run(tar.arg("-xf").arg(&zip).arg("-C").arg(&work), "unpack the update")?;
        let new = work.join("scope.exe");
        if !new.is_file() {
            return Err("the download has no scope.exe in it".into());
        }
        let old = old_exe(exe);
        let _ = std::fs::remove_file(&old);
        std::fs::rename(exe, &old).map_err(|e| format!("can't move the old scope aside: {e}"))?;
        if let Err(e) = std::fs::rename(&new, exe) {
            let _ = std::fs::rename(&old, exe);
            return Err(format!("can't put the new scope in place: {e}"));
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// Downloads `release`, checks its signature and puts it in place of `app`. The running process
/// goes on from the old files (they stay open) until it restarts.
#[cfg(not(windows))]
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

/// Starts `exe` once this process has exited (so two scopes never run at once).
#[cfg(windows)]
pub fn relaunch_after_exit(exe: &Path) {
    let script = format!(
        "Wait-Process -Id {} -ErrorAction SilentlyContinue; Start-Process -FilePath '{}'",
        std::process::id(),
        exe.display().to_string().replace('\'', "''")
    );
    let mut cmd = std::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script]);
    crate::platform::detach(&mut cmd);
    let _ = cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}

/// Opens `app` once this process has exited (so two scopes never run at once).
#[cfg(not(windows))]
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
            let from = if cfg!(windows) { "an installed scope.exe" } else { "scope.app" };
            self.state = State::Failed(format!("scope isn't running from {from}; download the update from the release page"));
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
        let r = parse_release_for(json, "-macos.zip").unwrap();
        assert_eq!(r.version, "0.2.0");
        assert_eq!(r.zip_url, "https://x/scope-0.2.0-macos.zip");
        assert_eq!(r.page, "https://github.com/o/r/releases/tag/v0.2.0");
        assert_eq!(parse_release_for(&json.replace("-macos.zip", ".tar.gz"), "-macos.zip"), None);
        assert_eq!(parse_release_for(&json.replace(r#""prerelease": false"#, r#""prerelease": true"#), "-macos.zip"), None);
        assert_eq!(parse_release("not json"), None);
    }

    #[test]
    fn windows_takes_its_zip_and_the_checksums() {
        let json = r#"{
            "tag_name": "v0.3.0", "draft": false, "prerelease": false, "html_url": "p",
            "assets": [
                {"name": "scope-macos.zip", "browser_download_url": "https://x/scope-macos.zip"},
                {"name": "scope-windows-x64.zip", "browser_download_url": "https://x/scope-windows-x64.zip"},
                {"name": "SHA256SUMS", "browser_download_url": "https://x/SHA256SUMS"}
            ]
        }"#;
        let r = parse_release_for(json, "-windows-x64.zip").unwrap();
        assert_eq!(r.zip_url, "https://x/scope-windows-x64.zip");
        assert_eq!(r.sums_url.as_deref(), Some("https://x/SHA256SUMS"));
        assert_eq!(parse_release_for(&json.replace("windows-x64", "linux"), "-windows-x64.zip"), None);
    }

    #[test]
    fn downloads_are_checked_against_sha256sums() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let sums = "ba7816BF  scope-macos.zip\n0123abcd *scope-windows-x64.zip\n";
        assert_eq!(checksum_for(sums, "scope-windows-x64.zip").as_deref(), Some("0123abcd"));
        assert_eq!(checksum_for(sums, "scope-macos.zip").as_deref(), Some("ba7816bf"));
        assert_eq!(checksum_for(sums, "other.zip"), None);
    }

    #[test]
    fn only_an_installed_scope_exe_updates_itself() {
        assert_eq!(installed_exe(Path::new("C:/Users/me/AppData/Local/Programs/scope/scope.exe")).is_some(), true);
        assert_eq!(installed_exe(Path::new("C:/src/sessman/target/release/scope.exe")), None);
        assert_eq!(installed_exe(Path::new("C:/tools/other.exe")), None);
        assert_eq!(old_exe(Path::new("C:/x/scope.exe")), PathBuf::from("C:/x/scope.exe.old"));
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
        let release = Release { version: "9.9.9".into(), zip_url: format!("file://{}", zip.display()), sums_url: None, page: String::new() };

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
