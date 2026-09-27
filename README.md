# RC505 RS

A five-track desktop loop station in Rust, CPAL and egui. It combines keyboard performance with mouse-driven sound design, working toward the RC‑505mkII workflow while adding piano-roll editing and parameter visualization.

[中文](README_CN.md) · [Detailed Chinese manual](docs/USER_GUIDE_CN.md) · [Hardware comparison](docs/RC505_REFERENCE.md) · [Roadmap](AGENTS/PLAN.md)

![Performance workspace](docs/images/performance.png)

## Run

Windows is the primary development and verification platform. Install Rust and the MSVC C++ build tools, then run:

```powershell
cargo run --release --bin rc505_rs
```

The optional launcher selects devices, latency compensation and a project. Both executables share the same theme:

```powershell
cargo build --release --bins
.\target\release\rc505_launcher.exe
```

To edit without opening audio streams:

```powershell
cargo run --release --bin rc505_rs -- --offline
cargo run --release --bin rc505_rs -- --offline --data-dir=E:\RC505-data
```

The optional `asio` feature requires the appropriate SDK/driver environment; this revision was verified with default features.

## Record and perform

Open or create a project, check **Audio devices & latency**, set BPM, then press `1` to record track 1. Press again to finish on the next beat. While playing, press again to overdub; finish overdubbing at the loop boundary. `F1` stops that track.

| Control | Action |
|---|---|
| `1`–`5` | Record / play / overdub / finish each track |
| `F1`–`F5`; `Space` | Individual stop; all start/stop |
| Click track title; left/right arrows | Select the Track FX target |
| `Q W E R`; `U I O P` | Input FX; selected track's FX switches |
| `T` | Switch FX keys between slot toggles and bank selection |
| `Z X` / `C V` / `B N` / `M ,` / `. /` | Track 1–5 volume down/up pairs |
| `Shift` + fader keys | Fine adjustment |
| Click FX slot → Expand editor | Open the visual preset workspace |
| `Ctrl+S`; expanded editor `Esc` | Save parameters; return to performance |

Faders use a dB scale. A tap changes 0.5 dB, holding accelerates, and releasing stops. Shift changes 0.1 dB per tap and runs at 1/8 hold speed. Configure the maximum rate under **Keyboard faders & shortcuts**. Multiple tracks can move independently. Performance shortcuts yield to text input, expanded editing and inactive windows.

The expanded editor supports monophonic piano-roll drawing, moving/resizing notes, undo/redo, pattern copying, transposition, grid snapping, filter response, AHDSR curves and versioned single-slot presets. Narrow windows collapse the quick editor to keep performance controls visible.

![Piano roll](docs/images/sequence.png)

**Projects currently save parameters and sequences, not recorded loop audio.** Recordings are session-only and do not survive project switching or exit. Default data lives under `%APPDATA%\rc505_rs`; `--data-dir` selects an isolated workspace.

## Scope and verification

This revision improves existing effects; it adds no new FX types. Work includes Vocoder envelope shaping/formant controls, Roll1/2 parameters and stereo capture, tempo-synced delay, reverb dry/wet and density controls, reliable note triggers, envelope-based thresholds and zero-drive filter linearity.

The [official BOSS parameter guide](https://static.roland.com/assets/media/pdf/RC-505mk2_Parameter_eng04_W.pdf) informs the implementation. This is independent DSP with documented differences, not a claim of hardware-identical sound. Hardware A/B listening and driver-latency calibration remain outstanding.

```powershell
cargo check --all-targets
cargo test --all-targets
cargo build --release --bins
```

See [architecture](docs/ARCHITECTURE.md), [validation](docs/VALIDATION.md), [contributor instructions](AGENTS.md) and the [roadmap](AGENTS/PLAN.md). Audio persistence, callback allocation/lock removal, sample-accurate scheduling and full hardware routing remain priorities.
