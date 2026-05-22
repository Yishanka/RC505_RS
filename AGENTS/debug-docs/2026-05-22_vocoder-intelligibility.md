# Vocoder Intelligibility Debug

## 问题描述

用户使用 Tr1 作为 vocoder carrier：Tr1 是由 MyDelay 增益得到、带旋律的合成器声音。切到另一个 InputFX bank，在 Q slot 挂载 vocoder 并选择 Tr1。暂停 Tr1 后录制 Tr3 并讲话，录进去的声音只有一些合成器噪声，听不出“我在玩 loopstation”等张口闭口与发音轮廓，也听不出 Tr1 的旋律。

## 复现方式

1. 用 MyDelay/合成器声音录制一段带旋律内容到 Tr1。
2. 在另一个 InputFX bank 的 Q slot 选择 `Vocoder`，Carrier 选择 `Tr1`。
3. 保持 metronome 运行，暂停 Tr1。
4. 开始录制 Tr3 并讲话。
5. 回放 Tr3，观察是否有可辨识的语音 articulation 与 Tr1 旋律感。

## 根因定位

问题主要在 vocoder DSP 算法，不是单纯操作参数错误：

- 之前为了修复 vocoder 过小声，直接将 modulator band envelope 乘以较大增益后 clamp 到 `1.0`。
- 人声 bandpass 后的多个频段很容易一起触顶，结果 envelope 失去动态差异，vocoder 变成“整体打开 carrier”，而不是按语音频谱和张口闭口塑形。
- 默认 `Bands=8` 对语音可懂度偏粗，配合较慢 release 时更容易糊成合成器噪声。
- 纯 filter-bank vocoder 对辅音/齿音本来就弱；如果 carrier 频谱不够宽，发音细节会更难听出来。

暂停 Tr1 作为 carrier 的 engine 路径仍然存在：metronome 开启时，暂停轨会通过 `carrier_cursor` 继续生成只读 carrier 快照，不混入主输出。

## 修改方案

- `src/dsp/vocoder.rs`
  - 将硬 clamp 包络改为软饱和 `1 - exp(-x)`，避免大声输入把所有频段一口气顶满。
  - 扩展分析/合成频率范围到 `90Hz-7200Hz`，覆盖更多旋律基频与语音高频信息。
  - 增加由语音整体包络控制的 carrier body，帮助保留 Tr1 的旋律/音高存在感，但不讲话时不会常开漏声。
  - 增加高频 sibilance 路径，让辅音和齿音更容易出来。
- `src/config/vocoder_configs.rs`
  - 新建 vocoder 默认 `Bands=16`、`Attack=6ms`、`Release=80ms`、`Level=100`，更适合可懂语音。

## 影响范围

- 影响 vocoder DSP 的湿声音色与可懂度。
- 不改变 InputFX 固定链路顺序。
- 不改变 Project 存档字段。
- 不改变暂停轨 carrier 的 engine 行为。
- 已有工程中已经保存过的 vocoder slot 不会自动套用新默认值，需要手动调整参数。

## 手工验证步骤

1. 使用已有工程时，将当前 vocoder 参数调到 `Bands=16`、`Attack=6`、`Release=80`、`Level=100`、`Mix=100`。
2. 按复现流程录制 Tr1 carrier，再暂停 Tr1 并保持 metronome 开启。
3. 录制 Tr3 时讲话，预期能听到 Tr1 的旋律/音高存在感，并随讲话出现明显开合与发音轮廓。
4. 对比 `Bands=8` 和 `Bands=16`，预期 16 bands 的发音更清楚。
5. 停止 metronome 或选择空 carrier track，预期 vocoder 湿声消失或明显变弱。

## 残余风险

- 这是实时 lightweight vocoder，不是高阶相位声码器或 ML 声码器；中文声母、英文辅音的清晰度仍依赖 carrier 是否有足够宽的频谱。
- 如果 Tr1 carrier 本身非常窄、很暗或只有低频，语音可懂度仍会有限。
- 新默认参数只影响新建 vocoder slot；旧 slot 需要手动调参或重建。
