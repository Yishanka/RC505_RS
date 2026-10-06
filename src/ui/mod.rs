pub mod audio_fx_panel;
pub mod automation;
mod beat;
pub mod calibration;
pub mod editor;
pub mod help;
pub mod init;
pub mod navigation;
mod parameter_input;
pub mod parameters;
pub mod performance;
pub mod phrases;
pub mod piano_roll;
pub mod replay_panel;
pub mod replays;
pub mod shortcuts;
pub mod storage;
pub mod theme;
pub mod visualizer;

#[cfg(debug_assertions)]
pub mod preview;

#[cfg(debug_assertions)]
pub mod capture;

#[cfg(debug_assertions)]
pub mod regression;
