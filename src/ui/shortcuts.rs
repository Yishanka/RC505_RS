use crate::{
    app::{
        MyApp,
        shortcuts::{Bindings, Chord},
    },
    app_support::language::Language,
};
use eframe::egui::{self, Key};

#[derive(Default)]
pub struct ShortcutEditor {
    pub open: bool,
    pub draft: Bindings,
    pub capture: Option<(String, usize)>,
    pub message: String,
    group: String,
    search: String,
}
impl ShortcutEditor {
    pub fn open(&mut self, bindings: &Bindings) {
        self.draft = bindings.clone();
        self.capture = None;
        self.message.clear();
        self.open = true;
    }
    pub fn handle_input(&mut self, input: &egui::InputState, language: Language) {
        if input.key_pressed(Key::Escape) {
            if self.capture.take().is_none() {
                self.open = false;
            }
            return;
        }
        if let Some((id, index)) = self.capture.clone() {
            if let Some(chord) = input.events.iter().find_map(Chord::from_event) {
                if !chord.allowed() {
                    self.message = language
                        .choose(
                            "This key is reserved for navigation or the operating system.",
                            "该按键保留给导航或操作系统，请选其他组合。",
                        )
                        .into();
                    return;
                }
                let current = self
                    .draft
                    .entries()
                    .iter()
                    .find(|d| d.id == id)
                    .map(|d| self.draft.chords(d).to_vec())
                    .unwrap_or_default();
                let chords = self.draft.overrides.entry(id).or_insert(current);
                if index < chords.len() {
                    chords[index] = chord;
                } else {
                    chords.push(chord);
                }
                self.capture = None;
                self.message.clear();
            }
        }
    }
}
pub fn draw(ctx: &egui::Context, app: &mut MyApp) {
    if !app.shortcut_editor.open {
        return;
    }
    let lang = app.language;
    let mut open = true;
    let mut apply = false;
    egui::Window::new(lang.choose("Keyboard shortcuts","键盘快捷键"))
        .id(egui::Id::new("shortcut-editor")).open(&mut open)
        .default_size([850.0,610.0]).min_width(560.0).resizable(true).show(ctx,|ui|{
        ui.label(lang.choose("Click a key, press its replacement, then Save.","点击键帽，按下新按键，保存后生效。"));
        super::theme::control_row(ui,|ui|{
            ui.label(lang.choose("Find","查找"));
            ui.add(egui::TextEdit::singleline(&mut app.shortcut_editor.search).desired_width(150.0));
            egui::ComboBox::from_id_source("shortcut-group").selected_text(match app.shortcut_editor.group.as_str(){"global"=>lang.choose("Global","全局"),"performance"=>lang.choose("Performance","演奏"),"tracks"=>lang.choose("Tracks","轨道"),"fx"=>lang.choose("Effects","效果器"),"faders"=>lang.choose("Faders","推子"),_=>lang.choose("All groups","全部分类")}).show_ui(ui,|ui|{
                for (key,en,zh) in [("","All groups","全部分类"),("global","Global","全局"),("performance","Performance","演奏"),("tracks","Tracks","轨道"),("fx","Effects","效果器"),("faders","Faders","推子")]{ui.selectable_value(&mut app.shortcut_editor.group,key.into(),lang.choose(en,zh));}
            });
            if ui.button(lang.choose("Reset all","全部恢复默认")).clicked(){app.shortcut_editor.draft=Bindings::defaults();app.shortcut_editor.capture=None;}
        });
        if app.shortcut_editor.capture.is_some(){ui.colored_label(super::theme::accent(ui),lang.choose("Press the new key combination… Esc cancels","请按新组合键……Esc 取消"));}
        let entries=app.shortcut_editor.draft.entries().to_vec();
        let query=app.shortcut_editor.search.to_lowercase();
        egui::ScrollArea::vertical().id_source("shortcut-rows").max_height((ctx.screen_rect().height()-260.0).max(180.0)).show(ui,|ui|{
            egui::Grid::new("shortcuts-grid").num_columns(4).striped(true).spacing([14.0,8.0]).show(ui,|ui|{
                for d in entries.iter().filter(|d|(app.shortcut_editor.group.is_empty()||d.group==app.shortcut_editor.group)&&(query.is_empty()||d.en.to_lowercase().contains(&query)||d.zh.contains(&query))) {
                    ui.add(egui::Label::new(lang.choose(&d.en,&d.zh)).wrap(false));
                    let current=app.shortcut_editor.draft.chords(d).to_vec();
                    for index in 0..2 {
                        ui.horizontal(|ui|{
                            let label=current.get(index).map(Chord::label).unwrap_or_else(||lang.choose("Assign…","设置…").into());
                            if ui.add(egui::Button::new(egui::RichText::new(label).monospace()).min_size(egui::vec2(96.0,28.0))).clicked(){
                                app.shortcut_editor.capture=Some((d.id.clone(),index.min(current.len())));
                            }
                            if current.get(index).is_some() && ui.small_button("×").on_hover_text(lang.choose("Unbind","取消绑定")).clicked(){
                                let mut next=current.clone();next.remove(index);app.shortcut_editor.draft.overrides.insert(d.id.clone(),next);app.shortcut_editor.capture=None;
                            }
                        });
                    }
                    if ui.small_button(lang.choose("Reset","重置")).clicked(){app.shortcut_editor.draft.overrides.remove(&d.id);app.shortcut_editor.capture=None;}
                    ui.end_row();
                }
            });
        });
        let conflict=app.shortcut_editor.draft.conflict();
        if let Some((key,a,b))=&conflict {
            let name=|id:&str| entries.iter().find(|d|d.id==id).map(|d|lang.choose(&d.en,&d.zh)).unwrap_or(id).to_string();
            ui.colored_label(egui::Color32::LIGHT_RED,format!("{}: {key} — {} / {}",lang.choose("Conflict","按键冲突"),name(a),name(b)));
        }
        if !app.shortcut_editor.message.is_empty(){ui.label(&app.shortcut_editor.message);}
        ui.separator();
        ui.label(lang.choose("While editing, arrows and edit shortcuts stay local; recording and faders remain active. Text entry pauses performance keys.","编辑时方向键和编辑组合键留给当前控件，录放和推子仍可用；输入文字时暂停演奏键。"));
        ui.horizontal(|ui|{
            apply=ui.add_enabled(conflict.is_none()&&!app.read_only,egui::Button::new(lang.choose("Save shortcuts","保存快捷键"))).clicked();
            if ui.button(lang.choose("Cancel","取消")).clicked(){app.shortcut_editor.open=false;}
        });
    });
    if apply {
        match app.shortcut_editor.draft.save() {
            Ok(()) => {
                app.shortcuts = app.shortcut_editor.draft.clone();
                app.shortcut_editor.open = false;
                app.status = lang.choose("Shortcuts saved", "快捷键已保存").into();
            }
            Err(e) => app.shortcut_editor.message = e.to_string(),
        }
    }
    if !open {
        app.shortcut_editor.open = false;
    }
}
