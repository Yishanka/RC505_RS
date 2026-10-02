use crate::app::MyApp;
use eframe::egui;
use std::sync::atomic::Ordering;
pub fn draw(ctx: &egui::Context, app: &mut MyApp) {
    if !app.calibration_open {
        return;
    }
    let lang = app.language;
    let mut open = true;
    egui::Window::new(lang.choose("Loopback calibration","回环延迟校准")).id(egui::Id::new("calibration-wizard"))
        .open(&mut open).default_width(610.0).anchor(egui::Align2::CENTER_CENTER,egui::Vec2::ZERO).collapsible(false).resizable(false).show(ctx,|ui| {
            ui.label(lang.choose("This measures electrical output → input. Software cannot verify your physical sockets or cabling.","这里测量线路输出到线路输入的电气回环。软件无法自动确认你是否有线路接口或已正确接线。"));
            ui.label(format!("{}: {}",lang.text("Input device"),app.audio.curr_input_name()));
            ui.label(format!("{}: {}",lang.text("Output device"),app.audio.curr_output_name()));
            ui.separator();
            let held=app.calibration_held();let measuring=app.audio.diagnostics.calibrating.load(Ordering::Relaxed);
            if !held {
                ui.strong(lang.choose("1. Mute monitoring before connecting a cable","1. 先静音，再接线"));
                ui.label(lang.choose("Use line-level output and input. A microphone array or headphones held near a mic are not this measurement path. Disable hardware direct monitoring.","需要线路输出和线路输入。内置麦克风阵列、将耳机靠近麦克风都不是这里的测量路径。请关闭声卡硬件直通监听。"));
                let reason=app.calibration_reason();
                ui.colored_label(if reason.is_none(){super::theme::accent(ui)}else{egui::Color32::YELLOW},lang.choose(if reason.is_none(){"Software ready; confirm physical loopback next."}else{"Not ready to test:"},if reason.is_none(){"软件条件已满足，仍需确认实际回环接线。"}else{"暂不能测试："}));
                if let Some(reason)=reason {ui.label(lang.text(reason));}
                if ui.add_enabled(reason.is_none(),egui::Button::new(lang.choose("Prepare test — mute monitoring","准备测试：静音监听"))).clicked(){app.prepare_calibration();}
            } else {
                ui.colored_label(egui::Color32::YELLOW,lang.choose("MONITORING MUTED — awaiting disconnection confirmation","监听已静音——只有确认拔线后才恢复"));
                ui.strong(lang.choose("2. Connect line output L → line input L","2. 连接线路左输出 → 线路左输入"));
                ui.add_enabled_ui(!measuring,|ui|{ui.checkbox(&mut app.loopback_connected,lang.choose("Loopback connected; hardware direct monitoring OFF","已连接回环线，并已关闭硬件直通监听"));});
                if measuring {ui.spinner();ui.label(lang.choose("Measuring three probes (about 3 seconds)…","正在测量三次探测信号（约 3 秒）…"));}
                else {
                    if ui.add_enabled(app.loopback_connected && app.calibration_reason().is_none(),egui::Button::new(lang.choose("Measure / retry","开始测量 / 重试"))).clicked(){app.calibrate();}
                    if let Some(value)=app.measurement {
                        ui.label(format!("{}: {:.3} ms · {} {}",lang.choose("Measured delay","测量延迟"),value.frames as f64*1000.0/value.sample_rate as f64,value.frames,lang.choose("samples","采样")));
                        if ui.button(lang.choose("Apply compensation","应用补偿")).clicked(){app.apply_measurement();}
                    }
                    ui.separator();ui.strong(lang.choose("3. Disconnect the cable, then restore monitoring","3. 拔掉回环线，再恢复监听"));
                    if ui.button(lang.choose("Cable disconnected — restore monitoring","已拔线，恢复监听")).clicked(){app.restore_monitoring();}
                }
                ui.label(lang.choose("Failure, device changes, closing this window and restarting the app do not automatically restore monitoring.","测量失败、设备变化、关闭本窗口或重启软件，都不会自动恢复监听。"));
            }
            ui.separator();ui.label(lang.text(&app.status));
            super::theme::caption(ui,lang.choose("Save configuration after applying. Compensation aligns recordings; monitoring latency remains.","应用后保存配置。补偿用于录音对齐，不会消除耳机监听延迟。"));
        });
    app.calibration_open = open;
}
