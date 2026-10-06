# RC‑505mkII 对照与实现依据

更新日期：2026-10-04。本文记录音源、调制、回放和效果时间对齐的实现依据；完整范围见[效果清单](RC505_MK2_FX_CATALOG_CN.md)和[算法说明](FX_IMPLEMENTATION_CN.md)。公开文档用于校对行为，不能证明自研 DSP 与硬件逐采样或听感一致；未使用 Serum 的非公开代码或逆向产物。

## 已查阅的主要来源

- [BOSS 官方说明书入口](https://www.boss.info/us/support/by_product/rc-505mk2/owners_manuals/) 与 [Parameter Guide rev.04](https://static.roland.com/assets/media/pdf/RC-505mk2_Parameter_eng04_W.pdf)：重点参考 OSC BOT、Vocoder、Delay、Roll、Reverb 的用途和参数。本项目的完整钢琴卷帘是软件扩展。
- [FL Studio Piano roll 官方手册](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/pianoroll.htm)：借鉴音高/时间网格、吸附、右键删除、拖动移动与右边缘改长度等交互，没有复刻界面素材。
- [W3C Audio EQ Cookbook / Robert Bristow-Johnson](https://www.w3.org/TR/audio-eq-cookbook/)：现有 biquad 结构与响应图的数学依据。

## 当前对应关系

| 模块 | 本项目当前状态 | 与硬件的差距 / 后续验收依据 |
|---|---|---|
| 五轨循环 | 采样时钟、音频持久化、Undo/Redo、One Shot、Reverse、Stop、固定长度与量化 | 完整硬件同步/通道矩阵、Tempo Sync音频伸缩、Assign/MIDI未覆盖 |
| 输入 NS | 全局输入噪声门，开关与 −80～0 dBFS 阈值，默认关闭 | 参考官方 MIC/INST 输入 NS；硬件只公开 0～100 深度，软件阈值和时间常数为独立设计，见[参数手册第10页](https://static.roland.com/assets/media/pdf/RC-505mk2_Parameter_eng04_W.pdf#page=10) |
| FX bank/slot | Input / Track 各4×4；Track 按轨启用 | 串并联、Assign、MIDI、详细路由尚未覆盖 |
| OSC | 基础/元音/采样波形、单音/8/16声部、Legato/Glide、960 PPQ链接乐句、固定AHDSR视窗、双LFO、内部滤波 | 软件音源扩展；不能将复音/采样能力称为OSC BOT的完整硬件覆盖 |
| Vocoder | 音轨/输入声道载波、Tone/Mod Sens、频谱包络归一化、formant移动 | 输入仅一个立体声设备；Attack毫秒、Bands/Release/Formant/Sibilance属于软件参数；灵敏度和频响需A/B标定 |
| Roll | 捕获前级效果输出；Roll1反馈/Roll2次数；Time、细分、Balance | 按公开控制语义独立实现；反馈曲线、取样时机和瞬态没有实机标定 |
| Delay / Panning Delay | 共用延迟核，1–2000ms与拍点同步，Direct/Effect、高低切、次数或百分比反馈、立体声双抽头 | 次数以衰减阈值映射；BOSS抽头与交叉反馈拓扑未实测，滑动读头改变时间会改变音高 |
| Reverb | 扩散器+4线FDN，干湿、密度、RT60、0–500ms预延迟、高切Hz | 旧高切百分比映射到原频率；Size/Width/12秒上限为软件扩展；需实测IR/听感 |
| Filter | TPT状态变量、Q、Drive、干湿、扫频和阶梯；可视曲线 | 深度映射等为自研；应实测扫频、共振、饱和及电平 |
| 旧 MyDelay | 合并为 OSC 的采样波形/采样音色；旧配置需重新捕获或导入素材 | 旧JSON没有保存捕获缓存，无法从参数恢复历史采样；旧回放需注意迁移提示 |
| 新音高/调制/动态效果 | 共振峰保持/偏移、PDC、四段EQ、调制与阶梯、mono拓宽；Transpose是控制序列 | 原机专有子型号与听感尚未完整标定；PDC不消除物理监听延迟 |

## 延续的算法约定

1. 优先修可测的正确性问题：tick 触发必须是单采样脉冲；Drive=0 应保持线性；门限应跟随包络而非音频符号/过零点。
2. Roll 的行为改为单片段短周期重复，保留原有 Step 字段并明确旧工程听感会变化。Roll1/2在同一已有FX类型内选择模式，不新增FX种类。
3. 滤波器可视曲线从 DSP 系数计算，包络曲线由同一状态机模拟，避免画出与算法无关的装饰图。
4. Vocoder 使用共振峰对比度与整组包络归一化，替代早期的逐带限幅。离线双频载波测试验证 formant 移动方向，尚未证明元音可懂度或硬件等效性。
5. 公开手册描述的是参数用途，不提供专有DSP。没有使用Serum非公开源码，也不将通用合成器算法误称为RC‑505内部算法。
6. rev.04的TRACK说明明确：Reverse和One Shot不进入叠录；One Shot再次播放键重触发；Stop含Immediate/Fade/Loop，再次Stop立即停止。软件沿用这些可核对语义，但量化UI统一为Off/Beat/Measure/Loop，并非原机所有LOOP SYNC子参数的完整复制。
7. 实时线程设计参考[PortAudio回调约束](https://portaudio.com/docs/v19-doxydocs/writing_a_callback.html)：避开分配、文件I/O和mutex。CPAL输入/输出时间戳只作诊断；补偿建议来自实际回环，不假设驱动时间戳包含全部硬件延迟。
8. 静默录入对应官方第12页 **INPUT THRU OFF**：切断输入到输出的直通分支，输入处理与轨道录音继续，已有循环播放保持。合成音源也走软件的同一输入总线。
9. 新音色、包络和移调算法会改变旧项目/回放的声音。当前写入 renderer 10，记录配置及 PDC 应用标记；旧2/3/4回放关闭PDC，旧2～7回放旁路输入噪声门。接受旧版文件不代表保留每种历史DSP的逐位输出。

## 硬件听感验证建议

为每个效果准备统一干声、固定采样率和电平；记录硬件参数、输入、输出、软件版本。分别比较静态频响/动态包络、过渡点击、立体声相关性、噪声底和电平。测试应包含人声、持续谐波音、瞬态、静音，最终再做响度匹配的盲听。每个差异要落到具体控制量或测量，不以“听起来像”替代验收。
