use crate::config::config_type::{ConfigSet, EnumConfig};
use cpal::traits::{DeviceTrait, HostTrait};

pub struct SystemConfigs {
    pub follow_system_output: bool,
    pub sel_idx: Option<usize>,
    pub input_device: EnumConfig<String>,
    pub output_device: EnumConfig<String>,
}

impl SystemConfigs {
    /// Pure defaults: loading presets and offline replay must never touch a driver.
    pub fn new() -> Self {
        Self {
            follow_system_output: true,
            input_device: EnumConfig::new("Input Device", String::new(), Vec::new()),
            output_device: EnumConfig::new("Output Device", String::new(), Vec::new()),
            sel_idx: Some(0),
        }
    }
    /// Called explicitly by the interactive application's device setup path.
    pub fn refresh(&mut self) {
        let host = cpal::default_host();
        self.input_device.options = host
            .input_devices()
            .into_iter()
            .flatten()
            .filter_map(|d| d.name().ok())
            .collect();
        self.output_device.options = host
            .output_devices()
            .into_iter()
            .flatten()
            .filter_map(|d| d.name().ok())
            .collect();
        if self.input_device.value.is_empty() {
            self.input_device.value = host
                .default_input_device()
                .and_then(|d| d.name().ok())
                .unwrap_or_default();
        }
        if self.follow_system_output || self.output_device.value.is_empty() {
            self.output_device.value = host
                .default_output_device()
                .and_then(|d| d.name().ok())
                .unwrap_or_default();
        }
    }
}

impl ConfigSet for SystemConfigs {
    fn next(&mut self) {
        if self.sel_idx.is_none() {
            self.sel_idx = Some(0);
        } else {
            self.sel_idx = Some((self.sel_idx.unwrap() + 1) % 2);
        }
    }

    fn prev(&mut self) {
        if self.sel_idx.is_none() {
            self.sel_idx = Some(0);
        } else {
            self.sel_idx = Some((self.sel_idx.unwrap() + 1) % 2);
        }
    }

    fn confirm(&mut self) {
        match self.sel_idx {
            Some(0) => {
                self.input_device.value = self.input_device.confirm();
            }
            Some(1) => {
                self.output_device.value = self.output_device.confirm();
            }
            _ => {}
        }
        // self.sel_idx = None
    }
}
