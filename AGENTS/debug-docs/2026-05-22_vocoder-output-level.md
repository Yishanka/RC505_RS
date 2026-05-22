# Vocoder Output Level Debug

## 问题描述

添加 vocoder 后出现两个现象：

1. 不开启 vocoder 时，用户感觉整体录制/监听声音变响，MyDelay 的 Threshold 比之前更容易触发。
2. 开启 vocoder 后，实时录放听不到；讲话再大声也听不到明显的调制后声音。

## 复现方式

1. 保持外设与输入增益不变。
2. 在 InputFX 中不开启 vocoder，仅使用 MyDelay，调高 Threshold 后观察是否仍触发。
3. 开启 vocoder，Carrier 选择一个已有声音的 track，Mix 保持 100，讲话并监听输出。

## 根因定位

### MyDelay / 未开启 Vocoder

对比 vocoder 引入前后的代码后确认：

- `src/engine/input_fx.rs` 中 MyDelay 的输入仍然使用 `(input_l + input_r) * 0.5` 的原始输入。
- MyDelay Threshold 仍然按 `UI 0-100 -> DSP 0.0-1.0` 映射。
- `src/dsp/my_delay.rs` 中触发条件仍为 `input.abs() >= threshold`。
- vocoder 未启用时，新增的 track carrier 快照不会进入 MyDelay 的触发检测，也不会改变输入增益。

因此，本次没有发现“未开启 vocoder 时 MyDelay 阈值/输入增益被代码直接改高”的证据。仍需实机排查输入设备驱动增益、系统输入音量、ASIO/WASAPI host 差异或监听叠加导致的体感变化。

### Vocoder 无声

Vocoder DSP 确实收到了 modulator 输入，也会读取 track carrier 快照；问题在电平量级：

- 当前湿声算法为 `carrier_band * modulator_envelope`。
- `modulator_envelope` 直接取语音 bandpass 后的绝对值，通常远低于全量程输入。
- 结果是 carrier 又乘了一次很小的包络，再除以 band 归一化，最终湿声电平接近听不见。
- 当 `Mix=100` 时干声被完全移除，所以用户会感觉实时录放消失。

## 修改方案

在 `src/dsp/vocoder.rs` 中增加 `MODULATOR_ENV_GAIN`，将分析侧 band envelope 放大后再驱动 carrier：

- 保留原有 attack/release、band 数、carrier 读取逻辑。
- 只调整调制包络量级，不改变 MyDelay、Oscillator、Filter、Reverb 的路径。
- 包络提升后仍 clamp 到 `1.0`，避免大声输入直接产生无限增益。

## 影响范围

- 修改范围：`src/dsp/vocoder.rs`
- 文档范围：`AGENTS/debug-docs/2026-05-22_vocoder-output-level.md`
- 不影响 Project 存档字段。
- 不影响 MyDelay Threshold 映射与触发条件。

## 手工验证步骤

1. 不开启 vocoder，只开启 MyDelay，确认 Threshold 行为与修改前一致。
2. 录制 Track1 并播放，InputFX 开启 vocoder，Carrier 选择 `Tr1`。
3. `Mix=100` 时讲话，应能听到以 Track1 为 carrier 的调制湿声。
4. 将 `Mix` 降到 50 左右，应同时听到部分干声与 vocoder 湿声。
5. 暂停 Track1 且保持 metronome 开启，继续讲话，应仍可听到暂停轨作为 carrier 的 vocoder 输出。
6. 停止 carrier track 或选择空 track，`Mix=100` 时仍可能无声，这是 vocoder 没有 carrier 的预期行为。

## 残余风险

- `MODULATOR_ENV_GAIN` 是经验增益，需要实机听感确认；不同麦克风输入增益下可能还需要微调。
- vocoder carrier 来自 output callback 的 track 快照，input callback 使用的是最近一次快照，仍存在一个音频块级别的延迟。
- 第一项“未开启 vocoder 但体感变响”未在代码路径中找到直接增益变化；若仍复现，应继续检查系统/驱动输入电平与实时监听叠加路径。
