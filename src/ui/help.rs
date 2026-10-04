use super::theme;
use crate::app::MyApp;
use eframe::egui;
pub fn draw(ctx: &egui::Context, app: &mut MyApp) {
    let lang = crate::app_support::language::Language::current(ctx);
    if !app.help_open {
        return;
    }
    let mut open = true;
    egui::Window::new(lang.text("RC505 RS · Guide")).open(&mut open).default_size([820.0,590.0]).resizable(true).show(ctx,|ui|{
        ui.horizontal_wrapped(|ui|{for (index,label) in [lang.text("Getting started"),lang.text("Signal flow"),lang.text("Keyboard"),lang.text("Saving & replay"),lang.text("Latency"),lang.text("Updates")].into_iter().enumerate(){ui.selectable_value(&mut app.help_tab,index,label);}});
        ui.separator();egui::ScrollArea::vertical().show(ui,|ui|{match app.help_tab{
            0=>{
                ui.heading(lang.choose("Start with a loop", "从一段循环开始"));
                ui.label(lang.choose("Key names below are defaults; Keyboard shows your active custom bindings.","下面的键名是默认值；「键盘」页显示自定义后的实际键位。"));
                for (en,zh) in [
                    ("Choose a project, then check devices in Audio. Projects always open stopped.","选择工程，在「音频」面板检查设备。工程总是从停止状态打开。"),
                    ("1–5: record, finish, play, overdub. F1–F5 or Shift+1–5 stop individual tracks.","1–5：录音、结束、播放、叠录。F1–F5 或 Shift+1–5 停止对应轨。"),
                    ("Click a track title, use Left/Right, or Ctrl+1–5 to select a track. Track controls edit reverse, one shot, stop behavior, quantization and length.","点击轨道标题、左右方向键或 Ctrl+1–5 选择轨道；左面板可调整倒放、单次播放、停止方式、量化与长度。"),
                    ("Metronome starts performance; volume is beside it. Audition uses a private clock. Both are monitor-only, excluded from tracks and exported replay audio.","节拍器启动正式演奏，旁边调节音量。独立试听使用私有时钟；二者只进入监听，不录入轨道或回放导出音频。"),
                    ("Select an FX slot for quick editing. Expand opens the full preset editor, pinned to that bank and slot.","选择效果槽快速调整；展开后可完整编辑预设。编辑目标固定为该效果组与槽位。"),
                    ("F6: top bar; F7: left panel; F8: FX. Tab selects a control; Left/Right fine-tunes, Up/Down takes larger steps, Enter types a value.","F6 顶部栏，F7 左面板，F8 效果器。Tab 换参数；左右微调，上下快调，Enter 输入数值。"),
                    ("Esc leaves an editor or panel first. From performance, Esc returns to the project browser after a save prompt. Alt+F4 closes the application.","Esc 先离开编辑器或面板；在演奏界面再按 Esc，会提示保存并返回工程选择页。Alt+F4 关闭软件。"),
                    ("Hold Delete for 0.75 seconds or press it twice within 350 ms to clear selected-track audio (recoverable with track Undo). A single short press does nothing. Changing tracks or focus cancels the gesture.","长按 Delete 0.75 秒，或在 350 毫秒内双击，清空所选轨音频，可用轨道撤销恢复。单次短按不删除；切换轨道或焦点会取消手势。"),
                    ("Theme selects Mint, Rose or Ember across the application. Audio can disable the background FFT spectrum. The icon stays unchanged.","顶部主题切换薄荷绿、雾粉、橙红，全软件统一，图标不变。音频页可关闭后台 FFT 频谱。"),
                    ("Grey keycaps show keyboard shortcuts. Icons and adjacent text describe actions. 中文 / EN changes the interface language and remembers your choice.","灰底键帽表示快捷键，图标与旁边文字表示动作。顶部 中文 / EN 可切换界面语言并记住选择。"),
                ] { ui.label(lang.choose(en,zh)); ui.add_space(10.0); }
            }
            1=>{
                ui.heading(lang.choose("Where the sound goes", "声音从哪里来，到哪里去"));
                theme::card().show(ui,|ui| {
                    ui.colored_label(theme::accent(ui),lang.choose("Audio input → Input FX → recording / overdub + monitoring", "声卡输入 → 输入效果 → 录音 / 叠录，同时监听"));
                    ui.add_space(15.0);
                    ui.colored_label(theme::secondary(ui),lang.choose("Recorded loops → Track FX → track faders → master output", "循环音频 → 轨道效果 → 各轨推子 → 总混音 → 声卡输出"));
                });
                for (en,zh) in [
                    ("Vocoder track carriers are taken after Track FX and before faders. Lowering a fader does not silence its carrier or erase recorded audio.","声码器轨道载波取自轨道效果之后、推子之前。推低音量不会关闭载波，也不会抹掉录音。"),
                    ("Session selects Input FX order. Legacy groups preserve older projects: mix Oscillator and input, add MyDelay, then Vocoder, Filter and Reverb. Slot A → D processes in slot order.","工程面板可选择输入效果顺序。旧版分组保留旧工程：振荡器与输入混合，加入 MyDelay，再经过声码器、滤波器和混响。槽位 A → D 按槽位顺序处理。"),
                    ("Silent input turns Input Thru OFF: all input processing and recording continue, while only the direct input-to-output branch is muted. Existing loops play normally; finish recording behaves as usual.","静默录入关闭 Input Thru：输入处理与录音正常进行，只切断输入直接到输出的分支。已有循环正常播放，结束录音仍遵循原录放逻辑。"),
                    ("Faders change playback volume, not recording level. Output above 0 dBFS clips; Session shows peaks and clipped-frame counts.","推子调整播放音量，不影响录入音量。输出超过 0 dBFS 会削波；工程面板显示峰值与削波帧数。"),
                ] { ui.label(lang.choose(en,zh)); ui.add_space(10.0); }
            }
            2=>{
                ui.heading(lang.choose("Performance shortcuts", "演奏快捷键"));
                if ui.button(lang.choose("Customize shortcuts", "自定义快捷键")).clicked() {app.shortcut_editor.open(&app.shortcuts);}
                ui.label(lang.choose("Current bindings (not a fixed default list):", "以下显示当前实际键位："));
                egui::Grid::new("help-keyboard").striped(true).spacing([24.0,8.0]).show(ui,|ui| {
                    for d in app.shortcuts.entries() {
                        let keys=app.shortcuts.label(d.command);
                        ui.monospace(if keys.is_empty(){lang.choose("Unbound","未绑定")}else{&keys});
                        ui.label(lang.choose(&d.en,&d.zh));ui.end_row();
                    }
                    for (key,en,zh) in [
                        ("Ctrl+Tab / Ctrl+Shift+Tab","Next / previous expanded editor page","完整编辑器下一页 / 上一页"),
                        ("Tab / Shift+Tab","Next / previous control","下一个 / 上一个控件"),
                        ("← → / ↑ ↓","Fine / coarse parameter adjustment","参数微调 / 快调"),
                        ("Enter","Activate or type a number","操作按钮或输入数值"),
                        ("Esc","Editor / panel → performance → projects","编辑器 / 面板 → 演奏 → 工程选择"),
                        ("Alt+F4","Close application (save prompt)","关闭软件（提示保存）"),
                    ] {ui.monospace(key);ui.label(lang.choose(en,zh));ui.end_row();}
                });
                ui.add_space(12.0);
                ui.label(lang.choose("Fader taps move 0.5 dB. Holding 180 ms starts a ramp to the track's speed over 450 ms. Opposite keys cancel. Audio smoothing is 5 ms. Speed is saved per track (1–60 dB/s). Hardware keyboard rollover still applies.","推子短按变化 0.5 dB；按住 180 ms 后开始连续变化，在 450 ms 内加速到设定速度。反向键抵消，声音端平滑 5 ms。速度逐轨保存（1–60 dB/s），多键冲突仍取决于键盘硬件。"));
                ui.label(lang.choose("Recording, FX and faders remain active in the FX editor. Arrows, Delete and editing shortcuts stay local. Text entry, menus, help and the replay player pause performance keys.","编辑 FX 时仍可录放、切效果和推推子；方向键、删除和编辑组合键留给当前编辑区。输入文字、打开菜单、帮助或回放播放器时暂停演奏键。"));
            }
            3=>{
                ui.heading(lang.text("Saving & replay"));
                for (en,zh) in [
                    ("Configuration saves parameters, sequences, track modes and fader speeds while keeping the last audio snapshot reference.","保存配置会保存参数、序列、轨道模式和推子速度，保留上一次音频快照引用。"),
                    ("Configuration + audio saves five tracks as lossless 32-bit float WAV, undo state and a versioned manifest. The project pointer changes only after background writes complete; older revisions remain.","配置与音频快照保存五轨无损 32-bit float WAV、撤销状态和版本化清单。后台写入完成后才更新工程指针，旧版本保留。"),
                    ("Start replay capture with all tracks stopped. Existing loops remain; FX tails and the clock reset. Capture records dry input and accepted commands at exact engine sample positions, not the master output.","五轨停止时开始回放录制。保留已有循环，重置效果尾音与时钟；录制原始输入及引擎实际接受操作的采样位置，不是直接录制主输出。"),
                    ("After capture, name and save the take, export a WAV, keep a draft or discard it into replay trash. WAV export uses the same DSP renderer. Takes are limited to 30 minutes; each track to 5 minutes.","结束后可命名保存回放、导出 WAV、保留草稿或丢弃到回放回收站。WAV 由同一 DSP 渲染器计算；单次回放限 30 分钟，单轨音频限 5 分钟。"),
                    ("The project screen and Replays / F10 open one global library. Playback calculates audio live and opens a temporary performance panel. Space pauses, the slider seeks by reconstructing DSP history, and Esc returns. Playback never creates a full WAV; Export is explicit and exported files have a Delete button.","工程选择页和回放库 / F10 打开同一个全局回放库。播放实时演算并打开临时演奏面板；空格暂停，拖动进度重建所选位置，Esc 返回。播放不生成整段 WAV，只有主动导出才生成，并可在导出列表删除。"),
                    ("Pause anywhere and choose Import this position. Any existing or new project can receive its five tracks and settings after confirmation. Import changes the working copy; save config + audio snapshot to keep it, or Discard to restore the previously saved project.","任意位置暂停后点「导入当前时刻」，确认后可导入任意已有工程或新工程。导入只改变工作区；保存配置与音频快照才保留，离开时放弃修改可恢复原已保存版本。"),
                    ("Older beta replays are opened with an algorithm-change notice and may sound different. Original files stay intact. Input gaps or queue overflow invalidate capture; resolve the take draft before leaving the project.","旧测试版回放会显示算法变更提示，重新演算的音色可能不同，原始文件保留。输入缺失或队列溢出会使录制报错；离开工程前需要处理回放草稿。"),
                ] { ui.label(lang.choose(en,zh)); ui.add_space(10.0); }
            }
            4=>{
                ui.heading(lang.choose("Timing and latency", "精度与延迟"));
                for (en,zh) in [
                    ("Recording and quantization follow the output sample clock. Configuration changes apply at audio callback boundaries. Keyboard input still has OS and UI scheduling latency.","录放与量化跟随输出采样时钟，配置在音频回调边界应用。键盘输入仍有操作系统和界面调度延迟。"),
                    ("Start at 128 buffer frames and inspect gaps, overflow and peak callback time. Smaller buffers may reduce latency but increase dropouts. Drivers may ignore requested sizes.","可从 128 帧缓冲开始，观察缺帧、溢出与回调峰值。更小缓冲可能减少延迟，也更容易断音。驱动可能忽略请求值。"),
                    ("Clock adaptation limits drift between input and output devices; it cannot remove converter, driver or USB buffering latency.","输入时钟适配可限制输入输出设备间的漂移，无法消除转换器、驱动或 USB 缓冲延迟。"),
                    ("Open Audio / calibration, stop performance and prepare MUTED monitoring before connecting line output L to line input L. Disable hardware direct monitoring. After measurement, disconnect the cable before restoring monitoring. Failure and restart stay muted.","打开音频 / 校准，停止演奏并先静音，再接线路左输出到线路左输入；关闭硬件直通。测后先拔线，再确认恢复监听。失败及重启保持静音；内置麦克风/蓝牙耳机无法完成此线路测试。"),
                    ("Compensation aligns recordings; it does not reduce audible monitoring delay. Recalibrate after changing devices, sample rate or buffer size.","补偿调整录音对齐，不降低耳朵听到的监听延迟。更换设备、采样率或缓冲后需重新测量。"),
                ] { ui.label(lang.choose(en,zh)); ui.add_space(10.0); }
            }
            _=>{
                ui.heading(format!("RC505 RS {}",env!("CARGO_PKG_VERSION")));
                ui.label(format!("Data: {}",crate::app_support::paths::appdata_root().unwrap_or_default().display()));
                ui.label(format!("Downloads: {}",crate::app_support::paths::downloads_dir().display()));
                ui.horizontal(|ui|{
                    if ui.button(lang.text("Open data folder")).clicked(){if let Some(path)=crate::app_support::paths::appdata_root(){if let Err(error)=std::process::Command::new("explorer.exe").arg(path).spawn(){app.status=error.to_string();}}}
                    if ui.button(lang.text("Open download folder")).clicked(){if let Err(error)=std::process::Command::new("explorer.exe").arg(crate::app_support::paths::downloads_dir()).spawn(){app.status=error.to_string();}}
                });
                ui.add_space(12.0);
                if ui.add_enabled(!app.busy(),egui::Button::new(lang.text("Check for updates"))).clicked(){app.check_update();}
                if let Some(release)=&app.update {
                    ui.label(format!("Latest release: {}",release.version));
                    if crate::updater::newer(&release.version)&&ui.add_enabled(!app.busy(),egui::Button::new(lang.text("Download and verify"))).clicked(){app.download_update();}
                }
                if app.update_installer.is_some() {
                    if ui.add_enabled(!app.busy()&&app.stopped()&&!app.taking()&&app.draft.is_none(),egui::Button::new(lang.text("Save snapshot, close and install update"))).clicked(){app.install_update();}
                }
                ui.label(lang.text(&app.status));
                ui.separator();
                ui.label(lang.choose("Updates save the current snapshot before closing and replacing the program. Data stays in your chosen directory. After success, one verified installer is retained in the download directory. Older applications may not read newer project formats.","更新前先保存当前快照，正常退出后替换程序。数据保留在指定目录；成功后下载目录仅保留一份已校验安装包。旧软件可能无法读取新版工程格式。"));
                ui.hyperlink_to(lang.text("Release downloads and change log"),"https://github.com/Yishanka/RC505_RS/releases");
            }
        }});
    });
    if !open {
        app.help_open = false;
    }
}
