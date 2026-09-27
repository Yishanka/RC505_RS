# 0.2验证记录

2026-09-27，Windows，Rust默认feature。本文件只记录实际执行结果；物理声卡与RC‑505mkII对照另列。

## 自动化

- `cargo check --all-targets`通过。
- `cargo test --all-targets`：37项通过，包含新增存档、采样时钟、回放、实时分配、面板键盘导航以及Shift数字/标点、失焦恢复隔离测试。启动器没有独立单元测试。
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

已实际完成本地0.2.0安装基线→正式[Release 0.2.3](https://github.com/Yishanka/RC505_RS/releases/tag/v0.2.3)的更新通路：安装版检查版本、从GitHub下载、SHA-256校验、等待正常退出、运行安装器并重新启动。主程序和启动器均为0.2.3，新图标及版本资源已嵌入。

程序安装在 `E:/SJTU/rc505`，数据在 `E:/SJTU/rc505/data`，下载缓存在 `E:/installer`。迁移后的10个原有文件（含3份录屏）更新前后哈希全部相同。旧版更新助手不会执行新缓存策略，已用新版本的缓存归一命令完成这次过渡；当前只保留 `RC505-RS-setup.exe` 和校验清单。后续由0.2.3助手自动在成功更新后归一缓存。

云端独立Windows环境最初暴露测试进程的驱动访问异常。配置构造原先会枚举声卡；已把扫描移到交互式启动，项目/预设/离线回放只操作纯配置。修复后Rust1.98.1云端检查及35项测试通过，本机Rust1.93.0也通过。

实际升级验证发现0.2.1会校验空白的可选导入目录并中止。0.2.2改为显式选择是否导入；新增独立测试身份的安装器smoke test，不修改正常产品的注册表/快捷方式。已验证不导入的新安装、再次安装、数据哨兵保留，以及只保留一个最新版安装缓存并保留无关软件文件。

0.2.2 Windows x64构建改为静态C运行库，35项测试通过；检查主程序和启动器PE导入表，均无VCRUNTIME/MSVCP外部依赖。

安装器测试加入不兼容PSModulePath模拟；0.2.3更新助手使用系统Windows PowerShell的绝对路径及系统模块目录，本机测试通过。图标检查了真实alpha和16–256px ICO层，32px缩略图清晰；原图、ICO和生成提示保存于仓库。

包含图标与环境修复的云端[完整验证](https://github.com/Yishanka/RC505_RS/actions/runs/36308713262)已通过：37项测试及安装器smoke test全部成功。

## 未验证的设备范围

- 用户声卡的长时间压力、拔插/休眠、64/128帧稳定性、ASIO驱动。
- 物理回环补偿准确度与耳朵听到的实际监听延迟。
- 实际键盘多键冲突、输入法和手部演奏体验。
- RC‑505mkII成对干声/输出测量、元音可懂度、Roll瞬态、Delay时间切换和Reverb声场的响度匹配A/B。
- 系统突然断电、磁盘满、安全软件拦截等故障组合；当前提交与哈希机制不代表所有文件系统均有相同断电保证。

回放起点要求五轨停止并重置FX尾音；5分钟/轨、30分钟/回放；renderer版本不匹配时拒绝播放，不静默更换算法。完整语义和限制见[操作手册](USER_GUIDE_CN.md)。
# 0.2.4 界面与启动验证（2026-09-27）

- `cargo check --all-targets --locked`、`cargo test --all-targets --locked` 通过：主程序 41 项，启动器 2 项。覆盖删除短按/长按/双击、切轨与取消、旧语言配置迁移、动作按钮 Enter 激活；保留原有 DSP、回放与快照测试。
- `cargo build --release --bins --locked` 与本地安装器打包通过。安装器验证两次无导入安装、数据哨兵保持、两个 PE 的 GUI 子系统、维护命令管道输出，以及仅保留一份安装包。
- 使用独立 `var/ui-verification/language-test` 数据目录与离线预览检查中文大窗口、英文 960×720 小窗口、双语工程选择页、钢琴卷帘、英文帮助；音频启动器检查语言按钮与渲染。小窗口工作区纵向滚动，五轨保持在窗口宽度内。截图见 `docs/images/`。
- 本次未改动音频 DSP。真实键盘连击手感、声卡延迟与听感仍需实际演奏验证；离线截图不作为音频测试。
