# 安装、数据迁移与更新

RC505 RS 目前仅支持 **Windows x64**。日常使用请下载发布版；GitHub 的 **Source code** 压缩包和仓库源码需要自行编译，不是安装包。

## 1. 在发布页选择下载文件

1. 打开项目[最新发布页](https://github.com/Yishanka/RC505_RS/releases/latest)，确认仓库为 **Yishanka / RC505_RS**。也可以从仓库首页右侧 **Releases（发布）** 进入。[GitHub 发布页说明](https://docs.github.com/en/repositories/releasing-projects-on-github/viewing-your-repositorys-releases-and-tags)
2. 向下滚到更新说明底部，点击 **Assets（资源）** 展开下载列表。若列表显示加载失败，刷新页面再试。
3. 点击 **`RC505-RS-版本号-windows-x64-setup.exe`**，等待下载完成。文件名中的版本号以页面实际提供的为准。

| Assets 中的文件 | 用途 | 首次安装需要吗？ |
|---|---|---|
| `RC505-RS-版本号-windows-x64-setup.exe` | 完整安装向导，包含运行所需的程序文件 | **需要，只下载这一份即可** |
| `RC505-RS-版本号-windows-x64-portable.zip` | 免安装的便携程序，见下方说明 | 不需要，与安装包二选一 |
| `SHA256SUMS.txt` | 可选的下载完整性校验值 | 可选，不影响安装 |
| `update.json` | 软件内更新功能读取的版本清单 | 不需要手动下载 |
| `Source code (zip)` / `Source code (tar.gz)` | GitHub 自动提供的源码归档，需要自行编译 | 不用于直接安装 |

安装包是**单个自包含 EXE**，不需要从其他地方补下载主程序，也不需要安装 Rust。浏览器下载这一份文件的位置由浏览器决定；安装向导中的「下载目录」用于**之后的软件更新**，不会自动移动这次浏览器下载的文件。

## 2. 下载和启动时的安全提示

当前安装包尚无代码签名，浏览器和 Windows 可能显示信誉提示。先确认自己从上述项目官方发布页下载，文件名与所选资源一致。若要核对下载完整性，可额外下载同一 Release 的 `SHA256SUMS.txt`，在文件所在文件夹打开 PowerShell，执行 `Get-FileHash -Algorithm SHA256 -LiteralPath '.\实际安装包文件名.exe'`，将结果与清单中对应文件的一行逐字比较。哈希不一致时不要运行，应重新下载；相同哈希验证的是下载内容一致，不能替代对发布来源的判断。

以下继续按钮仅适用于**确认来源可信、且只是下载不常见或应用未识别的信誉提示**：

| 出现位置与提示 | 可以怎么操作 |
|---|---|
| Microsoft Edge 下载列表提示「不常下载 / isn't commonly downloaded」 | 打开浏览器下载列表（通常为 `Ctrl+J`），点文件旁的 **`…`**，依次选 **保留 / Keep → 显示更多 / Show more → 仍然保留 / Keep anyway**。然后等待文件真正下载完成。[Microsoft 操作说明](https://learn.microsoft.com/en-us/troubleshoot/microsoft-edge/development/download-failures#check-security-and-smartscreen-settings) |
| 双击 EXE 后，Windows SmartScreen 显示「Windows 已保护你的电脑 / Windows protected your PC」 | 点 **更多信息 / More info**，再次核对应用文件名；若提供 **仍要运行 / Run anyway**，确认来源后可点击继续。[Microsoft SmartScreen 说明](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/publish-first-app#step-6-handle-smartscreen-for-new-apps) |

不同 Windows、浏览器版本或组织策略可能改变文字或不提供继续按钮。其他浏览器请先查看下载列表中的具体原因，不把所有「已拦截」提示都当作信誉不足。

若提示**检测到病毒／恶意软件**，或是 **Smart App Control（智能应用控制）／组织策略** 拦截，请停止安装，将文件版本和完整提示报告给项目；受管理电脑可联系管理员。不要关闭防护、添加排除项或解除组织限制来安装。Smart App Control 并没有单独允许某个应用的「仍要运行」入口。[Microsoft Smart App Control 问答](https://support.microsoft.com/en-us/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)

## 3. 运行安装向导，直到工程选择页

安装向导目前使用英文；下面给出中文含义。安装面向当前 Windows 账户。

1. 下载完成后，在浏览器下载列表点击「在文件夹中显示」，或在文件资源管理器找到该 EXE，**双击运行**。若看到信誉提示，按上一节区分处理。
2. 若出现欢迎页，点击 **Next（下一步）**。在 **Select Destination Location（选择安装位置）** 选择程序目录；可点击 **Browse…（浏览）** 更换位置，然后点 **Next**。
3. 在 **Data and download folders（数据与下载目录）** 中确认两个路径，再点 **Next**。三个目录的区别如下。

   | 页面／字段 | 保存内容 | 选择建议 |
   |---|---|---|
   | Select Destination Location | 主程序、音频设置启动器、帮助文档 | 选择用于安装 RC505 RS 的目录 |
   | Project data folder（工程数据目录） | 工程、轨道音频快照、音色、乐句、回放和设置 | 默认是程序目录下的 `data`；也可放在有足够空间的其他可写目录 |
   | Installer download folder（安装包下载目录） | 以后通过软件更新下载的安装包和校验文件 | 与工程数据分开放置；可按磁盘空间选择位置 |

4. 在 **Import existing data (optional)（可选：导入已有数据）**，首次使用或普通版本更新**保持不勾选** **Copy existing RC505 RS projects and settings（复制已有工程与设置）**，直接点 **Next**。只有要从另一个旧数据目录复制内容时才勾选；随后在 **Choose existing data（选择已有数据）** 页面选择原来的数据根目录，而不是其中某一个工程或 WAV 文件，再点 **Next**。
5. 若显示开始菜单或附加任务页面，可保留默认设置。按需勾选 **Create a desktop shortcut（创建桌面快捷方式）**，继续点 **Next**；在确认页面核对路径，点击 **Install（安装）**。
6. 等待安装完成。完成页保留 **Open RC505 RS（打开 RC505 RS）** 勾选，点击 **Finish（完成）**。也可取消勾选，稍后从开始菜单的 **RC505 RS** 或桌面快捷方式打开主程序。
7. 看到 **Choose your project / 选择工程** 页面，首次启动完成。顶部可切换中文/英文；点击 **New / 新建** 创建工程，或选中已有工程后点击 **Open project / 打开工程**。音频设备设置和第一次录音见[操作手册](USER_GUIDE_CN.md#2-第一次录制)。

开始菜单中的 **Audio setup（音频设置）** 是可选设备准备页，日常可直接打开主程序。路径设置保存在程序旁的 `install-settings.json`；软件内 **F12 → 更新** 会显示当前版本、数据和下载位置，并提供打开数据／下载目录按钮。

### 便携包

选择便携方式时，下载 `…portable.zip`，右键选择 Windows **全部解压缩 / Extract All**，将整个文件夹解压到可写位置，再双击其中的 `rc505_rs.exe`。不要只从压缩包中拖出一个 EXE，也不要直接在压缩包预览中运行。

默认在程序旁使用 `data` 和 `downloads`；可修改同目录 `install-settings.json` 中的路径。便携版不会创建安装向导、开始菜单快捷方式或 Windows 卸载项；需要这些功能可选择 setup EXE。显式指定 `--data-dir` 启动时，以该参数为准。

## 4. 数据位置与迁移

数据目录包含工程配置、轨道音频快照、音色预设、乐句、回放及键位设置。回放库位于 `replays/`，与来源工程独立；只有主动导出时才在 `replays/exports/` 生成完整输出 WAV。

导入采用复制与 SHA-256 校验，保留源文件。范围包括工程与轨道快照（`projects`）、音色（`presets`）、乐句（`clips`）、回放（`replays`）、键位（`keyboard.json`）和设备设置（`launcher_config.json`）；不复制旧安装的所有权标记、日志或缓存。目标存在同名但内容不同的文件时停止，不覆盖合并。成功后生成 `migration.json`，记录来源与复制数量。未保存到磁盘的临时循环或采样无法从配置恢复。

移动数据前，先退出所有 RC505 RS 窗口，再复制完整数据目录，修改 `install-settings.json` 的 `data_dir` 并重启确认。不要在演奏或后台保存期间移动文件。0.3 以前保存在工程目录内的完整回放和导出文件，会在打开回放库时迁移到全局 `replays/`。

需要核对实际位置时，打开 **F12 → 更新**，查看路径或点击「打开数据目录」。

OSC 临时采样需要明确「保存为音色」后才能随工程引用恢复；普通配置保存或轨道快照不会自动保留临时采样。具体规则见[用户手册](USER_GUIDE_CN.md)。

## 5. 软件更新

**旧版无需先卸载。** 启动时默认在后台检查新版，发现后显示「有更新」，不会自动下载或安装。可在 **帮助 / F12 → 更新 / Updates** 关闭「启动时检查更新」。

1. 按 **F12** 打开帮助，点击 **更新 / Updates** 页，再点 **检查更新 / Check for updates**。
2. 显示可用新版本后，点 **下载并校验 / Download and verify**，等待完成。安装包保存到所选下载目录，软件核对 SHA-256；失败时查看状态提示并重试。
3. 停止演奏（包括节拍器和独立试听），结束回放录制；若有草稿，先选择保存、保留或丢弃。然后点击 **保存快照，退出并安装更新 / Save snapshot, close and install update**。
4. 有打开的工程时，先保存当前配置与音频快照，再退出并运行更新安装；工程选择页没有活动工程时直接退出更新。完成后重新打开工程选择页。

如果安装更新按钮是灰色，先关闭帮助，结束轨道录放、关闭节拍器与试听、处理回放草稿，并等待后台保存完成，然后重新打开更新页。

也可以从 Releases **手动下载新版 setup EXE**：先保存工程并关闭 RC505 RS 和 Audio setup，再双击新版安装包，沿用原来的程序、数据、下载目录，导入旧数据选项保持不勾选，完成安装即可。保留原数据目录才能继续看到原工程；改到另一个空目录会显示新的数据空间。

安装版只跟随已发布的 Release，不会将每次源码提交作为软件更新。升级沿用应用标识和已选路径，保留工程、快照及回放。成功后下载目录仅保留一份 `RC505-RS-setup.exe`、校验清单和更新助手；失败时保留已有安装包，可查看下载目录的 `update-error.log` 并重试。

## 6. 卸载

在 Windows「设置 → 应用 → 已安装的应用」选择 RC505 RS，点击「卸载」；也可使用开始菜单的卸载入口。先保存并关闭主程序和音频启动器。

卸载时询问是否同时删除应用数据，**默认选择 No（保留）**。选择 Yes 会永久删除提示目录中的工程、快照、音色、乐句、回放、键位和软件设置，包括回收站。不同安装若共用同一数据目录，这些共享数据也会受影响。软件先按当前配置显示实际路径；确认后路径改变会中止删除。

不会递归清空整个数据根目录，下载包与其他文件保留。若数据条目包含链接、仍被占用，或与程序/下载目录重叠，会拒绝自动删除；可以选择保留数据卸载，再自行整理目录。便携版没有 Windows 卸载项，可删除程序文件并独立保留数据。

## 7. 源码与贡献

源码与发布版分开提供。需要自行修改时，可 fork 或 clone [源码仓库](https://github.com/Yishanka/RC505_RS)，在 Windows 上编译和验证，再提交 Pull Request。构建入口见[项目 README](../README_CN.md)，实现约定见[架构说明](ARCHITECTURE.md)；预编译发布版不需要安装 Rust 工具链。
