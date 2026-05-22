# Vocoder Carrier Stream Debug

## 问题描述

用户重新明确 vocoder 期望：

- 麦克风、Oscillator、MyDelay 等输入声音都可以进入 vocoder 的 modulator 侧。
- Track carrier 负责提供旋律、音高和音色。
- 听感应类似“某个 carrier 音色在演奏旋律，但开合和频谱轮廓由 modulator 控制”。

实际现象是：vocoder 响度大致跟 carrier 是否有声一致，但输出只有“沙沙”的噪声，没有 Tr1 的旋律，也没有 Tr1 或 Oscillator 的音色。

## 复现方式

1. 用 MyDelay 或 Oscillator 录制一段有明确旋律的声音到 Tr1。
2. 开启 vocoder，Carrier 选择 Tr1。
3. 使用麦克风、Oscillator 或 MyDelay 作为 modulator。
4. 监听或录制 vocoder 输出。

## 根因定位

根因不在“是否允许 Oscillator/MyDelay 进入 modulator”，而在 carrier 数据传输方式。

旧实现中：

- output callback 每个音频块内会逐帧生成 track carrier。
- 但 callback 结束时只把每个 track 的最后一帧写入 `track_carriers`。
- input callback 读取该 `track_carriers` 快照后，在整个输入块的每一帧都重复使用同一个 carrier sample。

这意味着 vocoder DSP 收到的 carrier 不是连续的 Tr1 音频流，而是以音频块为单位的 sample-and-hold 信号。该信号会把原本的旋律波形打碎成块级阶梯和宽频噪声，因此表现为：

- carrier 有声时 vocoder 有响度；
- carrier 无声时 vocoder 无声；
- 但 carrier 的旋律、音高和音色无法保留，只剩沙沙噪声。

## 修改方案

- `src/engine/audio_io.rs`
  - 将 `track_carriers` 从 `Vec<Option<(f32, f32)>>` 改为每个 track 一条 `VecDeque<Option<(f32, f32)>>` carrier 队列。
  - output callback 每一帧都将对应 track 的 carrier frame 入队。
  - input callback 每一帧从队列中取出对应 track 的 carrier frame，并传给 vocoder。
  - 队列保留有限长度，避免 input/output callback 短时不同步时无限增长。

本次保留 vocoder 的 modulator 路由：InputFX 中的麦克风、Oscillator、MyDelay 等仍然会进入 vocoder 调制侧。

## 影响范围

- 影响 Track carrier 从 output callback 到 input callback 的传输方式。
- 不改变 vocoder 参数 UI。
- 不改变 Project 保存字段。
- 不改变暂停轨作为 carrier 的行为。
- 不改变 InputFX 中 Oscillator/MyDelay 可作为 modulator 的行为。

## 手工验证步骤

1. 录制一段明确旋律到 Tr1，最好使用宽频 carrier，例如方波、锯齿波、明亮的 MyDelay 合成器。
2. 开启 vocoder，Carrier 选择 `Tr1`。
3. 用麦克风说话或开启 Oscillator/MyDelay 作为 modulator。
4. 预期 vocoder 输出不再是纯沙沙噪声，应能听到 Tr1 的旋律/音高随 modulator 开合。
5. 暂停 Tr1 且保持 metronome 开启，预期 Tr1 仍可作为隐藏 carrier。
6. 停止 metronome 或让 Tr1 不再提供 carrier，预期 vocoder 湿声消失或明显变弱。

## 残余风险

- carrier 队列仍然跨 input/output callback，存在一个音频块级别的延迟。
- 如果 input callback 长时间早于 output callback，队列可能短时为空，此时该帧 carrier 为 None。
- 如果 carrier 本身频谱很窄或音量过小，vocoder 的旋律和可懂度仍会受限。
