use super::*;
impl MyApp {
    pub fn delete_replay(&mut self, path: PathBuf) {
        self.request_storage_action(ui::storage::Action::Replay(path));
    }
    pub fn discard_take(&mut self) {
        if let Some(path) = self.draft.clone() {
            self.request_storage_action(ui::storage::Action::Discard(path));
        }
    }
    pub fn delete_replay_export(&mut self, path: PathBuf) {
        self.request_storage_action(ui::storage::Action::Export(path));
    }
}
