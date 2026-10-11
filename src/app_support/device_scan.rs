//! One bounded, reusable device-enumeration worker per interactive app instance.
//! Timeout reports a stalled driver; it never starts a replacement query/thread.
/// Driver-independent names shared by the main app and audio preflight.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceList {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub default_input: Option<String>,
    pub default_output: Option<String>,
    pub warnings: Vec<String>,
}
impl DeviceList {
    pub fn apply_selection(&self, input: &mut String, output: &mut String, follow_output: bool) {
        if input.is_empty() {
            *input = self.default_input.clone().unwrap_or_default();
        }
        if follow_output || output.is_empty() {
            *output = self.default_output.clone().unwrap_or_default();
        }
    }
}
use cpal::traits::{DeviceTrait, HostTrait};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(5);
type ScanResult = Result<DeviceList, String>;

pub struct DeviceScan {
    requests: SyncSender<()>,
    results: Receiver<ScanResult>,
    started: Option<Instant>,
    timeout_reported: bool,
    dead: bool,
    pending_error: Option<String>,
}
impl DeviceScan {
    pub fn new() -> Self {
        Self::with_scan(enumerate)
    }
    fn with_scan(mut scan: impl FnMut() -> ScanResult + Send + 'static) -> Self {
        let (requests, receive_requests) = mpsc::sync_channel(1);
        let (send_results, results) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("audio-device-scan".into())
            .spawn(move || {
                while receive_requests.recv().is_ok() {
                    let result = catch_unwind(AssertUnwindSafe(&mut scan))
                        .unwrap_or_else(|_| Err("Audio device scan failed unexpectedly.".into()));
                    if send_results.send(result).is_err() {
                        break;
                    }
                }
            });
        // Dropping the handle intentionally never joins a potentially stuck driver.
        // Closing the channels lets this one worker exit when/if the query returns.
        let pending_error = worker
            .err()
            .map(|error| format!("Cannot start audio device scan: {error}"));
        Self {
            requests,
            results,
            started: None,
            timeout_reported: false,
            dead: pending_error.is_some(),
            pending_error,
        }
    }
    pub fn request(&mut self) -> bool {
        self.request_at(Instant::now())
    }
    fn request_at(&mut self, now: Instant) -> bool {
        if self.dead || self.started.is_some() {
            return false;
        }
        match self.requests.try_send(()) {
            Ok(()) => {
                self.started = Some(now);
                self.timeout_reported = false;
                true
            }
            Err(TrySendError::Full(_)) => false,
            Err(TrySendError::Disconnected(_)) => {
                self.dead = true;
                self.pending_error = Some("Audio device scan worker is unavailable.".into());
                false
            }
        }
    }
    /// Remains true after timeout until the existing request actually completes.
    pub fn pending(&self) -> bool {
        self.started.is_some()
    }
    pub fn timed_out(&self) -> bool {
        self.pending() && self.timeout_reported
    }
    pub fn poll(&mut self) -> Option<ScanResult> {
        self.poll_at(Instant::now())
    }
    fn poll_at(&mut self, now: Instant) -> Option<ScanResult> {
        if let Some(error) = self.pending_error.take() {
            return Some(Err(error));
        }
        if self.dead {
            return None;
        }
        match self.results.try_recv() {
            Ok(result) => {
                self.started = None;
                self.timeout_reported = false;
                return Some(result);
            }
            Err(TryRecvError::Disconnected) => {
                self.dead = true;
                self.started = None;
                return Some(Err("Audio device scan worker is unavailable.".into()));
            }
            Err(TryRecvError::Empty) => {}
        }
        if !self.timeout_reported
            && self
                .started
                .is_some_and(|start| now.saturating_duration_since(start) >= TIMEOUT)
        {
            self.timeout_reported = true;
            return Some(Err(
                "Device scan timed out; the driver is still busy.".into()
            ));
        }
        None
    }
}

fn enumerate() -> ScanResult {
    // Host and every Device are created and dropped solely on this worker.
    let host = cpal::default_host();
    let mut result = DeviceList::default();
    let inputs = host
        .input_devices()
        .map_err(|e| format!("Input device enumeration failed: {e}"))?;
    let outputs = host
        .output_devices()
        .map_err(|e| format!("Output device enumeration failed: {e}"))?;
    for device in inputs {
        match device.name() {
            Ok(name) => result.inputs.push(name),
            Err(error) => result
                .warnings
                .push(format!("Input device name unavailable: {error}")),
        }
    }
    for device in outputs {
        match device.name() {
            Ok(name) => result.outputs.push(name),
            Err(error) => result
                .warnings
                .push(format!("Output device name unavailable: {error}")),
        }
    }
    result.default_input = host
        .default_input_device()
        .and_then(|device| match device.name() {
            Ok(name) => Some(name),
            Err(error) => {
                result
                    .warnings
                    .push(format!("Default input name unavailable: {error}"));
                None
            }
        });
    result.default_output = host
        .default_output_device()
        .and_then(|device| match device.name() {
            Ok(name) => Some(name),
            Err(error) => {
                result
                    .warnings
                    .push(format!("Default output name unavailable: {error}"));
                None
            }
        });
    result.inputs.sort();
    result.inputs.dedup();
    result.outputs.sort();
    result.outputs.dedup();
    Ok(result)
}

#[cfg(test)]
#[path = "device_scan_tests.rs"]
mod tests;
