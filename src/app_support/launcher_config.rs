use std::fs;

use serde::{Deserialize, Serialize};

const DEFAULT_BPM: usize = 120;
const DEFAULT_LATENCY_COMP_MS: usize = 85;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LauncherConfig {
    pub input_device: String,
    pub output_device: String,
    pub default_bpm: usize,
    pub latency_comp_ms: usize,
    #[serde(default)]
    pub last_project: String,
}

impl LauncherConfig {
    pub fn bpm(&self) -> usize {
        self.default_bpm.clamp(30, 300)
    }

    pub fn latency_comp_ms(&self) -> usize {
        self.latency_comp_ms.min(500)
    }
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            input_device: String::new(),
            output_device: String::new(),
            default_bpm: DEFAULT_BPM,
            latency_comp_ms: DEFAULT_LATENCY_COMP_MS,
            last_project: String::new(),
        }
    }
}

pub fn load() -> Option<LauncherConfig> {
    let path = crate::app_support::paths::launcher_config_path();
    if !path.exists() {
        return None;
    }
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

#[allow(dead_code)]
pub fn save(config: &LauncherConfig) -> anyhow::Result<()> {
    let path = crate::app_support::paths::launcher_config_path();
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(config)?;
    fs::write(path, raw)?;
    Ok(())
}
