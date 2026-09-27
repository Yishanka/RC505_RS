# RC‑505mkII 对照与实现依据

核对日期：2026-09-27。本轮不添加 FX 类型。公开文档用于校对行为，无法证明自研 DSP 与硬件逐采样或听感一致，也没有使用 Serum 的非公开代码或逆向产物。

## 已查阅的主要来源

- [BOSS 官方说明书入口](https://www.boss.info/us/support/by_product/rc-505mk2/owners_manuals/) 与 [Parameter Guide rev.04](https://static.roland.com/assets/media/pdf/RC-505mk2_Parameter_eng04_W.pdf)：重点参考 OSC BOT、Vocoder、Delay、Roll、Reverb 的用途和参数。本项目的完整钢琴卷帘是软件扩展。
- [FL Studio Piano roll 官方手册](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)：借鉴音高/时间网格、吸附、右键删除、拖动移动与右边缘改长度等交互，没有复刻界面素材。
- [W3C Audio EQ Cookbook / Robert Bristow-Johnson](https://www.w3.org/TR/audio-eq-cookbook/)：现有 biquad 结构与响应图的数学依据。

## 当前对应关系

| 模块 | 本项目当前状态 | 与硬件的差距 / 后续验收依据 |
|---|---|---|
| 五轨循环 | 采样时钟、音频持久化、Undo/Redo、One Shot、Reverse、Stop、固定长度与量化 | 完整硬件同步/通道矩阵、Tempo Sync音频伸缩、Assign/MIDI未覆盖 |
| FX bank/slot | Input / Track 各4×4；Track 按轨启用 | 串并联、Assign、MIDI、详细路由尚未覆盖 |
| Oscillator | 4种基础波形、门限、AHDSR、音符序列、内部滤波 | 可服务于 OSC BOT 类演奏；波形与参数不是完整对应 |
| Vocoder | 音轨/输入声道载波、Tone/Mod Sens、频谱包络归一化、formant移动 | 输入仅一个立体声设备；Attack毫秒、Bands/Release/Formant/Sibilance属于软件参数；灵敏度和频响需A/B标定 |
| Roll | 捕获前级效果输出；Roll1反馈/Roll2次数；Time、细分、Balance | 按公开控制语义独立实现；反馈曲线、取样时机和瞬态没有实机标定 |
| Delay | 1–2000ms及拍点同步；独立Direct/Effect、反馈低切、立体声、插值与平滑 | 反馈仍为百分比，没有测得硬件次数映射；滑动读头改变时间会改变音高 |
| Reverb | 扩散器+4线FDN，干湿、密度、RT60、0–500ms预延迟、高切Hz | 旧高切百分比映射到原频率；Size/Width/12秒上限为软件扩展；需实测IR/听感 |
| Filter | RBJ biquad、Q、Drive、干湿；可视曲线 | 门限与调制语义为自研；应实测扫频、共振、饱和及电平 |
| MyDelay | 100ms片段采样、音符驱动周期循环 | 自定义效果，不使用硬件名称对应关系来声称一致 |

## 本轮算法决策

1. 优先修可测的正确性问题：tick 触发必须是单采样脉冲；Drive=0 应保持线性；门限应跟随包络而非音频符号/过零点。
2. Roll 的行为改为单片段短周期重复，保留原有 Step 字段并明确旧工程听感会变化。Roll1/2在同一已有FX类型内选择模式，不新增FX种类。
3. 滤波器可视曲线从 DSP 系数计算，包络曲线由同一状态机模拟，避免画出与算法无关的装饰图。
4. 按用户补充授权重写Vocoder，延续原先共振峰对比度的思路，并用整组包络归一化替换逐带限幅。离线双频载波测试验证formant移动方向，尚未证明元音可懂度或硬件等效性。
5. 公开手册描述的是参数用途，不提供专有DSP。没有使用Serum非公开源码，也不将通用合成器算法误称为RC‑505内部算法。
6. rev.04的TRACK说明明确：Reverse和One Shot不进入叠录；One Shot再次播放键重触发；Stop含Immediate/Fade/Loop，再次Stop立即停止。软件沿用这些可核对语义，但量化UI统一为Off/Beat/Measure/Loop，并非原机所有LOOP SYNC子参数的完整复制。
7. 实时线程设计参考[PortAudio回调约束](https://portaudio.com/docs/v19-doxydocs/writing_a_callback.html)：避开分配、文件I/O和mutex。CPAL输入/输出时间戳只作诊断；补偿建议来自实际回环，不假设驱动时间戳包含全部硬件延迟。

## 硬件听感验证建议

为每个效果准备统一干声、固定采样率和电平；记录硬件参数、输入、输出、软件版本。分别比较静态频响/动态包络、过渡点击、立体声相关性、噪声底和电平。测试应包含人声、持续谐波音、瞬态、静音，最终再做响度匹配的盲听。每个差异要落到具体控制量或测量，不以“听起来像”替代验收。
