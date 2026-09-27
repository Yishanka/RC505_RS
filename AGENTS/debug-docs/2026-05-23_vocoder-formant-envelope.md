# Vocoder Formant Envelope Debug

## 问题描述

上一轮修复后，vocoder 已经能保留 Tr1 carrier 的旋律，但听感仍不像硬件 vocoder。用户举例说明：平淡 bass 作为 carrier，通过人声“啊哦啊哦”调制时，应能得到类似 vowel/formant 扫动的 bass 质感，而不仅是音量开合或辅音清晰度。

本次目标不是特化 growl bass，而是让 vocoder 更通用地响应 modulator 的元音/formant 频谱轮廓。

## 复现方式

1. 录制一段平稳、有谐波的 bass 或合成器到 Tr1。
2. 开启 vocoder，Carrier 选择 Tr1。
3. 用人声持续发“啊哦啊哦”，或用其它有明显频谱变化的输入作为 modulator。
4. 监听 vocoder 输出是否出现随口型变化的频谱扫动，而不是仅有旋律和音量变化。

## 根因定位

现有算法已经从块级 carrier 快照改为连续 carrier 流，因此旋律能保留。但 DSP 仍主要是基础 filter-bank：

- 每个 band 的包络直接乘对应 carrier band。
- 之前只提高了整体 envelope 对比，未专门强调元音/formant 峰值。
- carrier body 虽能保留旋律，但过多会稀释 modulator 的频谱塑形。

RC-505 类 vocoder 的核心听感不只是辅音清晰，而是 modulator 的动态频谱包络，尤其是 200Hz-2800Hz 附近的元音共振峰，对 carrier 产生明显重塑。

## 修改方案

- `src/dsp/vocoder.rs`
  - 增强 band envelope 对比，从 `1.45` 提升到 `1.9`。
  - 新增 formant 区域权重：对 `50Hz-12000Hz` 的动态峰值给予更强塑形能力。
  - 使用局部 band 平均检测 formant peak，而不是只看全局平均。
  - 降低 carrier body gain，减少未调制 carrier 对 formant 塑形的遮蔽。
  - 对 wet 输出加入轻微 tanh saturation，让被重塑后的 carrier 更稳定、更有存在感。

本次没有改变 carrier 队列传输，也没有改变 InputFX 中麦克风/Oscillator/MyDelay 都可作为 modulator 的行为。

## 影响范围

- 影响 vocoder 湿声的频谱塑形和 formant 响应。
- 不新增 UI 参数。
- 不改变 Project 存档字段。
- 不改变 Tr1-Track5 carrier 选择逻辑。
- 保留上一轮 carrier 连续流修复。

## 手工验证步骤

1. 用有谐波但相对平稳的 bass/合成器录制 Tr1。
2. Vocoder 选择 Tr1，建议先用 `Bands=8-10`、`Attack=6`、`Release=80`、`Level=100`、`Mix=100`。
3. 持续发“啊哦啊哦”，预期能听到 carrier 的频谱随口型产生更明显的扫动。
4. 对比修改前，预期不是只听到音量开合，而是中低频/中频质感随 vowel 改变。
5. 确认 Tr1 旋律仍保留，没有退回沙沙噪声。

## 残余风险

- 仍是轻量实时 filter-bank vocoder，不是完整商业硬件算法复刻。
- 如果 carrier 太接近纯正弦或谐波不足，formant 重塑仍不明显。
- 如果用户需要更强烈效果，后续可能需要增加可调的 Formant/Color/Drive 参数，但本次未新增 UI。
