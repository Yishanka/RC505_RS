# Architecture and maintenance

## Boundaries

```text
main.rs                    startup, window, debug preview entry
app.rs                     lifecycle, config sync, loop scheduling
app/actions.rs             shared mouse/keyboard application commands
app/keyboard.rs            performance keys + legacy parameter navigation
app/faders.rs              independent dB key faders, integrated speed curve
ui/performance.rs          responsive transport, FX racks, track controls
ui/editor.rs               pinned FX target, quick/full editor, preset workflow
ui/parameters.rs           shared parameter widgets + DSP-derived visuals
ui/piano_roll.rs           pointer interaction, selection, bounded edit history
ui/compact.rs              retained hardware-style keyboard display
config/sequence_edit.rs    pure monophonic editing + normalized step boundaries
config/time_mode.rs         shared milliseconds / musical-time conversion
config/*                   editable parameter values (single source of truth)
presets.rs                 versioned single-slot codec using project migration
project.rs                 project codec, validation, atomic parameter writes
engine/device_config.rs    common input/output f32 format negotiation
engine/audio_io.rs         callbacks, track audio, routing, faders, waveform snapshot
engine/input_fx.rs          config snapshots → input FX DSP
engine/track_fx.rs          config snapshots → per-track DSP
dsp/*                      stateful DSP independent of egui
```

The old `ui/looper.rs` painted track/FX controls without pointer actions. Its active compact display is now isolated in `compact.rs`; dead display-only tracks/racks are removed. Keyboard routing moved out of the application lifecycle. New mouse controls and legacy keyboard track keys call the same commands.

No trait-object plugin graph or new FX types were introduced. The existing typed enums are retained until routing and state ownership justify a larger graph migration. Adding an effect must still cover config, runtime conversion, DSP, editor, persistence and documentation.

## Sequence representation

12 ticks per quarter-note beat, maximum 384 ticks. `note_seq` contains pitch/rest per tick; `step_len_seq` has nonzero lengths only at step starts. Repeated same-pitch notes therefore have distinct retriggers. Event editing is an adapter over these arrays, preserving the project format.

On import, sequences are bounded, pitch octaves clamped and step lengths rebuilt from markers/value changes. Overlap replacement retains unaffected fragments. Undo stores paired array snapshots, capped at64; the clipboard survives target selection while history does not.

`StepTrigger` converts start markers to one sample per absolute tick. Input sequences use the metronome phase; Track Filter uses the loop playhead. GUI curves do not advance the audio DSP state.

## Audio path

```text
device input → input envelope detector
  input + normalized OSC bus → MyDelay bus → Vocoder slots
  → Filter slots → Reverb slots → monitor + record/overdub input

loop buffer → Track FX (slot order) → track fader → master mix
                                   └→ Vocoder carrier (pre-fader)
master mix + processed input → hard-clamped device output
```

This is the specialized processing order, **not** arbitrary A→B→C→D for all Input FX. Each effect group follows slot order. Track FX follows slot order across effect types. Roll keeps recent processed stereo audio while bypassed and freezes a slice on activation, so preceding Track FX are captured. Switching banks clears Roll history; changing slice length recollects input. Its two-second buffer is prepared on the control path, not allocated in its processing function.

Vocoder uses cached analysis/synthesis bandpass banks, attack/release envelopes, spectral contrast normalization, interpolated formant shift and tone tilt. Input carriers use one stereo device's opposite channel as modulator. Track carriers are post-Track-FX, pre-fader. Reverb adds input allpass diffusion and independent dry/wet levels to the existing four-line FDN. These are independent implementations; public parameter names do not establish hardware-equivalent transfer functions.

UI builds FX runtime snapshots before acquiring a mutex; `try_lock` swaps a snapshot and retires the previous allocation after unlocking. If busy, the next UI frame retries. This shortens UI-held locks but is **not a lock-free engine**. Callbacks still lock and allocate; recording growth, carrier queue storage, DSP lazy allocations and independent device clocks need further work.

Track gain targets use the shared config and are smoothed in the output callback. Bounded waveform overviews use `try_lock`, 96 bins ×8 samples/track, cached at10Hz. They are visual overviews, not full peak caches.

Keyboard faders use independent key-down state for each track, with an integrated dB speed curve, tap increments and a fine modifier. Opposing keys cancel. OS autorepeat is ignored for performance toggles. Focus, expanded editing, text input and legacy mode suspend fader control. Mouse and key faders change the same gain targets. This is not a substitute for testing physical keyboard rollover.

Track commands are shared by mouse/keyboard. A pending finish ignores subsequent record taps until the engine acknowledges playback; Stop during initial recording requests finish-then-stop. Pause cancels queued record/overdub operations. Clearing a track clears the recording buffer, frozen Roll, Delay tail, filter state and queued carrier audio as well as the display state.

## Persistence and compatibility

- Project fields remain compatible. Optional `track_levels` defaults to unity and `fader_speed_db` to24. New effect fields have explicit legacy defaults; time mode, carrier strings and Roll subdivision round-trip through the same codec as presets. Nonfinite/out-of-range values are sanitized at import.
- Project save errors reach the UI; a failed save prevents an exit. Project/index writes use a temporary file, sync then rename.
- Missing project JSON means a new default project; unreadable/malformed JSON reports failure.
- Single-slot presets have version1 and reuse project conversion. Input/Track types are checked. Loading preserves bypass state, devices, tempo and unrelated slots. Save-as-new uses exclusive creation.
- Audio buffers and UI state are not serialized. Fader state is in `AppConfig`, not duplicated in `Track`.
- Intentional sonic changes include single-sample sequence triggers, envelope-following thresholds, linear zero-drive filters, captured-slice Roll and revised Vocoder spectral envelopes. The user's previous Vocoder formant work informed the new shaping; the file is intentionally rewritten under the expanded authorization. Old parameter compatibility does not promise bit-identical audio.

## Validation

See [VALIDATION.md](VALIDATION.md). No audio driver is required for the pure DSP, model, codec or egui pointer tests. `--offline` disables device streams/retries. `--data-dir=...` isolates application files.

Debug builds accept `--ui-preview=performance|performance-small|sequence|filter|envelope|vocoder|roll|reverb` with **both** isolation flags. The launcher accepts `--ui-preview=launcher --data-dir=...`. These render actual application frames, write PPM screenshots under `var/ui-verification/` and close. They are visual fixtures, not saved demo projects or driver tests. Release builds do not contain the capture facility. `ui/theme.rs` and `ui/capture.rs` are shared by both binaries.
