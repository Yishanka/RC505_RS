# RC505 免费版

中文版说明文档。English version: [README.md](./README.md)

## 1. 简介

这是一个模拟真实 BOSS RC-505 MK2 的本地 Looper 软件。RC-505 实在太贵了，而很多免费的软件 looper 功能又少。所以我决定用 Rust 自己做一个，能加多少功能就加多少。

目前它还在早期阶段，非常粗糙，但核心流程已经能用：多轨循环、节拍同步、输入效果、轨道效果、工程配置保存与读取。

## 2. RC-505 是什么

RC-505 这类设备的核心就是现场循环：你先录一段，再叠一段，按节奏开关轨道，再实时加效果。这个项目在软件里复现的是这种工作方式：5 条轨道，加两层效果系统，分别是 Input FX（录入前处理）和 Track FX（轨道播放时处理）。

我不想买一个真的 RC-505，所以大部分东西都是用俺寻思之力做的，凑活用。

## 3. 项目结构

项目使用 Rust 开发，目前只支持 Windows。音频 I/O 使用 `cpal`（默认 WASAPI，可选 ASIO feature），界面使用 `eframe/egui`，工程数据用 `serde` 写入 JSON。

整体分层比较明确：`app.rs` 和 `ui/*` 负责状态和界面，`config/*` 负责可编辑参数，`engine/*` 负责实时音频流程，`dsp/*` 负责算法，`project.rs` 负责工程保存和读取。配置数据与 DSP 运行时状态是分开的，参数变更会同步到音频线程。

大致结构如下：

```text
src/
  app.rs                应用状态机与按键逻辑
  ui/                   初始页与主界面绘制
  config/               所有参数配置
  engine/
    audio_io.rs         音频输入输出、ring buffer、轨道时序
    input_fx.rs         Input FX 运行时与处理
    track_fx.rs         Track FX 运行时与处理
    metronome.rs        节拍时钟
  dsp/                  envelope/filter/osc/reverb/delay/roll/my_delay/note
  project.rs            工程索引和 JSON 存取
```

## 4. 运行与使用

### 运行方式

当前最稳妥的方式是本地从源码运行。

```powershell
git clone <your-repo-url>
cd rc505_rs
cargo run --release
```

如果你的设备支持 ASIO，也可以这样运行：

```powershell
cargo run --release --features asio
```

如果只想拿可执行文件，也可以先编译，再运行 `target/release/rc505_rs.exe`。或者直接在 Release 里下载 `.exe` 文件即可。

### 基本操作流程

启动后先进入工程列表页。`Up/Down` 选择工程，`Enter` 进入工程；在 `[ NEW PROJECT ]` 上按 `Enter` 新建；`R` 重命名；`Delete` 删除。

进入工程后有两个工作状态：`Loop` 和 `Screen`，按 `S` 切换。`Loop` 里主要是录音和播放控制，`Screen` 里主要是参数编辑。

`Loop` 状态下，`1..5` 控制五条轨道：空轨道进入录音，播放中的轨道进入叠录，录音或叠录状态再次触发会在下一拍收尾并回到播放，暂停轨道会按当前时间线对齐恢复播放。`F1..F5` 暂停轨道，`Left/Right` 选中轨道，`Delete` 清除当前选中轨道。

效果控制由 `T` 切换两种模式：`Bank` 和 `Single`。Bank 模式下，`QWER` 切 Input FX bank，`UIOP` 切 Track FX bank。Single 模式下，`QWER` 开关当前 Input bank 的四个槽位，`UIOP` 开关“当前选中轨道”在当前 Track bank 的四个槽位。

### Screen 参数编辑

`Screen` 状态下，`B` 进入 Beat 设置，`M` 进入 System 设置，`QWER` 进入 Input FX 槽位编辑，`UIOP` 进入 Track FX 槽位编辑。大部分页面是 `Left/Right` 切字段，`Up/Down` 改枚举值。数值参数直接按数字键输入，`Backspace` 删除。`Enter` 用于进入子页面或执行序列编辑里的 Push/Pop。

**现在整体是键盘优先设计。界面看起来像面板，但目前不是鼠标点击式流程。**

### 已实现效果（当前版本）

Input FX 是 4 个 bank，每个 bank 4 个槽位。槽位类型有 `Oscillator`、`Filter`、`Reverb`、`MyDelay`。

Oscillator 包含波形、音量、阈值、音高序列、AHDSR 包络，还带独立的滤波器与滤波包络。MyDelay 是一个自定义的短采样循环类效果，循环长度可由音符控制，也有独立的 AHDSR 和滤波/滤波包络。Input Filter 是双二阶滤波（LPF/HPF/BPF/Notch），带 drive 和 mix。Reverb 使用 FDN 风格算法，提供 size/decay/predelay/width/high-cut/low-cut。

Track FX 也是 4 个 bank x 4 个槽位，并且有“每条轨道独立开关矩阵”，所以一个 bank 的槽位定义可以共享，但每条轨道自己决定开关。当前已实现 `Delay`、`Roll`、`Filter`。其中 Track Filter 带 `Seq` 和 `Env` 子页面，可以做节奏门控和包络摆动。

### Note / Seq / Envelope 说明

Note 和 Seq 都是 tick 逻辑（每拍 12 tick，最大 32 拍）。Step 选项包括 `1/6`、`1/4`、`1/3`、`1/2`、`2/3`、`3/4`、`5/6`、`1`、`2`。`Push` 追加一个步长块，`Pop` 删除最后一个块。

Envelope 使用 AHDSR，并扩展了 `Start` 和 tension 参数。当前映射里 tension 默认 `100` 是线性，低于它会获得上凸的曲线，高于它会获得凸曲线，上限是 `1000`。

### 延迟补偿

Windows 音频链路通常会有明显往返延迟。Beat 设置里的 `Latency Complement`（毫秒）会参与录音对齐：录音结束时会做补偿，叠录写入位置也会偏移补偿，Track FX 的时间线也会同步偏移，以保证和补偿后的轨道音频对齐。

这个值和硬件强相关，在不同设备上差异很大，建议按自己的设备单独校准。

### 工程保存与文件位置

工程默认保存在 `%APPDATA%/rc505_rs/projects`。如果系统拿不到 `%APPDATA%`，会回退到本地 `projects/`。有一个工程索引文件，以及每个工程独立的 JSON 配置文件。

从 Loop/Screen 退出到初始页，或者关闭窗口时，会弹出保存确认：`Y` 保存，`N` 不保存，`Esc` 取消退出。当前版本保存的是配置状态（beat/system/fx），不保存已经录进轨道的音频波形数据。

当前已知限制：如果你切换某个槽位的效果类型（例如从 Oscillator 改成 Filter），该槽位参数会被重新初始化，旧参数会丢失。

## 5. 后续计划

现在还有不少 bug 和边角问题。我还没做系统化测试，所以非常欢迎提 issue。

后面会继续做时序稳定性、效果器扩展和交互优化。Vocoder、变调相关效果、以及更完整的轨道级工作流，都会是后续方向。

欢迎 PR，也欢迎直接提建议。这个项目就是边做边学，公开推进。
