# RC505 RS

A five-track loop station for Windows, with keyboard performance controls and visual sound editing. Built with Rust, CPAL and egui, and inspired by the BOSS RC‑505mkII workflow. This is an independent implementation with software extensions; hardware-identical sound has not been verified.

**Supported platform: Windows x64 only.** macOS and Linux are not supported.

[中文](README_CN.md) · [Download the latest release](https://github.com/Yishanka/RC505_RS/releases/latest) · [User guide (Chinese)](docs/USER_GUIDE_CN.md) · [Installation and updates (Chinese)](docs/INSTALL_UPDATE_CN.md)

![Performance workspace](docs/images/performance.png)

## Download, install and open

1. Open the [latest release](https://github.com/Yishanka/RC505_RS/releases/latest), scroll down and expand **Assets**.
2. Download **`RC505-RS-version-windows-x64-setup.exe`**. This single EXE contains the installer; you do not need the other assets. **Source code (zip / tar.gz)** requires compilation. The separate `…portable.zip` package is an alternative that runs without installation.
3. Once the download finishes, double-click the setup EXE. If a browser or Windows reputation warning appears, check the source as described below before continuing. The installer uses English.
4. In **Select Destination Location**, choose the application folder and click **Next**. In **Data and download folders**, choose **Project data folder** and **Installer download folder**. Data defaults to `data` beside the application; all three locations can be chosen separately.
5. On **Import existing data (optional)**, leave the option unchecked and click **Next** for a new installation or a normal update. Select it only when copying existing data from another data folder.
6. Choose whether to **Create a desktop shortcut**, continue to **Install**, then leave **Open RC505 RS** checked and click **Finish**.
7. Installation is complete when the project browser appears. Later, open **RC505 RS** from the Start menu or desktop shortcut. Select **Open project** or **New**; the language switch is at the top.

The installer is unsigned, so a reputation warning may appear. **Continue only after confirming that the file came from the official `Yishanka/RC505_RS` release linked above, and the warning is only about an uncommon download or an unrecognized application.** In Edge, use the download item's `…` menu, then **Keep → Show more → Keep anyway**. For Windows **“Windows protected your PC”**, select **More info → Run anyway**, if available. [Microsoft Edge guidance](https://learn.microsoft.com/en-us/troubleshoot/microsoft-edge/development/download-failures#check-security-and-smartscreen-settings), [Windows guidance](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/publish-first-app#step-6-handle-smartscreen-for-new-apps).

If antivirus reports malware, or Smart App Control or an organization policy blocks execution without a continue option, stop and report the exact message to the project. Do not disable protection to install. [Smart App Control does not offer an individual-app exception](https://support.microsoft.com/en-us/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions). See the [installation guide](docs/INSTALL_UPDATE_CN.md) for optional checksum verification and portable usage.

**You do not need to uninstall an older version.** In the app, select **F12 → Updates → Check for updates → Download and verify**. Stop performance and resolve any replay draft, then select **Save snapshot, close and install update**. Alternatively, save and close the app, run the new setup EXE, keep the existing three folders, and leave data import unchecked. Startup update checks are optional; downloading and installing remain manual. Updates preserve saved data, and Windows uninstallation keeps it by default.

## Start playing

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
