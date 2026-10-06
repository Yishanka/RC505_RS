# RC505 RS

A five-track loop station for Windows, with keyboard performance controls and visual sound editing. Built with Rust, CPAL and egui, and inspired by the BOSS RC‑505mkII workflow. This is an independent implementation with software extensions; hardware-identical sound has not been verified.

**Supported platform: Windows x64 only.** macOS and Linux are not supported.

[中文](README_CN.md) · [Download the latest release](https://github.com/Yishanka/RC505_RS/releases/latest) · [User guide (Chinese)](docs/USER_GUIDE_CN.md) · [Installation and updates (Chinese)](docs/INSTALL_UPDATE_CN.md)

![Performance workspace](docs/images/performance.png)

## Install and play

Download the **setup EXE** from Releases. The installer lets you choose application, data and download folders separately. Data defaults to `data` beside the application. A portable ZIP is also available. Updates preserve saved projects, audio, sound presets and replays. Uninstalling from Windows **Installed apps** keeps data by default; deleting application data is a separate, optional choice.

On launch, the app checks for a newer release in the background. It prompts when one is available; downloading and installing remain manual. **F12 → Updates** provides update controls and the startup-check preference.

1. Create or open a project, then choose your input in **Audio**. Output follows the Windows default device unless you choose a fixed device.
2. Set the tempo. Press **1–5** to record, finish a loop, play or overdub. **Shift+1–5** or **F1–F5** stops the corresponding track; **Space** starts/stops all tracks.
3. Click an FX slot to adjust it in the compact panel, or expand it for visual editing. Performance keys remain available while editing parameters.
4. **Ctrl+S** saves configuration. **Ctrl+Shift+S** saves configuration and a track-audio snapshot.
5. **Esc** returns from editing to performance, then to the project browser. **F12** opens help. The top **Keys** button customizes shortcuts.

## Sound and performance

- Five tracks with overdub, bounded undo/redo, reverse, one-shot playback, stop modes and recording quantization.
- Independent keyboard faders and speeds, momentary FX, silent input monitoring and a monitor-only metronome.
- OSC with synthetic, vocal and sampled waveforms; mono/8/16 voices, legato/glide, editable envelopes, two LFOs and an internal filter. Capture a sound directly in the compact panel. New samples remain temporary until explicitly saved as a sound preset.
- Polyphonic piano roll with separate sound and phrase presets, note audition, shared phrases and next-loop phrase changes.
- Input and track FX for pitch, harmony, distortion, dynamics, EQ, delay, reverb and modulation, plus master filtering/compression/reverb. See the [FX catalogue](docs/RC505_MK2_FX_CATALOG_CN.md) for coverage and differences from the hardware.
- Global input noise gate with a threshold control, effect-latency alignment and measured recording compensation. Internally generated OSC notes retain their musical timing.
- A global replay library: capture input and sample-timed operations with **F9**, open the library with **F10**, then play, seek, pause or import the current state into a project. Playback recalculates the audio; it creates a complete WAV only when you export one.
- Chinese/English UI, three colour themes and a background frequency spectrum.

![Piano roll](docs/images/sequence.png)

## Source code

Releases contain ready-to-use Windows applications; this repository contains source code. You may fork or clone it, compile it for your own use, and modify it. Issues and pull requests are welcome.

For a local Windows x64 build, install Rust with the MSVC toolchain and Visual Studio C++ Build Tools, then run:

```powershell
git clone https://github.com/Yishanka/RC505_RS.git
cd RC505_RS
cargo build --release --bins --locked
.\target\release\rc505_rs.exe --data-dir=.\local-data
```

This runs your compiled application with a separate data folder. It does not install or replace a release installation. The optional audio-setup launcher is `target\release\rc505_launcher.exe`.

[Release notes](docs/RELEASE_NOTES_CN.md) · [Hardware references](docs/RC505_REFERENCE.md) · [Roadmap](docs/PLAN.md)
