# 安装、迁移和更新

## 使用发布包

在 [GitHub Releases](https://github.com/Yishanka/RC505_RS/releases) 下载 `RC505-RS-版本-windows-x64-setup.exe`。安装器为当前 Windows 用户安装，不要求把程序或数据放在 C 盘。

向导分别选择：

1. **程序目录**：例如 `E:\SJTU\rc505`。
2. **数据目录**：默认跟随所选程序目录，为 `程序目录\data`；例如 `E:\SJTU\rc505\data`。可独立改为其他磁盘目录。
3. **下载目录**：以后检查更新所下载的安装包保存在这里；例如 `E:\installer`。
4. **导入旧数据**（可跳过）：选择含 `projects` 和 `launcher_config.json` 的旧目录。原开发版通常是 `%APPDATA%\rc505_rs`。
5. 可选桌面快捷方式；开始菜单提供主程序、音频准备页和卸载入口。

安装设置写在程序旁的 `install-settings.json`。无论从哪个工作目录启动，都会使用同一数据路径。命令行 `--data-dir=...` 优先于安装设置，因此测试和开发仍可隔离。

数据默认结构：

```text
RC505 RS/
  rc505_rs.exe
  rc505_launcher.exe
  install-settings.json
  data/
    launcher_config.json
    keyboard.json          全局自定义键位
    replays/
      <回放>/initial + input.wav + events.jsonl + replay.json
      <回放>/samples/      按内容去重的 OSC 采样素材
      exports/*.wav        仅显式导出时创建，可在回放库删除
      trash/               删除/丢弃后可恢复的回放
    projects/
      projects_index.json
      <工程标识>.json
      <工程标识>.json.assets/
        snapshots/<版本>/manifest.json + track/undo WAV
    media/                 可存放自己的录屏等资料
```

软件本身录制回放操作与输入音频，不录制屏幕视频。手动录屏可放在 `data\media`；任何录屏工具自身的默认目录仍需要在该工具里设置。

0.3 起回放属于全局库。打开回放库时，旧工程目录中的完整回放及导出文件会迁到 `data/replays`；工程的录音快照仍归原工程所有。回放的来源工程字段只作信息，不限制导入目标。

## 数据迁移

导入使用复制和 SHA-256 校验，原文件保持不变。目标已有同名且内容不同的文件时停止，不进行覆盖合并。成功后生成 `migration.json` 记录来源及复制数量。导入包含旧工程、预设和硬件配置；旧版本从未保存到磁盘的临时 loop 无法从 JSON 恢复。

可在 PowerShell 独立执行：

```powershell
& 'E:\SJTU\rc505\rc505_rs.exe' '--migrate-data=C:\Users\你的名字\AppData\Roaming\rc505_rs' '--data-dir=E:\SJTU\rc505\data'
& 'E:\SJTU\rc505\rc505_rs.exe' '--data-dir=E:\SJTU\rc505\data' --verify-data
& 'E:\SJTU\rc505\rc505_rs.exe' --installation-info
```

以后移动数据：先退出所有 RC505 RS 窗口，复制完整 `data` 目录到新位置，修改 `install-settings.json` 的 `data_dir`，再重启确认。更新向导也可以指定路径。不要在演奏或后台保存时移动文件夹。

## 正常更新

打开软件 **Help / F12 → Updates**：

1. **Check for updates** 读取官方仓库最新 Release 的 `update.json`。
2. **Download and verify** 下载到安装时选定的目录，核对 SHA-256；不执行未通过校验的包。
3. 停止轨道、结束并处理回放草稿后，点 **Save snapshot, close and install update**。
4. 软件先完成当前工程的 config + audio snapshot 保存，再正常退出。独立助手等待进程退出后运行安装器，成功后重新打开工程选择页。

升级复用固定应用标识、程序位置及数据路径，不删除工程、快照、回放或视频资料。成功更新后，下载目录只保留一个固定名称的 `RC505-RS-setup.exe` 及其校验清单，旧版本安装缓存自动清理；下载/安装未成功时先保留旧包。清理范围限定为本软件的版本化安装缓存。卸载仅移除已安装的程序文件，保留数据。更新失败记录位于下载目录 `update-error.log`，可用保留的安装包重试。

安装包未使用商业代码签名证书；Windows 对首次下载的开源安装包可能显示信誉提示。请从上述仓库下载并核对同一 Release 的 `SHA256SUMS.txt`。SHA-256 验证下载完整性；发布账户本身仍是更新信任来源。

便携 ZIP 解压即可运行，附带相对路径设置，数据和下载放在可执行文件旁的 `data` / `downloads`，不依赖启动时工作目录。可修改 `install-settings.json` 为绝对路径或显式传 `--data-dir`。只有源码构建且没有安装设置时才沿用 `%APPDATA%\rc505_rs`。便携包不会自动注册卸载项。

## 开发 → 推送 → Release → 更新

普通 `main` 推送执行 Windows 检查、测试和安装器验证；不会把每个未发版提交推送给演奏用户。正式交付步骤：

```powershell
# 修改源码和文档，更新 Cargo.toml 中的版本，并更新 Cargo.lock
cargo check --all-targets
cargo test --all-targets
cargo build --release --bins
# 本机检查安装包（需要 Inno Setup 6）
powershell -NoProfile -File scripts/package.ps1 -SkipBuild

git add <本次需要发布的文件>
git commit -m "Describe the change"
git push origin main
git tag v0.3.0  # 示例；必须与 Cargo.toml 一致，已有 tag 不要重建
git push origin v0.3.0
```

`.github/workflows/release.yml` 在 Windows 上验证、构建，然后使用仓库 Actions 的内置凭据发布安装包、便携包、校验和、更新清单。tag 和 Cargo 版本不一致会失败；测试失败不会发布。Release 发布成功后，安装版才能检查到更新。已发布 tag 不应移动；修复应使用新的版本号。

打包参考 [Inno Setup 官方文档](https://jrsoftware.org/ishelp/contents.htm)。FFmpeg 不是本版本依赖；无损 WAV 读写由 Hound 完成，回放音频由 Rust DSP 渲染。

Windows x64使用[静态C运行库链接](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes)，不要求用户另装VC++运行库。打包时自动执行隔离的安装/再次安装/数据保留/缓存清理测试；失败则不发布。

## 开发缓存与体积

发布构建保持 `opt-level=3`，使用 Thin LTO、单一 codegen unit 和调试信息剥离；保留 panic 展开与故障日志，不使用禁用向量化的 `opt-level="z"`。这些设置影响编译/链接时间，实际体积比较见 [VALIDATION](VALIDATION.md)。[Cargo 官方构建配置](https://doc.rust-lang.org/cargo/reference/profiles.html)

`target` 是开发编译缓存，不会装进用户的软件目录。需要释放空间时可在没有编译任务运行的情况下执行 `cargo clean`；它不删除源码和 `data`，下次构建会重新编译依赖。默认开发配置仍保留调试信息和增量编译，便于调试。

```powershell
# 先预览；只选择至少一天前、名称匹配已知测试/打包规则的临时产物
./scripts/clean-dev.ps1
./scripts/clean-dev.ps1 -Apply
# 真正渲染隔离的离线界面（需要 Python 与 Pillow）；PNG 替代体积较大的 PPM
python scripts/preview-ui.py projects-small performance-small-rose shortcuts-small
# 可选：同一源码比较原默认发布配置和当前配置，结束后保留当前release程序
./scripts/measure-release-size.ps1
```

清理脚本保留 `var` 中的 PDF、视频、参考调研和工具目录；失败测试可暂留用于诊断。成功的安装测试、打包暂存与 UI 导航测试会自行清理。文档采用的截图放在 `docs/images`。

成功打包后，开发目录 `dist` 只保留本次版本的安装器、便携包和校验清单；历史发布仍可从GitHub下载。这与安装版 `E:\installer` 的单份更新缓存是两套独立规则。
