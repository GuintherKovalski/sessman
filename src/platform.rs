//! What differs between macOS and Windows, in one place: the home folder, opening files and
//! links, finding and starting `claude`, listing and stopping processes, and the console a
//! `scope` command prints to. Everything else in scope is the same on both.
//!
//! macOS (and other Unixes) keep what scope always did: `ps`, signals, process groups, `open`.
//! On Windows there is no `ps` and no signals: processes are listed with `sysinfo`, a run is
//! stopped with `taskkill /T` (its whole tree), and every helper process is started without a
//! console window, which would otherwise flash up each time scope runs `claude` or `git`.
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// "Cmd" on macOS, "Ctrl" elsewhere: for shortcuts shown in the UI.
pub const CMD: &str = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };

/// What the file manager is called, for "Open in …" buttons.
pub const FILE_MANAGER: &str = if cfg!(target_os = "macos") {
    "Finder"
} else if cfg!(windows) {
    "Explorer"
} else {
    "file manager"
};

/// The user's home folder: `$HOME`, or on Windows `%USERPROFILE%` (where Claude Code keeps
/// `.claude`) when `HOME` isn't set.
pub fn home() -> PathBuf {
    home_from(|k| std::env::var_os(k))
}

fn home_from(var: impl Fn(&str) -> Option<std::ffi::OsString>) -> PathBuf {
    let nonempty = |k: &str| var(k).filter(|v| !v.is_empty());
    let found = if cfg!(windows) {
        nonempty("USERPROFILE").or_else(|| nonempty("HOME")).or_else(|| {
            let (drive, path) = (nonempty("HOMEDRIVE")?, nonempty("HOMEPATH")?);
            let mut p = drive;
            p.push(path);
            Some(p)
        })
    } else {
        nonempty("HOME")
    };
    found.map(PathBuf::from).unwrap_or_default()
}

/// [`home`] as a string, for code that joins paths as text.
pub fn home_string() -> String {
    home().to_string_lossy().into_owned()
}

/// Claude Code's folder: `~/.claude`, or `$CLAUDE_CONFIG_DIR` when set.
pub fn claude_dir() -> PathBuf {
    match std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        Some(d) => PathBuf::from(d),
        None => home().join(".claude"),
    }
}

/// The folder name Claude Code keeps a project's sessions under in `~/.claude/projects`: the
/// project's path with every character that isn't a letter or digit made `-`
/// (`/Users/me/app` → `-Users-me-app`, `C:\Users\me\app` → `C--Users-me-app`).
pub fn project_key(path: &str) -> String {
    path.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// A path as a shell command line gives it: on Windows with `/` instead of `\`, which Claude
/// Code's Bash tool (Git Bash) and `cmd` both take, and which survives being quoted.
pub fn shell_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    if cfg!(windows) { s.replace('\\', "/") } else { s.into_owned() }
}

/// The separator between PATH entries.
pub const PATH_SEP: char = if cfg!(windows) { ';' } else { ':' };

/// Opens a file, folder or link with what the system opens it with (Finder, Explorer, the
/// browser, the image viewer).
pub fn open(target: impl AsRef<OsStr>) {
    let target = target.as_ref();
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg(target);
        c
    };
    #[cfg(windows)]
    let mut cmd = {
        // `start` would take the first quoted argument as a window title, and `explorer`
        // can't open links with `&` in them; the URL protocol handler opens both.
        let mut c = Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler").arg(target);
        c
    };
    #[cfg(not(any(target_os = "macos", windows)))]
    let mut cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(target);
        c
    };
    hide_window(&mut cmd);
    let _ = cmd.stdin(std::process::Stdio::null()).spawn();
}

/// Starts `cmd` without a console window. Windows gives every console program (claude, git,
/// curl, taskkill) a window of its own when a GUI app starts it; nothing elsewhere.
pub fn hide_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;

/// Runs `cmd` as the leader of its own process group, so stopping it reaches what it starts
/// too (Unix: a new process group; Windows: a new group, no console window, and [`stop`] ends
/// its whole tree).
pub fn own_group(cmd: &mut Command) -> &mut Command {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }
    cmd
}

/// Starts `cmd` on its own, outliving scope (the restart after an update).
#[cfg(windows)]
pub fn detach(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS)
}

/// Whether `path` is a program that can be run: executable bits on Unix; on Windows a file
/// whose extension is in `PATHEXT` (`.exe`, `.cmd`, …).
pub fn is_program(path: &Path) -> bool {
    let Ok(meta) = path.metadata() else { return false };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy()).to_ascii_lowercase());
        ext.is_some_and(|e| program_extensions().contains(&e))
    }
}

/// The extensions Windows runs without being told the program (`PATHEXT`), lowercase.
pub fn program_extensions() -> Vec<String> {
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    pathext.split(';').map(str::trim).filter(|e| !e.is_empty()).map(str::to_ascii_lowercase).collect()
}

/// The names a bare program name can be found under: itself on Unix; on Windows itself when it
/// has an extension, then with each of [`program_extensions`] (`claude` → `claude.exe`,
/// `claude.cmd`, …).
pub fn program_names(bin: &str) -> Vec<String> {
    if !cfg!(windows) || Path::new(bin).extension().is_some() {
        return vec![bin.to_owned()];
    }
    program_extensions().iter().map(|e| format!("{bin}{e}")).collect()
}

/// Whether `bin` names a path (rather than a program to look up on PATH).
pub fn is_path(bin: &str) -> bool {
    bin.contains('/') || (cfg!(windows) && (bin.contains('\\') || bin.contains(':')))
}

// ---------------------------------------------------------------------------------------------
// Processes

/// The words of a command line as [`processes`] gives it: split at spaces, tabs and line
/// breaks, but not at the no-break spaces a Windows path with spaces keeps (see
/// `sys::command_line`).
pub fn words(line: &str) -> impl Iterator<Item = &str> {
    line.split([' ', '\t', '\n', '\r']).filter(|w| !w.is_empty())
}

/// One process, as `ps -o pid=,ppid=,pgid=,%cpu=,rss=,etime=,args=` gives it (processes.rs).
pub use crate::processes::Proc;

/// Every process on the machine, with its command line.
pub fn processes() -> Vec<Proc> {
    #[cfg(unix)]
    {
        use std::process::Stdio;
        let out = Command::new("ps").args(["-axww", "-o", "pid=,ppid=,pgid=,%cpu=,rss=,etime=,args="]).stdin(Stdio::null()).output();
        out.map(|o| crate::processes::parse_ps(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default()
    }
    #[cfg(not(unix))]
    {
        sys::processes()
    }
}

/// Whether process `pid` is still running.
pub fn alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let ok = unsafe { libc::kill(pid as i32, 0) } == 0;
        ok || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(not(unix))]
    {
        sys::alive(pid)
    }
}

/// How hard to stop a process.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stop {
    /// Like Ctrl+C in a terminal (Unix: SIGINT).
    Interrupt,
    /// Asked to end (Unix: SIGTERM).
    Terminate,
    /// Ended at once (Unix: SIGKILL).
    Kill,
}

/// Stops process `pid`; with `tree`, everything it started too (Unix: its process group when
/// it leads one; Windows: its tree of children).
///
/// Windows has no signals for programs without a window: the first two ask `taskkill` to end
/// it, which a console program like `claude` ignores, so a [`Stop::Kill`] follows them.
pub fn stop(pid: u32, how: Stop, tree: bool) {
    #[cfg(unix)]
    {
        let sig = match how {
            Stop::Interrupt => libc::SIGINT,
            Stop::Terminate => libc::SIGTERM,
            Stop::Kill => libc::SIGKILL,
        };
        let target = if tree && unsafe { libc::getpgid(pid as i32) } == pid as i32 { -(pid as i32) } else { pid as i32 };
        unsafe { libc::kill(target, sig) };
    }
    #[cfg(not(unix))]
    {
        let mut cmd = Command::new("taskkill");
        cmd.arg("/PID").arg(pid.to_string());
        if tree {
            cmd.arg("/T");
        }
        if how == Stop::Kill {
            cmd.arg("/F");
        }
        hide_window(&mut cmd);
        let _ = cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    }
}

/// Stops `pid` the gentle way, then (after `grace`, if `still` says it is still wanted) the
/// hard way. On Windows the gentle way doesn't reach console programs, so it goes straight to
/// the hard one.
pub fn stop_then_kill(pid: u32, gentle: Stop, tree: bool, grace: std::time::Duration, still: impl FnOnce() -> bool + Send + 'static) {
    if cfg!(windows) {
        std::thread::spawn(move || stop(pid, Stop::Kill, tree));
        return;
    }
    stop(pid, gentle, tree);
    std::thread::spawn(move || {
        std::thread::sleep(grace);
        if still() && alive(pid) {
            stop(pid, Stop::Kill, tree);
        }
    });
}

/// The Windows process list, through sysinfo. Built (and tested) on every platform so it is
/// checked wherever scope is developed; used on Windows only.
#[cfg(any(windows, test))]
pub(crate) mod sys {
    use super::Proc;
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

    fn kind() -> ProcessRefreshKind {
        ProcessRefreshKind::nothing().with_cpu().with_memory().with_cmd(UpdateKind::Always).with_exe(UpdateKind::OnlyIfNotSet)
    }

    /// Kept between calls: CPU use is measured from one refresh to the next.
    static SYSTEM: std::sync::Mutex<Option<System>> = std::sync::Mutex::new(None);

    pub fn processes() -> Vec<Proc> {
        let mut guard = SYSTEM.lock().unwrap_or_else(|e| e.into_inner());
        let sys = guard.get_or_insert_with(System::new);
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind());
        let mut out: Vec<Proc> = sys
            .processes()
            .iter()
            .map(|(pid, p)| {
                let args = command_line(p.cmd().iter().map(|a| a.to_string_lossy().into_owned()).collect(), || {
                    p.exe().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| p.name().to_string_lossy().into_owned())
                });
                Proc {
                    pid: pid.as_u32(),
                    ppid: p.parent().map_or(0, Pid::as_u32),
                    // No process groups to speak of: each process is its own.
                    pgid: pid.as_u32(),
                    cpu: p.cpu_usage(),
                    rss_kb: p.memory() / 1024,
                    elapsed: p.run_time(),
                    args,
                }
            })
            .collect();
        out.sort_by_key(|p| p.pid);
        out
    }

    /// A command line from its arguments, joined with spaces as `ps` shows one; just the
    /// program when the arguments can't be read (another user's process). A Windows path with
    /// spaces in it (`C:\Program Files\nodejs\node.exe`, a user folder with a space) keeps
    /// them as no-break spaces, so it stays one word for what reads the line word by word.
    pub fn command_line(args: Vec<String>, program: impl FnOnce() -> String) -> String {
        if args.is_empty() {
            return program();
        }
        let drive_path = |a: &str| {
            let b = a.as_bytes();
            b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/')
        };
        args.iter().map(|a| if drive_path(a) { a.replace(' ', "\u{a0}") } else { a.clone() }).collect::<Vec<_>>().join(" ")
    }

    pub fn alive(pid: u32) -> bool {
        let mut sys = System::new();
        let pid = Pid::from_u32(pid);
        sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing());
        sys.process(pid).is_some()
    }
}

// ---------------------------------------------------------------------------------------------
// Console

/// Before a `scope` command prints anything: on Windows scope is a windowed app, which gets no
/// console, so it borrows the one of the terminal it was run from. (Piped output, as Claude
/// Code's Bash tool reads it, needs nothing.)
pub fn attach_console() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn project_folders_are_named_like_claude_codes() {
        assert_eq!(project_key("/Users/me/my.app"), "-Users-me-my-app");
        assert_eq!(project_key(r"C:\Users\me\my_app"), "C--Users-me-my-app");
    }

    #[test]
    fn home_comes_from_the_environment() {
        let env = |pairs: &'static [(&'static str, &'static str)]| move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| OsString::from(v));
        if cfg!(windows) {
            assert_eq!(home_from(env(&[("USERPROFILE", r"C:\Users\me"), ("HOME", "/x")])), PathBuf::from(r"C:\Users\me"));
            assert_eq!(home_from(env(&[("HOMEDRIVE", "C:"), ("HOMEPATH", r"\Users\me")])), PathBuf::from(r"C:\Users\me"));
        } else {
            assert_eq!(home_from(env(&[("HOME", "/Users/me"), ("USERPROFILE", "x")])), PathBuf::from("/Users/me"));
        }
        assert_eq!(home_from(env(&[])), PathBuf::new());
    }

    #[test]
    fn paths_and_programs() {
        assert!(is_path("/usr/local/bin/claude") && is_path("~/bin/claude") && !is_path("claude"));
        assert_eq!(is_path(r"C:\tools\claude.exe"), cfg!(windows));
        assert_eq!(program_names("claude.exe"), vec!["claude.exe".to_owned()]);
        if !cfg!(windows) {
            assert_eq!(program_names("claude"), vec!["claude".to_owned()]);
        }
    }

    #[test]
    fn this_process_is_listed_with_its_parent_and_command_line() {
        let me = std::process::id();
        let procs = sys::processes();
        let p = procs.iter().find(|p| p.pid == me).expect("this test process is listed");
        assert!(p.ppid != 0 && p.rss_kb > 0, "{p:?}");
        assert!(!p.args.is_empty());
        assert!(sys::alive(me));
        assert!(!sys::alive(u32::MAX - 7));
    }

    #[test]
    fn command_lines_keep_windows_paths_whole() {
        let args = vec![r"C:\Program Files\nodejs\node.exe".into(), r"C:\Users\Jo Smith\npm\claude-code\cli.js".into(), "-p".into(), "say hi".into()];
        let line = sys::command_line(args, String::new);
        assert_eq!(line, "C:\\Program\u{a0}Files\\nodejs\\node.exe C:\\Users\\Jo\u{a0}Smith\\npm\\claude-code\\cli.js -p say hi");
        assert_eq!(words(&line).count(), 5, "a path with spaces is one word");
        assert_eq!(sys::command_line(Vec::new(), || "claude.exe".into()), "claude.exe");
    }
}
