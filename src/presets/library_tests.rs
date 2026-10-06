use super::*;
use crate::config::{
    FxKind, InputFx,
    osc_configs::{SampleAsset, SampleCapture, SavedSampleRef},
};
use serde_json::json;
use std::sync::Arc;

struct Fixture {
    root: PathBuf,
    sounds: PathBuf,
    projects: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from("var").join(format!("library-delete-{}", crate::session::id()));
        let sounds = root.join("presets");
        let projects = root.join("projects");
        fs::create_dir_all(&sounds).unwrap();
        fs::create_dir_all(&projects).unwrap();
        Self {
            root,
            sounds,
            projects,
        }
    }
    fn write(&self, path: &str, value: serde_json::Value) -> PathBuf {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let path = self.root.canonicalize().unwrap();
        assert!(path.starts_with(Path::new("var").canonicalize().unwrap()));
        crate::storage::remove_tree(&Path::new("var").canonicalize().unwrap(), &path).unwrap();
    }
}
fn reference(name: &str, checksum: &str, embedded: bool) -> serde_json::Value {
    json!({"input_fx":{"banks":[{"slots":[{"osc":{
        "sample":if embedded {json!({"frames":[0.0,0.1,0.0,-0.1]})} else {serde_json::Value::Null},
        "sample_temporary":false,
        "sample_ref":{"preset":name,"sha256":checksum}
    }}]}]}})
}
#[test]
fn library_delete_frees_file_and_pcm_without_a_trash_copy() {
    let f = Fixture::new();
    let sound = f.write(
        "presets/Bass.json",
        json!({"sample":{"frames":[0.2,0.2,0.2,0.2]}}),
    );
    let untouched = f.write(
        "presets/Other.json",
        json!({"sample":{"frames":[0.3,0.3,0.3,0.3]}}),
    );
    assert_eq!(
        delete_sound_in(&f.sounds, &f.projects, &sound, None).unwrap(),
        DeleteSoundResult::Deleted
    );
    assert!(!sound.exists());
    assert!(untouched.exists());
    assert_eq!(fs::read_dir(&f.sounds).unwrap().count(), 1);
    assert!(!f.root.join("trash").exists());
    assert!(!f.sounds.join("trash").exists());
    let phrase = f.write(
        "clips/Melody.json",
        json!({"version":1,"clip":{"name":"Melody"}}),
    );
    let copied = f.write("projects/Copied.json", json!({"clip":{"name":"Melody"}}));
    crate::storage::remove_file(&f.root.join("clips"), &phrase).unwrap();
    assert!(!phrase.exists());
    assert!(copied.exists());
}

#[test]
fn library_create_new_never_overwrites_or_leaves_a_second_copy() {
    let f = Fixture::new();
    let path = f.sounds.join("Created.json");
    write_new(&path, b"{\"version\":2}").unwrap();
    let before = fs::read(&path).unwrap();
    assert!(write_new(&path, b"replacement").is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_dir(&f.sounds).unwrap().count(), 1);
}
#[test]
fn library_delete_protects_real_project_snapshot_backup_and_preset_references_only() {
    let f = Fixture::new();
    let sound = f.write(
        "presets/Bass.json",
        json!({"sample":{"frames":[0.2,0.2,0.2,0.2]}}),
    );
    let checksum = format!("{:x}", Sha256::digest(fs::read(&sound).unwrap()));
    let paths = [
        "projects/P.json",
        "projects/P.json.bak",
        "projects/P.json.assets/snapshots/current/manifest.json",
        "projects/trash/old/P.json",
        "presets/Linked.json",
    ];
    for path in paths {
        f.write(path, reference("Bass", &checksum, false));
    }
    f.write("projects/Embedded.json", reference("Bass", &checksum, true));
    f.write(
        "projects/P.json.assets/replays/old/initial/manifest.json",
        reference("Bass", &checksum, false),
    );
    f.write(
        "projects/P.json.assets/replay-trash/old/initial/manifest.json",
        reference("Bass", &checksum, false),
    );
    f.write(
        "projects/Different.json",
        reference("Bass", &"0".repeat(64), false),
    );
    let DeleteSoundResult::UsedBy(owners) =
        delete_sound_in(&f.sounds, &f.projects, &sound, None).unwrap()
    else {
        panic!()
    };
    assert_eq!(owners.len(), paths.len());
    assert!(sound.exists());
    assert!(
        !owners
            .iter()
            .any(|p| p.contains("Embedded") || p.contains("Different"))
    );
    for path in paths {
        crate::storage::remove_file(f.root.join(path).parent().unwrap(), &f.root.join(path))
            .unwrap();
    }
    assert_eq!(
        delete_sound_in(&f.sounds, &f.projects, &sound, None).unwrap(),
        DeleteSoundResult::Deleted
    );
}
#[test]
fn library_delete_cannot_remove_a_live_saved_sample_until_it_is_cleared() {
    let f = Fixture::new();
    let sound = f.write(
        "presets/Bass.json",
        json!({"sample":{"frames":[0.2,0.2,0.2,0.2]}}),
    );
    let mut c = AppConfig::new(120, 0, 5);
    c.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
    let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx else {
        panic!()
    };
    o.sample = Some(Arc::new(SampleAsset::new(
        "Bass".into(),
        48000,
        vec![0.0, 0.2, 0.0, -0.2],
    )));
    o.sample_ref = Some(SavedSampleRef {
        preset: "Bass".into(),
        sha256: format!("{:x}", Sha256::digest(fs::read(&sound).unwrap())),
        content_hash: o.sample.as_ref().unwrap().content_hash,
        sample_rate: 48000,
        frames: 4,
    });
    o.sample_temporary = false;
    assert_eq!(
        delete_sound_in(&f.sounds, &f.projects, &sound, Some(&c)).unwrap(),
        DeleteSoundResult::UsedBy(vec!["Current project".into()])
    );
    let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx else {
        panic!()
    };
    o.clear_sample();
    assert_eq!(
        delete_sound_in(&f.sounds, &f.projects, &sound, Some(&c)).unwrap(),
        DeleteSoundResult::Deleted
    );
}
#[test]
fn library_delete_refuses_unreadable_reference_metadata_without_removing_the_sound() {
    let f = Fixture::new();
    let sound = f.write(
        "presets/Bass.json",
        json!({"sample":{"frames":[0.2,0.2,0.2,0.2]}}),
    );
    fs::write(f.projects.join("broken.json"), b"{").unwrap();
    assert!(delete_sound_in(&f.sounds, &f.projects, &sound, None).is_err());
    assert!(sound.exists());
}
#[test]
fn clearing_osc_sample_cancels_late_import_and_completed_capture_without_writing_files() {
    let mut c = AppConfig::new(120, 0, 5);
    c.input_fx.set_slot_kind(0, 0, FxKind::Oscillator);
    let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx else {
        panic!()
    };
    o.sample = Some(Arc::new(SampleAsset::new(
        "Temporary".into(),
        48000,
        vec![0.0, 0.2, 0.0, -0.2],
    )));
    let weak = Arc::downgrade(o.sample.as_ref().unwrap());
    let mailbox = Arc::new(SampleCapture::new(20));
    o.capture = Some(mailbox.clone());
    let (tx, rx) = std::sync::mpsc::channel();
    o.sample_job = Some(rx);
    let before = crate::project::fingerprint_data_from_config(&c);
    let Some(InputFx::Oscillator(o)) = &mut c.input_fx.banks[0].slots[0].fx else {
        panic!()
    };
    o.clear_sample();
    assert!(weak.upgrade().is_none());
    mailbox.state.store(2, std::sync::atomic::Ordering::Release);
    assert!(
        tx.send(Ok(SampleAsset::new(
            "Late".into(),
            48000,
            vec![0.0, 0.2, 0.0, -0.2]
        )))
        .is_err()
    );
    c.poll_synth_assets();
    let after = crate::project::fingerprint_data_from_config(&c);
    assert!(
        before != after,
        "Clearing must publish a configuration action to recording/replay"
    );
    let Some(InputFx::Oscillator(o)) = &c.input_fx.banks[0].slots[0].fx else {
        panic!()
    };
    assert!(
        o.sample.is_none()
            && o.sample_ref.is_none()
            && o.capture.is_none()
            && o.sample_job.is_none()
    );
}
