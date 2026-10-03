use super::*;
impl MyApp {
    pub fn delete_replay(&mut self, path: PathBuf) {
        if self.read_only || self.busy() || self.taking() || self.player_open {
            return;
        }
        match crate::replay::library::trash(&path) {
            Ok(()) => {
                if self.draft.as_ref() == Some(&path) {
                    self.draft = None;
                }
                if self
                    .rendered
                    .as_ref()
                    .is_some_and(|(root, _)| *root == path)
                {
                    self.rendered = None;
                }
                self.replay_list = crate::replay::library::list();
                self.status=self.language.choose("Replay deleted. Restore last deleted recovers it; exported WAV files remain.","回放已删除，可恢复上一次删除；已导出的 WAV 文件保留。").into();
            }
            Err(e) => self.status = e.to_string(),
        }
    }
    pub fn discard_take(&mut self) {
        if let Some(path) = self.draft.clone() {
            self.delete_replay(path);
        }
    }
    pub fn restore_replay(&mut self) {
        if self.read_only || self.busy() {
            return;
        }
        match crate::replay::library::restore_last() {
            Ok(found) => {
                self.replay_list = crate::replay::library::list();
                self.status = self
                    .language
                    .choose(
                        if found {
                            "Replay restored"
                        } else {
                            "No deleted replay to restore"
                        },
                        if found {
                            "回放已恢复"
                        } else {
                            "没有可恢复的回放"
                        },
                    )
                    .into();
            }
            Err(e) => self.status = e.to_string(),
        }
    }
    pub fn delete_replay_export(&mut self, path: PathBuf) {
        if self.read_only || self.busy() {
            return;
        }
        match crate::replay::library::delete_export(&path) {
            Ok(()) => {
                if self
                    .rendered
                    .as_ref()
                    .is_some_and(|(_, result)| result.wav == path)
                {
                    self.rendered = None;
                }
                self.replay_exports = crate::replay::library::exports();
                self.status = self
                    .language
                    .choose(
                        "Exported WAV deleted; the replay inputs are preserved.",
                        "已删除导出 WAV，回放原始输入仍保留。",
                    )
                    .into();
            }
            Err(error) => self.status = error.to_string(),
        }
    }
}
