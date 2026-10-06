use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

fn path() -> PathBuf {
    PathBuf::from("var").join(format!("writer-cleanup-{}", session::id()))
}
fn begin(root: &Path) -> Writer {
    Writer::begin(
        root.to_owned(),
        "source.json".into(),
        0,
        AudioSnapshot::empty(8000),
        crate::project::data_from_config(&AppConfig::new(120, 0, 5)),
    )
    .unwrap()
}
fn remove(root: &Path) {
    let parent = Path::new("var").canonicalize().unwrap();
    let root = root.canonicalize().unwrap();
    assert!(root.starts_with(&parent));
    crate::storage::remove_tree(&parent, &root).unwrap();
}

#[test]
fn replay_writer_cleanup_never_adopts_or_truncates_an_existing_directory() {
    let root = path();
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("input.wav"), b"existing recording").unwrap();
    assert!(
        Writer::begin(
            root.clone(),
            "other.json".into(),
            0,
            AudioSnapshot::empty(8000),
            crate::project::data_from_config(&AppConfig::new(120, 0, 5))
        )
        .is_err()
    );
    assert_eq!(
        fs::read(root.join("input.wav")).unwrap(),
        b"existing recording"
    );
    remove(&root);
}
#[test]
fn replay_writer_cleanup_removes_audio_event_and_asset_failures() {
    for mode in 0..3 {
        let root = path();
        let mut writer = begin(&root);
        writer.audio(0, &[[0.2, -0.2]; 128]).unwrap();
        match mode {
            0 => assert!(writer.audio(130, &[[0.0; 2]]).is_err()),
            1 => {
                writer.next_sequence = MAX_EVENTS;
                assert!(
                    writer
                        .event(128, EventKind::Action(Action::Metronome(true)))
                        .is_err()
                );
            }
            _ => {
                let mut c = AppConfig::new(120, 0, 5);
                c.input_fx
                    .set_slot_kind(0, 0, crate::config::FxKind::Oscillator);
                if let Some(crate::config::InputFx::Oscillator(o)) =
                    &mut c.input_fx.banks[0].slots[0].fx
                {
                    o.sample = Some(Arc::new(crate::config::osc_configs::SampleAsset::new(
                        "Asset".into(),
                        8000,
                        vec![0.1; 80],
                    )));
                }
                writer.event_bytes = MAX_LOG_BYTES;
                assert!(
                    writer
                        .event(128, EventKind::Config(crate::project::data_from_config(&c)))
                        .is_err()
                );
                assert!(
                    root.join("samples").exists(),
                    "Exercise cleanup after detached PCM was written"
                );
            }
        }
        assert!(writer.finish(128).is_err());
        assert!(!root.exists());
    }
}
#[test]
fn replay_writer_cleanup_covers_invalid_finish_and_failed_manifest_commit() {
    for mode in 0..3 {
        let root = path();
        let mut writer = begin(&root);
        writer.audio(0, &[[0.1; 2]; 16]).unwrap();
        if mode == 1 {
            writer.initial.take().unwrap().join().unwrap().unwrap();
            writer.initial = Some(std::thread::spawn(|| {
                anyhow::bail!("Injected initial snapshot failure")
            }));
        } else if mode == 2 {
            fs::create_dir(root.join("replay.json")).unwrap();
        }
        assert!(writer.finish(if mode == 0 { 17 } else { 16 }).is_err());
        assert!(!root.exists());
    }
}
#[test]
fn replay_writer_cleanup_waits_for_the_initial_thread_before_unlinking() {
    let root = path();
    let mut writer = begin(&root);
    writer.initial.take().unwrap().join().unwrap().unwrap();
    let (release_tx, release_rx) = mpsc::channel();
    let (abort_started_tx, abort_started_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let finished = Arc::new(AtomicBool::new(false));
    let flag = finished.clone();
    let initial = root.join("initial");
    writer.initial = Some(std::thread::spawn(move || {
        release_rx.recv().unwrap();
        fs::write(initial.join("last.wav"), b"late snapshot bytes")?;
        flag.store(true, Ordering::Release);
        Ok(())
    }));
    let abort = std::thread::spawn(move || {
        abort_started_tx.send(()).unwrap();
        let result = writer.abort();
        done_tx.send(()).unwrap();
        result
    });
    abort_started_rx.recv().unwrap();
    assert!(matches!(
        done_rx.recv_timeout(std::time::Duration::from_millis(20)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    assert!(root.join("input.wav").exists());
    release_tx.send(()).unwrap();
    abort.join().unwrap().unwrap();
    assert!(finished.load(Ordering::Acquire));
    assert!(!root.exists());
}
#[test]
fn replay_writer_cleanup_drop_discards_unfinished_but_finish_keeps_valid_draft() {
    let unfinished = path();
    drop(begin(&unfinished));
    assert!(!unfinished.exists());
    let root = path();
    let mut writer = begin(&root);
    writer.audio(0, &[[0.2, -0.2]; 128]).unwrap();
    assert_eq!(writer.finish(128).unwrap(), root);
    assert_eq!(info(&root).unwrap().frames, 128);
    assert!(root.join("input.wav").exists() && root.join("initial/manifest.json").exists());
    remove(&root);
}
#[cfg(windows)]
#[test]
fn replay_writer_cleanup_failure_is_reported_and_can_be_retried_after_unlocking() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = path();
    let writer = begin(&root);
    let lock = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .share_mode(0)
        .open(root.join("locked.bin"))
        .unwrap();
    let error = writer.abort().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Incomplete recording could not be removed")
    );
    assert!(root.exists());
    drop(lock);
    remove(&root);
}
