# Vocoder Modulation Depth Debug

## 问题描述

上一轮修复 carrier 连续流后，Tr1 旋律已经能保留。但新的听感问题是：modulator 带来的“说话感”、开合感，或 Oscillator 方波作为 modulator 时的方波轮廓仍不够明显。

目标是在保留 Tr1 旋律/音色的前提下，增强 modulator 对 carrier 的频谱塑形。

## 复现方式

1. 录制一段有明确旋律的 Tr1 carrier。
2. 开启 vocoder，Carrier 选择 `Tr1`。
3. 使用麦克风讲话，或同时开启 Oscillator/MyDelay 作为 modulator。
4. 对比是否能听到更明显的说话开合、方波轮廓或 modulator 频谱变化。

## 根因定位

当前 vocoder 已经能收到连续 carrier，但 DSP 里仍有两点会削弱调制感：

- carrier body 路径用于保留旋律，但比例偏高时会遮蔽由 band envelope 产生的调制效果。
- 各 band envelope 直接乘 carrier band，频段之间的相对差异不够突出；听感容易变成“carrier 被整体音量打开”，而不是明显被 modulator 频谱塑形。

`Bands` 参数会影响听感：较小 band 数会让频段更粗、调制更明显；较大 band 数更细，但在当前轻量算法下可能显得薄和散。因此本次把新建默认值调低，但核心修复仍在 DSP 包络塑形。

## 修改方案

- `src/dsp/vocoder.rs`
  - 增加 band envelope 对比增强：根据平均 voice envelope 对每个 band 的 envelope 做相对强调。
  - 降低 carrier body gain，减少它对调制 band 的遮蔽。
  - 提高 sibilance gain，让高频瞬态/辅音/方波边缘更容易带出来。
- `src/config/vocoder_configs.rs`
  - 新建 vocoder 默认 `Bands` 从 `16` 调为 `10`，作为更强调调制感的默认起点。

## 影响范围

- 影响 vocoder 湿声音色和 modulator 明显程度。
- 不改变 carrier 队列传输逻辑，上一轮“旋律保留”的修复仍保留。
- 不改变 Project 存档字段。
- 已有 vocoder slot 不会自动套用新的默认 Bands，需要手动调参。

## 手工验证步骤

1. 使用已有 slot 时，先手动尝试 `Bands=10`，也可以对比 `Bands=8` 和 `Bands=16`。
2. 保持 `Attack=6`、`Release=80`、`Level=100`、`Mix=100`。
3. 用 Tr1 作为 carrier，麦克风讲话作为 modulator，确认说话开合和元音变化比修改前更明显。
4. 开启 Oscillator 方波作为 modulator，确认方波带来的粗糙开合/边缘感更明显。
5. 确认 Tr1 的旋律仍然存在，没有退回到沙沙噪声。

## 残余风险

- 这仍是轻量 filter-bank vocoder，无法完全等同商业 vocoder 或 phase vocoder 的清晰度。
- Band 数存在取舍：`8-10` 更有调制感，`12-16` 可能更细但更弱。
- 如果 carrier 频谱本身太暗或太窄，modulator 的说话感仍会受限。
