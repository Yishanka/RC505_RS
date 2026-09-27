use std::path::{Path, PathBuf};

pub fn appdata_root() -> Option<PathBuf> {
    // Useful for portable workspaces and isolated UI/testing sessions.
    if let Some(path) =
        std::env::args().find_map(|arg| arg.strip_prefix("--data-dir=").map(PathBuf::from))
    {
        return Some(path);
    }
    if let Some(settings) = installed_settings() {
        return Some(settings.data_dir);
    }
    std::env::var("APPDATA")
        .ok()
        .map(|appdata| Path::new(&appdata).join("rc505_rs"))
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct InstalledSettings {
    pub data_dir: PathBuf,
    pub download_dir: PathBuf,
}
pub fn installed_settings() -> Option<InstalledSettings> {
    let executable = std::env::current_exe().ok()?;
    let root = executable.parent()?;
    let mut settings: InstalledSettings =
        serde_json::from_slice(&std::fs::read(root.join("install-settings.json")).ok()?).ok()?;
    if !settings.data_dir.is_absolute() {
        settings.data_dir = root.join(settings.data_dir);
    }
    if !settings.download_dir.is_absolute() {
        settings.download_dir = root.join(settings.download_dir);
    }
    Some(settings)
}
pub fn downloads_dir() -> PathBuf {
    installed_settings()
        .map(|v| v.download_dir)
        .unwrap_or_else(|| {
            appdata_root()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("updates")
        })
}

pub fn projects_dir() -> PathBuf {
    appdata_root()
        .map(|root| root.join("projects"))
        .unwrap_or_else(|| PathBuf::from("projects"))
}

pub fn launcher_config_path() -> PathBuf {
    appdata_root()
        .map(|root| root.join("launcher_config.json"))
        .unwrap_or_else(|| PathBuf::from("launcher_config.json"))
}
