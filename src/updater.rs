use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
#[derive(Clone, Serialize, Deserialize)]
pub struct Release {
    pub schema: u32,
    pub version: String,
    pub file: String,
    pub url: String,
    pub sha256: String,
}
fn script() -> Result<PathBuf> {
    let root = crate::app_support::paths::downloads_dir();
    std::fs::create_dir_all(&root)?;
    let path = root.join("rc505-update.ps1");
    let content = include_str!("../scripts/update.ps1").as_bytes();
    if std::fs::read(&path).ok().as_deref() != Some(content) {
        crate::project::atomic_write(&path, content)?;
    }
    Ok(path)
}
fn command() -> Result<Command> {
    #[cfg(not(windows))]
    let mut command = Command::new("powershell.exe");
    #[cfg(windows)]
    let mut command = {
        let powershell_directory = PathBuf::from(
            std::env::var_os("SystemRoot").context("Windows system directory unavailable")?,
        )
        .join("System32/WindowsPowerShell/v1.0");
        let mut command = Command::new(powershell_directory.join("powershell.exe"));
        // A pwsh parent can export PowerShell 7's PSModulePath, which prevents
        // Windows PowerShell from autoloading its own Get-FileHash function.
        command.env("PSModulePath", powershell_directory.join("Modules"));
        command
    };
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(script()?);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    Ok(command)
}
pub fn check() -> Result<Release> {
    let output = command()?.args(["-Mode", "Check"]).output()?;
    ensure!(
        output.status.success(),
        "Update check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}
pub fn newer(version: &str) -> bool {
    version_numbers(version)
        .zip(version_numbers(env!("CARGO_PKG_VERSION")))
        .is_some_and(|(latest, current)| latest > current)
}
fn version_numbers(value: &str) -> Option<[u64; 3]> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some([
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ])
}

/// Startup uses its own bounded receiver rather than the project's I/O job.
/// A slow/offline release server must not lock editing or the performance keys.
pub struct StartupCheck {
    receiver: Option<std::sync::mpsc::Receiver<Result<Release, String>>>,
    started: bool,
    enabled: bool,
    offline: bool,
    pub state: StartupState,
}
pub enum StartupState {
    Disabled,
    Offline,
    Waiting,
    Checking,
    Current,
    Available(String),
    Failed(String),
}
impl StartupCheck {
    pub fn running(&self) -> bool {
        self.receiver.is_some()
    }
    pub fn new(enabled: bool, offline: bool) -> Self {
        Self {
            receiver: None,
            started: false,
            enabled,
            offline,
            state: if offline {
                StartupState::Offline
            } else if enabled {
                StartupState::Waiting
            } else {
                StartupState::Disabled
            },
        }
    }
    pub fn set_enabled(&mut self, enabled: bool, offline: bool) {
        self.enabled = enabled;
        self.offline = offline;
        // Retain an in-flight receiver when disabled. Re-enabling joins that
        // same request instead of spawning another PowerShell/network worker.
        if self.receiver.is_none() {
            self.started = false;
        }
        self.state = if offline {
            StartupState::Offline
        } else if !enabled {
            StartupState::Disabled
        } else if self.receiver.is_some() {
            StartupState::Checking
        } else {
            StartupState::Waiting
        };
    }
    pub fn poll(&mut self) -> Option<Release> {
        if self.enabled
            && !self.offline
            && !self.started
            && matches!(self.state, StartupState::Waiting)
        {
            self.started = true;
            self.state = StartupState::Checking;
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            self.receiver = Some(rx);
            if let Err(error) = std::thread::Builder::new()
                .name("startup-update-check".into())
                .spawn(move || {
                    let _ = tx.send(check().map_err(|e| e.to_string()));
                })
            {
                self.receiver = None;
                self.state = StartupState::Failed(error.to_string());
            }
        }
        let result = match self.receiver.as_ref()?.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("Update checker stopped unexpectedly".into())
            }
        };
        self.receiver = None;
        if !self.enabled || self.offline {
            return None;
        }
        match result {
            Ok(release) => {
                self.state = if newer(&release.version) {
                    StartupState::Available(release.version.clone())
                } else {
                    StartupState::Current
                };
                Some(release)
            }
            Err(error) => {
                self.state = StartupState::Failed(error);
                None
            }
        }
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    #[test]
    fn strict_versions_do_not_turn_corrupt_versions_into_updates() {
        assert!(newer("999.0.0"));
        assert!(!newer(env!("CARGO_PKG_VERSION")));
        for bad in [
            "v999.0.0",
            "999.any.0",
            "999.0",
            "999.0.0-preview",
            "999.0.0.1",
            "",
        ] {
            assert!(!newer(bad));
        }
        assert!(version_numbers("1.10.0") > version_numbers("1.9.99"));
    }
    #[test]
    fn offline_and_disabled_checks_never_start_a_worker() {
        for (enabled, offline) in [(true, true), (false, true), (false, false)] {
            let mut check = StartupCheck::new(enabled, offline);
            for _ in 0..5 {
                assert!(check.poll().is_none());
            }
            assert!(!check.started);
            assert!(check.receiver.is_none());
        }
    }
    #[test]
    fn results_are_polled_once_and_failures_are_nonblocking() {
        let (mut check, (tx, rx)) = (
            StartupCheck::new(true, false),
            std::sync::mpsc::sync_channel(1),
        );
        check.started = true;
        check.state = StartupState::Checking;
        check.receiver = Some(rx);
        assert!(check.poll().is_none());
        tx.send(Ok(Release {
            schema: 1,
            version: "999.0.0".into(),
            file: String::new(),
            url: String::new(),
            sha256: String::new(),
        }))
        .unwrap();
        assert!(check.poll().is_some());
        assert!(matches!(check.state, StartupState::Available(_)));
        assert!(check.poll().is_none());
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        check.receiver = Some(rx);
        tx.send(Err("offline fixture".into())).unwrap();
        assert!(check.poll().is_none());
        assert!(matches!(check.state, StartupState::Failed(_)));
        assert!(check.poll().is_none());
    }
    #[test]
    fn switching_startup_checks_off_and_on_keeps_one_inflight_request() {
        let mut check = StartupCheck::new(true, false);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        check.receiver = Some(rx);
        check.started = true;
        for _ in 0..10 {
            check.set_enabled(false, false);
            assert!(check.receiver.is_some());
            check.set_enabled(true, false);
            assert!(check.started);
            assert!(check.poll().is_none());
        }
        check.set_enabled(false, false);
        tx.send(Err("ignored disabled check".into())).unwrap();
        assert!(check.poll().is_none());
        assert!(matches!(check.state, StartupState::Disabled));
        assert!(check.receiver.is_none());
    }
}
pub fn download() -> Result<PathBuf> {
    let output = command()?
        .args(["-Mode", "Download", "-DownloadDir"])
        .arg(crate::app_support::paths::downloads_dir())
        .output()?;
    ensure!(
        output.status.success(),
        "Update download failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    Ok(PathBuf::from(
        result["installer"]
            .as_str()
            .context("Missing installer path")?,
    ))
}
pub fn install_after_exit(installer: &Path) -> Result<()> {
    ensure!(
        crate::app_support::paths::installed_settings().is_some(),
        "Use an installed or portable release to install updates; development builds are left untouched"
    );
    let executable = std::env::current_exe()?;
    let directory = executable.parent().context("Missing install directory")?;
    let mut command = command()?;
    command
        .args(["-Mode", "Install", "-Installer"])
        .arg(installer)
        .arg("-InstallDir")
        .arg(directory)
        .arg("-DataDir")
        .arg(std::path::absolute(
            crate::app_support::paths::appdata_root().context("Missing data directory")?,
        )?)
        .arg("-DownloadDir")
        .arg(crate::app_support::paths::downloads_dir())
        .arg("-WaitForProcess")
        .arg(std::process::id().to_string());
    command.spawn()?;
    Ok(())
}

pub fn cleanup_installer_cache(installer: &Path) -> Result<()> {
    let raw = std::fs::read_to_string(format!("{}.verified.json", installer.display()))?;
    let metadata: Release = serde_json::from_str(raw.trim_start_matches('\u{feff}'))?;
    ensure!(
        metadata.version == env!("CARGO_PKG_VERSION"),
        "Only the successfully installed current version can finalize its cache"
    );
    let output = command()?
        .args(["-Mode", "Cleanup", "-Installer"])
        .arg(installer)
        .arg("-DownloadDir")
        .arg(crate::app_support::paths::downloads_dir())
        .output()?;
    ensure!(
        output.status.success(),
        "Installer cache cleanup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
