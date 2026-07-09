# Launcher and EXE Packaging

## 任务分类

功能开发 / 功能扩展，附带启动链路修正。

## 功能目标

合并 PR #1 中的桌面启动器能力，让项目构建后产出两个可执行文件：

1. `rc505_rs.exe`：主 loopstation 程序
2. `rc505_launcher.exe`：启动前配置与工程管理 GUI

启动器保存的音频设备、延迟补偿和最近工程现在会被主程序读取，点击启动后主程序会直接打开启动器中选中的工程。BPM 仍然属于工程数据，不由启动器覆盖。

## 子功能与参数

1. Audio Setup
   - `input_device: String`
   - `output_device: String`
2. Hardware Settings
   - `latency_comp_ms: usize`，范围 `0..=500`
3. Project Manager
   - 复用 `projects_index.json`
   - 工程项字段仍为 `name` / `file`
4. Launch State
   - `last_project: String`
   - 主程序启动时按名称匹配工程；匹配成功则自动进入该工程

## 代码层级

1. UI
   - 新增 `src/bin/launcher.rs`，包含 Audio / Projects / About 三个 tab。
   - 移除了重复的 `[ + NEW PROJECT ]` 伪行，新工程统一通过输入框创建。
   - README / README_CN 更新为两个 EXE 的使用说明。
2. Config
   - 新增 `src/app_support/launcher_config.rs`，定义启动器配置 JSON。
   - 主程序 `MyApp::new()` 读取启动器配置并用于初始化 `AppConfig`。
3. DSP
   - 无改动。
4. Engine
   - 主程序启动时把 launcher 的设备和 latency 传入 `AudioIO::new()`。
   - 打开启动器选中工程时，launcher 的设备和 latency 会作为启动会话覆盖值。
5. Project
   - 启动器和主程序共用同一个工程索引目录。
   - 当工程列表为空时，启动器和主程序都会保证存在 `DEFAULT` 工程项。

## 兼容性说明

工程 JSON schema 没有变化。`projects_index.json` 继续使用已有结构。

`%APPDATA%` 存在时，数据仍在 `%APPDATA%/rc505_rs/projects`。没有 `%APPDATA%` 时，启动器跟随主程序回退到本地 `projects/`，不再使用 PR 原始代码里的 `rc505_data/projects`。

## 验证方法

已执行：

1. `cargo check`
2. `cargo build --release`
3. 检查 `target/release/rc505_rs.exe`
4. 检查 `target/release/rc505_launcher.exe`

人工验证建议：

1. 打开 `target/release/rc505_launcher.exe`
2. 在 Audio tab 选择输入/输出设备，设置 Latency Comp
3. 在 Projects tab 创建或选择一个工程
4. 点击 `Launch RC505`
5. 预期主程序打开后直接进入选中的工程，BPM 来自工程，设备和 latency 来自启动器

## 已知限制

本次没有做真实音频设备的人工录音/回放验证；已验证编译与 release EXE 产出。实际设备枚举、ASIO/WASAPI 切换仍需要在目标机器上手工确认。
