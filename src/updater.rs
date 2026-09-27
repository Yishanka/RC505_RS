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
    crate::project::atomic_write(&path, include_str!("../scripts/update.ps1").as_bytes())?;
    Ok(path)
}
fn command() -> Result<Command> {
    let mut command = Command::new("powershell.exe");
    #[cfg(windows)]
    {
        let powershell_directory = PathBuf::from(
            std::env::var_os("SystemRoot").context("Windows system directory unavailable")?,
        )
        .join("System32/WindowsPowerShell/v1.0");
        command = Command::new(powershell_directory.join("powershell.exe"));
        // A pwsh parent can export PowerShell 7's PSModulePath, which prevents
        // Windows PowerShell from autoloading its own Get-FileHash function.
        command.env("PSModulePath", powershell_directory.join("Modules"));
    }
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
    let parse =
        |value: &str| -> Vec<u32> { value.split('.').filter_map(|n| n.parse().ok()).collect() };
    parse(version) > parse(env!("CARGO_PKG_VERSION"))
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
