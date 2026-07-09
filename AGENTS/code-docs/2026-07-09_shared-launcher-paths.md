# Shared Launcher Paths

## 任务分类

结构调整 / 代码组织整理。

## 改动目标

让主程序和启动器共享数据路径与启动配置定义，避免两边各自维护 `%APPDATA%`、工程目录和 `launcher_config.json` 逻辑。

## 旧结构问题

PR 原始实现中，`src/bin/launcher.rs` 自己定义路径和 `LauncherConfig`：

1. Windows `%APPDATA%` 路径与主程序一致。
2. 无 `%APPDATA%` 时回退到 `rc505_data/projects`，而主程序回退到 `projects/`。
3. 主程序没有读取 `launcher_config.json`，导致启动器保存的设备、latency 和工程选择不会生效。

## 新结构说明

新增 `src/app_support/` 作为应用支撑模块，避免继续在 `src/` 根目录堆放零散单文件：

1. `src/app_support/paths.rs`
   - `projects_dir()`
   - `launcher_config_path()`
2. `src/app_support/launcher_config.rs`
   - `LauncherConfig`
   - `load()`
   - `save()`
   - latency 边界保护
3. `src/app_support/mod.rs`
   - 对外暴露 `paths` 和 `launcher_config`

`src/main.rs` 注册 `app_support`；`src/bin/launcher.rs` 通过 `#[path = "../app_support/mod.rs"]` 复用同一份源码。

`src/` 根目录保留主应用入口级模块，例如 `app.rs`、`project.rs`、`state.rs`、`track.rs`。启动器共享路径和启动配置属于应用支撑设施，放入子目录后边界更清楚。

## 行为变化

1. 主程序启动时会读取 launcher 配置。
2. 如果 `last_project` 能匹配现有工程，主程序会自动进入该工程。
3. 启动器会保证空工程列表下存在 `DEFAULT` 工程项。
4. 无 `%APPDATA%` 时，启动器工程目录从 `rc505_data/projects` 改为主程序一致的 `projects/`。

## 验证方法

已执行：

1. `cargo fmt`
2. `cargo check`
3. `cargo build --release`
4. 检查 release 目录中两个 EXE 均存在

## 风险

共享模块通过 `#[path]` 被 launcher bin 复用，而项目当前还不是 library crate。这个方式改动小，但如果后续继续增加跨 bin 共享代码，建议考虑抽出 `src/lib.rs`，让 main app 与 launcher 都依赖同一个 library crate。
