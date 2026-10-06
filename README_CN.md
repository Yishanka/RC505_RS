# RC505 RS

面向键盘演奏与可视化音色编辑的五轨 Loop Station，使用 Rust、CPAL 和 egui。参考 BOSS RC‑505mkII 的操作方式，并提供复音钢琴卷帘等软件扩展。这是独立实现，尚未验证与硬件的音色一致性。

**目前仅支持 Windows x64，不支持 macOS 或 Linux。**

[English](README.md) · [下载最新发布版](https://github.com/Yishanka/RC505_RS/releases/latest) · [详细操作手册](docs/USER_GUIDE_CN.md) · [安装与更新](docs/INSTALL_UPDATE_CN.md)

![演奏台](docs/images/performance.png)

## 从下载到打开软件

1. 打开[最新发布页](https://github.com/Yishanka/RC505_RS/releases/latest)，向下找到并展开 **Assets（资源）**。
2. 下载 **`RC505-RS-版本-windows-x64-setup.exe`**。首次安装只需这一个 EXE；不必下载便携包、源码或更新清单。**Source code (zip / tar.gz)** 是需要自行编译的源码；`…portable.zip` 是另一种免安装使用方式。
3. 下载完成后，双击这个 `setup.exe`。若浏览器或 Windows 显示信誉提示，先按下方说明核对来源；安装向导目前为英文。
4. 在 **Select Destination Location（选择安装位置）** 选择程序目录，点击 **Next（下一步）**。在 **Data and download folders（数据与下载目录）** 分别选择工程数据目录、更新安装包下载目录；数据默认在程序旁的 `data` 中，各目录都可自行选择。
5. 到 **Import existing data (optional)（可选：导入已有数据）**，首次使用或普通更新直接保持不勾选并点 **Next**。只有需要从另一个数据目录复制工程时，才勾选导入并选择原数据目录。
6. 按需勾选 **Create a desktop shortcut（创建桌面快捷方式）**，继续到 **Install（安装）**。完成后保留 **Open RC505 RS（打开软件）** 勾选，点击 **Finish（完成）**。
7. 看到工程选择页即安装完成。以后从开始菜单的 **RC505 RS** 或桌面快捷方式打开；顶部可切换中文/英文。选择工程后点「打开工程」，或点「新建」开始。

安装包尚无代码签名，可能出现信誉提示。**只有确认文件来自上面的 `Yishanka/RC505_RS` 官方发布页，且提示仅为下载不常见／应用未识别时**，才选择继续：Edge 下载列表中点文件旁的 `…` → **保留 / Keep** → **显示更多 / Show more** → **仍然保留 / Keep anyway**；Windows「已保护你的电脑」中点 **更多信息 / More info** → **仍要运行 / Run anyway**。这些按钮可能因系统策略而不提供。[Edge 官方说明](https://learn.microsoft.com/en-us/troubleshoot/microsoft-edge/development/download-failures#check-security-and-smartscreen-settings)、[Windows 官方说明](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/publish-first-app#step-6-handle-smartscreen-for-new-apps)

若提示病毒、恶意软件，或被 **Smart App Control / 组织策略** 阻止且没有继续按钮，请停止安装并向项目报告具体提示；不要关闭安全保护来安装。[Smart App Control 不提供单个应用放行入口](https://support.microsoft.com/en-us/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)。完整按钮对照、可选校验与便携方式见[安装说明](docs/INSTALL_UPDATE_CN.md)。

**已有版本无需先卸载。** 软件内按 **F12 → 更新 → 检查更新 → 下载并校验**；停止演奏、处理回放草稿后，点击 **保存快照，退出并安装更新**。也可关闭软件，直接运行新版安装包并沿用原来的三个目录，导入选项保持不勾选。启动时默认只检查新版，可在同一更新页关闭检查；更新保留已保存的数据，Windows 卸载默认也保留数据。

## 开始演奏

1. 新建或打开工程，在左侧「音频」选择输入设备。输出默认跟随 Windows；也可指定固定声卡。
2. 设置 BPM，按 `1` 录音，再按一次结束并循环；播放时再按进入叠录。其他轨道用 `2–5`。
3. 点击效果槽，在小面板调整参数，或展开进行可视化编辑。编辑参数时仍可用演奏键录放、调推子和切换效果。
4. **Ctrl+S** 保存配置；**Ctrl+Shift+S** 保存配置与五轨音频快照。
5. **Esc** 从编辑界面返回演奏，再返回工程选择页；**F12** 打开帮助。

顶部「键位」可自定义快捷键，支持冲突检查和恢复默认。下表为默认值，软件内键帽显示实际绑定。

| 操作 | 默认按键 |
|---|---|
| 轨 1–5 录放、叠录、完成 | `1–5` |
| 停止对应轨 / 全部开始或停止 | `Shift+1–5` 或 `F1–F5` / `Space` |
| 选轨 | `Ctrl+1–5` |
| 对应轨撤销 / 重做 | `Alt+1–5` / `Ctrl+Alt+1–5` |
| Input / Track FX | `Q W E R` / `U I O P` |
| 临时 FX / 选效果组 / 编辑槽 | `Shift` / `Alt` / `Ctrl` + FX 键 |
| 五轨推子下 / 上 | `Z X`、`C V`、`B N`、`M ,`、`. /` |
| 调整对应轨推子速度 | `Shift` + 对应推子键 |
| 顶部 / 左面板 / 效果面板 | `F6` / `F7` / `F8` |
| 录制回放 / 回放库 | `F9` / `F10` |
| 静默录入 / 节拍器 | `J` / `K` |
| 清空所选轨 | 长按 `Delete` 0.75 秒，或 350 ms 内双击 |

上下或 Tab 选择参数，左右调整数值：时间按 1 ms、分贝按 0.5 dB，长按连续变化；Enter 输入精确数值。推子短按 0.5 dB，长按逐渐加速，每轨可设置 1–60 dB/s。

## 音色、轨道与回放

- 五轨独立反向、单次播放、停止模式、录音长度与量化；录音、叠录和清空支持每轨最多 8 步撤销/重做，实际步数受音频内存预算限制。
- OSC 集成合成波形、人声波形和采样音色，提供单音/8/16 声部、连奏与滑音、可编辑包络、两套 LFO 和内部滤波。小面板可直接「捕获音色」；新采样默认临时，需明确「保存为音色」才能在重开工程时载入。
- 钢琴卷帘支持和弦、框选、多选编辑、单音试听、共享乐句与下一循环切换。音色预设和乐句预设分开保存。
- 输入与轨道效果包含移调、修音、和声、失真、动态、均衡、延迟、混响和调制等，主输出另有滤波、压缩与混响。覆盖范围和硬件差异见[效果清单](docs/RC505_MK2_FX_CATALOG_CN.md)。
- 全局输入噪声门可设置阈值，减少环境底噪进入效果器；提供效果延迟对齐及回环录入补偿，OSC 保持内部拍点。
- 静默录入关闭输入监听，轨道录音与已有循环照常运行；节拍器和独立试听只送监听，不写入轨道。
- 工程首页提供全局回放库。回放记录输入和采样编号操作，播放时实时演算，可暂停、拖动进度、导入任意工程或新工程；只有明确导出才生成完整 WAV。
- 顶部可切换中文/英文与薄荷绿、雾粉、橙红主题；背景显示输出频谱。

![钢琴卷帘](docs/images/sequence.png)

## 源码与自行编译

**发布版**是可直接安装使用的 Windows 程序；**源码仓库**用于自行编译和修改。允许 fork、clone 源码、自行编译使用与改动，欢迎提交 Issue 和 PR。

在 Windows x64 安装 Rust MSVC 工具链和 Visual Studio C++ Build Tools 后运行：

```powershell
git clone https://github.com/Yishanka/RC505_RS.git
cd RC505_RS
cargo build --release --bins --locked
.\target\release\rc505_rs.exe --data-dir=.\local-data
```

这会运行自行编译的程序，并使用独立的 `local-data` 目录，不会安装或替换已有发布版。可选音频设置启动器为 `target\release\rc505_launcher.exe`。

[更新记录](docs/RELEASE_NOTES_CN.md) · [硬件参考](docs/RC505_REFERENCE.md) · [规划](docs/PLAN.md)
