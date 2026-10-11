use super::*;
use crate::{
    config::{
        AppConfig, FxKind, InputFx,
        osc_configs::{SampleAsset, Waveform},
    },
    engine::loop_audio::OfflinePages,
};

fn fixture() -> (PathBuf, Store, AudioSnapshot, ProjectData) {
    let root = PathBuf::from("var").join(format!("recovery-{}", session::id()));
    let projects = root.join("projects");
    fs::create_dir_all(&projects).unwrap();
    let entry = ProjectEntry {
        name: "Recovery fixture".into(),
        file: "fixture.json".into(),
    };
    fs::write(
        projects.join(&entry.file),
        b"formal project must not change",
    )
    .unwrap();
    fs::create_dir_all(root.join("presets")).unwrap();
    fs::write(
        root.join("presets/saved.json"),
        b"saved sound must not change",
    )
    .unwrap();
    let store = Store::in_root(projects, entry).unwrap();
    let mut snapshot = AudioSnapshot::empty(8000);
    snapshot.at = 12345;
    snapshot.has_histories = true;
    for frame in 0..128 {
        assert!(snapshot.tracks[0].write(frame, [frame as f32 * 0.001, -0.2], &mut OfflinePages));
    }
    snapshot.histories[0]
        .undo
        .push(&snapshot.tracks[0], &mut OfflinePages);
    snapshot.tracks[0].write(0, [0.75, -0.5], &mut OfflinePages);
    let mut config = AppConfig::new(124, 0, 5);
    config.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
    if let Some(InputFx::Oscillator(osc)) = &mut config.input_fx.banks[0].slots[0].fx {
        osc.waveform.value = Waveform::Sample;
        osc.sample_temporary = true;
        osc.sample = Some(Arc::new(SampleAsset::new(
            "Unsaved capture".into(),
            8000,
            vec![0.0, 0.25, -0.25, 0.5],
        )));
    }
    let mut data = project::data_from_config(&config);
    data.snapshot = Some("formal-snapshot-reference".into());
    (root, store, snapshot, data)
}
fn clean(root: &Path) {
    let var = Path::new("var").canonicalize().unwrap();
    let root = root.canonicalize().unwrap();
    assert!(root.parent() == Some(var.as_path()));
    storage::remove_tree(&var, &root).unwrap();
}
fn saved(outcome: WriteOutcome) -> Info {
    match outcome {
        WriteOutcome::Saved(info) => info,
        other => panic!("Expected durable save, got {other:?}"),
    }
}

#[test]
fn recovery_roundtrip_keeps_temporary_pcm_tracks_history_and_formal_files_unchanged() {
    let (root, store, snapshot, data) = fixture();
    assert!(store.begin().unwrap().is_none());
    let info = saved(
        store
            .save(&store.ticket_at(123456789), &snapshot, &data)
            .unwrap(),
    );
    assert_eq!(info.captured_unix_ms, 123456789);
    assert_eq!(info.frame, 12345);
    assert_eq!(info.sample_rate, 8000);
    assert!(info.saved_unix_ms >= info.captured_unix_ms);
    let (audio, restored) = store.load(&info).unwrap();
    assert!(restored == data);
    assert_eq!(audio.tracks[0].len, 128);
    assert_eq!(audio.tracks[0].read(0), [0.75, -0.5]);
    assert_eq!(audio.histories[0].undo.len, 1);
    assert_eq!(audio.histories[0].undo.slots[0].read(0), [0.0, -0.2]);
    let osc = restored.input_fx.banks[0].slots[0].osc.as_ref().unwrap();
    assert!(osc.sample_temporary);
    assert_eq!(
        osc.sample.as_ref().unwrap().frames,
        vec![0.0, 0.25, -0.25, 0.5]
    );
    assert_eq!(
        fs::read(root.join("projects/fixture.json")).unwrap(),
        b"formal project must not change"
    );
    assert_eq!(
        fs::read(root.join("presets/saved.json")).unwrap(),
        b"saved sound must not change"
    );
    clean(&root);
}
#[test]
fn repeated_checkpoints_keep_one_audio_bundle_and_preserve_unknown_folders() {
    let (root, store, mut snapshot, data) = fixture();
    store.begin().unwrap();
    fs::create_dir_all(store.0.root.join("personal-notes")).unwrap();
    fs::write(store.0.root.join("personal-notes/keep.txt"), b"keep").unwrap();
    let mut last = None;
    for i in 0..10 {
        snapshot.at = 100 + i;
        last = Some(saved(
            store.save(&store.ticket(), &snapshot, &data).unwrap(),
        ));
        assert_eq!(
            SLOTS
                .iter()
                .filter(|name| store.0.root.join(name).exists())
                .count(),
            1
        );
    }
    let last = last.unwrap();
    assert_eq!(store.inspect().unwrap(), Some(last.clone()));
    assert_eq!(store.load(&last).unwrap().0.at, 109);
    assert!(store.0.root.join("personal-notes/keep.txt").exists());
    clean(&root);
}
#[test]
fn reopening_offers_existing_checkpoint_without_replacing_it_and_rejects_old_session() {
    let (root, old, snapshot, data) = fixture();
    old.begin().unwrap();
    let info = saved(old.save(&old.ticket(), &snapshot, &data).unwrap());
    let before = fs::read(old.0.root.join("head.json")).unwrap();
    let new = Store::in_root(root.join("projects"), old.entry().clone()).unwrap();
    assert_eq!(new.begin().unwrap(), Some(info.clone()));
    assert_eq!(fs::read(old.0.root.join("head.json")).unwrap(), before);
    assert!(new.load(&info).is_ok());
    assert!(matches!(
        old.save(&old.ticket(), &snapshot, &data).unwrap(),
        WriteOutcome::Superseded
    ));
    assert!(!old.clear(&old.invalidate()).unwrap());
    assert!(old.begin().is_err());
    assert_eq!(new.inspect().unwrap(), Some(info));
    clean(&root);
}
#[test]
fn clear_is_a_barrier_for_queued_writes_and_the_next_session_can_save() {
    let (root, store, snapshot, data) = fixture();
    store.begin().unwrap();
    saved(store.save(&store.ticket(), &snapshot, &data).unwrap());
    let lock = store.lock().unwrap();
    let old_ticket = store.ticket();
    let worker_store = store.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        tx.send(()).unwrap();
        worker_store.save(&old_ticket, &snapshot, &data).unwrap()
    });
    rx.recv().unwrap();
    let clear_ticket = store.invalidate();
    drop(lock);
    assert!(store.clear(&clear_ticket).unwrap());
    assert!(matches!(worker.join().unwrap(), WriteOutcome::Superseded));
    assert!(store.inspect().unwrap().is_none());
    assert!(SLOTS.iter().all(|s| !store.0.root.join(s).exists()));
    assert!(
        !store.clear(&clear_ticket).unwrap(),
        "A consumed clear ticket cannot be reused"
    );
    assert!(store.begin().unwrap().is_none());
    let info = saved(
        store
            .save(
                &store.ticket(),
                &AudioSnapshot::empty(8000),
                &project::data_from_config(&AppConfig::new(120, 0, 5)),
            )
            .unwrap(),
    );
    assert_eq!(info.frame, 0);
    assert_eq!(
        fs::read(root.join("projects/fixture.json")).unwrap(),
        b"formal project must not change"
    );
    clean(&root);
}
#[test]
fn stale_load_and_corrupt_manifest_or_audio_are_rejected_without_changing_formal_data() {
    let (root, store, snapshot, data) = fixture();
    store.begin().unwrap();
    let old = saved(store.save(&store.ticket(), &snapshot, &data).unwrap());
    let info = saved(store.save(&store.ticket(), &snapshot, &data).unwrap());
    assert!(store.load(&old).is_err());
    let path = store.slot_path(info.slot);
    let wav = path.join("track-1.wav");
    fs::write(&wav, b"broken").unwrap();
    assert!(store.load(&info).is_err());
    fs::write(path.join("manifest.json"), b"{}").unwrap();
    assert!(store.load(&info).is_err());
    assert!(store.clear(&store.invalidate()).unwrap());
    assert!(store.inspect().unwrap().is_none());
    clean(&root);
}
#[test]
fn cancelled_bundle_writing_checks_during_long_audio_and_never_publishes_a_manifest() {
    let (root, _, mut snapshot, data) = fixture();
    for frame in 128..20000 {
        snapshot.tracks[0].write(frame, [0.2, 0.2], &mut OfflinePages);
    }
    let calls = std::cell::Cell::new(0);
    let bundle = root.join("partial");
    let result = session::write_bundle_cancellable(&bundle, &snapshot, data, &|| {
        calls.set(calls.get() + 1);
        calls.get() > 4
    });
    assert!(result.is_err());
    assert!(calls.get() > 4);
    assert!(!bundle.join("manifest.json").exists());
    assert!(fs::metadata(bundle.join("track-1.wav")).unwrap().len() < 20000 * 8);
    clean(&root);
}
#[test]
#[cfg(windows)]
fn occupied_previous_audio_keeps_latest_checkpoint_bounded_and_discard_can_retry() {
    use std::os::windows::fs::OpenOptionsExt;
    let (root, store, snapshot, data) = fixture();
    store.begin().unwrap();
    let first = saved(store.save(&store.ticket(), &snapshot, &data).unwrap());
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(1 | 2)
        .open(store.slot_path(first.slot).join("track-1.wav"))
        .unwrap();
    let latest = match store.save(&store.ticket(), &snapshot, &data).unwrap() {
        WriteOutcome::SavedCleanupWarning(info, _) => info,
        other => panic!("Expected cleanup warning: {other:?}"),
    };
    assert!(store.load(&latest).is_ok());
    assert_eq!(
        SLOTS
            .iter()
            .filter(|name| store.0.root.join(name).exists())
            .count(),
        2
    );
    let discard = store.invalidate();
    assert!(store.clear(&discard).is_err());
    assert!(store.inspect().unwrap().is_none());
    assert!(store.slot_path(first.slot).join("entry.json").exists());
    drop(held);
    assert!(store.clear(&discard).unwrap());
    assert!(SLOTS.iter().all(|s| !store.0.root.join(s).exists()));
    clean(&root);
}
#[test]
#[cfg(windows)]
fn recovery_rejects_linked_slot_before_any_outside_mutation() {
    use std::os::windows::process::CommandExt;
    let (root, store, snapshot, data) = fixture();
    store.begin().unwrap();
    let outside = root.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("keep.wav"), b"safe").unwrap();
    let link = store.0.root.canonicalize().unwrap().join(SLOTS[0]);
    let destination = outside.canonicalize().unwrap();
    let status = std::process::Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(destination)
        .creation_flags(0x08000000)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    assert!(store.save(&store.ticket(), &snapshot, &data).is_err());
    assert!(store.clear(&store.invalidate()).is_err());
    assert_eq!(fs::read(outside.join("keep.wav")).unwrap(), b"safe");
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
    fs::remove_dir(link).unwrap();
    clean(&root);
}
#[test]
fn project_identity_and_unknown_slot_guards_prevent_cross_project_changes() {
    let (root, store, snapshot, data) = fixture();
    assert!(
        Store::in_root(
            root.join("projects"),
            ProjectEntry {
                name: "Bad".into(),
                file: "../outside.json".into()
            }
        )
        .is_err()
    );
    store.begin().unwrap();
    let unknown = store.slot_path(0);
    fs::create_dir_all(&unknown).unwrap();
    fs::write(unknown.join("keep.wav"), b"unknown").unwrap();
    assert!(store.save(&store.ticket(), &snapshot, &data).is_err());
    assert!(unknown.join("keep.wav").exists());
    let other = Store::in_root(
        root.join("projects"),
        ProjectEntry {
            name: "Other".into(),
            file: "other.json".into(),
        },
    )
    .unwrap();
    other.begin().unwrap();
    let foreign = other.ticket();
    assert!(matches!(
        store.save(&foreign, &snapshot, &data).unwrap(),
        WriteOutcome::Superseded
    ));
    assert!(!store.clear(&foreign).unwrap());
    clean(&root);
}

#[test]
fn interrupted_slot_marker_is_cleaned_only_with_matching_staging_identity() {
    let (root, store, snapshot, data) = fixture();
    store.begin().unwrap();
    let slot = store.slot_path(0);
    fs::create_dir_all(&slot).unwrap();
    fs::write(slot.join("entry.1234.tmp"), b"partial metadata").unwrap();
    assert!(store.clear(&store.invalidate()).is_err());
    assert!(slot.join("entry.1234.tmp").exists());
    let stage = SlotOwner {
        version: VERSION,
        project_file: store.entry().file.clone(),
        slot: 0,
    };
    project::atomic_write(
        &store.0.root.join("staging.json"),
        &serde_json::to_vec(&stage).unwrap(),
    )
    .unwrap();
    assert!(store.clear(&store.invalidate()).unwrap());
    assert!(!slot.exists());
    let info = saved(store.save(&store.ticket(), &snapshot, &data).unwrap());
    assert!(store.load(&info).is_ok());
    clean(&root);
}
