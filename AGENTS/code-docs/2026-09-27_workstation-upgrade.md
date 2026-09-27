# 2026-09-27 工作台与现有FX升级

任务类型：交互扩展、算法修正、结构整理与文档维护。用户明确允许重写AGENTS/PLAN，并补充授权完善既有Vocoder和Roll；本轮没有新增FX种类。

## 实现

- 主界面由仅绘制的面板改为可操作工作台，快速/展开视图共享参数，提供固定槽位编辑、钢琴卷帘、DSP曲线和单槽预设。
- 启动器保留并共享主题，传递隔离数据路径和离线选项；窄窗口优先展示演奏控制。
- 键盘推子独立处理五轨：分贝、短按、积分加速、Shift精细、失焦停止；演奏开关忽略OS自动重复。录放命令与鼠标共用。
- 修正音符在一个tick内持续retrigger、阈值过零抖动、MyDelay重复采样、Filter零Drive饱和、录制完成等待态以及暂停/清空任务的逻辑。
- Vocoder重写为缓存滤波器组和立体声合成，保留原共振峰增强意图；整组包络归一化修正逐带削平，并增加Tone、灵敏度、Formant、Sibilance和输入载波。
- Roll替换整loop叠加，捕获前级处理后的立体声片段，增加两种模式及完整配置链路；Delay节拍同步；Reverb独立干湿/密度/扩展预延迟。
- 新参数通过工程和预设codec保存，具有旧字段默认值；波形显示有界缓存；FX快照在锁外构建；输入输出采用共同格式。

## 验证与限制

自动化、窗口截图及人工验收项统一在 [验证记录](../../docs/VALIDATION.md)。Vocoder原始未提交版本曾备份到本地 `var/ui-verification/vocoder-before-enhancement.rs`，不作为发布源码；原2026-05调试文档保留。

本轮不宣称回调完全无锁/无分配、实机采样级时序或硬件音色一致。工程仍不保存loop音频，Input FX仍按类型分组处理。剩余工作按 [PLAN](../PLAN.md) 的P0/P1优先推进。

用户手册在 [USER_GUIDE_CN](../../docs/USER_GUIDE_CN.md)，架构在 [ARCHITECTURE](../../docs/ARCHITECTURE.md)，来源与差异在 [RC505_REFERENCE](../../docs/RC505_REFERENCE.md)。旧多层维护规则退为历史资料，根目录AGENTS.md是当前入口。
