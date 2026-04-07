# RC505 for Free!

English README. Chinese version: [README_CN.md](./README_CN.md)

## 1. Brief Intro

This project is a free local looper app inspired by the BOSS RC-505 MK2. The real RC-505 Mk2 hardware is expensive, and many free software loopers are too limited for live ideas. So I decided to build one myself in Rust and keep adding features step by step.

This is still an early version and still rough in many places, but the core workflow is already usable: multi-track looping, beat sync, input effects, track effects, and project save/load.

## 2. What Is RC-505

The RC-505 style workflow is basically live looping with multiple tracks, quick switching, and effects that react to rhythm. You record short phrases, layer them, mute and unmute tracks, and shape sound in real time. This project follows that idea with 5 tracks and two effect layers: Input FX (before recording) and Track FX (on playback tracks).

I am not trying to make a strict 1:1 hardware clone, because I don't have one =(. Many details here are my own design choices, especially in FX behavoir, FX parameters and UI behavior. The goal is practical live use first, then gradual refinement.

## 3. About the Project

The app is written in Rust and currently targets Windows desktop. Audio I/O is built on `cpal` (WASAPI by default, optional ASIO feature), UI is built with `eframe/egui`, and project data is stored in JSON with `serde`.

The architecture is split into clear layers: UI/input handling in `app.rs` and `ui/*`, user-editable parameter trees in `config/*`, real-time audio routing in `engine/*`, DSP algorithms in `dsp/*`, and project persistence in `project.rs`. Runtime DSP state is separated from config data so parameter edits can be pushed into the audio thread safely.

At a high level:

```text
src/
  app.rs                app state machine + key handling
  ui/                   init screen + looper screen drawing
  config/               all editable parameters (beat/system/fx)
  engine/
    audio_io.rs         input/output streams, ring buffer, track timeline
    input_fx.rs         Input FX runtime and processing
    track_fx.rs         Track FX runtime and processing
    metronome.rs        beat timing
  dsp/                  envelope/filter/osc/reverb/delay/roll/my_delay/note
  project.rs            save/load project index and per-project JSON
```

## 4. How To Run and Play

### Build and Run

Right now the safest path is building from source locally.

```powershell
git clone <your-repo-url>
cd rc505_rs
cargo run --release
```

If you want to try ASIO on Windows and your devices support it:

```powershell
cargo run --release --features asio
```

If you only want a binary, you can also build once and run `target/release/rc505_rs.exe`.

I upload a binary file to the release without ASIO. You can just download and open it. 

### Basic Operation Flow

When the app starts, you enter the project list. Use `Up/Down` to select, `Enter` to open, `Enter` on `[ NEW PROJECT ]` to create, `R` to rename and `Enter` to determine the name, and `Delete` to remove a project.

Inside a project there are two working states: `Loop` and `Screen`. Press `S` to switch. In `Loop`, you mainly control record/play/dub and the on/off of the FXes. In `Screen`, you edit settings and FX parameters.

In `Loop` state, `1..5` controls tracks. Empty track goes to record, playing track goes to overdub, recording or dubbing track schedules stop on next beat and returns to play, and paused track resumes with timeline alignment. `F1..F5` pauses tracks. `Left/Right` selects track. `Delete` clears the selected track.

FX has two control modes toggled by `T`: `Bank` mode and `Single` mode. In Bank mode, `QWER` switches Input FX bank and `UIOP` switches Track FX bank. In Single mode, `QWER` toggles Input FX slots in current input bank, and `UIOP` toggles Track FX slots for the **currently selected track** in current track bank.

### Screen Editing

In `Screen` state, `B` opens Beat settings, `M` opens System settings, `QWER` opens Input FX slot editing, and `UIOP` opens Track FX slot editing. Most pages use `Left/Right` to move between fields and `Up/Down` to change enum values. Numeric fields accept number keys and `Backspace`. `Enter` is used for entering sub-pages or applying Push/Pop edits in sequence editors.

The UI is intentionally keyboard-first. It looks like a panel, but controls are not mouse-click workflow yet.

### Implemented FX (Current Version)

Input FX has 4 banks x 4 slots. Slot type can be `Oscillator`, `Filter`, `Reverb`, or `MyDelay`.

Oscillator includes waveform, level, threshold, note sequence, AHDSR envelope, plus its own filter and filter-envelope. MyDelay is a custom short-capture looping texture effect with note-driven loop length, its own AHDSR, and filter/filter-envelope. Input Filter is a biquad filter (LPF/HPF/BPF/Notch) with drive and wet mix. Reverb is an FDN-style reverb with size/decay/predelay/width/high-cut/low-cut.

Track FX also has 4 banks x 4 slots, and per-track enable states, so one bank definition can be shared while each track chooses on/off independently. Implemented track effects are `Delay`, `Roll`, and `Filter`. The track filter includes its own `Seq` and `Env` sub-pages, so cutoff motion can be rhythm-gated and envelope-shaped during playback.

### About Note / Seq / Envelope

I don't know the logic of sequencer in RC-505, so I implemented these Fx. Note and Seq are tick-based (12 ticks per beat, max 32 beats). Step options include `1/6`, `1/4`, `1/3`, `1/2`, `2/3`, `3/4`, `5/6`, `1`, and `2`. `Push` appends one step block, `Pop` removes the latest block.

Envelope is AHDSR plus `Start` and tension controls to provide a 'LFO' function. In the current mapping, tension default `100` means linear, values below it bend one way, values above it bend the other way, and max is `1000`.

### Latency Compensation (IMPORTANT)

Windows audio paths can have noticeable round-trip latency. Beat settings include `Latency Complement` (ms), which is used in recording alignment logic. Recorded buffers are compensated when recording stops, overdub write positions are offset accordingly, and track-FX timeline processing is shifted to stay phase-aligned with compensated track audio.

This setting is hardware-dependent. A value that works on one machine may not work on another, so treat it as a per-device calibration value.

### Save, Load, and Project Files

Projects are stored under `%APPDATA%/rc505_rs/projects` (fallback to local `projects/` if `%APPDATA%` is unavailable). There is one index file for the project list and one JSON file per project.

When exiting from loop/screen to init (or closing window), the app asks whether to save: `Y` save, `N` discard, `Esc` cancel exit. Beat/system/fx settings are persisted. Audio track waveform buffers are not persisted yet; this version stores configuration state, not recorded audio clips.

Known limitation for now: if you change an FX slot type (for example Oscillator -> Filter), that slot is reinitialized and its previous parameter set is lost.

## 5. Future Development

There are still many bugs and edge cases. I have not done systematic testing yet, so issue reports are very welcome.

The roadmap is to keep improving timing stability, expand DSP choices, and improve usability. Vocoder, pitch-related effects, and more refined track-level workflows are all candidates for future work.

If you want to contribute, PRs and suggestions are welcome. I am building this in public and learning while doing it.

