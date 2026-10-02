use std::fs;

use serde::{Deserialize, Serialize};

const DEFAULT_LATENCY_COMP_MS: usize = 85;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LauncherConfig {
    #[serde(default)]
    pub theme: super::appearance::ThemeColor,
    #[serde(default)]
    pub calibration_guard: bool,
    #[serde(default = "default_follow_output")]
    pub visualizer_enabled: bool,
    /// Missing in older versions: migrate to following the system, not a stale device name.
    #[serde(default = "default_follow_output")]
    pub follow_system_output: bool,
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
            theme: Default::default(),
            calibration_guard: false,
            visualizer_enabled: true,
            follow_system_output: true,
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
fn default_follow_output() -> bool {
    true
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn older_fixed_device_names_migrate_to_following_without_brand_rules() {
        let old: LauncherConfig = serde_json::from_str(
            r#"{"input_device":"mic","output_device":"Old USB device","latency_comp_ms":0}"#,
        )
        .unwrap();
        assert!(old.follow_system_output);
        assert_eq!(old.theme, super::super::appearance::ThemeColor::Mint);
        assert!(!old.calibration_guard);
        assert!(old.visualizer_enabled);
        let mut fixed = old;
        fixed.follow_system_output = false;
        fixed.theme = super::super::appearance::ThemeColor::Rose;
        fixed.calibration_guard = true;
        fixed.visualizer_enabled = false;
        let restored: LauncherConfig =
            serde_json::from_str(&serde_json::to_string(&fixed).unwrap()).unwrap();
        assert!(!restored.follow_system_output);
        assert_eq!(restored.theme, super::super::appearance::ThemeColor::Rose);
        assert!(restored.calibration_guard);
        assert!(!restored.visualizer_enabled);
        assert_eq!(restored.output_device, "Old USB device");
    }
}
