// src/state.rs

#[derive(PartialEq, Clone, Copy)]
pub enum AppState {
    Init,
    MainScreen,
    MainLoop,
}

#[derive(PartialEq, Clone, Copy)]
pub enum TrackState {
    Empty,
    Record,
    Play,
    NxtPlay,
    Dub,
    Pause,
}

#[derive(PartialEq, Clone, Copy)]
pub enum ScreenState {
    Empty,
    Beat,
    SYS, 
    FxSelect,
    InFxOsc,
    InFxOscAudio,
    InFxNote,
    InFxOscAudioEnv,
    InFxOscFilter,
    InFxOscFilterEnv,
    InFxFilter,
}

#[derive(PartialEq, Clone, Copy)]
pub enum FxState {
    Bank,
    Single, 
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
