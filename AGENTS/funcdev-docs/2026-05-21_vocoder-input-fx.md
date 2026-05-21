# Vocoder InputFX

## 功能目标

新增 `Vocoder` 作为 InputFX。麦克风/输入声音作为 modulator，track 输出作为 carrier，经多段带通包络跟随后输出。InputFX 在输入回调中处理，因此录制到 track 的声音是已经经过 vocoder 调制后的结果。

## 子功能与参数

- `Carrier`：enum 参数，取值为 `Tr1` / `Tr2` / `Tr3` / `Tr4` / `Tr5`。
- `Bands`：vocoder 分析/合成频段数，4-16。
- `Attack(ms)`：包络跟随上升时间，0-200 ms。
- `Release(ms)`：包络跟随释放时间，0-1000 ms。
- `Level`：湿声输出电平，0-100。
- `Mix(%)`：干湿混合，0-100。

## 效果链顺序

当前 InputFX 引擎不是严格按 slot 顺序逐个串联，而是固定分组处理：

1. Oscillator 先生成声音并混入输入信号。
2. MyDelay 再基于原始输入生成声音并混入。
3. Vocoder 处理上一步得到的总输入信号，使用所选 track 作为 carrier。
4. Filter 处理 vocoder 后的输出。
5. Reverb 处理最后输出。

因此，人声会先进入 vocoder；若同一 bank 里还启用了 Filter/Reverb，则 vocoder 后的声音会继续通过这些 InputFX。

Oscillator/MyDelay 等自动播放声音会进入 vocoder 的 modulator 侧。若它们和 vocoder 同时启用，并且 vocoder carrier 选择了一个有声音的 track，则 carrier 会被“人声 + Oscillator/MyDelay 输出”的包络共同调制。`Mix=100` 时听到的是 vocoder 湿声；`Mix<100` 时会保留一部分 vocoder 前的干声/自动播放声。

## 涉及代码层级

- UI：`src/ui/looper.rs`
  - `FxSelect` 增加 `Vocoder` 选项。
  - 增加 `InFxVocoder` 参数页与 breadcrumb。
- Config：`src/config/vocoder_configs.rs`、`src/config/input_fx_configs.rs`、`src/config/mod.rs`
  - 新增 vocoder 参数结构与 track-only carrier enum。
  - `InputFx` / `FxKind` 增加 `Vocoder`，参与 slot 类型循环。
- DSP：`src/dsp/vocoder.rs`、`src/dsp/mod.rs`
  - 新增多频段 vocoder DSP，使用输入信号包络驱动 carrier 各频段输出。
- Engine：`src/engine/input_fx.rs`、`src/engine/audio_io.rs`
  - InputFX runtime/state 接入 vocoder。
  - output callback 保存每个 track 经 TrackFX 处理后的最新输出帧，input callback 将该快照作为 track carrier 传给 vocoder。
- Project：`src/project.rs`
  - 保存/读取 `Vocoder` 类型与所有参数。

## 兼容性说明

旧工程没有 `vocoder` 字段时通过 `serde(default)` 保持兼容。旧工程里如果曾保存合成器 carrier 或 `SynthHz` 字段，读取时会忽略 `SynthHz`，无法识别的 carrier 会保留默认 `Tr1`。

Track carrier 使用的是对应 track 已经经过 TrackFX 的输出。播放中的 track 始终可提供 carrier。暂停 track 在 metronome 开启时也会用独立的只读 carrier 光标继续提供 carrier；该光标不影响真实播放头。空轨、删除轨或 metronome 关闭时的暂停轨不提供 carrier。

## 手工测试步骤

1. 启动程序，进入 MainScreen。
2. 按 `Q/W/E/R` 选择一个 InputFX slot，在 `FxSelect` 中用左右键切到 `Vocoder`，按 Enter。
3. 录制一个 track，给它打开 TrackFX，再把 vocoder carrier 切到对应 `Tr1`-`Tr5`。
4. 播放该 track 时讲话，应听到以该 track 输出为 carrier 的调制音色。
5. 暂停该 track，保持 metronome 运行，继续讲话，应仍能听到该 track 作为 carrier；关闭 metronome 后暂停轨 carrier 应消失。
6. 同时启用 Oscillator 或 MyDelay 与 Vocoder，确认这些自动声音会参与调制 carrier。
7. 录制到任一 track，回放时应听到录进去的是 vocoder 后的声音。
8. 保存工程、返回项目页再载入，确认 slot 仍为 `Vocoder` 且参数保留。

## 已知限制与后续点

- Track carrier 是 output callback 的最新帧快照，和 input callback 存在一个音频块级别的时间差；实时演奏可用，但不是采样精确对齐。
- 暂停轨 carrier 使用独立只读光标推进，不改变该 track 的真实播放头；重新播放时仍按原播放逻辑对齐。
