# 0.2验证记录

2026-09-27，Windows，Rust默认feature。本文件只记录实际执行结果；物理声卡与RC‑505mkII对照另列。

## 自动化

- `cargo check --all-targets`通过。
- `cargo test --all-targets`：35项通过，包含新增存档、采样时钟、回放、实时分配和面板键盘导航测试。启动器没有独立单元测试。
- 两个release可执行目标成功构建。仍有未使用旧配置辅助字段及风格告警，不声称warnings清零。
- `cargo clippy --all-targets`返回0；存在风格告警，未启用 `-D warnings`。
- 五轨重FX、参数交换、录放/叠录、COW快照、Undo、Clear测试统计音频渲染区间分配及释放次数，结果0。
- 不同BPM（73/117/129/299）及44.1/48/96kHz的10000拍边界无累积舍入漂移。
- 补偿尾部、固定长度、One Shot、Reverse、Stop模式、Undo/Redo和snapshot共享页不可变性。
- 回放包含录音、叠录、Undo/Redo、参数变化、Stop/Play；输出与原始计算逐采样逐位一致，最终loop相同；损坏命令文件拒绝渲染。
- 回放保留非整数毫秒的采样补偿；不同起始绝对frame不改变暂停轨Vocoder载波相位。Track Filter序列实测门限段与开启段的输出能量差，避免只更换时钟字段但未接通调制。
- Snapshot float位模式、Undo文件、SHA-256损坏检测、路径穿越拒绝、44.1→48kHz时长与正弦误差。
- 三次回环相关性分析用已知173采样延迟验证精确定位；静音输入拒绝建议。这是算法测试，不是物理声卡测量。
- 保留序列重叠/触发、egui钢琴卷帘鼠标事件、Filter线性、MyDelay持续采样、Vocoder频谱移动、Roll冻结/次数、Reverb衰减、旧工程codec与预设测试。

Windows刷盘测试发现只读句柄调用sync_all会被拒绝，已修为可写句柄。保存失败不会更新工程指针或继续退出。

## 视觉检查

实际egui渲染器离线截图：工程选择页、1320×900演奏台、960×720小窗口、完整钢琴卷帘、Vocoder、Audio setup启动器。修正中文设备名字体回退、固定面板滚动条和小窗口上方面板高度。文档截图来自应用运行输出，非设计稿。

```powershell
cargo build --bins
.\target\debug\rc505_rs.exe --offline --data-dir=var/ui-verification/data --ui-preview=projects
.\target\debug\rc505_rs.exe --offline --data-dir=var/ui-verification/data --ui-preview=performance
.\target\debug\rc505_rs.exe --offline --data-dir=var/ui-verification/data --ui-preview=performance-small
.\target\debug\rc505_rs.exe --offline --data-dir=var/ui-verification/data --ui-preview=sequence
.\target\debug\rc505_launcher.exe --data-dir=var/ui-verification/data --ui-preview=launcher
```

离线截图不能当成现场音频、键盘rollover、鼠标演奏流或声卡测试。面板键盘和钢琴卷帘动作由egui输入事件测试补充。

## 安装与迁移

Inno Setup6.7.3从官方Release下载，验证了有效Pyrsys B.V.签名。打包脚本、发布工作流及更新助手在本仓库维护，安装包与临时构建文件不进入git。

现有数据已在隔离目录完成复制及校验演练：6个工程索引条目、4个实际配置文件；另2个条目在源目录已缺失文件。迁移保留它们并记录缺失信息，不用默认配置冒充恢复成功。原数据未修改。

正式Release、E盘安装与跨版本更新结果将在完成后补充到本记录。

## 未验证的设备范围

- 用户声卡的长时间压力、拔插/休眠、64/128帧稳定性、ASIO驱动。
- 物理回环补偿准确度与耳朵听到的实际监听延迟。
- 实际键盘多键冲突、输入法和手部演奏体验。
- RC‑505mkII成对干声/输出测量、元音可懂度、Roll瞬态、Delay时间切换和Reverb声场的响度匹配A/B。
- 系统突然断电、磁盘满、安全软件拦截等故障组合；当前提交与哈希机制不代表所有文件系统均有相同断电保证。

回放起点要求五轨停止并重置FX尾音；5分钟/轨、30分钟/回放；renderer版本不匹配时拒绝播放，不静默更换算法。完整语义和限制见[操作手册](USER_GUIDE_CN.md)。
