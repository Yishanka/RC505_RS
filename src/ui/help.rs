use super::theme;
use crate::app::MyApp;
use eframe::egui;
pub fn draw(ctx: &egui::Context, app: &mut MyApp) {
    if !app.help_open {
        return;
    }
    let mut open = true;
    egui::Window::new("RC505 RS · Guide").open(&mut open).default_size([820.0,590.0]).resizable(true).show(ctx,|ui|{
        ui.horizontal_wrapped(|ui|{for (index,label) in ["Getting started","Signal flow","Keyboard","Saving & replay","Latency","Updates"].into_iter().enumerate(){ui.selectable_value(&mut app.help_tab,index,label);}});
        ui.separator();egui::ScrollArea::vertical().show(ui,|ui|{match app.help_tab{
            0=>{
                ui.heading("从一段循环开始");
                for line in ["选择工程后，在左侧 Audio 中检查设备。工程总是从停止状态打开。", "1–5 开始录音，再按同一键结束录音；播放时再按进入 Overdub。Shift + 数字或 F1–F5 停止对应轨。", "鼠标点轨道标题，或 Ctrl + 1–5 选择轨道。左面板编辑此轨的 Reverse、One shot、Stop、量化及固定长度。", "点 FX 槽选择编辑目标；右面板做快速修改，Expand 打开完整编辑器。目标固定为 bank + slot，不会因切换演奏 bank 悄悄改变。", "A / F7 进入左面板，D / F8 进入右面板，F6 进入顶部栏。Tab / 上下选择控件，左右调节，Enter 操作；Esc 返回演奏。", "演奏区位置固定。小窗口可滚动工作区，两个上方面板内部滚动，不会因展开信息把轨道挤走。"]{ui.label(line);ui.add_space(10.0);}
            }
            1=>{
                ui.heading("声音从哪里来，到哪里去");
                ui.label("输入 = 麦克风 / 声卡录入的声音；loop = 已经录好的循环。它们在最后混合到声卡输出。");
                theme::card().show(ui,|ui|{
                    ui.colored_label(theme::ACCENT,"声卡输入 → Input FX → 录音 / 叠录，同时监听");
                    ui.add_space(15.0);ui.colored_label(theme::TRACK,"循环音频 → Track FX → 各轨推子 → 总混音 → 声卡输出");
                    ui.label("Vocoder 的轨道载波来自 Track FX 之后、推子之前。因此推低音量不等于关闭载波。");
                });
                ui.label("Session 中可选择 Input FX 顺序。Legacy groups 保留旧工程：Oscillator 与输入混合，加入 MyDelay，再经过 Vocoder、Filter、Reverb。Slot A → D 则按槽位顺序处理；Oscillator / MyDelay 在所在槽位加入声音。");
                ui.label("推子在录好的 loop 之后：拉低推子不会抹掉录音，也不改变录入的音量。输出超过 0 dBFS 会削波；Session 电平表显示峰值和削波次数。");
            }
            2=>{
                ui.heading("演奏快捷键");
                egui::Grid::new("help-keyboard").striped(true).spacing([24.0,12.0]).show(ui,|ui|{
                    for (key,action) in [
                        ("1–5","录音 / 播放 / Overdub / 结束；One shot 时重触发"),
                        ("Shift + 1–5 / F1–F5","停止对应轨；等待结束时再按立即停止"),
                        ("Space / Shift + Space","全部开始或停止 / 立即停止全部"),
                        ("Ctrl + 1–5 / ← →","选择轨道"),
                        ("Alt + 1–5","对应轨 Overdub Undo / Redo"),
                        ("Ctrl + Z / Ctrl + Y","所选轨 Undo / Redo；编辑器中为音符撤销"),
                        ("Ctrl + Delete，按住 0.75 秒","清空所选轨；误触短按不会清空"),
                        ("Q W E R / U I O P","切换 Input FX / 所选 Track FX 的四个槽"),
                        ("Shift + FX 键，按住","临时开启效果，松键或失焦恢复原状态"),
                        ("Alt + FX 键 / Ctrl + FX 键","直接选 bank / 选中槽并进入右面板"),
                        ("Z X · C V · B N · M , · . /","五轨推子下 / 上；可同时独立操作"),
                        ("Shift + 每轨推子键","降低 / 提高该轨变化速度，不改变当前音量"),
                        ("A / F7 · D / F8 · F6","左面板 · 右面板 · 顶部栏"),
                        ("Tab / Shift+Tab / ↑ ↓","在当前面板内选择控件"),
                        ("Esc","退出完整编辑器 / 返回演奏焦点"),
                        ("T","停止时 Tap tempo"),
                        ("Ctrl+S / Ctrl+Shift+S","仅保存 config / config + 音频快照"),
                        ("F9 / F12","开始、结束回放录制 / 内置帮助"),
                    ]{ui.monospace(key);ui.label(action);ui.end_row();}
                });
                ui.add_space(12.0);ui.label("推子：短按 0.5 dB；按住 180 ms 后开始连续变化，在 450 ms 内加速至设定速度。反向键抵消；松开即停，声音端有 5 ms 平滑。速度 1–60 dB/s，逐轨保存。键盘自身的多键冲突仍取决于硬件。");
                ui.label("文本输入、完整编辑器、帮助、失焦时不会触发演奏键。组合键不依赖系统自动重复。Shift 同时用于推子速度和临时 FX，可在同一只手上组合，但会影响当前所有被按住的推子键。");
            }
            3=>{
                ui.heading("保存与回放");
                for line in ["保存 config：保存参数、序列、轨道模式和推子速度；保留上一次音频 snapshot 引用。", "保存 config 与音频 snapshot：保存五轨无损 32-bit float WAV、Undo 状态和版本化清单。后台写入完成后才更新工程指针；旧版本继续保留。", "回放录制从五轨停止时开始。保留已有 loop，重置 FX 尾音和时钟，然后记录原始输入与引擎实际接受的操作采样编号。录制期间可正常录放、叠录、切换效果及修改参数。", "结束后可以命名保存回放，或导出 WAV；导出用相同 DSP 重新计算，不是录下主输出。单次回放限 30 分钟，单轨音频限 5 分钟。", "Replay library 可打开回放文件夹并验证 SHA-256。渲染后的独立播放器只播放回放，Space 暂停 / 继续，结束后可重新播放，不混入现场输入。", "导入把回放结束时的五轨与最终 config 保存为新的快照版本，仅允许原工程或新建工程。导入后停止播放；回放原件不变。新工程是避免替换当前工作的默认选择。", "回放绑定 renderer 版本及原采样率。不同版本不会静默猜测兼容；不同输出采样率只转换最终播放音频。快照换采样率在后台重采样。", "断音或录制队列溢出会报错，不把缺失输入的回放标成有效。窗口退出、切换工程前需先处理回放草稿。完整草稿会出现在历史中以便恢复。"]{ui.label(line);ui.add_space(10.0);}
            }
            4=>{
                ui.heading("精度与延迟");
                ui.label("录放和量化使用声卡输出的采样时钟，UI 只显示状态。配置在回调边界应用，操作记录引擎实际采用的采样位置；键盘本身仍有操作系统和 UI 调度延迟。");
                ui.label("减小 buffer 可能降低延迟，也更容易断音。先试 128 frames，再根据 Audio 的 input gaps、overflow、peak callback 检查。驱动可能忽略请求值，实际 callback frames 才是运行时数据。");
                ui.label("输入与输出若不是同一硬件时钟，输入适配器会缓慢调节采样比率，避免循环漂移；这不会消除声卡 ADC / DAC、驱动或 USB 缓冲延迟。");
                ui.label("自动建议需要实际回环测量：将输出 L 连接到输入 L，关闭扬声器，停止全部轨道，点击 Measure loopback。三次低电平伪随机测试的相关性与延迟差都合格才允许应用。补偿以整数采样保存，界面显示毫秒只是换算。");
                ui.label("补偿是调整录音对齐，不能降低耳朵听到的监听延迟。换设备、采样率、buffer 后重新校准；不要把旧设备的补偿当成通用常数。");
            }
            _=>{
                ui.heading(format!("RC505 RS {}",env!("CARGO_PKG_VERSION")));
                ui.label(format!("Data: {}",crate::app_support::paths::appdata_root().unwrap_or_default().display()));
                ui.label(format!("Downloads: {}",crate::app_support::paths::downloads_dir().display()));
                ui.horizontal(|ui|{
                    if ui.button("Open data folder").clicked(){if let Some(path)=crate::app_support::paths::appdata_root(){if let Err(error)=std::process::Command::new("explorer.exe").arg(path).spawn(){app.status=error.to_string();}}}
                    if ui.button("Open download folder").clicked(){if let Err(error)=std::process::Command::new("explorer.exe").arg(crate::app_support::paths::downloads_dir()).spawn(){app.status=error.to_string();}}
                });
                ui.add_space(12.0);
                if ui.add_enabled(!app.busy(),egui::Button::new("Check for updates")).clicked(){app.check_update();}
                if let Some(release)=&app.update {
                    ui.label(format!("Latest release: {}",release.version));
                    if crate::updater::newer(&release.version)&&ui.add_enabled(!app.busy(),egui::Button::new("Download and verify")).clicked(){app.download_update();}
                }
                if app.update_installer.is_some() {
                    if ui.add_enabled(!app.busy()&&app.stopped()&&!app.taking()&&app.draft.is_none(),egui::Button::new("Save snapshot, close and install update")).clicked(){app.install_update();}
                }
                ui.label(&app.status);
                ui.separator();
                ui.label("更新只替换程序。当前工程先保存 config 与音频 snapshot，程序正常退出后才运行安装器。下载包保存在你选择的目录，可手动重新安装或回退程序版本。新版工程格式可能无法由更旧的软件读取，旧快照不会自动删除。");
                ui.hyperlink_to("Release downloads and change log","https://github.com/Yishanka/RC505_RS/releases");
            }
        }});
    });
    if !open {
        app.help_open = false;
    }
}
