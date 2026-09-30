//! Driver queries stay on this worker, never in the audio callback or GUI draw.
use cpal::traits::{DeviceTrait, HostTrait};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

pub struct DefaultOutput {
    pub id: String,
    pub name: String,
    pub device: cpal::Device,
}
pub struct OutputWatch {
    stop: Arc<AtomicBool>,
    rx: mpsc::Receiver<Result<DefaultOutput, String>>,
}
impl OutputWatch {
    pub fn new() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("system-output-watch".into())
            .spawn(move || {
                while !worker_stop.load(Ordering::Relaxed) {
                    let result = (|| -> anyhow::Result<DefaultOutput> {
                        let before = default_id()?;
                        let device = cpal::default_host()
                            .default_output_device()
                            .ok_or_else(|| anyhow::anyhow!("No system output device"))?;
                        let name = device.name()?;
                        let id = default_id()?;
                        anyhow::ensure!(before == id, "System output is changing; retrying");
                        Ok(DefaultOutput { id, name, device })
                    })()
                    .map_err(|e| e.to_string());
                    let _ = tx.try_send(result);
                    std::thread::sleep(Duration::from_millis(500));
                }
            })
            .expect("output watch thread");
        Self { stop, rx }
    }
    pub fn poll(&self) -> Option<Result<DefaultOutput, String>> {
        self.rx.try_recv().ok()
    }
}
impl Drop for OutputWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Windows endpoint IDs distinguish devices even when their friendly names match.
#[cfg(windows)]
pub fn default_id() -> anyhow::Result<String> {
    use windows::Win32::{
        Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender},
        System::Com::*,
    };
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let result = (|| -> anyhow::Result<String> {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let endpoint = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let id = endpoint.GetId()?;
            let text = id.to_string();
            CoTaskMemFree(Some(id.0.cast()));
            Ok(text?)
        })();
        if initialized {
            CoUninitialize();
        }
        result
    }
}
#[cfg(not(windows))]
pub fn default_id() -> anyhow::Result<String> {
    Ok(cpal::default_host()
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("No system output device"))?
        .name()?)
}
