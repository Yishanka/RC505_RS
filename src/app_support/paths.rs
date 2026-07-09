use std::path::{Path, PathBuf};

fn appdata_root() -> Option<PathBuf> {
    std::env::var("APPDATA")
        .ok()
        .map(|appdata| Path::new(&appdata).join("rc505_rs"))
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
