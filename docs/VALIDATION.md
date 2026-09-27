# 验证记录

日期：2026-09-27。Windows，Rust默认feature，两个可执行目标。以下区分已执行检查和需要实际演奏设备的后续验收。

## 已执行

| 检查 | 结果 |
|---|---|
| `cargo check --all-targets` | 通过 |
| `cargo test --all-targets` | 24项通过，无失败；启动器无独立单元测试 |
| `cargo build --release --bins` | 主程序与启动器构建成功 |
| `cargo clippy --all-targets` | 返回0；仍有风格告警，未达到 `-D warnings` 清零标准 |
| 修改的Rust源文件rustfmt | 已格式化 |
| 原生egui窗口离线截图 | 演奏台1320×900、窄窗口960×720、钢琴卷帘、滤波器、AHDSR、Vocoder、Roll、Reverb、启动器780×640 |

截图以当前显示缩放渲染，PNG像素尺寸为逻辑尺寸的1.5倍。检查了布局、窄窗口优先展示轨道、编辑器返回入口、参数可见性、网格及图形。文档使用实际应用渲染的 [演奏台](images/performance.png)、[钢琴卷帘](images/sequence.png)、[启动器](images/launcher.png)，没有使用设计稿替代运行结果。

## 自动化覆盖

- 序列：重叠替换保留两侧音符、重复同音独立触发、复制/序列化边界、异常导入限制。
- egui鼠标事件：画音符、选中/拖动移动、右边缘改长度、删除、撤销和重做；选中后网格位置保持稳定。
- 触发器：每tick仅触发一次、同音连续step及重启；在第一个tick内AHDSR能够完成Attack。
- 输入门限与MyDelay：持续正弦的过零点不会使门限反复关闭；连续输入能完成采样。
- Filter：零Drive保持线性，纯干声为恒等映射。
- Vocoder：纯干声、无载波、左右载波独立、释放到静音；双频载波的输出频谱随正向formant移动朝较高分量偏移。
- Roll：冻结过去的处理后立体声，随后输入变化不替换冻结内容；有限Roll2在指定循环次数后返回干声。
- Reverb：脉冲输出有限且衰减；500ms预延迟前无湿声；独立干湿的零电平端点。
- 键盘推子：短按、精细调整、30/144FPS变化量一致、多轨反向变化、上下界静音/满音量、停止/恢复。
- 轨道命令：录音中Stop请求完成后停止；等待完成时连续触发不会取消停止或提前叠录。
- 清空音轨：同时清除冻结Roll、Delay尾音与载波队列；清空后输入静音不会重新播放旧片段。
- 存档：音量/推子速度与新增FX参数往返、旧字段默认值、预设链路匹配、加载保留BPM/开关。
- 声卡协商：使用输入/输出交集，支持共同的单/双声道格式；没有交集时返回错误。

严格测试曾暴露Roll开启帧混入当前输入和Vocoder逐带削平包络的问题；修正后保留这些回归用例。

## 复现视觉检查

```powershell
cargo build --bins
.\target\debug\rc505_rs.exe --offline --data-dir=var/ui-verification/data --ui-preview=performance
.\target\debug\rc505_rs.exe --offline --data-dir=var/ui-verification/data --ui-preview=performance-small
.\target\debug\rc505_rs.exe --offline --data-dir=var/ui-verification/data --ui-preview=sequence
.\target\debug\rc505_launcher.exe --data-dir=var/ui-verification/data --ui-preview=launcher
```

其他主程序模式：`filter`、`envelope`、`vocoder`、`roll`、`reverb`。窗口绘制数帧后输出 `var/ui-verification/<mode>.ppm` 并关闭。该目录在git中忽略；截图转换成PNG后，仅选定文档图片放进 `docs/images/`。release不包含捕获工具。

原生桌面自动化连接在本会话中无法连接；窗口外观用应用自身截图接口验证，钢琴卷帘动作用egui输入事件验证。没有把未执行的OS级鼠标/多键操作计作通过。

## 实际演奏验收（尚未执行）

1. 使用耳机和目标声卡，分别测试44.1/48kHz；录制脉冲与一小节乐句，检查补偿、循环接缝、叠录边界和长时间漂移。
2. 同时按 `Z + V`，确认轨1下降、轨2上升；Shift精调；松键、失焦、进入编辑器和文本输入时停止变化。检查实际键盘能识别需要的组合。
3. 长按Q应只切换一次FX；不同轨录放键可同时触发。测试录音中Stop、待完成时连续触发、清空后恢复等路径。
4. 对同一干声在硬件与软件录制匹配参数输出；进行响度匹配的Vocoder元音/齿音、Roll瞬态、Delay反馈、Reverb脉冲与听感比较。
5. 同时开启五轨及多个效果并持续编辑，统计CPU、回调最坏耗时、underrun和延迟；实际切换声卡、休眠恢复及ASIO环境另行验收。
6. 测试正常保存、目录不可写、损坏JSON、重新打开、两进程同时管理工程等情形。单元测试已覆盖参数codec，未覆盖所有文件系统故障。

## 已知边界

工程尚不保存loop音频；回调仍有锁和动态分配；时序仍依赖系统时钟及现有补偿流程；Input FX按类型分组处理；硬件全通道路由、撤销叠录、MIDI与自动化尚未完成。Vocoder和Roll的参数语义已向官方手册靠近，但未完成实机声学标定。下一步见 [PLAN](../AGENTS/PLAN.md)。
