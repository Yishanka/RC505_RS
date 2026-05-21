# Vocoder Carrier Behavior Change

## 问题描述

Vocoder 初版同时提供合成器 carrier 与 track carrier。当前需求要求删除合成器 carrier 与 `SynthHz`，并支持 metronome 开启时暂停轨也可作为 carrier。

同时需要明确 InputFX 链路顺序，以及 Oscillator/MyDelay 等自动声音与 vocoder 同时启用时的行为。

## 触发方式

在 InputFX 中选择 `Vocoder`，旧版本可选择 `Saw` / `Sin` / `Tri` / `Sqr`，并配置 `SynthHz`。暂停 track 后，即使 metronome 仍在运行，该 track 也不会继续作为 carrier。

## 根因定位

- Config/UI/Project/DSP 中保留了 synth carrier 分支与 `synth_freq_hz` 参数。
- Audio output callback 只为正在播放的 track 写入 carrier 快照，暂停 track 不会进入 TrackFX 与 carrier 生成路径。
- 文档没有明确当前 InputFX 引擎的固定处理顺序：Oscillator/MyDelay -> Vocoder -> Filter -> Reverb。

## 修改方案

- `src/config/vocoder_configs.rs`
  - `VocoderCarrier` 改为只保留 `Track1`-`Track5`。
  - 删除 `synth_freq_hz` 与相关范围常量。
- `src/dsp/vocoder.rs`
  - 删除内置 oscillator carrier 生成逻辑。
  - DSP 只接收外部 track carrier。
- `src/engine/input_fx.rs`
  - runtime 删除 `synth_freq_hz`。
  - vocoder 参数只从 track carrier 快照读取。
- `src/engine/audio_io.rs`
  - 增加暂停轨 carrier 的只读 `carrier_cursor`。
  - `pause_at_progress_now` 在进入暂停时按当前 loop progress 对齐 carrier 光标。
  - metronome 开启时，暂停且允许 carrier 的 track 继续经 TrackFX 生成 carrier 快照，但不混入主输出、不推进真实播放头。
- `src/project.rs`
  - 保存时不再写 `SynthHz`。
  - 读取旧工程时忽略旧 `SynthHz`；无法识别的旧 synth carrier 回落到默认 `Tr1`。
- `src/ui/looper.rs` / `src/app.rs`
  - 参数页删除 `SynthHz` 行，参数索引同步调整。

## 影响范围

- Vocoder carrier 类型与参数 UI。
- Vocoder DSP 输入参数。
- Track 暂停状态下的 carrier 生成路径。
- Project 保存/载入 vocoder 字段。
- InputFX 行为文档。

## 手工验证步骤

1. 在 `FxSelect` 中选择 `Vocoder`，确认 carrier 只显示 `Tr1`-`Tr5`，没有 `Saw/Sin/Tri/Sqr` 和 `SynthHz`。
2. 录制 Track1 并播放，Vocoder carrier 选择 `Tr1`，讲话应听到 vocoder 输出。
3. 暂停 Track1，保持 metronome 运行，讲话仍应听到以 Track1 为 carrier 的 vocoder 输出。
4. 停止 metronome 或让所有 track 进入非活动状态后，暂停轨不再提供 carrier。
5. 同时启用 Oscillator/MyDelay 与 Vocoder，确认自动声音参与 vocoder 调制。
6. 保存并重新载入工程，确认 vocoder 参数仍正确。

## 残余风险

- 暂停轨 carrier 与 input callback 之间仍有一个音频块级别的快照延迟。
- 暂停轨 carrier 会驱动对应 TrackFX 状态推进；这是为了让作为 carrier 的声音包含 TrackFX 效果，但意味着隐藏 carrier 与再次恢复播放后的 TrackFX 状态可能不完全等同于从静止状态恢复。
