//! Updates from GitHub releases. One check runs in the background at start-up (and on Help ▸ Check for Updates…).
//! A Windows MSI install and a Linux AppImage download the new version in the background and switch to it on the
//! next start; any other install shows the release page.
//!
//! Release assets are named `instaoptics-<version>-<os>-<arch>.<ext>`, e.g. `instaoptics-0.2.0-windows-x64.msi`.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

const RELEASES: &str = "https://api.github.com/repos/Oneptica/instaOptics/releases/latest";
const PAGE: &str = "https://github.com/Oneptica/instaOptics/releases/latest";
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    /// A newer version this install cannot replace by itself: shown with the release page.
    Available(String),
    Downloading(String),
    /// Downloaded; installs when the app closes (Windows) or is already in place (AppImage).
    Ready(String),
    Failed(String),
}

/// How this copy was installed, which decides whether it can update itself.
enum Install {
    /// Installed from the MSI: the new MSI runs after exit and upgrades in place.
    Msi,
    /// Running as an AppImage: the new file replaces it (allowed while it runs).
    AppImage(PathBuf),
    Other,
}

fn install_kind() -> Install {
    if let Some(path) = std::env::var_os("APPIMAGE") {
        return Install::AppImage(PathBuf::from(path));
    }
    // The MSI installs into Program Files or the user's Programs folder; a copy run from anywhere else (a build, an
    // unzipped archive) is left alone.
    if cfg!(windows)
        && let Ok(exe) = std::env::current_exe()
    {
        let text = exe.to_string_lossy().to_lowercase();
        if text.contains("\\program files") || text.contains("\\appdata\\local\\programs\\") {
            return Install::Msi;
        }
    }
    Install::Other
}

fn asset_suffix() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some("-windows-x64.msi")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("-linux-x86_64.AppImage")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("-linux-aarch64.AppImage")
    } else {
        None
    }
}

fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    Some((parts.next()??, parts.next()??, parts.next().flatten().unwrap_or(0)))
}

fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse_version(latest), parse_version(current)), (Some(a), Some(b)) if a > b)
}

fn download_dir() -> PathBuf {
    std::env::temp_dir().join("instaoptics-update")
}

/// The check and, where this install can update itself, the download. Runs on a worker thread.
fn check(tx: std::sync::mpsc::Sender<State>, ctx: egui::Context) {
    let send = |s: State| {
        let _ = tx.send(s);
        ctx.request_repaint();
    };
    let release: serde_json::Value = match ureq::get(RELEASES).header("User-Agent", "instaOptics").call().and_then(|mut r| r.body_mut().read_json()) {
        Ok(v) => v,
        Err(e) => return send(State::Failed(e.to_string())),
    };
    let tag = release["tag_name"].as_str().unwrap_or_default().trim_start_matches('v').to_string();
    if !is_newer(&tag, VERSION) {
        return send(State::UpToDate);
    }
    let asset = asset_suffix().and_then(|suffix| {
        let name = format!("instaoptics-{tag}{suffix}");
        release["assets"].as_array()?.iter().find(|a| a["name"].as_str() == Some(name.as_str()))?["browser_download_url"].as_str().map(|u| (name, u.to_string()))
    });
    let install = install_kind();
    let (Some((name, url)), false) = (asset, matches!(install, Install::Other)) else {
        return send(State::Available(tag));
    };
    send(State::Downloading(tag.clone()));
    let dir = download_dir();
    let file = dir.join(&name);
    let result = std::fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|_| {
        let mut response = ureq::get(&url).header("User-Agent", "instaOptics").call().map_err(|e| e.to_string())?;
        let mut out = std::fs::File::create(&file).map_err(|e| e.to_string())?;
        std::io::copy(&mut response.body_mut().as_reader(), &mut out).map_err(|e| e.to_string())?;
        Ok(())
    });
    if let Err(e) = result {
        return send(State::Failed(e));
    }
    if let Install::AppImage(current) = install {
        // A rename over the running AppImage is safe: the running copy keeps its open inode.
        let staged = current.with_extension("AppImage.new");
        let swapped = std::fs::copy(&file, &staged).map_err(|e| e.to_string()).and_then(|_| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
            }
            std::fs::rename(&staged, &current).map_err(|e| e.to_string())
        });
        if let Err(e) = swapped {
            return send(State::Failed(e));
        }
    }
    send(State::Ready(tag));
}

pub struct Updates {
    pub state: State,
    rx: Option<Receiver<State>>,
    started: bool,
}

impl Default for Updates {
    fn default() -> Self {
        // Development builds check only when asked.
        Self { state: State::Idle, rx: None, started: cfg!(debug_assertions) }
    }
}

impl Updates {
    pub fn check_now(&mut self, ctx: &egui::Context) {
        if matches!(self.state, State::Checking | State::Downloading(_) | State::Ready(_)) {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        self.state = State::Checking;
        self.rx = Some(rx);
        std::thread::spawn(move || check(tx, ctx));
    }

    pub fn frame(&mut self, ctx: &egui::Context) {
        if !std::mem::replace(&mut self.started, true) {
            self.check_now(ctx);
        }
        if let Some(rx) = &self.rx {
            while let Ok(s) = rx.try_recv() {
                self.state = s;
            }
        }
    }

    /// The status bar entry: text, whether it is an error, and the page to open on click.
    pub fn status(&self) -> Option<(String, bool, Option<&'static str>)> {
        Some(match &self.state {
            State::Idle => return None,
            State::Checking => ("Checking for updates…".into(), false, None),
            State::UpToDate => (format!("instaOptics {VERSION} is up to date"), false, None),
            State::Available(v) => (format!("instaOptics {v} is available"), false, Some(PAGE)),
            State::Downloading(v) => (format!("Downloading instaOptics {v}…"), false, None),
            State::Ready(v) => (format!("instaOptics {v} installs when you restart"), false, None),
            State::Failed(e) => (format!("Update check failed: {e}"), true, None),
        })
    }

    /// Starts the downloaded MSI once the app has exited; it upgrades the install and the next start runs the new
    /// version.
    pub fn on_exit(&mut self) {
        let State::Ready(tag) = &self.state else { return };
        if !matches!(install_kind(), Install::Msi) {
            return;
        }
        if let Some(suffix) = asset_suffix() {
            let file = download_dir().join(format!("instaoptics-{tag}{suffix}"));
            let _ = std::process::Command::new("msiexec").arg("/i").arg(file).arg("/passive").arg("/norestart").spawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.2.1", "0.2.0"));
        assert!(is_newer("v1.0", "0.9.9"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.8", "0.2.0"));
        assert!(!is_newer("garbage", "0.2.0"));
    }
}
