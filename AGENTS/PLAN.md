# 创建自己的 RC505

## Stage 0: 安装与初始化 

### 1. 安装底层的 C++ 构建工具（必选）

Rust 在 Windows 上默认使用 **MSVC**（Microsoft Visual C++）工具链。既然你熟悉 C++，可能已经装了 Visual Studio。

- 如果你已经安装了 **Visual Studio (2019 或更高版本)**：
  - 打开 **Visual Studio Installer**。
  - 确保勾选了 **“使用 C++ 的桌面开发” (Desktop development with C++)**。

------

### 2. 下载并运行 Rustup

`rustup` 是 Rust 的官方安装程序和版本管理器（就像 Node 的 nvm 或者 Python 的 pyenv，但更好用）。

1. 访问 [rust-lang.org](https://www.rust-lang.org/tools/install)。

2. 下载 `rustup-init.exe` (通常是 64 位版本)。

3. 运行该程序。你会看到一个黑色窗口：

   - 如果你已经装好了上面的 C++ 工具，直接按 **`1`** (Proceed with installation (default))。

4. 窗口内容

   The Cargo home directory is located at:

     `C:\Users\sk804\.cargo`

   This can be modified with the `CARGO_HOME` environment variable.

   The `cargo`, `rustc`, `rustup` and other commands will be added to
   Cargo's bin directory, located at:

     `C:\Users\sk804\.cargo\bin`

   This path will then be added to your `PATH` environment variable by
   modifying the `PATH` registry key at `HKEY_CURRENT_USER\Environment`.

   You can uninstall at any time with `rustup self uninstall` and
   these changes will be reverted.

------

### 3. 验证安装

重启你的终端输入：

```PowerShell
rustc --version
cargo --version
```

看到版本号返回，就说明 Rust 已经成功“入驻”你的 Windows 系统了。

------

### 4. 编辑器配置

在 Windows 上开发 Rust，**VS Code** 是目前体验最好的：

1. 打开 VS Code。
2. 在插件市场搜索并安装 **`rust-analyzer`**。
   - *注意：* 不要装那个只叫 "Rust" 的插件，`rust-analyzer` 才是现在的业界标准，它能提供极强的代码补全、类型推导和错误实时检测。
3. 安装 **`CodeLLDB`** 插件：这能让你在 Windows 上像调试 C++ 一样，直接给 Rust 代码打断点、看变量。

------

### 5. 快速尝试

你可以直接在你希望存放项目的文件夹下，打开 PowerShell 输入：

```PowerShell
cargo new my_test_project
cd my_test_project
cargo run
```

当屏幕打印出 `Hello, world!` 时，你的 Rust 之旅就正式开启了。

---

### 6. 基础构建方法

创建项目:
```bash
cargo new rc505_rs
cd rc505_rs
```

运行项目**: `cargo run`

### 7. 建议的项目架构

由于音频处理对实时性要求极高，建议采用**生产者-消费者模型**，将音频回调（实时线程）与 UI/逻辑处理（非实时线程）完全隔离。

```text
rc505_rs/
├── Cargo.toml
├── src/
│   ├── main.rs          # 程序入口，初始化音频流
│   ├── engine/          # 核心音频处理逻辑
│   │   ├── track.rs     # 单个 Loop 轨道的逻辑（录制、播放、叠加）
│   │   ├── mixer.rs     # 混合 5 个轨道并应用 Master FX
│   │   └── buffer.rs    # 循环缓冲区（Circular Buffer）管理
│   ├── dsp/             # 各种音效实现（Delay, Reverb, EQ 等）
│   ├── midi/            # 处理 MIDI 输入，映射到控制逻辑
│   └── ui/              # 界面显示（基于 egui 或 iced）

```

---

## Stage 1: MVP

### 1. UI: `egui`

- [x] 一个基本的形态
- [x] 操控：我希望通过键盘操控，目前 MVP 大概需要这些操控
  - [x] 初始界面：纵向排列的工程名称
    - [x] 按上下选择工程；
    - [x] 按 enter 进入选中工程；
    - [x] 按 delete 删除选中工程；
    - [x] 选中最下面的工程后再按一次下可以新建工程，输入名字再 enter 即可新建工程并自动进入工程；
    - [x] 进入工程会读取预设（如 BPM），缺失（比如新建的工程）就会使用默认值，并进入 Looper 界面；
  - [x] Looper 界面，Loop 状态:
    - [x] 按 12345，空 Track -> 录制；录制/叠录 -> 播放；播放 -> 叠录；暂停 -> 播放
    - [x] 按 f1 f2 f3 f4 f5，暂停对应 track
    - [ ] 按空格全部播放/暂停
    - [x] 按左右选 track
      <!-- TODO：看一下 rc 的操作 -->
    - [x] 按 s 进入 Screen 状态
    - [x] 按 esc 保存当前预设或不保存，退回初始界面
    - [x] 按 T 切换 bank / single 状态
      - [x] bank 状态按 qwer 切换使用的 input 效果 bank，按 uiop 切换使用的 output 效果 bank；两者不影响
      - [x] single 状态按 qwer 开启关闭 input 效果，按 uiop 为选中的 track 加上/关闭 output 效果；两者不影响
      - eg: T 切换为 bank -> Q 选择 bank1（其他类似） -> T 切换为 Single -> Q 为当前的输入添加 bank1 的 Q 效果
  - [x] Looper 界面，Screen 状态: 
    - [x] 按 B 进入 Beat 设置
    - [x] 按 M 进入 System 设置
    - [ ] 按 N 进入 Track 设置
    - [x] Beat
      - [x] 设置 1：工程的 BPM，BPM 可以额外通过敲击空格计算 BPM。
    - [x] System
      - [x] 设置 1： 输入设备，可通过**上下**选择本地的输入设备
      - [x] 设置 2： 输出设备，可通过**上下**选择本地的输出设备
    - [ ] Track
      - [ ] 
    - [x] 按 s 进入 Loop 状态
    - [x] 按 esc 保存当前预设或不保存，退回初始界面
    - [x] 按 T 切换 bank / single 状态，
      - [x] bank 状态按 qwer 切换当前设置的 Input 效果 bank，按 uiop 切换使用的 output 效果 bank；两者不影响
      - [x] single 状态按 qwer 为每个按键绑定一个效果，按 uiop 为每个按键绑定一个效果
      - eg: T 切换为 bank -> Q 选择 bank1 -> T 切换为 Single -> Q 设置 bank1 的 Q 效果, W 设置 bank1 的 W 效果
    - [x] 设置修改，跟 FX 的输入设置一样，不需要通过 enter 去 confirm 

### 2. Loop: `cpal `+ `WASAPI`

- [x] 节拍器
  - [x] cofig: 设置 BPM
  - [x] 节拍器；第一次开始录制时启动节拍器；每个时间点可以获取下一拍的时间，用于开始/结束；
- [x] 录制
  - [x] 实现一个 AudioIO，负责输入和输出声音
  - [x] 实现每个 track 的录制播放
  - [x] 叠录实现
  - [x] 大部分录制/叠录的 bug 修复
  - [x] 录制 latency 补偿设置化
- [x] 根据节拍器的 UI
  - [x] 红条连续连续运动，4 beat 时间内匀速走完，然后重新走
  - [x] 每拍闪烁一次
<!-- - [ ] 内置节拍器
  - [ ] Loop 模式下给一个按钮（B），按下则打开节拍器
  - [x] 不开启节拍器则默认和现在逻辑一致
  - [ ] 开启节拍器：
    - 若当前所有轨道都 Empty，循环播放节四拍的引导拍子；当开始录制时播放完当前四拍的引导拍子才开始录制，并开始第一拍
    - 若其他情况，节拍器默认不生效，还是和现在逻辑一致 -->
- [ ] bpm 修改后把音频加快

### 3. Input FX

- [x] **Oscillator**
1. **Audio**：声音波形与响度等控制
2. **Note**: Note 输入音效
3. **Filter**: Filter 输入音效，只对 Osc 滤波
  
- [x] **Osc-Audio**
1. **Waveform**：Enum，Sine, Saw, Square, Tri，基础振荡器波形，决定音色质感。
2. **Level**：Numeric，0 - 100 ，OSC 的输出总增益（响度）。 
3. **threshold**：Numeric，人声触发阈值。只有麦克风音量超过此值，OSC 才会发声。
<!-- 4. **Detune**：Numeric，，音柱偏离程度 -->
<!-- 5. **Num**：Numeric，1-16，几根音柱 -->
4. **Envelope**：Envelope 输入音效，默认控制 Level

- [x] **Note**
1. 一个 beat 有 12 个 ticks，最长 32 拍（8 个小节）
2. C0-C9 Enum
3. step: 1/16, 1/8, 1/4, 1/2, 1 enum
4. push/pop 按钮，将当前设置 push 进去，将音高一样的 pop 出来
<!-- 5. up_oct/down_oct：当前 seq 内部同时升高/降低一个八度 -->
>
> 算法：
>   - 生成器：根据当前 Sequencer 的 Note 计算频率 $f$。
>   - 相位累加器：$\phi_t = (\phi_{t-1} + \frac{2\pi f}{f_{sample}}) \pmod{2\pi}$。
>   - 波形查表：
>     - Sine: $\sin(\phi)$
>     - Square: $\text{if } \phi < \pi \text{ then } 1 \text{ else } -1$
>    - Saw: $\frac{\phi}{\pi} - 1$
>
> 特点：音色极其纯净、电子感强。

- [x] **Envelope**：
1. **Attack**：Numeric，0ms - 200ms，声音渐入时间
2. **Hold**: Numeric，，最大响度持续时间
3. **Decay**：Numeric，，最大响度结束后响度逐渐缩减的时间
4. **Sustain**：Numeric，，结束响度占最大响度的百分比
5. **Release**：Numeric，10ms - 500ms，停止输入后的余音长度
6. **Tension-A, Tension-D, Tension-R**：Numeric，0-100，Attack，Decay，Release 的曲线形状，50 是直线，0-50 是上凸，50-100 是凸

- [x] **Filter**
1. **Filter Type (类型)**: Enum
  * **LPF (低通)**: 削弱高频，声音变闷（最常用）。
  * **HPF (高通)**: 削弱低频，声音变薄（类似于小收音机效果）。
  * **BPF (带通)**: 只保留中间频段，产生类似鸣笛或电话的声音。
  * **Notch (陷波)**: 挖掉中间一个频段，产生特殊的相位感。
2. **Cutoff Frequency (截止频率)**: Numeric, 20Hz - 20,000Hz
  * **注意**：这个参数在 UI 上必须是**对数增长**的，因为人耳对频率的感知是对数的。
3. **Resonance / Q (共鸣/品质因数)**: Numeric, 0.1 - 10.0 (或 0-100%)
  * 在截止频率处产生一个突起。高共鸣会让声音听起来有“嘶嘶”的口哨感或“哇”的电音感。
4. * **Drive / Saturation (驱动/饱和)**: `Numeric`, `0 - 100`
  * 在滤波前增加一点失真。滤波器在处理带有温和失真的信号时，共鸣感会更强、更好听。
5. **Dry/Wet Mix (干湿比)**: `Numeric`, `0% - 100%`
  * 控制原声和滤波声的比例。

- [x] **MyDelay**
1. **MyDelay-Audi**：设置组合
2. **Note**
3. **LPF** 

- [x] **MyDelay-Audi** 
1. Level，Numeric，同 osc
2. Threshold，Numeric，同 osc

- [x] **Reverb**: 使用 **FDN (Feedback Delay Network)** 算法
1. **Size**：Numeric，改变延迟线的基础长度。数值越大，感觉空间越空旷。
2. **Decay Time / RT60 (衰减时间)**：Numeric，定义残响降低 $60\text{dB}$ 所需的时间
3. **Width (宽度)**：混响在立体声场中的展开程度。
4. **Delay (预延迟)**：Numeric，直达声和早期反射声之间的时间差。
5. **High Cut**：模拟墙壁材质。如果墙是混凝土，高频反射强；如果墙挂满窗帘，高频消失快。调高会让混响听起来更“暗”、更温暖。
6. **Low Cut (低频切除)**。

OSC 声音太响了！
note 放在 N 上，mydelay 就不出声

-  [ ] EQ
1. 

#### 4. Output Fx

- [ ] **Delay**
1. **step**: 

- [ ] **Roll**
1. 

#### 5. Project

- [x] 创建工程，在工程路径存储该工程的参数
- [x] 详见 `note_project_file.md`


### 6. Debug

- [ ] 看视频确定 outputfx 的操作逻辑
- [ ] 毛糙声音（叠音后）
- [ ] 把 numeric 都改成浮点数吧





-----










## Stage 2: engine dev

- [ ] Vocoder

在 Vocoder 架构中，有两个核心角色：

1. **调制信号 (Modulator)**：**人声**（提供口型、辅音、频谱特征）。
2. **载波信号 (Carrier)**：选中的**输入波形**（Input, Track1-5）（提供音高、谐波、能量）。

### Vocoder 核心算法流程

Vocoder 的本质是：提取人声在不同频段的能量，并用这些能量去实时控制载波信号在对应频段的音量。

1. 载波生成 (The Carrier)

选择的输入（Input，track1-5）带来一个波形
* *注意*：不能为纯正弦波，因为正弦波只有单一频率，没有频谱空间供声码器“过滤”。
* 音高由你的 `Sequencer` 决定。

2. 分析滤波器组 (Analysis Filter Bank)

将你的**人声输入**通过一组带通滤波器（Bandpass Filters）。

* 通常分为 8 到 32 个频段（频段越多，话语越清晰）。
* 例如：第 1 频段处理 100Hz-200Hz，第 2 频段处理 200Hz-400Hz... 直到高频。

3. 包络检波 (Envelope Detection)

对于人声的每一个频段，计算它当前的“能量强度”。

* 算法：对该频段的信号取绝对值，并进行低通滤波（LPF），得到一个平滑的能量曲线。
* 这个曲线代表了你说话时，某个特定频率的“开口度”。

4. 合成滤波器组 (Synthesis Filter Bank)

将载波也通过一组**完全相同**的滤波器。

* 这样我们就得到了载波在 100Hz-200Hz 的部分，200Hz-400Hz 的部分...

5. 调制 (Modulation) —— 关键点

将人声第 $n$ 频段的能量，乘以载波第 $n$ 频段的信号。
$$ Output_n = Carrier_Band_n \times Modulator_Envelope_n$$

* 这意味着：如果你说话时发出了“呜”的声音（低频能量高），OSC 的低频部分就会被放大，高频被关掉。如果你发出“嘶”的声音（高频能量高），OSC 的高频谐波就会亮起来。

6. 求和 (Summation)

将所有频段处理后的信号相加，输出最终音频。

### Track Settings (IO...)
 <!-- 可以选择录制拍数 -->
 <!-- 使用 ASIO 驱动（Windows）
默认 WASAPI 往往带 100ms+ 的安全缓冲。ASIO 通常能降到 5–20ms。
设备没有原生 ASIO 时，用 ASIO4ALL 作为折中。 -->
<!-- 设置时数值泛型的实现 -->

```bash
# llvm
PS E:\SJTU\projects\rc505_rs> cargo run --features asio
   Compiling asio-sys v0.2.5
   Compiling cpal v0.15.3
   Compiling rc505_rs v0.1.0 (E:\SJTU\projects\rc505_rs)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.37s
     Running `target\debug\rc505_rs.exe`
ASIO host is available but selected devices are not in ASIO. Falling back to default host.

# delete
winget uninstall -e --id LLVM.LLVM
[System.Environment]::SetEnvironmentVariable("LIBCLANG_PATH", $null, "User")
[System.Environment]::SetEnvironmentVariable("CPAL_ASIO_DIR", $null, "User")
Remove-Item Env:LIBCLANG_PATH -ErrorAction SilentlyContinue
Remove-Item Env:CPAL_ASIO_DIR -ErrorAction SilentlyContinue
cargo clean -p asio-sys
echo $env:LIBCLANG_PATH
echo $env:CPAL_ASIO_DIR
Test-Path "C:\Program Files\LLVM\bin\libclang.dll"
```

## Stage 3: dsp dev

## Stage 4: Product on Windows










<!-- ① 驱动程序协议 (Windows 的软肋)MME/DirectSound: 延迟极大 ($100ms \sim 500ms$)，完全没戏。WASAPI (Exclusive Mode): Windows 现代标准，能做到 $20ms \sim 40ms$。ASIO: 唯一王者。这是专业音频硬件的协议，可以直接绕过系统内核，实现 $2ms \sim 10ms$ 的延迟。限制: 如果你的声卡不支持 ASIO 驱动（或者你没装 ASIO4ALL），代码层面无论怎么优化都很难跌破 $20ms$。

3. “终极作弊”手段：硬件监听 (Direct Monitoring)
如果你观察真正的专业音频接口（如 Focusrite Scarlett 或 BOSS RC-505 硬件本身），它们解决延迟的方法不是优化代码，而是硬件直通：

麦克风的声音进入声卡后，在进入电脑之前，直接分出一路信号发给耳机。

这才是真正的物理零延迟。

4. 针对你项目的建议：
既然你在写 Loopstation，你需要一个延迟补偿机制：

测量当前的输入输出总延迟（Round-trip Latency）。

在录制音轨时，将录到的音频数据向前移动这个延迟量。

这样当你回放时，循环的开头才会和你的节拍器对得严丝合缝。 -->
<!-- 
方案

定义一个可调常量（毫秒）INPUT_LATENCY_COMP_MS。
启动音频流时按当前设备配置换算为样本偏移：
latency_comp_samples = sample_rate * ms / 1000 * channels
两处应用补偿：
普通录音结束时：把整段录音前移 latency_comp_samples，尾部补 0，长度不变（避免改 loop 长度）。
叠录时：把写入位置从当前播放游标 idx 提前到 write_idx = idx - comp (mod len)，对齐到更接近拍点的位置。
实现点

新增常量和工具函数：
audio_io.rs (line 13) INPUT_LATENCY_COMP_MS
audio_io.rs (line 17) shift_buffer_earlier_in_place(...)
process_timeline 增加补偿参数并在录音结束时应用：
audio_io.rs (line 70)
audio_io.rs (line 88)
在 build_streams 中按配置计算补偿样本数，并传入输入/输出回调：
audio_io.rs (line 166)
audio_io.rs (line 183)
audio_io.rs (line 213)
叠录写入索引前移补偿：
audio_io.rs (line 226) -->


<!-- 
录音设备开启了“自动增益/噪声抑制/回声消除”
会做“语音门限”或“语音优先”处理。
结果就是：只有达到一定响度才会放行，或者需要持续语音激活；停说话后会衰减一段时间再静音。
Windows 的“通信模式”或麦克风阵列驱动
一些厂商驱动会在检测到人声时才拉升增益。
录音设备格式/增益设置过低
声音本来就弱，加上降噪门控，键盘敲击被抑制。
建议做法（不改代码）

打开 Windows “声音” -> 录音设备 -> 属性
关闭：回声消除、噪声抑制、自动增益。
试试取消“允许应用独占控制”/或反过来开启（看哪边更稳定）。
录音设备级别拉高一点，或在系统里关闭“通信时降低其他音量”。
如果是笔记本麦克风阵列，换一个普通单指向 USB 麦克风试试。

Windows 声音设备设置
关闭“增强/回声消除/自动增益”等 DSP（会引入大量缓冲）。
把录音设备和播放设备的“格式”设为一致（采样率/位深，例如 48k/16bit）。
关闭“允许应用独占控制”可能会更稳定，但延迟可能略高；开启独占模式可降低延迟。 -->


<!-- 

核心算法：包络跟随器 (Envelope Follower)
这是让你声音“驱动”波形的第一步。算法不需要理解你在唱什么，它只需要知道你声音的“能量”有多大。

算法逻辑：
- 整流 (Rectification)：将输入的音频采样值取绝对值 $|x(t)|$。
- 平滑 (Smoothing)：使用一个低通滤波器（通常是简单的一阶滤波）来提取音量的外轮廓。
  $$E_t = \alpha \cdot |x_t| + (1 - \alpha) \cdot E_{t-1}$$
  其中 $\alpha$ 由你设置的 Attack 和 Release 时间决定。
- 映射 (Mapping)：将提取出的能量 $E_t$ 直接乘以 OSC 生成的波形采样。

结果：当你大声喊，OSC 就大声响；当你闭嘴，OSC 瞬间静音。在这种方案中，你的声音不直接参与构音。声音只是一个
**“开关”和“力度控制器”**。

* **逻辑判定**：如果 `Input_Level > Threshold`，则判定为“激活”状态。
* 当处于“激活”状态时，查找当前时间点所在的 **Sequence Note**。
* OSC 根据该音高实时产生波形，并叠加 `Attack/Release` 包络。
* 用户闭嘴时，Loop 轨道静音（即便 Sequencer 在走）。
* 用户唱歌/发声时，音轨录入的是带有用户节奏、但音高完美的合成器声音。 -->