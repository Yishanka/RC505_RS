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
                for (en,zh) in [
                    ("Choose a project, then check devices in Audio. Projects always open stopped.","选择工程，在「音频」面板检查设备。工程总是从停止状态打开。"),
                    ("1–5: record, finish, play, overdub. F1–F5 or Shift+1–5 stop individual tracks.","1–5：录音、结束、播放、叠录。F1–F5 或 Shift+1–5 停止对应轨。"),
                    ("Click a track title, use Left/Right, or Ctrl+1–5 to select a track. Track controls edit reverse, one shot, stop behavior, quantization and length.","点击轨道标题、左右方向键或 Ctrl+1–5 选择轨道；左面板可调整倒放、单次播放、停止方式、量化与长度。"),
                    ("Select an FX slot for quick editing. Expand opens the full preset editor, pinned to that bank and slot.","选择效果槽快速调整；展开后可完整编辑预设。编辑目标固定为该效果组与槽位。"),
                    ("F6 focuses the top bar; F7 the left panel; F8 the right panel. Tab or Up/Down chooses a control; Left/Right adjusts it; Enter activates it.","F6 进入顶部栏，F7 进入左面板，F8 进入右面板。Tab 或上下键选择控件，左右键调整，Enter 操作。"),
                    ("Esc leaves an editor or panel first. From performance, Esc returns to the project browser after a save prompt. Alt+F4 closes the application.","Esc 先离开编辑器或面板；在演奏界面再按 Esc，会提示保存并返回工程选择页。Alt+F4 关闭软件。"),
                    ("Hold Delete for 0.75 seconds or press it twice within 350 ms to clear selected-track audio and undo history. A single short press does nothing. Changing tracks or focus cancels the gesture.","长按 Delete 0.75 秒，或在 350 毫秒内双击，清空所选轨音频及撤销历史。单次短按不删除；切换轨道或焦点会取消手势。"),
                    ("Grey keycaps show keyboard shortcuts. Icons and adjacent text describe actions. 中文 / EN changes the interface language and remembers your choice.","灰底键帽表示快捷键，图标与旁边文字表示动作。顶部 中文 / EN 可切换界面语言并记住选择。"),
                ] { ui.label(lang.choose(en,zh)); ui.add_space(10.0); }
            }
            1=>{
                ui.heading(lang.choose("Where the sound goes", "声音从哪里来，到哪里去"));
                theme::card().show(ui,|ui| {
                    ui.colored_label(theme::ACCENT,lang.choose("Audio input → Input FX → recording / overdub + monitoring", "声卡输入 → 输入效果 → 录音 / 叠录，同时监听"));
                    ui.add_space(15.0);
                    ui.colored_label(theme::TRACK,lang.choose("Recorded loops → Track FX → track faders → master output", "循环音频 → 轨道效果 → 各轨推子 → 总混音 → 声卡输出"));
                });
                for (en,zh) in [
                    ("Vocoder track carriers are taken after Track FX and before faders. Lowering a fader does not silence its carrier or erase recorded audio.","声码器轨道载波取自轨道效果之后、推子之前。推低音量不会关闭载波，也不会抹掉录音。"),
                    ("Session selects Input FX order. Legacy groups preserve older projects: mix Oscillator and input, add MyDelay, then Vocoder, Filter and Reverb. Slot A → D processes in slot order.","工程面板可选择输入效果顺序。旧版分组保留旧工程：振荡器与输入混合，加入 MyDelay，再经过声码器、滤波器和混响。槽位 A → D 按槽位顺序处理。"),
                    ("Faders change playback volume, not recording level. Output above 0 dBFS clips; Session shows peaks and clipped-frame counts.","推子调整播放音量，不影响录入音量。输出超过 0 dBFS 会削波；工程面板显示峰值与削波帧数。"),
                ] { ui.label(lang.choose(en,zh)); ui.add_space(10.0); }
            }
            2=>{
                ui.heading(lang.choose("Performance shortcuts", "演奏快捷键"));
                egui::Grid::new("help-keyboard").striped(true).spacing([24.0,12.0]).show(ui,|ui| {
                    for (key,en,zh) in [
                        ("1–5","Record / play / overdub / finish; retrigger one shot","录音 / 播放 / 叠录 / 结束；单次播放时重新触发"),
                        ("F1–F5 / Shift+1–5","Stop track; press again to stop immediately while queued","停止对应轨；等待停止时再按立即停止"),
                        ("Space / Shift+Space","Start or stop all / immediately stop all","全部启停 / 立即停止全部"),
                        ("← → / Ctrl+1–5","Select a track","选择轨道"),
                        ("Alt+1–5","Track overdub undo / redo","对应轨叠录撤销 / 重做"),
                        ("Ctrl+Z / Ctrl+Y","Selected track undo / redo; note history inside piano roll","所选轨撤销 / 重做；钢琴卷帘中为音符撤销 / 重做"),
                        ("Delete","Clear selected track: hold 0.75 s OR double-press within 350 ms","清空所选轨：长按 0.75 秒，或 350 毫秒内双击"),
                        ("Q W E R / U I O P","Toggle input / selected track FX slots","切换输入 / 所选轨道效果槽"),
                        ("Shift+FX key","Hold for temporary effect; release restores previous state","按住临时开启效果，松开恢复原状态"),
                        ("Alt+FX / Ctrl+FX","Select bank / select slot and focus editor","选择效果组 / 选中效果槽并进入编辑面板"),
                        ("Z X · C V · B N · M , · . /","Track faders down / up; independent simultaneous control","五轨推子下 / 上；可同时独立操作"),
                        ("Shift+fader keys","Adjust that track's fader speed","调节该轨推子的变化速度"),
                        ("F6 · F7 · F8","Top bar · left panel · right panel","顶部栏 · 左面板 · 右面板"),
                        ("Ctrl+Tab / Ctrl+Shift+Tab","Next / previous expanded editor page","完整编辑器下一页 / 上一页"),
                        ("Tab / Shift+Tab / ↑ ↓","Move between controls in the focused panel","在当前面板中选择控件"),
                        ("Esc","Editor / panel → performance → project browser (save prompt)","编辑器 / 面板 → 演奏界面 → 工程选择页（提示保存）"),
                        ("T","Tap tempo while stopped","停止时击拍定速"),
                        ("Ctrl+S / Ctrl+Shift+S","Save configuration / configuration and audio snapshot","保存配置 / 配置与音频快照"),
                        ("F9 / F12","Start or end replay capture / help","开始或结束回放录制 / 帮助"),
                        ("Alt+F4","Close application (save prompt)","关闭软件（提示保存）"),
                    ] { theme::keycap(ui,key); ui.label(lang.choose(en,zh)); ui.end_row(); }
                });
                ui.add_space(12.0);
                ui.label(lang.choose("Fader taps move 0.5 dB. Holding 180 ms starts a ramp to the track's speed over 450 ms. Opposite keys cancel. Audio smoothing is 5 ms. Speed is saved per track (1–60 dB/s). Hardware keyboard rollover still applies.","推子短按变化 0.5 dB；按住 180 ms 后开始连续变化，在 450 ms 内加速到设定速度。反向键抵消，声音端平滑 5 ms。速度逐轨保存（1–60 dB/s），多键冲突仍取决于键盘硬件。"));
                ui.label(lang.choose("Performance shortcuts are suspended during text entry, full editing, help, playback and loss of focus. Delete in the piano roll removes a note immediately; it never clears track audio.","文本输入、完整编辑器、帮助、回放播放器及窗口失焦时暂停演奏键。钢琴卷帘中的 Delete 立即删除音符，不会清空轨道音频。"));
            }
            3=>{
                ui.heading(lang.text("Saving & replay"));
                for (en,zh) in [
                    ("Configuration saves parameters, sequences, track modes and fader speeds while keeping the last audio snapshot reference.","保存配置会保存参数、序列、轨道模式和推子速度，保留上一次音频快照引用。"),
                    ("Configuration + audio saves five tracks as lossless 32-bit float WAV, undo state and a versioned manifest. The project pointer changes only after background writes complete; older revisions remain.","配置与音频快照保存五轨无损 32-bit float WAV、撤销状态和版本化清单。后台写入完成后才更新工程指针，旧版本保留。"),
                    ("Start replay capture with all tracks stopped. Existing loops remain; FX tails and the clock reset. Capture records dry input and accepted commands at exact engine sample positions, not the master output.","五轨停止时开始回放录制。保留已有循环，重置效果尾音与时钟；录制原始输入及引擎实际接受操作的采样位置，不是直接录制主输出。"),
                    ("After capture, name and save the take, export a WAV, or keep a draft. WAV export uses the same DSP renderer. Takes are limited to 30 minutes; each track to 5 minutes.","结束后可命名保存回放、导出 WAV 或保留草稿。WAV 由同一 DSP 渲染器计算；单次回放限 30 分钟，单轨音频限 5 分钟。"),
                    ("Replay library verifies SHA-256 and opens a dedicated player. Space pauses or resumes; live input is muted. Import the final state into the source project or a new project only. Previous snapshot revisions remain.","回放库校验 SHA-256 并提供独立播放器。空格暂停 / 继续，现场输入静音。最终状态仅可导入来源工程或新工程；保留旧快照版本。"),
                    ("Replay files require the matching renderer version and source sample rate. Input gaps or queue overflow invalidate a capture. Resolve the replay draft before leaving the project.","回放要求匹配渲染器版本与原采样率。输入缺失或队列溢出会使录制报错；离开工程前需要处理回放草稿。"),
                ] { ui.label(lang.choose(en,zh)); ui.add_space(10.0); }
            }
            4=>{
                ui.heading(lang.choose("Timing and latency", "精度与延迟"));
                for (en,zh) in [
                    ("Recording and quantization follow the output sample clock. Configuration changes apply at audio callback boundaries. Keyboard input still has OS and UI scheduling latency.","录放与量化跟随输出采样时钟，配置在音频回调边界应用。键盘输入仍有操作系统和界面调度延迟。"),
                    ("Start at 128 buffer frames and inspect gaps, overflow and peak callback time. Smaller buffers may reduce latency but increase dropouts. Drivers may ignore requested sizes.","可从 128 帧缓冲开始，观察缺帧、溢出与回调峰值。更小缓冲可能减少延迟，也更容易断音。驱动可能忽略请求值。"),
                    ("Clock adaptation limits drift between input and output devices; it cannot remove converter, driver or USB buffering latency.","输入时钟适配可限制输入输出设备间的漂移，无法消除转换器、驱动或 USB 缓冲延迟。"),
                    ("For a recommendation, connect output L to input L, disconnect speakers, stop all tracks and choose Measure loopback. All three probes must pass correlation and consistency checks. Compensation is stored in integer samples.","获得补偿建议需要将左输出连接左输入、断开扬声器、停止所有轨道，再点击测量回环延迟。三次探测的相关性与一致性均通过后才允许应用，补偿以整数采样保存。"),
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
