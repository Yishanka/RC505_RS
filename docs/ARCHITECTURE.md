# Architecture

## Ownership and scheduling

`engine/core.rs::RenderCore` is the single device-independent renderer. The output callback owns it; UI does not mutate track audio or advance transport. `SampleClock` uses a u64 frame counter and derives every beat boundary from the original rational tempo, avoiding cumulative rounding. Playback, capture, compensation tails, overdub and sequences use this clock. GUI state is a bounded `EngineView` snapshot.

Configuration defaults are pure data. Device enumeration is an explicit interactive startup operation, never part of project/preset conversion or offline replay. This also lets the regression suite run on Windows machines without audio devices.

The input callback sanitizes stereo frames and pushes them into an SPSC ring. The output callback adapts input clock drift with cubic interpolation, runs track FX to obtain pre-fader carriers, processes Input FX, writes recordings/overdubs, applies faders and emits the master mix. Monitor and loop audio share one processing domain. Queue starvation/overflow and callback duration are counted without callback logging.

Control messages are bounded (128). Parameters/runtimes are constructed outside the callback, swapped on receipt and retired through the worker queue. The renderer has no egui dependency. No callback mutex or per-frame Vec remains. Memory reclamation, WAV I/O and serialization run on workers. The zero-allocation test covers five-track rendering, existing heavy FX, parameter exchange, snapshot sharing, overdub, undo and clear; this is not a hardware deadline guarantee.

## Physical output routing (0.2.5)

The global `follow_system_output` preference defaults to true, including migration of older preferences that contain only a concrete device name. A worker checks the Windows `eConsole` render endpoint every 500 ms. It compares opaque endpoint IDs, not friendly names or brands; fixed-device mode remains explicit. See Microsoft's [default endpoint API](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-getdefaultaudioendpoint) and [endpoint identity API](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdevice-getid).

Output handoff prepares a silent candidate stream, releases the old stream, and transfers the exact `OutputState` through a bounded queue. The callback owner returns its state at stream teardown; the teardown channel is never used during per-frame rendering. No snapshot reload or transport reset is involved. Loop buffers, pending actions, recording mode, effect state and renderer frame numbering remain owned by the same renderer. If an output is absent, its state is parked and input capture paused; replay capture is marked incomplete after loss of input.

The renderer stays at its original rate. A 64-tap, 256-phase windowed-sinc converter adapts only the physical output, with an exact bypass at equal rates. Coefficients are constructed outside callbacks; mono is downmixed and additional hardware channels are zeroed. Device changes invalidate loopback probes by a generation counter, preventing late results from being applied to a new route. Runtime driver teardown can still block or interrupt sound; this is not a guarantee against arbitrary driver failures.

## Audio pages and undo

`LoopAudio` stores stereo f32 frames in shared 8192-frame pages (storage granularity, not device buffering). Page tables reserve five-minute capacity per track. A dedicated worker supplies prepared pages. Snapshots/undo share references; subsequent writes obtain a fresh page and copy only that page. Old references go to a bounded retirement queue. Pool exhaustion stops the affected recording and reports it, instead of growing a Vec in the callback. Five tracks at 48 kHz, five minutes each need about576 MB for one full audio generation; undo/COW snapshots can increase peak memory.

Each track keeps up to eight recording/overdub/clear transactions using preallocated page tables. Each undo/redo stack evicts oldest entries above 2048 referenced pages (128 MiB), while retaining at least one entry. Long loops can therefore keep fewer than eight steps; a single latest step can exceed that budget. New writes clear redo. Snapshot save persists both stacks, reusing WAV assets with identical immutable page identity. Legacy single-level undo remains only for renderer 2 compatibility. Reverse and One Shot suppress overdub. Stop may be immediate, loop-end or fade; a second stop bypasses a pending playback stop. Recording finalization waits for the configured capture tail, preserving exact target length.

## Persistence and replay

`project.rs` retains the legacy JSON parameter codec and explicit defaults. New optional fields include track options, independent fader rates, routing, sample calibration and snapshot reference. Delay Mix migrates to independent Direct/Effect levels. Reverb percent high damping maps to the previous frequency before conversion to Hz. Unique file identifiers survive rename.

`session.rs` writes version 3 asset bundles: config, five tracks, both history stacks, legacy undo, sample rate, lengths and SHA-256. Version 2 bundles migrate the previous undo into the correct undo/redo stack. A new revision is staged, flushed and renamed before updating the project pointer. Existing versions and a prior project JSON backup remain. Missing/corrupt data fails before replacing the runtime. Index recovery and a project trash service are centralized here; the launcher no longer owns an independent project database. A per-data-folder editor lock makes a second editor read-only.

`replay.rs` stores an initial audio/config bundle, dry input float WAV and ordered JSONL events `{frame, sequence, kind}`. Begin Take requires stopped tracks and swaps in prepared clean DSP state; this makes the initial condition explicit instead of omitting old effect tails. Commands are logged at the sample where the renderer accepts them. The worker detects input gaps and queue loss. A completed take has hashes and a renderer version; incomplete takes cannot masquerade as valid replay.

Offline rendering runs the same `RenderCore` at the original sample rate. A regression fixture verifies bit-identical output and final loop contents for recording, overdub, undo/redo and parameter changes. Renderer 3 removes stopped fixed-note fallback and adds bounded history actions; renderer 2 preserves its previous fallback and one-level undo. Both paths have bit-exact replay fixtures. Future algorithm changes must increment the renderer version or preserve an old renderer. Cross-compiler/CPU bit identity is not promised. Import materializes the final loops/config as a new snapshot in the source project or a new project; arbitrary other-project overwrite is prohibited.

## UI and installation

`app/actions.rs` shares commands between mouse and keyboard. `keyboard.rs` separates performance, text, full editor and panel focus. F6, F7 and F8 select top/left/right; navigation registers focusable controls. Momentary FX store their original bank/track/slot so release restores the correct target even after selection changes. Faders integrate elapsed time, with independent direction and rate controllers.

Navigation requests are deferred until the current scope has drawn its controls. The expanded editor owns a separate focus scope; it never traverses compact-panel IDs. Scene changes clear hidden widget focus, and navigation locks prevent double traversal by the app and egui. This preserves the Windows AccessKit invariant that the focus node exists in the current accessibility tree. The process-isolated UI regression enables accessibility and exercises editor navigation, tab changes and dialogs. Unexpected panics are logged to the chosen data directory's `logs/last-panic.log`.

Recording/overdub beat indicators derive their four-beat phase from the engine's elapsed sample count. Piano roll owns a bounded two-axis viewport, with sticky keyboard/ruler headers and explicit middle-button panning; scrolling never doubles as note dragging.

On Windows, `performance_keys.rs` bridges egui0.27's dropped/translated Shift symbols using key state for the explicitly bound performance keys. It checks both application focus and foreground process identity before polling, suppresses held keys when returning from editing, and modifies only the command router's input copy. The original text input stream is unchanged.

`ui/theme.rs` supplies fonts/colors; `performance.rs` uses fixed quick-panel geometry and internal scrolling. `editor.rs`, `parameters.rs` and `piano_roll.rs` edit the same config with pinned bank/slot identity. Legacy painted screens and the old wall-clock Track/Metronome scheduler are removed.

`app_support/paths.rs` resolves explicit `--data-dir`, then executable-adjacent `install-settings.json`, then the legacy AppData fallback. The installer defaults data to the program's `data` folder. `maintenance.rs` performs copy/verify migration before audio or GUI startup. `updater.rs` uses an embedded PowerShell helper to validate GitHub release metadata, download and hash the installer, and wait for normal app exit before installation. No updater token or GitHub credential is distributed.

## DSP scope

Legacy Input FX: input + normalized Oscillator bus + MyDelay bus → Vocoder → Filter → Reverb. Serial mode executes A→D, with generators added at their slot. Track FX follows slot order. Vocoder carriers are after Track FX and before faders. Filter coefficients update at an eight-sample control interval while moving; Reverb feedback coefficients at32 samples. Fixed parameters use cached coefficients.

Existing DSP/control semantics are independently implemented from public references. Hardware A/B, long-running driver stress, physical loopback and keyboard rollover remain real-device acceptance work. See [validation](VALIDATION.md) and [references](RC505_REFERENCE.md).


## Monitoring and calibration (0.2.7)

Formal transport, metronome and private audition are separate state. Starting the click starts transport; disabling the click leaves transport running. Audition has an independently prepared DSP runtime, pinned bank/slot/track and private sample counter. It bypasses slot enable and trigger thresholds. Callback mixes the precomputed click and audition after `RenderCore::process`, so neither enters track buffers or offline replay output. Starting capture retires audition and resets formal DSP/transport. Capture eligibility checks stopped tracks, not the global transport flag.

The final output feeds an 8192-frame SPSC visualization queue. A dedicated worker runs stereo Hann-window radix-2 FFTs at about 30 Hz (4096/8192/16384 samples by renderer rate), merges channel power without phase cancellation, maps 64 logarithmic bands and smooths release. Twiddles, permutation and work buffers are prepared off-thread. Callback work stays constant and may drop visual samples instead of waiting. Disabling the background stops sample submission and FFT work; fully silent windows skip FFT.

Calibration first persists a monitoring guard, then requests silence. Only after callback acknowledgement may the user confirm electrical loopback and request probes. Completion/failure never restores monitoring. Output handoff preserves the guard; startup reloads it. Explicit cable-disconnected confirmation clears it. Physical sockets and hardware direct monitoring cannot be detected by the application.


## Replay presentation and appearance (0.2.8)

Offline rendering also records bounded-rate `EngineView` samples and sample-stamped configuration deltas in `replay/visuals.rs`. These cache the visible audio state without a second live DSP pass. `ui/replay_panel.rs` owns a boxed, isolated display configuration and editor. It maps the player's sample cursor to the replay source rate, applies deltas and draws the shared performance workspace in read-only mode. The live model is restored before configuration synchronization; no display operation is sent back as a performance edit. Closing the temporary panel retains the live project's buffers and parameters. Old replays have no pointer/focus/window-layout history.

Replay delete/discard validates ownership and canonical managed paths, then renames the folder into its project's `replay-trash`. Restore uses a fresh identity and never overwrites a live replay; exported WAVs stay independent. Project deletion uses the existing project trash and now has a direct button.

ThemeColor is a global, serde-defaulted launcher preference (Mint/Rose/Ember). The UI palette lives in `theme.rs`; custom graphics read the current palette from egui instead of fixed accent constants. It is not part of project configuration or replay events, and the icon is unchanged.
