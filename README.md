# RC505 RS

A five-track desktop loop station for keyboard performance and visual sound editing, built with Rust, CPAL and egui. Inspired by the RC‑505mkII workflow; this is an independent implementation, not a claim of hardware-identical audio.

[中文](README_CN.md) · [Windows downloads](https://github.com/Yishanka/RC505_RS/releases) · [User guide](docs/USER_GUIDE_CN.md) · [Install, migrate and update](docs/INSTALL_UPDATE_CN.md)

![Performance workspace](docs/images/performance.png)

The installer lets you choose application, data and download folders. Data defaults to `data` beside the application. Updates preserve projects, audio snapshots, replay takes and media. Existing development data can be copied from `%APPDATA%\rc505_rs` without deleting the originals.

Start at the project browser. Check audio devices, set tempo, press **1–5** to record/play/overdub/finish; **Shift+1–5** or **F1–F5** stop tracks. **Ctrl+Shift+S** saves configuration and audio; **Ctrl+S** saves configuration while retaining the previous audio snapshot. **F12** opens the built-in guide and updater.

Features include per-track key faders and speeds, momentary FX, piano roll, visual filters/envelopes, snapshot persistence, sample-stamped replays, independent playback/export, overdub undo/redo, reverse, one-shot, stop modes, fixed recording lengths and quantization. Input FX can preserve legacy routing or follow slot order. No new FX types were added in this release.

The metronome starts performance; independent FX audition uses its own clock. Both are monitor-only and excluded from recorded loops and replay exports. Track recording/overdub/clear history supports up to eight steps (page-budget bounded): **Alt+1–5** undo, **Ctrl+Alt+1–5** redo. Piano-roll history uses **Ctrl+Z/Y**. **F9** captures a replay; **F10** opens the replay library and a temporary performance panel that reproduces track, fader and FX states. Direct discard/delete/restore controls are available. The background is a worker-generated stereo FFT spectrum, with Mint/Rose/Ember palettes selectable at the top. Loop length is explicit, so note edits cannot silently extend it.

Replay stores dry input and ordered commands, then executes the same DSP for export. It starts from stopped tracks with clean FX state. Five-minute track and thirty-minute take limits keep memory and WAV sizes bounded. Physical latency still requires measurement; the built-in loopback test recommends compensation only after three consistent probes.

The calibration wizard explains physical line-loopback requirements. Mute before connecting the cable; disconnect before explicitly restoring monitoring. Measurement failure, device changes and application restart keep the monitoring guard active.

```powershell
cargo check --all-targets
cargo test --all-targets
cargo build --release --bins
cargo run --bin rc505_rs -- --offline --data-dir=var/development
```

Pushing a matching version tag runs the Windows release workflow and publishes the installer, portable ZIP, checksums and update manifest. See the [release workflow](docs/INSTALL_UPDATE_CN.md), [architecture](docs/ARCHITECTURE.md), [validation](docs/VALIDATION.md), [hardware references](docs/RC505_REFERENCE.md) and [roadmap](AGENTS/PLAN.md).
