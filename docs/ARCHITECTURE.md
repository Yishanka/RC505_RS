# Architecture

## Ownership and scheduling

`engine/core.rs::RenderCore` is the single device-independent renderer. The output callback owns it; UI does not mutate track audio or advance transport. `SampleClock` uses a u64 frame counter and derives every beat boundary from the original rational tempo, avoiding cumulative rounding. Playback, capture, compensation tails, overdub and sequences use this clock. GUI state is a bounded `EngineView` snapshot.

The input callback sanitizes stereo frames and pushes them into an SPSC ring. The output callback adapts input clock drift with cubic interpolation, runs track FX to obtain pre-fader carriers, processes Input FX, writes recordings/overdubs, applies faders and emits the master mix. Monitor and loop audio share one processing domain. Queue starvation/overflow and callback duration are counted without callback logging.

Control messages are bounded (128). Parameters/runtimes are constructed outside the callback, swapped on receipt and retired through the worker queue. The renderer has no egui dependency. No callback mutex or per-frame Vec remains. Memory reclamation, WAV I/O and serialization run on workers. The zero-allocation test covers five-track rendering, existing heavy FX, parameter exchange, snapshot sharing, overdub, undo and clear; this is not a hardware deadline guarantee.

## Audio pages and undo

`LoopAudio` stores stereo f32 frames in shared 8192-frame pages (storage granularity, not device buffering). Page tables reserve five-minute capacity per track. A dedicated worker supplies prepared pages. Snapshots/undo share references; subsequent writes obtain a fresh page and copy only that page. Old references go to a bounded retirement queue. Pool exhaustion stops the affected recording and reports it, instead of growing a Vec in the callback. Five tracks at 48 kHz, five minutes each need about576 MB for one full audio generation; undo/COW snapshots can increase peak memory.

Undo is one whole overdub transaction per track. Beginning another overdub replaces the prior undo point. Snapshot save serializes both versions. Reverse and One Shot suppress overdub. Stop may be immediate, loop-end or fade; a second stop bypasses a pending playback stop. Recording finalization waits for the configured capture tail, preserving exact target length.

## Persistence and replay

`project.rs` retains the legacy JSON parameter codec and explicit defaults. New optional fields include track options, independent fader rates, routing, sample calibration and snapshot reference. Delay Mix migrates to independent Direct/Effect levels. Reverb percent high damping maps to the previous frequency before conversion to Hz. Unique file identifiers survive rename.

`session.rs` writes version2 asset bundles: config, five track WAVs, undo WAVs, sample rate, lengths and SHA-256. A new revision is staged, flushed and renamed before updating the project pointer. Existing versions and a prior project JSON backup remain. Missing/corrupt data fails before replacing the runtime. Index recovery and a project trash service are centralized here; the launcher no longer owns an independent project database. A per-data-folder editor lock makes a second editor read-only.

`replay.rs` stores an initial audio/config bundle, dry input float WAV and ordered JSONL events `{frame, sequence, kind}`. Begin Take requires stopped tracks and swaps in prepared clean DSP state; this makes the initial condition explicit instead of omitting old effect tails. Commands are logged at the sample where the renderer accepts them. The worker detects input gaps and queue loss. A completed take has hashes and a renderer version; incomplete takes cannot masquerade as valid replay.

Offline rendering runs the same `RenderCore` at the original sample rate. A regression fixture verifies bit-identical output and final loop contents for recording, overdub, undo/redo and parameter changes. Future algorithm changes must increment the renderer version or preserve an old renderer. Cross-compiler/CPU bit identity is not promised. Import materializes the final loops/config as a new snapshot in the source project or a new project; arbitrary other-project overwrite is prohibited.

## UI and installation

`app/actions.rs` shares commands between mouse and keyboard. `keyboard.rs` separates performance, text, full editor and panel focus. F6, A/F7 and D/F8 select top/left/right; navigation registers focusable controls. Momentary FX store their original bank/track/slot so release restores the correct target even after selection changes. Faders integrate elapsed time, with independent direction and rate controllers.

`ui/theme.rs` supplies fonts/colors; `performance.rs` uses fixed quick-panel geometry and internal scrolling. `editor.rs`, `parameters.rs` and `piano_roll.rs` edit the same config with pinned bank/slot identity. Legacy painted screens and the old wall-clock Track/Metronome scheduler are removed.

`app_support/paths.rs` resolves explicit `--data-dir`, then executable-adjacent `install-settings.json`, then the legacy AppData fallback. The installer defaults data to the program's `data` folder. `maintenance.rs` performs copy/verify migration before audio or GUI startup. `updater.rs` uses an embedded PowerShell helper to validate GitHub release metadata, download and hash the installer, and wait for normal app exit before installation. No updater token or GitHub credential is distributed.

## DSP scope

Legacy Input FX: input + normalized Oscillator bus + MyDelay bus → Vocoder → Filter → Reverb. Serial mode executes A→D, with generators added at their slot. Track FX follows slot order. Vocoder carriers are after Track FX and before faders. Filter coefficients update at an eight-sample control interval while moving; Reverb feedback coefficients at32 samples. Fixed parameters use cached coefficients.

Existing DSP/control semantics are independently implemented from public references. Hardware A/B, long-running driver stress, physical loopback and keyboard rollover remain real-device acceptance work. See [validation](VALIDATION.md) and [references](RC505_REFERENCE.md).
