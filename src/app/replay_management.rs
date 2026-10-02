use super::*;
impl MyApp {
    pub fn delete_replay(&mut self, path: PathBuf) {
        if self.read_only || self.busy() || self.taking() || self.player_open {
            return;
        }
        let Some(index) = self.active_project_idx else {
            return;
        };
        match crate::replay::trash(&self.projects[index], &path) {
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
                self.replay_list = crate::replay::list(&self.projects[index]);
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
        let Some(index) = self.active_project_idx else {
            return;
        };
        match crate::replay::restore_last(&self.projects[index]) {
            Ok(found) => {
                self.replay_list = crate::replay::list(&self.projects[index]);
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
}
