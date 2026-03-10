use crate::config::config_type::{ConfigSet, EnumConfig};
use cpal::traits::{DeviceTrait, HostTrait};

pub struct SystemConfigs {
    pub sel_idx: Option<usize>,
    pub input_device: EnumConfig<String>,
    pub output_device: EnumConfig<String>,
}

impl SystemConfigs {
    pub fn new() -> Self {
        let host = cpal::default_host();

        let in_devices: Vec<String> = host
            .input_devices()
            .unwrap()
            .map(|d| d.name().unwrap_or_default())
            .collect();

        let out_devices: Vec<String> = host
            .output_devices()
            .unwrap()
            .map(|d| d.name().unwrap_or_default())
            .collect();

        let def_in = host
            .default_input_device()
            .and_then(|d| d.name().ok())
            .unwrap_or_default();
        let def_out = host
            .default_output_device()
            .and_then(|d| d.name().ok())
            .unwrap_or_default();

        Self {
            input_device: EnumConfig::new("Input Device", def_in, in_devices),
            output_device: EnumConfig::new("Output Device", def_out, out_devices),
            sel_idx: Some(0),
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
