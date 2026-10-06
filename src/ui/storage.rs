//! Explicit storage management. Scans and mutations run on a worker.
use crate::{app::MyApp, project, storage};
use eframe::egui;
use std::{path::PathBuf, sync::mpsc};

#[derive(Clone)]
pub enum Action {
    Project(project::ProjectEntry),
    Replay(PathBuf),
    Discard(PathBuf),
    Export(PathBuf),
    Audio(project::ProjectEntry),
    Prune(project::ProjectEntry),
    DeleteItem(storage::Item),
    Restore(storage::Item),
    EmptyTrash(Vec<storage::Item>),
}
struct Completed {
    action: Option<Action>,
    result: Result<(), String>,
    inventory: Option<storage::Inventory>,
    projects: Vec<project::ProjectEntry>,
    replays: Vec<(PathBuf, String)>,
    exports: Vec<PathBuf>,
}
#[derive(Default)]
pub struct StorageUi {
    pub open: bool,
    pub confirm: Option<Action>,
    inventory: storage::Inventory,
    job: Option<mpsc::Receiver<Completed>>,
    pub message: String,
}
impl StorageUi {
    pub fn modal_open(&self) -> bool {
        self.open || self.confirm.is_some() || self.busy()
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn close(&mut self) {
        if !self.busy() {
            if self.confirm.is_some() {
                self.confirm = None;
            } else {
                self.open = false;
            }
        }
    }
    fn start(&mut self, action: Option<Action>, entries: Vec<project::ProjectEntry>) {
        if self.busy() {
            return;
        }
        let refresh = self.open;
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.message.clear();
        std::thread::spawn(move || {
            let result = action
                .as_ref()
                .map_or(Ok(()), |action| execute(action, &entries))
                .map_err(|e| format!("{e:#}"));
            let projects = project::load_index();
            let inventory = refresh.then(|| storage::inventory(&projects));
            let _ = tx.send(Completed {
                action,
                result,
                inventory,
                projects,
                replays: crate::replay::library::list(),
                exports: crate::replay::library::exports(),
            });
        });
    }
}
fn execute(action: &Action, entries: &[project::ProjectEntry]) -> anyhow::Result<()> {
    match action {
        Action::Project(entry) => {
            project::storage::delete(entry, entries)?;
        }
        Action::Replay(path) | Action::Discard(path) => crate::replay::library::delete(path)?,
        Action::Export(path) => crate::replay::library::delete_export(path)?,
        Action::Audio(entry) => project::storage::delete_audio(entry)?,
        Action::Prune(entry) => {
            project::storage::prune(entry)?;
        }
        Action::DeleteItem(item) => storage::delete_item(item)?,
        Action::Restore(item) => match item.kind {
            storage::ItemKind::ProjectTrash => {
                project::storage::restore_trash(&item.path)?;
            }
            storage::ItemKind::ReplayTrash => crate::replay::library::restore(&item.path)?,
            storage::ItemKind::IncompleteReplay => {
                anyhow::bail!("Incomplete recordings cannot be restored")
            }
        },
        Action::EmptyTrash(items) => {
            let mut errors = Vec::new();
            let mut removed = 0;
            for item in items {
                match storage::delete_item(item) {
                    Ok(()) => removed += 1,
                    Err(error) => errors.push(format!("{}: {error}", item.name)),
                }
            }
            anyhow::ensure!(
                errors.is_empty(),
                "Deleted {removed} items; some items remain. Refresh and retry: {}",
                errors.join("; ")
            );
        }
    }
    Ok(())
}
impl MyApp {
    pub fn open_storage(&mut self) {
        if self.busy()
            || self.taking()
            || self.player_open
            || self.app_state != crate::state::AppState::Init
            || self.active_project_idx.is_some()
        {
            return;
        }
        self.storage_ui.open = true;
        self.storage_ui.start(None, self.projects.clone());
    }
    pub fn request_storage_action(&mut self, action: Action) {
        if self.busy() || self.read_only || self.taking() || self.player_open {
            return;
        }
        if self.app_state != crate::state::AppState::Init
            && self.view.tracks.iter().any(|track| {
                matches!(
                    track.mode,
                    crate::engine::core::Mode::Recording | crate::engine::core::Mode::Overdub
                ) || track.pending
            })
        {
            self.status = self
                .language
                .choose(
                    "Stop recording before managing files.",
                    "请先结束录音，再管理文件。",
                )
                .into();
            return;
        }
        if matches!(
            action,
            Action::Project(_)
                | Action::Audio(_)
                | Action::Prune(_)
                | Action::Restore(_)
                | Action::EmptyTrash(_)
                | Action::DeleteItem(_)
        ) && (self.app_state != crate::state::AppState::Init
            || self.active_project_idx.is_some())
        {
            return;
        }
        self.storage_ui.confirm = Some(action);
        self.storage_ui.message.clear();
    }
}
fn size(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / 1048576.0)
}
#[cfg(debug_assertions)]
pub fn preview(app: &mut MyApp, mode: &str) {
    let root = crate::app_support::paths::appdata_root().unwrap();
    assert!(
        std::env::args().any(|a| a == "--offline")
            && root
                .components()
                .next()
                .is_some_and(|c| c.as_os_str() == "var"),
        "Storage preview needs isolated data"
    );
    app.app_state = crate::state::AppState::Init;
    app.active_project_idx = None;
    let entry = project::ProjectEntry {
        name: "Ambient bass session".into(),
        file: "preview-project.json".into(),
    };
    if mode.starts_with("delete-project") {
        app.storage_ui.confirm = Some(Action::Project(entry));
        return;
    }
    if mode.starts_with("discard-replay") {
        app.storage_ui.confirm = Some(Action::Discard(root.join("replays/draft-preview")));
        return;
    }
    app.storage_ui.open = true;
    app.storage_ui.inventory.projects = vec![storage::ProjectUsage {
        entry,
        bytes: 128 * 1048576,
        unused: 24 * 1048576,
    }];
    app.storage_ui.inventory.items = vec![
        storage::Item {
            path: root.join("projects/trash/preview-project"),
            name: "Melody sketch".into(),
            bytes: 18 * 1048576,
            kind: storage::ItemKind::ProjectTrash,
            restorable: true,
        },
        storage::Item {
            path: root.join("replays/trash/preview-replay"),
            name: "Live take 02".into(),
            bytes: 36 * 1048576,
            kind: storage::ItemKind::ReplayTrash,
            restorable: true,
        },
        storage::Item {
            path: root.join("replays/draft-preview"),
            name: "Unfinished recording".into(),
            bytes: 4 * 1048576,
            kind: storage::ItemKind::IncompleteReplay,
            restorable: false,
        },
    ];
}
pub fn draw(ctx: &egui::Context, app: &mut MyApp) {
    let completed = app
        .storage_ui
        .job
        .as_ref()
        .and_then(|job| match job.try_recv() {
            Ok(value) => Some(Ok(value)),
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(())),
            Err(mpsc::TryRecvError::Empty) => None,
        });
    if let Some(completed) = completed {
        app.storage_ui.job = None;
        match completed {
            Ok(done) => {
                app.projects = done.projects;
                app.sel_project_idx = app
                    .sel_project_idx
                    .min(app.projects.len().saturating_sub(1));
                app.replay_list = done.replays;
                app.replay_exports = done.exports;
                if let Some(inventory) = done.inventory {
                    app.storage_ui.inventory = inventory;
                }
                match done.result {
                    Ok(()) => {
                        if let Some(Action::Replay(path) | Action::Discard(path)) = &done.action {
                            if app.draft.as_ref() == Some(path) {
                                app.draft = None;
                            }
                            if app.rendered.as_ref().is_some_and(|(root, _)| root == path) {
                                app.rendered = None;
                            }
                        }
                        if let Some(Action::Export(path)) = &done.action {
                            if app.rendered.as_ref().is_some_and(|(_, r)| &r.wav == path) {
                                app.rendered = None;
                            }
                        }
                        if done.action.is_some() {
                            app.storage_ui.confirm = None;
                            app.storage_ui.message =
                                app.language.choose("Completed", "操作完成").into();
                            app.status = app.storage_ui.message.clone();
                        }
                    }
                    Err(error) => {
                        app.storage_ui.message = error;
                        app.status = app.storage_ui.message.clone();
                    }
                }
            }
            Err(()) => {
                app.storage_ui.message = app
                    .language
                    .choose(
                        "Storage operation did not finish. Refresh and retry.",
                        "存储操作未完成，请刷新并重试。",
                    )
                    .into();
            }
        }
    }
    if app.storage_ui.busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(40));
    }
    let lang = app.language;
    if app.storage_ui.open {
        let mut open = true;
        let mut action = None;
        let mut refresh = false;
        let busy = app.busy();
        egui::Window::new(lang.choose("Storage and recycle bin", "数据管理与回收站"))
            .id(egui::Id::new("storage-manager"))
            .open(&mut open)
            .default_size([740.0, 530.0])
            .max_width(880.0)
            .vscroll(true)
            .show(ctx, |ui| {
                ui.set_enabled(app.storage_ui.confirm.is_none());
                super::theme::control_row(ui, |ui| {
                    if super::navigation::register(
                        ui.add_enabled(!busy, egui::Button::new(lang.choose("Refresh", "刷新"))),
                    )
                    .clicked()
                    {
                        refresh = true;
                    }
                    if busy {
                        ui.spinner();
                        ui.label(lang.choose("Working…", "正在处理…"));
                    }
                });
                ui.separator();
                ui.strong(lang.choose("Saved project audio", "工程音频快照"));
                ui.label(lang.choose(
                    "Current version and one backup are kept.",
                    "保留当前版本及一份备份。",
                ));
                for item in &app.storage_ui.inventory.projects {
                    super::theme::card().show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(format!("{} · {}", item.entry.name, size(item.bytes)));
                        super::theme::control_row(ui, |ui| {
                            ui.label(format!(
                                "{} {}",
                                lang.choose("Unused:", "未引用："),
                                size(item.unused)
                            ));
                            if super::navigation::register(ui.add_enabled(
                                !busy && !app.read_only && item.unused > 0,
                                egui::Button::new(lang.choose("Clean unused", "清理未引用音频")),
                            ))
                            .clicked()
                            {
                                action = Some(Action::Prune(item.entry.clone()));
                            }
                            if super::navigation::register(ui.add_enabled(
                                !busy && !app.read_only && item.bytes > 0,
                                egui::Button::new(
                                    lang.choose("Delete saved audio", "删除已保存音频"),
                                ),
                            ))
                            .clicked()
                            {
                                action = Some(Action::Audio(item.entry.clone()));
                            }
                        });
                    });
                }
                for incomplete in [false, true] {
                    ui.separator();
                    ui.strong(if incomplete {
                        lang.choose("Incomplete recordings", "未完成录制")
                    } else {
                        lang.choose("Recycle bin", "回收站")
                    });
                    let items: Vec<_> = app
                        .storage_ui
                        .inventory
                        .items
                        .iter()
                        .filter(|i| {
                            matches!(i.kind, storage::ItemKind::IncompleteReplay) == incomplete
                        })
                        .collect();
                    if !incomplete
                        && super::navigation::register(ui.add_enabled(
                            !busy && !app.read_only && !items.is_empty(),
                            egui::Button::new(lang.choose("Empty recycle bin", "清空回收站")),
                        ))
                        .clicked()
                    {
                        action = Some(Action::EmptyTrash(
                            items.iter().map(|i| (*i).clone()).collect(),
                        ));
                    }
                    if items.is_empty() {
                        ui.label(lang.choose("No items", "没有项目"));
                    }
                    for item in items {
                        super::theme::card().show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            let kind = match item.kind {
                                storage::ItemKind::ProjectTrash => lang.choose("Project", "工程"),
                                storage::ItemKind::ReplayTrash => lang.choose("Replay", "回放"),
                                storage::ItemKind::IncompleteReplay => {
                                    lang.choose("Recording", "录制")
                                }
                            };
                            ui.add(
                                egui::Label::new(format!(
                                    "{kind} · {} · {}",
                                    item.name,
                                    size(item.bytes)
                                ))
                                .wrap(true),
                            );
                            super::theme::control_row(ui, |ui| {
                                if item.restorable
                                    && super::navigation::register(ui.add_enabled(
                                        !busy && !app.read_only,
                                        egui::Button::new(lang.choose("Restore", "恢复")),
                                    ))
                                    .clicked()
                                {
                                    action = Some(Action::Restore(item.clone()));
                                }
                                if super::navigation::register(ui.add_enabled(
                                    !busy && !app.read_only,
                                    egui::Button::new(lang.choose("Delete", "删除")),
                                ))
                                .clicked()
                                {
                                    action = Some(Action::DeleteItem(item.clone()));
                                }
                            });
                        });
                    }
                }
                for error in &app.storage_ui.inventory.errors {
                    ui.label(error);
                }
                if !app.storage_ui.message.is_empty() {
                    ui.label(&app.storage_ui.message);
                }
            });
        if !open && !busy {
            app.storage_ui.open = false;
        }
        if refresh {
            app.storage_ui.start(None, app.projects.clone());
        }
        if let Some(action) = action {
            app.request_storage_action(action);
        }
    }
    let Some(action) = app.storage_ui.confirm.clone() else {
        return;
    };
    let (title, verb) = match &action {
        Action::Restore(_) => (
            lang.choose("Restore item", "恢复项目"),
            lang.choose("Restore", "恢复"),
        ),
        Action::Discard(_) => (
            lang.choose("Discard recording", "丢弃录制"),
            lang.choose("Discard", "丢弃"),
        ),
        Action::Prune(_) => (
            lang.choose("Clean unused audio", "清理未引用音频"),
            lang.choose("Clean", "清理"),
        ),
        _ => (
            lang.choose("Confirm deletion", "确认删除"),
            lang.choose("Delete", "删除"),
        ),
    };
    let mut open = true;
    let mut confirm = false;
    let mut cancel = false;
    let busy = app.busy();
    egui::Window::new(title).id(egui::Id::new("storage-confirm")).open(&mut open).collapsible(false).resizable(false).default_width(480.0).max_width(620.0).anchor(egui::Align2::CENTER_CENTER,egui::Vec2::ZERO).show(ctx,|ui|{
        let name=match &action{
            Action::Project(e)|Action::Audio(e)|Action::Prune(e)=>e.name.clone(),
            Action::Replay(p)=>app.replay_list.iter().find(|(path,_)|path==p).map(|(_,name)|name.clone()).unwrap_or_else(||lang.choose("Replay","回放").into()),
            Action::Discard(_)=>if app.take_name.trim().is_empty(){lang.choose("Current recording","当前录制").into()}else{app.take_name.clone()},
            Action::Export(p)=>p.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            Action::DeleteItem(i)|Action::Restore(i)=>i.name.clone(),
            Action::EmptyTrash(items)=>format!("{} {}",items.len(),lang.choose("items","个项目"))
        };ui.label(name);
        ui.label(match &action{
            Action::Project(_)=>lang.choose("Delete this project and its saved audio?\nReplays, exports and shared sounds remain.","删除此工程及其保存的音频？\n回放、导出文件和共享音色保留。"),
            Action::Audio(_)=>lang.choose("Delete saved track audio and undo history, including the backup?\nConfiguration and sounds remain.","删除轨道音频及撤销历史（包括备份）？\n配置和音色保留。"),
            Action::Prune(_)=>lang.choose("Clean unused audio snapshots?\nCurrent and backup audio remain.","清理未引用的音频快照？\n当前版本及备份音频保留。"),
            Action::Replay(_)|Action::Discard(_)=>lang.choose("Delete this recording?\nExported files remain.","删除此录制？\n导出文件保留。"),
            Action::Export(_)=>lang.choose("Delete this WAV file?","删除此 WAV 文件？"),
            Action::Restore(_)=>lang.choose("Restore this item to its library?","将此项目恢复到库中？"),
            _=>lang.choose("Delete the selected files?","删除所选文件？"),
        });
        if busy{ui.spinner();ui.label(lang.choose("Working…","正在处理…"));}
        ui.add_enabled_ui(!busy&&!app.read_only&&ctx.input(|i|i.focused),|ui|{super::theme::control_row(ui,|ui|{
            let no=super::navigation::register(ui.button(lang.text("Cancel")));cancel=no.clicked();
            let yes=super::navigation::register(ui.button(verb));confirm=yes.clicked();
            #[cfg(debug_assertions)] ctx.data_mut(|d|d.insert_temp(egui::Id::new("storage-confirm-buttons"),(no.rect,yes.rect)));
        });});
        if !app.storage_ui.message.is_empty(){ui.label(&app.storage_ui.message);}
    });
    if !busy && (!open || cancel) {
        app.storage_ui.confirm = None;
    }
    if confirm {
        app.storage_ui.start(Some(action), app.projects.clone());
    }
}
