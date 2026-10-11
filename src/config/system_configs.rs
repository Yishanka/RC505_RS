pub use crate::app_support::device_scan::DeviceList;
use crate::config::config_type::{ConfigSet, EnumConfig};

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
    /// Apply a completed scan without touching any audio driver. A named input
    /// stays selected when temporarily unplugged; refresh cannot switch microphones.
    pub fn apply_devices(&mut self, devices: &DeviceList) {
        self.input_device.options = devices.inputs.clone();
        self.output_device.options = devices.outputs.clone();
        devices.apply_selection(
            &mut self.input_device.value,
            &mut self.output_device.value,
            self.follow_system_output,
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    fn names(input: &str, output: &str) -> DeviceList {
        DeviceList {
            inputs: vec![input.into()],
            outputs: vec![output.into()],
            default_input: Some(input.into()),
            default_output: Some(output.into()),
            warnings: Vec::new(),
        }
    }
    #[test]
    fn device_scan_applying_names_keeps_explicit_input_and_fixed_output_when_missing() {
        let mut config = SystemConfigs::new();
        assert!(config.input_device.value.is_empty() && config.input_device.options.is_empty());
        config.apply_devices(&names("First microphone", "First output"));
        assert_eq!(config.input_device.value, "First microphone");
        assert_eq!(config.output_device.value, "First output");
        config.apply_devices(&names("Second microphone", "Second output"));
        assert_eq!(
            config.input_device.value, "First microphone",
            "Refresh must not switch a selected microphone"
        );
        assert_eq!(config.input_device.options, vec!["Second microphone"]);
        assert_eq!(config.output_device.value, "Second output");
        config.follow_system_output = false;
        config.output_device.value = "Explicit headphones".into();
        config.apply_devices(&DeviceList::default());
        assert_eq!(config.input_device.value, "First microphone");
        assert_eq!(config.output_device.value, "Explicit headphones");
        assert!(config.input_device.options.is_empty() && config.output_device.options.is_empty());
        config.apply_devices(&names("First microphone", "Explicit headphones"));
        assert_eq!(config.input_device.value, "First microphone");
        assert_eq!(config.output_device.value, "Explicit headphones");
    }
    #[test]
    fn device_scan_initial_defaults_are_applied_only_when_available_and_payload_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<DeviceList>();
        let mut config = SystemConfigs::new();
        config.apply_devices(&DeviceList::default());
        assert!(config.input_device.value.is_empty());
        let list = names("Arrived microphone", "Arrived output");
        config.apply_devices(&list);
        assert_eq!(config.input_device.value, "Arrived microphone");
        config.follow_system_output = true;
        config.apply_devices(&DeviceList::default());
        assert!(config.output_device.value.is_empty());
        assert_eq!(config.input_device.value, "Arrived microphone");
    }
}
