//! UI preference only: never serialized into projects, presets or replay commands.
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "zh-CN")]
    Chinese,
    #[serde(rename = "en", other)]
    English,
}
impl Language {
    pub fn choose<'a>(self, english: &'a str, chinese: &'a str) -> &'a str {
        if self == Self::Chinese {
            chinese
        } else {
            english
        }
    }
    pub fn current(ctx: &egui::Context) -> Self {
        ctx.data(|d| d.get_temp(egui::Id::new("ui-language")))
            .unwrap_or_default()
    }
    pub fn apply(self, ctx: &egui::Context) {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("ui-language"), self));
    }
    pub fn text<'a>(self, source: &'a str) -> &'a str {
        for &(english, chinese) in TRANSLATIONS {
            if source == english || source == chinese {
                return if self == Self::Chinese {
                    chinese
                } else {
                    english
                };
            }
        }
        source
    }
}
const TRANSLATIONS: &[(&str, &str)] = &[
    ("Transpose", "移调"),
    ("Electric", "电音校音"),
    ("Harmonist", "手动和声"),
    ("Distortion", "失真"),
    ("Dynamics", "动态处理"),
    ("Equalizer", "均衡器"),
    ("Octave", "八度"),
    ("Auto Pan", "自动声像"),
    ("Panning Delay", "左右抽头延迟"),
    ("Phaser", "相位器"),
    ("Flanger", "镶边"),
    ("Sustainer", "延音压缩"),
    ("Manual Pan", "手动声像"),
    ("Stereo Enhance", "立体声扩展"),
    ("Tremolo", "振幅颤音"),
    ("Vibrato", "音高颤音"),
    ("Step Slicer", "步进切片"),
    ("Freeze", "冻结延音"),
    ("Chorus", "合唱"),
    ("Repeats (0 = manual feedback)", "次数（0 手动）"),
    (
        "Connect audio before testing",
        "音频未连接，请先检查设备并重新连接",
    ),
    (
        "Read-only editor cannot persist the monitoring safety guard",
        "当前是只读实例，无法保存监听保护状态",
    ),
    ("Finish replay recording first", "请先结束回放录制"),
    (
        "Stop the player and audition first",
        "请先关闭回放播放器并停止独立试听",
    ),
    (
        "Stop the performance and all five tracks first",
        "请先停止演出和所有轨道",
    ),
    ("Wait for the current operation", "请等待当前操作完成"),
    (
        "Disconnect the loopback cable and restore monitoring first",
        "请先拔掉回环线并恢复监听",
    ),
    ("Connect audio before auditioning", "请先连接音频设备再试听"),
    (
        "Capture or import a sample first",
        "请先捕获输入或导入采样素材",
    ),
    ("Close the replay player first", "请先关闭回放播放器"),
    (
        "Add notes to the piano roll first",
        "请先在钢琴卷帘中添加音符",
    ),
    (
        "Record track audio before auditioning its step filter",
        "请先录入轨道音频，再试听步进滤波",
    ),
    (
        "Record track audio before previewing this sound",
        "请先录入轨道音频，再试听候选音色",
    ),
    (
        "Save the replay or keep it as a draft first",
        "请先保存回放或选择保留为草稿",
    ),
    (
        "Replay recording is starting or finishing",
        "回放正在准备或收尾，请稍候",
    ),
    ("This editor is read-only", "当前编辑器为只读实例"),
    (
        "Connect audio before recording a replay",
        "请先连接音频设备再录制回放",
    ),
    (
        "Stop all five tracks before recording a replay",
        "请先停止五条轨道，再录制回放",
    ),
    (
        "Finish track recording or overdub before ending the replay",
        "请先结束轨道录音或叠录，再结束回放录制",
    ),
    (
        "Prepare and confirm the loopback cable first",
        "请先进入静音准备并确认回环接线",
    ),
    ("Sin", "正弦波"),
    ("Sqr", "方波"),
    ("Tri", "三角波"),
    (
        "Esc: editor → performance → project browser (save prompt).",
        "Esc：编辑器 → 演奏界面 → 工程选择页（先提示保存）。",
    ),
    ("Attack(ms)", "起音 (ms)"),
    ("Hold(ms)", "保持 (ms)"),
    ("Decay(ms)", "衰减 (ms)"),
    ("Sustain(%)", "持续 (%)"),
    ("Release(ms)", "释音 (ms)"),
    ("Start(%)", "起始电平 (%)"),
    ("Tension-A", "起音曲率"),
    ("Tension-D", "衰减曲率"),
    ("Tension-R", "释音曲率"),
    ("Compensation (ms)", "延迟补偿 (ms)"),
    ("Level", "电平"),
    ("Threshold", "阈值"),
    ("Direct level (%)", "原声音量 (%)"),
    ("Effect level (%)", "效果音量 (%)"),
    ("Direct level(%)", "原声音量 (%)"),
    ("Effect level(%)", "效果音量 (%)"),
    ("Feedback low cut (Hz; 0 = thru)", "反馈低切 (Hz；0 为直通)"),
    ("Time mode", "时间模式"),
    ("Milliseconds", "毫秒"),
    ("1/4 note", "1/4"),
    ("1/8 note", "1/8"),
    ("1/16 note", "1/16"),
    ("1/8 dotted", "1/8 附点"),
    ("1/8 triplet", "1/8 三连音"),
    ("1 bar", "1 小节"),
    ("2 bars", "2 小节"),
    ("4 bars", "4 小节"),
    ("Time(ms)", "时间 (ms)"),
    ("Feedback(%)", "反馈 (%)"),
    ("HighDamp(Hz)", "高频阻尼 (Hz)"),
    ("Mix(%)", "干湿混合 (%)"),
    ("Note", "音符"),
    ("Step", "步长"),
    ("Seq", "序列"),
    ("Waveform", "波形"),
    ("OSC level (%)", "OSC 电平（%）"),
    ("RECT (25%)", "矩形脉冲（25%）"),
    ("Detune Saw", "双锯失谐"),
    ("Vintage Saw", "暖化锯齿"),
    ("Vocal", "人声波形"),
    ("Sample", "采样"),
    (
        "Legacy MyDelay was migrated to OSC; capture or import a sample first.",
        "旧 MyDelay 已迁移为 OSC；请先捕获或导入采样素材。",
    ),
    ("Sine", "正弦波"),
    ("Square", "方波"),
    ("Saw", "锯齿波"),
    ("Triangle", "三角波"),
    ("High cut (Hz)", "高切 (Hz)"),
    ("Density", "密度"),
    ("Size", "空间大小"),
    ("PreDelay(ms)", "预延迟 (ms)"),
    ("Width(%)", "立体声宽度 (%)"),
    ("LowCut(Hz)", "低切 (Hz)"),
    ("Type", "类型"),
    ("Cutoff(Hz)", "截止频率 (Hz)"),
    ("Drive(%)", "驱动 (%)"),
    ("Base cycle", "基础周期"),
    ("Mode", "模式"),
    ("Repeat (0 = infinite)", "重复次数 (0 为无限)"),
    ("Balance(%)", "混合比例 (%)"),
    ("Input Device", "输入设备"),
    ("Output Device", "输出设备"),
    ("Sibilance(%)", "齿音 (%)"),
    ("Carrier", "载波"),
    ("Bands", "频段数"),
    ("Tone", "音色"),
    ("Mod sensitivity", "调制灵敏度"),
    ("Formant (semitones)", "共振峰 (半音)"),
    (
        "Carrier thru (dry carrier channel)",
        "载波直通（载波通道干声）",
    ),
    ("Step gate sequencer", "步进门控序列"),
    ("Append step", "添加步进"),
    ("Remove last", "移除末步"),
    (
        "Captures a short input fragment and repeats it at the selected pitch. This custom effect needs incoming audio.",
        "捕获短输入片段并以所选音高重复。此自研效果需要输入音频。",
    ),
    (
        "Diffusion + FDN reverb / High cut in Hz / Decay is RT60.",
        "扩散与 FDN 混响；高切以 Hz 表示，衰减为 RT60。",
    ),
    (
        "Stereo input: carrier on selected channel, voice on the other. One stereo device, not two independent devices.",
        "立体声输入：所选通道作为载波，另一通道输入人声。使用同一立体声设备。",
    ),
    (
        "Record a harmonically rich carrier to the selected track. A paused carrier follows the running timeline without playing dry.",
        "将谐波丰富的载波录入所选轨道。停止的载波轨会跟随运行时钟，但不播放干声。",
    ),
    (
        "Formant moves the spectral envelope; carrier pitch is preserved. Sibilance emphasizes upper analysis bands.",
        "共振峰移动频谱包络，保留载波音高；齿音增强高频分析频段。",
    ),
    (
        "Captures recent audio including preceding Track FX. Division shortens the frozen slice; Off repeats the full cycle using Feedback / Repeat. Repeat 0 = infinite.",
        "捕获近期音频（含前级轨道效果）。分割会缩短冻结切片；关闭分割则按反馈或重复次数循环完整周期，0 次为无限。",
    ),
    (
        "With no recent history, capture waits for one slice. Toggle the slot off/on to capture again. Sync time is limited to the 2 s capture buffer.",
        "无历史音频时，等待捕获一段切片。关闭再启用此槽可重新捕获；同步时长受 2 秒缓冲限制。",
    ),
    ("Track", "轨道"),
    ("Audio", "音频"),
    ("Session", "工程"),
    ("FX EDITOR", "效果编辑器"),
    ("INPUT FX", "输入效果"),
    ("TRACK FX", "轨道效果"),
    ("Top controls", "顶部栏"),
    ("Performance", "演奏"),
    ("Left controls", "左侧面板"),
    ("FX controls", "效果面板"),
    ("Tap tempo", "击拍定速"),
    ("Start / stop all", "全部启停"),
    ("Save", "保存"),
    ("Configuration", "仅配置"),
    ("Configuration + audio", "配置与音频"),
    ("Record take", "录制回放"),
    ("End take", "结束录制"),
    ("Projects", "返回工程"),
    ("Help", "帮助"),
    ("Reverse", "倒放"),
    ("One shot", "单次播放"),
    ("Stop", "停止"),
    ("Immediate", "立即"),
    ("Loop end", "循环结束"),
    ("Fade out", "淡出"),
    ("Quantize", "量化"),
    ("Off", "关闭"),
    ("Beat", "拍"),
    ("Measure", "小节"),
    ("Loop", "循环"),
    ("Length", "长度"),
    (
        "0 = finish manually; 1–128 = fixed length in 4/4. Maximum audio length is five minutes.",
        "0 为手动结束；1–128 为固定 4/4 小节数。最长音频为五分钟。",
    ),
    (
        "0 bars = manual finish · Reverse / One shot disable overdub",
        "0 小节为手动结束；倒放或单次播放时不支持叠录",
    ),
    ("Undo", "撤销"),
    (
        "Hold Delete for 0.75 s or double-press within 350 ms. Clear can be undone from track history.",
        "长按 Delete 0.75 秒或在 350 毫秒内双击。可用轨道撤销恢复清空前的音频。",
    ),
    ("Redo", "重做"),
    ("Clear audio", "清空音频"),
    (
        "Hold Delete for 0.75 s or double-press within 350 ms. Clears the selected track and its undo audio.",
        "长按 Delete 0.75 秒，或在 350 毫秒内双击，清空所选轨及其撤销音频。",
    ),
    (
        "Click twice within 350 ms, or hold for 0.75 s. Release to cancel a hold.",
        "350 毫秒内双击按钮，或按住 0.75 秒；松开可取消长按。",
    ),
    ("Hold 0.75 s / double-press", "长按 0.75 秒 / 双击"),
    ("Expand selected FX", "展开所选效果"),
    ("Buffer", "缓冲"),
    ("Reconnect", "重新连接"),
    (
        "Loopback: connect output L to input L; disconnect speakers. Monitoring is muted during probes.",
        "回环：连接左输出到左输入，断开扬声器。测量时监听静音。",
    ),
    ("Measure loopback", "测量回环延迟"),
    ("Apply", "应用"),
    ("Save configuration", "保存配置"),
    ("Save audio snapshot", "保存音频快照"),
    ("Input FX order", "输入效果顺序"),
    ("Legacy groups", "旧版分组"),
    ("Slot A → D", "槽位 A → D"),
    ("Replay library & import", "回放库与导入"),
    ("Signal flow and operation guide", "信号流与操作指南"),
    ("On", "启用"),
    (
        "Shift: hold effect · Alt: bank · Ctrl: edit",
        "Shift 按住临时启用 · Alt 选择组 · Ctrl 编辑",
    ),
    ("EMPTY", "空轨"),
    ("RECORDING", "录音中"),
    ("OVERDUB", "叠录中"),
    ("PLAYING", "播放中"),
    ("STOPPED", "已停止"),
    ("QUEUED", "等待量化点"),
    ("Speed", "速度"),
    ("Record", "录音"),
    ("Play", "播放"),
    ("Finish", "结束录音"),
    ("Retrigger", "重新触发"),
    ("Overdub", "叠录"),
    ("Choose your project", "选择工程"),
    (
        "↑ ↓ select · Enter opens · N creates",
        "↑ ↓ 选择 · Enter 打开 · N 新建",
    ),
    ("Open project", "打开工程"),
    ("New", "新建"),
    ("Rename", "重命名"),
    ("Manage", "管理"),
    ("Move selected to Trash", "将所选工程移入回收站"),
    ("Restore last deleted", "恢复最近删除的工程"),
    ("New project", "新建工程"),
    ("Rename project", "重命名工程"),
    ("Project name", "工程名称"),
    ("Confirm", "确认"),
    ("Cancel", "取消"),
    ("Save before leaving", "离开前保存"),
    (
        "Choose what to keep in this project.",
        "选择要保留的工程内容。",
    ),
    (
        "Save configuration and audio snapshot",
        "保存配置与音频快照",
    ),
    ("Save configuration only", "仅保存配置"),
    (
        "Configuration only keeps the previous saved audio, not the current loops.",
        "仅保存配置会保留上次已保存的音频，不会保存当前循环音频。",
    ),
    ("Discard session changes", "放弃本次修改"),
    ("Shape your next loop", "编辑你的循环音色"),
    (
        "Select an FX slot below to edit. Expand opens a full sound-design workspace.",
        "选择下方效果槽快速调整；点击展开进入完整音色编辑器。",
    ),
    ("Enabled", "启用"),
    ("Back to performance", "返回演奏"),
    ("Expand", "展开"),
    ("Sequence preview", "序列试听"),
    (
        "Run the sequencer clock without recording. Oscillator threshold still applies; use 0 for ungated preview.",
        "运行序列时钟而不录音。振荡器阈值仍生效，设为 0 可直接试听。",
    ),
    ("Editing an inactive bank.", "当前编辑的是未激活的效果组。"),
    ("Activate this bank", "激活此组"),
    ("Preset", "预设"),
    ("Name for a new preset", "新预设名称"),
    ("Save as new", "另存为新预设"),
    ("Preset saved", "预设已保存"),
    ("Load preset…", "载入预设…"),
    ("Sound", "音色"),
    ("Piano roll", "钢琴卷帘"),
    ("Amp envelope", "音量包络"),
    ("Filter", "滤波器"),
    ("Filter envelope", "滤波包络"),
    ("Open piano roll", "打开钢琴卷帘"),
    (
        "Choose an effect type, then enable the slot in the rack.",
        "先选择效果类型，再在效果架启用此槽。",
    ),
    (
        "Choose a playback effect. Enable it independently for each track.",
        "选择播放效果，可分别对每轨启用。",
    ),
    ("Empty", "空槽"),
    ("Oscillator", "振荡器"),
    ("Reverb", "混响"),
    ("MyDelay", "MyDelay"),
    ("Vocoder", "声码器"),
    ("Delay", "延迟"),
    ("Roll", "切片循环"),
    ("PIANO ROLL", "钢琴卷帘"),
    ("View octave", "显示八度"),
    ("Zoom", "缩放"),
    ("Bars", "小节"),
    ("Snap", "吸附"),
    ("1/24 triplet", "1/24 三连音"),
    ("1/12 triplet", "1/12 三连音"),
    (
        "Resize the loop; shortening trims notes. Undo restores them.",
        "调整循环长度；缩短会裁剪音符，可撤销恢复。",
    ),
    ("Copy pattern", "复制乐句"),
    ("Paste", "粘贴"),
    ("Duplicate", "重复一遍"),
    ("Clear notes", "清空音符"),
    ("Starter melody", "示例旋律"),
    (
        "Transpose the entire pattern in semitones",
        "以半音为单位移调整个乐句",
    ),
    (
        "Replace with a one-bar C-minor phrase; Undo restores the previous pattern",
        "替换为一小节 C 小调乐句；可撤销恢复原乐句",
    ),
    ("Start tick", "起始格"),
    ("Selected", "已选"),
    (
        "Select a note to edit its start and length.",
        "选择音符以编辑起始位置和长度。",
    ),
    (
        "Left click: draw / select  •  Drag: move  •  Drag right edge: resize  •  Right click / Delete: erase  •  Ctrl+Z / Ctrl+Y  •  Monophonic",
        "左键绘制 / 选择 · 拖动移动 · 拖右边缘调整长度 · 右键 / Delete 删除音符 · Ctrl+Z / Ctrl+Y 撤销 / 重做 · 单音序列",
    ),
    (
        "FILTER RESPONSE / steady-state, 48 kHz • drag to set cutoff and resonance",
        "滤波频响 / 稳态、48 kHz · 拖动设置截止频率与共振",
    ),
    (
        "Linear response includes dry/wet; drive and envelope motion are not shown.",
        "线性频响包含干湿混合；不显示失真与包络变化。",
    ),
    ("Replay captured", "回放录制完成"),
    (
        "Name this take, export audio, or leave it in draft history.",
        "命名保存、导出音频，或保留在草稿历史中。",
    ),
    ("Save replay", "保存回放"),
    ("Export audio", "导出音频"),
    ("Keep as draft", "保留为草稿"),
    ("Replay library", "回放库"),
    (
        "Replay folder containing replay.json",
        "含 replay.json 的回放文件夹",
    ),
    ("Open folder", "打开文件夹"),
    ("Render / open", "渲染 / 打开"),
    ("Open independent player", "打开独立播放器"),
    ("Import into new project", "导入到新工程"),
    ("Import into source project", "导入到来源工程"),
    (
        "Creates a new snapshot revision in the source project; previous revisions remain. Opens the resulting project.",
        "在来源工程中创建新快照并打开；保留旧快照版本。",
    ),
    ("Replay player", "回放播放器"),
    ("Pause", "暂停"),
    ("Close player", "关闭播放器"),
    (
        "Live input is muted while this player is open.",
        "播放器打开时，现场输入静音。",
    ),
    ("RC505 RS · Guide", "RC505 RS · 使用指南"),
    ("Getting started", "快速上手"),
    ("Signal flow", "信号流"),
    ("Keyboard", "快捷键"),
    ("Saving & replay", "保存与回放"),
    ("Latency", "延迟"),
    ("Updates", "更新"),
    ("Open data folder", "打开数据目录"),
    ("Open download folder", "打开下载目录"),
    ("Check for updates", "检查更新"),
    ("Download and verify", "下载并校验"),
    (
        "Save snapshot, close and install update",
        "保存快照，退出并安装更新",
    ),
    ("Release downloads and change log", "版本下载与更新记录"),
    ("AUDIO SETUP", "音频设置"),
    ("Prepare your session", "准备演奏"),
    ("Input device", "输入设备"),
    ("Output device", "输出设备"),
    (
        "Measure compensation inside RC505 RS → Audio. Project selection and data management live in the main application.",
        "在主程序的「音频」面板测量延迟补偿。工程选择和数据管理也在主程序中。",
    ),
    ("Open RC505 RS", "打开 RC505 RS"),
    ("Open offline editor", "打开离线编辑器"),
    ("Offline editing", "离线编辑"),
    ("Project ready.", "工程已就绪。"),
    ("Already up to date.", "已是最新版本。"),
    (
        "Configuration and audio snapshot saved.",
        "配置与音频快照已保存。",
    ),
    (
        "Configuration saved; the previous audio snapshot is retained.",
        "配置已保存；保留上次音频快照。",
    ),
    (
        "Finish and save or discard the replay take first.",
        "请先结束回放录制，并保存或处理草稿。",
    ),
    (
        "Replay captured. Save the take or export audio.",
        "回放录制完成，可以保存或导出音频。",
    ),
    (
        "Loopback measured. Review and apply the recommendation in Audio.",
        "回环测量完成，请在音频面板查看并应用建议。",
    ),
    (
        "Another editor owns this data folder; saving is disabled.",
        "另一个编辑器正在使用此数据目录，当前禁止保存。",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_preferences_default_to_chinese_and_language_round_trips() {
        let mut old: super::super::launcher_config::LauncherConfig = serde_json::from_str(
            r#"{"input_device":"device A","output_device":"device B","latency_comp_ms":85}"#,
        )
        .unwrap();
        assert_eq!(old.language, Language::Chinese);
        old.language = Language::English;
        let restored: super::super::launcher_config::LauncherConfig =
            serde_json::from_str(&serde_json::to_string(&old).unwrap()).unwrap();
        assert_eq!(restored.language, Language::English);
        assert_eq!(restored.input_device, "device A");
        assert_eq!(Language::English.text("清空音频"), "Clear audio");
        assert_eq!(Language::Chinese.text("user preset"), "user preset");
    }
}
