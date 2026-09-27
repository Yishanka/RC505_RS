# RC505 RS

面向键盘演奏与鼠标编辑的五轨桌面 Loop Station，使用 Rust、CPAL 和 egui 开发。目标是在逐步接近 BOSS RC‑505mkII 工作方式的同时，提供钢琴卷帘、参数可视化和预设编辑。

[English](README.md) · [完整操作手册](docs/USER_GUIDE_CN.md) · [硬件对照与来源](docs/RC505_REFERENCE.md) · [后续规划](AGENTS/PLAN.md)

![演奏工作台](docs/images/performance.png)

## 运行

当前主要开发和验证平台是 Windows。安装 Rust 与 Visual Studio 的 MSVC C++ 构建工具后：

```powershell
cargo run --release --bin rc505_rs
```

可选启动器负责选择声卡、延迟补偿和工程；与主程序采用相同主题：

```powershell
cargo build --release --bins
.\target\release\rc505_launcher.exe
```

只整理预设、暂时不用声卡：

```powershell
cargo run --release --bin rc505_rs -- --offline
# 便携/测试工作区，可替换为自己的目录
cargo run --release --bin rc505_rs -- --offline --data-dir=E:\RC505-data
```

已准备好 ASIO SDK 和驱动的环境可加 `--features asio`。本轮验证使用默认 feature，未验证 ASIO。

## 先录一段

1. 点击工程打开，或输入名字后 **Create and open**。
2. 在 **Audio devices & latency** 检查设备，设置 BPM。
3. 按 `1` 开始轨1录音，再按一次在下一拍结束并循环。
4. 播放时按 `1` 叠录，再按一次在loop边界结束叠录；`F1`停止。
5. 点击 FX 槽选择效果，开启 **On**；展开编辑器调整完整参数。
6. 按 `Ctrl+S` 保存参数。

**工程目前不保存录制的loop音频。** 它保存参数、序列、FX开关、音轨音量和键盘推子速度。录音只存在于当前会话；退出或切换工程会丢失。

## 演奏与编辑

| 操作 | 按键 / 鼠标 |
|---|---|
| 轨1–5录音、播放、叠录、完成 | `1`–`5` |
| 停止轨1–5 | `F1`–`F5` |
| 全部开始/停止 | `Space` |
| 选择Track FX作用的轨道 | 点击轨道标题或左右方向键 |
| Input FX / 所选轨Track FX开关 | `Q W E R` / `U I O P` |
| FX键切换为选择bank | `T` |
| 轨1–5降低/提高音量 | `Z X` / `C V` / `B N` / `M ,` / `. /` |
| 精细音量控制 | 按住 `Shift` |
| 选择/展开参数 | 点击FX槽 → **Expand editor** |
| 保存 / 返回演奏台 | `Ctrl+S` / 展开时 `Esc` |

推子短按0.5 dB、长按逐渐加速；Shift短按0.1 dB，长按速度降到1/8。最高速度在 **Keyboard faders & shortcuts** 中调节。不同轨道可同时升降，松键即停。鼠标推子也采用分贝刻度。输入文字、展开编辑器或窗口失去焦点时，演奏按键暂停。

展开界面提供单声部钢琴卷帘、拖动音符与长度、撤销/重做、复制pattern、移调、节拍吸附、滤波响应、AHDSR曲线与单槽预设。演奏台保留快速编辑区，小窗口可折叠参数区以优先显示轨道。

![钢琴卷帘](docs/images/sequence.png)

## 这一版的范围

没有增加FX类型。改进现有Vocoder、Roll、Reverb、Delay、Filter、Oscillator/MyDelay的参数链路和相关算法，包括Vocoder频谱包络塑形、Roll1/2参数、节拍同步、混响干湿与密度、单次音符触发和稳定门限。旧参数工程继续兼容，但修正算法会改变部分听感。

实现参考 [BOSS 官方参数手册](https://static.roland.com/assets/media/pdf/RC-505mk2_Parameter_eng04_W.pdf)，仍有明确的软件扩展与差异；没有实机A/B录音，不能宣称音色已经等同硬件。详细参数、载波接线、信号流和差异见[操作手册](docs/USER_GUIDE_CN.md)及[硬件对照](docs/RC505_REFERENCE.md)。

## 开发与验证

```powershell
cargo check --all-targets
cargo test --all-targets
cargo build --release --bins
```

- [架构说明](docs/ARCHITECTURE.md)：状态、命令、编辑器、配置、DSP和存档边界。
- [验证记录](docs/VALIDATION.md)：自动化覆盖、原生窗口截图、实机验收步骤及限制。
- [开发约定](AGENTS.md)：精简后的维护入口；旧AGENTS记录仅作为历史资料。

目前仍需完成音频持久化、采样级调度、回调锁/动态内存治理、完整RC‑505路由和听感标定；优先级见[规划](AGENTS/PLAN.md)。
