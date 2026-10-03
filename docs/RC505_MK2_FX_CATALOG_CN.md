[官方原文：RC‑505mkII Parameter Guide rev.04，效果表从第 34 页开始](https://static.roland.com/assets/media/pdf/RC-505mk2_Parameter_eng04_W.pdf#page=34)

[官方说明书入口](https://www.boss.info/us/support/by_product/rc-505mk2/owners_manuals/) · [官方产品规格](https://www.boss.info/us/products/rc-505mk2/) · [官方固件更新记录](https://www.boss.info/global/support/by_product/rc-505mk2/updates_drivers/5c6ad538-d4c8-4187-8a4b-384b7117061c/)

# RC‑505mkII 内置效果完整清单与中文速记

核对日期：2026-10-03。这里的“插件”指机器内置 FX 类型，不是另外安装的 VST。名称和可用位置按官方手册核对；中文栏是便于选型的简短说明，不是原文逐句翻译或对内部 DSP 的断言。

## 数量与版本怎么数

官网标称 **49 种 Input FX、53 种 Track FX**，每个区最多同时使用 4 个效果。[官方规格](https://www.boss.info/us/products/rc-505mk2/)

rev.04 手册将 **TAPE ECHO1/2**、**ROLL1/2** 分别展开为两张参数表。下面将每一组归为一个效果家族，列出 **49 个共用家族＋4 个 Track 专用家族**；两组的 1/2 版本都包含在表中。若把它们拆成独立行，就会有 51＋4 行，不能把这种整理方式擅自改称新的“官方 55 种”规格。MODE 子选项、失真子类型、音箱型号也不重复计数。官方更新记录确认旧 RC 算法后来被补进若干效果；菜单细节需结合固件版本理解。[版本说明](https://www.boss.info/global/support/by_product/rc-505mk2/updates_drivers/5c6ad538-d4c8-4187-8a4b-384b7117061c/)

## Input FX 与 Track FX 共用：49 个家族

“现状”按 RC505 RS **0.2.8** 的实际类型和路由标注：**部分**只表示有相关自研实现，不表示参数或听感等同硬件；**未有**表示尚未接入对应效果。页码指上方官方参数手册的印刷页码。完整硬件条目见原文第 34–42 页。

| # | 官方名称 | 简单中文说明 | 页 | RC505 RS 现状 | 操作 |
|---:|---|---|---:|---|---|
| 1 | LPF | 低通，削弱高频 | 34 | 部分：Input/Track Filter | 123 合一、完善 |
| 2 | BPF | 带通，突出一段频带 | 34 | 部分：Input/Track Filter | 123 合一、完善 |
| 3 | HPF | 高通，削弱低频 | 34 | 部分：Input/Track Filter | 123 合一、完善 |
| 4 | PHASER | 相位扫动，形成流动凹口 | 34 | 未有 | 做 |
| 5 | FLANGER | 镶边，梳状扫频感 | 35 | 未有 | 做 |
| 6 | SYNTH | 把输入加工成合成器质感 | 35 | 未有；不等于现有 Oscillator | 解释 |
| 7 | LO-FI | 降位深、降采样率的粗糙质感 | 35 | 未有 | / |
| 8 | RADIO | 收音机式窄带、劣化质感 | 35 | 未有 | / |
| 9 | RING.MOD | 环形调制，金属感边带 | 35 | 未有 | / |
| 10 | G2B | 吉他转低音贝斯效果 | 35 | 未有 | / | 
| 11 | SUSTAINER | 拉平音量，延长持续感 | 35 | 未有 | 做 | 
| 12 | AUTO RIFF | 从输入生成自动乐句 | 36 | 未有 |  解释 |
| 13 | SLOW GEAR | 渐入，弱化拨弦起音 | 36 | 未有 | / |
| 14 | TRANSPOSE | 对已有声音做移调 | 36 | 未有 | 优先做 |
| 15 | PITCH BEND | 连续弯音 | 36 | 未有 | 解释 |
| 16 | ROBOT | 固定音高的机器人声 | 36 | 未有 | / |
| 17 | ELECTRIC | 阶梯化音高、机械人声 | 36 | 未有 | 这个是电音吗？是就优先做 |
| 18 | HRM MANUAL | 按调性与音程配置和声 | 37 | 未有 | 优先做 |
| 19 | HRM AUTO (M) | MIDI 和弦参与控制和声 | 37 | 未有 | / |
| 20 | VOCODER | 载波与调制音结合的声码器 | 37 | 部分：仅 Input Vocoder | 完善 |
| 21 | OSC VOC (M) | MIDI 控制的振荡器声码器 | 37 | 未有；不能用普通 Vocoder 冒充 | / |
| 22 | OSC BOT | 按音符参数发声的振荡器 | 38 | 近似用途：Input Oscillator | 我们的应该是超集吧？是则不做，不是则完善 |
| 23 | PREAMP | 音箱/前级染色 | 38 | 未有 | / |
| 24 | DIST | 过载、失真等音色 | 38 | 未有；Filter Drive 不是完整替代 | 优先做 |
| 25 | DYNAMICS | 压缩、限幅等动态处理 | 38 | 未有 | 优先做 |
| 26 | EQ | 多频段均衡 | 38 | 未有；单个 Filter 不是 EQ | 优先做 |
| 27 | ISOLATOR | 分频段切除，可作节奏变化 | 39 | 未有 | / |
| 28 | OCTAVE | 叠加低八度声音 | 39 | 未有 | 优先做 |
| 29 | AUTO PAN | 自动左右声像运动 | 39 | 未有 | 优先做 |
| 30 | MANUAL PAN | 手动定位左右声像 | 39 | 未有 | 做 |
| 31 | STEREO ENHANCE | 立体声扩展 | 39 | 未有 | 做 |
| 32 | TREMOLO | 周期性音量起伏 | 39 | 未有；重复触发包络不算完整实现 | 做 |
| 33 | VIBRATO | 周期性音高摆动 | 39 | 未有 | 做 |
| 34 | PATTERN SLICER | 按既定节奏切分音量 | 40 | 未有 | / |
| 35 | STEP SLICER | 逐步设置的音量切分 | 40 | 未有；Track Filter 步进不等同它 | 做 |
| 36 | DELAY | 常规回声 | 40 | 部分：仅 Track Delay | 完善 | 
| 37 | PANNING DELAY | 左右分布的抽头回声 | 40 | 未有；普通立体声 Delay 不等同它 | 优先做 |
| 38 | REVERSE DELAY | 倒放式延迟片段 | 40 | 未有 | / |
| 39 | MOD DELAY | 带合唱式调制的回声 | 40 | 未有 | / |
| 40 | TAPE ECHO1 / TAPE ECHO2 | 磁带回声；旧版/新版算法 | 41 | 两版均未有 | / |
| 41 | GRANULAR DELAY | 短片段重复、颗粒嗡鸣 | 41 | 未有；MyDelay 是另一种自研用途 | / |
| 42 | WARP | 梦幻式空间变形 | 41 | 未有；官方描述不足以确定拓扑 | / |
| 43 | TWIST | 强烈旋转、加速感 | 41 | 未有；不能仅凭听感断言内部算法 | / |
| 44 | ROLL1 / ROLL2 | 冻结短循环并改变细分 | 41 | 部分：Track Roll 的两个模式 | 完善 |
| 45 | FREEZE | 冻结、延续当前声音 | 42 | 未有；Roll 的冻结不等同此效果 | 做 |
| 46 | CHORUS | 合唱式增厚、轻微失谐 | 42 | 未有 | 做 |
| 47 | REVERB | 常规混响空间 | 42 | 部分：仅 Input Reverb | 完善 |
| 48 | GATE REVERB | 截断式混响 | 42 | 未有 | / |
| 49 | REVERSE REVERB | 渐强式反向混响 | 42 | 未有 | / |

## 仅 Track FX：另外 4 个家族

这些是对已录循环的播放操作，不只是给实时输入套一个滤镜。手册第 43 页另有槽位限制：**TRACK FX MODE 为 MULTI 时，这四种只能放 FX A**。这不代表其他共用效果不能用于 Track FX。

| # | 官方名称 | 简单中文说明 | 页 | RC505 RS 现状 |
|---:|---|---|---:|---|
| 50 | BEAT SCATTER | 随拍重排、刮擦循环 | 43 | 未有 | / |
| 51 | BEAT REPEAT | 按拍重复，可正向/反向 | 43 | 未有；不等同 Track Roll | / |
| 52 | BEAT SHIFT | 按拍偏移播放位置 | 43 | 未有 | / |
| 53 | VINYL FLICK | 模拟拨动唱盘的变速感 | 43 | 未有 | / |

Track FX 插入目标可以指定轨道；并不意味着机器提供“五轨各自四个互不相关的硬件 FX 机架”。[官方：分别向轨道应用 Track FX](https://support.roland.com/hc/en-us/articles/30652941868443-RC-505mkII-How-do-I-apply-different-Track-FX-to-each-track)

## 不要遗漏，也不要混算

- **主效果**还有压缩、混响；输入/输出混音器还有相应的电平、EQ 等处理。它们不计入上面 49/53 的插入效果清单。[产品与混音说明](https://www.boss.info/us/products/rc-505mk2/)
- PREAMP 的音箱型号、DIST 的失真风格、DYNAMICS 的动态风格是各效果内部选项；需实现时再展开参数，不在这里重复算成几十个插件。
- 当前软件的 **MyDelay**、钢琴卷帘及独立试听是自研功能，不能加进“官方效果列表”。
- “音高相关”不等于“支持钢琴卷帘”：移调、弯音、低八度通常操纵已有音频；可演奏音源才需要 Note On/Off。具体架构建议见[设计讨论](DESIGN_SYNTH_SEQUENCER_CN.md)。
> 压缩、混响优先做

## 对这个项目最值得先看的缺口

这是建议优先级，不是已经决定开发的新 FX：

1. **Transpose / Pitch Bend / Octave**：先明确整轨音高变换、延迟与音质目标，补足现有 Track FX 的音高能力。
2. **已有 Reverb / Vocoder 的 Track 路由**：优先复用成熟实现，先定义载波、录入和旁路语义。
3. **Panning Delay**：适合扩展现有 Delay 内核，但须补真实抽头布局与反馈映射测量。
4. **Slicer / Tremolo**：可受益于统一的参数序列与 LFO 框架。
5. **Granular / Harmonist / Electric**：分别涉及采样重排、和声或音高分析；应独立验证，避免一次变成庞大合成器工程。
