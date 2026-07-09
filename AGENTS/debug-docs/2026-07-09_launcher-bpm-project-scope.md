# Launcher BPM Project Scope

## 问题描述

启动器最初把 BPM 和 latency 一起作为启动默认值保存，并在主程序打开启动器选中的工程后重新覆盖到 `AppConfig`。这会让工程自己的 BPM 被启动器配置覆盖。

## 复现方式

1. 在主程序中创建或打开一个工程，将 BPM 设置为非默认值，例如 `90`。
2. 退出并保存工程。
3. 启动器中将 BPM 设置为另一个值，例如 `120`。
4. 通过启动器打开该工程。
5. 修复前预期会看到工程 BPM 被启动器值覆盖。

## 根因定位

问题位于 `src/app.rs`：

1. `MyApp::new()` 使用 `LauncherConfig::bpm()` 初始化 `AppConfig`。
2. `load_selected_project_with_launcher()` 在 `project::apply_data_to_config()` 后再次调用 launcher 配置覆盖逻辑。
3. 旧的覆盖逻辑使用 `BeatConfigs::set_values()` 同时写入 BPM 和 latency。

## 修改方案

1. `LauncherConfig` 移除运行时使用的 `default_bpm` 字段与 `bpm()` 方法。
2. 启动器 Audio 页移除 BPM 输入框，只保留设备和 latency。
3. 主程序新增硬件配置覆盖语义：只从 launcher 覆盖输入设备、输出设备和 latency。
4. `BeatConfigs` 新增 `set_latency()`，避免为了设置 latency 顺手覆盖 BPM。
5. README 与 AGENTS 文档同步说明：BPM 属于工程，launcher 只负责硬件相关启动设置。

## 影响范围

1. UI：`src/bin/launcher.rs`
2. Config：`src/app_support/launcher_config.rs`、`src/config/beat_configs.rs`
3. App startup：`src/app.rs`
4. 文档：README / README_CN / AGENTS

## 手工验证步骤

1. `cargo build --release`
2. 用主程序创建两个工程，分别设置不同 BPM 并保存。
3. 打开 `target/release/rc505_launcher.exe`，设置 Latency Comp，选择其中一个工程并启动。
4. 在主程序中进入 Beat 设置页，确认 BPM 等于工程保存值，而不是启动器配置。
5. 进入 System / Beat 设置页，确认设备和 Latency Comp 来自启动器。

## 残余风险

旧的 `launcher_config.json` 如果含有 `default_bpm`，新的反序列化会忽略它；下次由启动器保存后该字段会自然消失。实际音频设备与延迟补偿仍需在目标机器手工确认。
