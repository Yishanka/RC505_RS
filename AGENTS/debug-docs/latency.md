# 录制延迟与多轨对齐方案总结

## 问题简述

核心问题：在“听着先前已录轨道进行新轨录制”时，回放后节奏会出现不对齐。  
且历史上出现过更严重现象：录制新轨时，原本已对齐的轨道（如 Track1/2）也会听起来被带偏。

这类问题本质是 **输入采集延迟 + 回调时钟域不一致 + 播放头漂移累积** 的组合。

---

## 原因分析

1. 输入采集有固有延迟  
麦克风输入到应用可用样本存在固定/半固定延迟（设备、驱动、缓冲都会引入）。若直接按拍点开始/结束录音，录到 buffer 的内容会整体“晚到”。

2. 录音停止时若不补尾，会截断有效尾部  
如果只把 buffer 整体左移做补偿，末尾可能被截断，导致 loop 有效内容和逻辑长度不一致。

3. 时间线调度必须单时钟域  
当前代码将时间线推进统一放到 output callback，避免 input/output 两个回调都改状态造成相位抖动与锁竞争干扰。

4. 暂停恢复与长期播放会有播放头漂移  
即使初始对齐正确，播放 cursor 与逻辑拍点（UI/metronome 的 anchor+loop_duration）长期可能出现微小偏差，累积后可感知。

---

## 解决方案与代码片段

### 1) 将毫秒补偿换算为采样点

在流构建时把用户配置的 `latency_comp(ms)` 换算为 `latency_comp_samples`：

```rust
let latency_comp_samples =
    ((config.sample_rate.0 as f32 * latency_comp as f32 / 1000.0) as usize)
        * config.channels as usize;
```

对应代码：`src/engine/audio_io.rs`（`build_streams`）

---

### 2) 录音停止采用“补尾 + 左移 + 定长回收”

停止录音时不是立刻结束，而是继续录 `latency_comp_samples` 个 sample：

```rust
track.record_target_len = Some(track.buffer.len());
track.record_tail_remaining = latency_comp_samples;
```

尾部录完后统一 finalize：

```rust
shift_buffer_earlier_in_place(&mut track.buffer, latency_comp_samples);
if let Some(target_len) = track.record_target_len {
    if track.buffer.len() > target_len {
        track.buffer.truncate(target_len);
    } else if track.buffer.len() < target_len {
        track.buffer.resize(target_len, 0.0);
    }
}
```

作用：
- 左移：把晚到的录音“拉回拍点”。
- 定长：保证 loop 时长不变，不破坏多轨循环关系。

对应代码：`src/engine/audio_io.rs`（`process_timeline` + `finalize_recording_stop`）

---

### 3) Overdub 按补偿提前写入

叠录时，不写当前播放点 `idx`，而是写到提前补偿后的位置：

```rust
let comp = latency_comp_samples % len;
let write_idx = (idx + len - comp) % len;
track.buffer[write_idx] = (track.buffer[write_idx] + input_processed).clamp(-1.0, 1.0);
```

作用：叠录内容与当前节拍更一致，减少“越叠越晚”。

对应代码：`src/engine/audio_io.rs`（output callback）

---

### 4) 时间线调度统一在 output callback（关键稳定性修复）

当前逻辑中，`process_timeline(now, latency_comp_samples)` 在 output callback 执行，避免双回调域抢状态。  
input callback 只做采样处理、入 ringbuffer、录音数据写入。

此外输入侧写入已改为块写入，降低锁持有时间：

```rust
track.buffer.extend_from_slice(&processed_block);
```

作用：减少录制新轨时对播放线程的干扰，避免“录 Track3 影响 Track1/2”。

对应代码：`src/engine/audio_io.rs`

---

### 5) Pause -> Play 按节拍策略恢复

在 `TrackState::Play`：
- 节拍器未开启：从头播放（progress=0）
- 节拍器开启：按当前逻辑进度恢复

```rust
let metronome_running = self.metronome.start_time().is_some();
let progress = if metronome_running {
    Some(self.tracks[idx].track_play_progress(now))
} else {
    self.tracks[idx].track_play_anchor_at = Some(now);
    Some(0.0)
};
engine.play_at_progress_now(idx, progress);
```

引擎侧把 progress 映射到 cursor：

```rust
let cursor = ((normalized * len as f32).floor() as usize).min(len - 1);
track.play_cursor = cursor;
```

对应代码：`src/app.rs` + `src/engine/audio_io.rs`

---

### 6) 运行时漂移纠偏（防累积）

播放/叠录态下按逻辑拍点计算目标 cursor，超过阈值才纠偏：

```rust
engine.sync_playhead_if_drift(idx, progress, 0.01);
```

引擎内判断循环距离，超阈值再重置播放头，避免频繁抖动：

```rust
if cyclic > max_drift {
    track.play_cursor = target;
    track.overdub_cursor = target;
}
```

对应代码：`src/app.rs` + `src/engine/audio_io.rs`

---

## 当前方案的结论

目前代码的延迟处理不是单点修补，而是完整链路：
- 采集延迟补偿（ms->samples）
- 停止录音补尾与定长恢复
- 叠录提前写入
- 单时钟域调度（防互相扰动）
- Pause/Play 相位恢复
- 漂移守护校正

这套组合正是为了解决“听着已录轨录新轨导致节奏不齐”这个核心问题，并避免新增录音反过来破坏已对齐轨道。
