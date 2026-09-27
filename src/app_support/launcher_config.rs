use std::fs;

use serde::{Deserialize, Serialize};

const DEFAULT_LATENCY_COMP_MS: usize = 85;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LauncherConfig {
    #[serde(default)]
    pub language: super::language::Language,
    #[serde(default = "default_buffer")]
    pub buffer_frames: u32,
    pub input_device: String,
    pub output_device: String,
    pub latency_comp_ms: usize,
    #[serde(default)]
    pub last_project: String,
}

impl LauncherConfig {
    pub fn buffer_frames(&self) -> u32 {
        if [64, 128, 256, 512, 1024].contains(&self.buffer_frames) {
            self.buffer_frames
        } else {
            128
        }
    }
    pub fn latency_comp_ms(&self) -> usize {
        self.latency_comp_ms.min(500)
    }
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            language: super::language::Language::default(),
            buffer_frames: 128,
            input_device: String::new(),
            output_device: String::new(),
            latency_comp_ms: DEFAULT_LATENCY_COMP_MS,
            last_project: String::new(),
        }
    }
}

fn default_buffer() -> u32 {
    128
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
    use std::io::Write;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = fs::File::create(&temporary)?;
    file.write_all(raw.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, path)?;
    Ok(())
}
