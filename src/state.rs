#[derive(PartialEq, Clone, Copy)]
pub enum AppState {
    Init,
    MainLoop,
}
#[derive(Clone, Copy, PartialEq)]
pub enum ProjectNameMode {
    Add,
    Rename,
}
#[derive(Clone, Copy, PartialEq)]
pub enum PendingExit {
    ToInit,
    CloseWindow,
}
