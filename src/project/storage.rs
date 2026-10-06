//! Project file transactions and bounded snapshot retention.
use super::{INDEX_FILE, ProjectData, ProjectEntry, ProjectIndex};
use crate::{session, storage as guard};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(serde::Serialize, Deserialize, Default)]
struct CleanupIntent {
    snapshots: Vec<String>,
}
fn intent_path(root: &Path, entry: &ProjectEntry) -> Result<PathBuf> {
    project_path(root, entry)?;
    Ok(root
        .join(format!("{}.assets", entry.file))
        .join("audio-cleanup.json"))
}
fn read_intent(root: &Path, entry: &ProjectEntry) -> Result<CleanupIntent> {
    let path = intent_path(root, entry)?;
    if !guard::exists(&path)? {
        return Ok(CleanupIntent::default());
    }
    guard::managed_child(path.parent().unwrap(), &path)?;
    ensure!(
        fs::metadata(&path)?.len() <= 65536,
        "Audio cleanup record is oversized"
    );
    let intent: CleanupIntent = serde_json::from_slice(&fs::read(&path)?)?;
    ensure!(
        intent.snapshots.len() <= 128,
        "Too many unfinished audio cleanup entries"
    );
    for name in &intent.snapshots {
        ensure!(name.len() <= 255, "Snapshot identity is too long");
        session::safe_child(Path::new("."), name)?;
    }
    Ok(intent)
}
fn write_intent(root: &Path, entry: &ProjectEntry, intent: &CleanupIntent) -> Result<()> {
    ensure!(
        intent.snapshots.len() <= 128,
        "Finish pending audio cleanup before deleting another snapshot"
    );
    for name in &intent.snapshots {
        ensure!(name.len() <= 255, "Snapshot identity is too long");
        session::safe_child(Path::new("."), name)?;
    }
    let path = intent_path(root, entry)?;
    let parent = path.parent().unwrap();
    if intent.snapshots.is_empty() {
        if guard::exists(&path)? {
            guard::remove_file(parent, &path)?;
        }
        return Ok(());
    }
    guard::ensure_directory(parent)?;
    super::atomic_write(&path, &serde_json::to_vec(intent)?)
}

pub fn project_path(root: &Path, entry: &ProjectEntry) -> Result<PathBuf> {
    let path = session::safe_child(root, &entry.file)?;
    ensure!(
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
            && !entry.file.eq_ignore_ascii_case(INDEX_FILE),
        "Invalid project identity"
    );
    Ok(path)
}
pub fn lock_project(root: &Path, entry: &ProjectEntry) -> Result<fs::File> {
    use fs2::FileExt;
    guard::checked_directory(root)?;
    let path = project_path(root, entry)?.with_extension("lock");
    if guard::exists(&path)? {
        guard::managed_child(root, &path)?;
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    file.try_lock_exclusive()
        .map_err(|_| anyhow::anyhow!("Project is open or being changed by another process"))?;
    Ok(file)
}
pub fn write_locked(root: &Path, entry: &ProjectEntry, data: &ProjectData) -> Result<()> {
    let path = project_path(root, entry)?;
    let backup = path.with_extension("json.bak");
    if guard::exists(&backup)? {
        guard::managed_child(root, &backup)?;
    }
    if guard::exists(&path)? {
        guard::managed_child(root, &path)?;
        let old = fs::read(&path)?;
        // Never replace a valid recovery copy with an unreadable configuration.
        if serde_json::from_slice::<ProjectData>(&old).is_ok() {
            super::atomic_write(&backup, &old)?;
        }
    }
    super::atomic_write(&path, &serde_json::to_vec_pretty(data)?)
}
#[derive(Deserialize)]
struct Pointer {
    #[serde(default)]
    snapshot: Option<String>,
}
fn retained(root: &Path, entry: &ProjectEntry) -> Result<HashSet<String>> {
    let path = project_path(root, entry)?;
    let mut keep = HashSet::new();
    for file in [path.clone(), path.with_extension("json.bak")] {
        if !guard::exists(&file)? {
            continue;
        }
        guard::managed_child(root, &file)?;
        // Refuse cleanup if either reference document is unreadable. Recovery
        // data is more important than reclaiming space on an uncertain graph.
        let pointer: Pointer = serde_json::from_slice(&fs::read(&file)?)?;
        if let Some(revision) = pointer.snapshot {
            session::safe_child(Path::new("."), &revision)?;
            keep.insert(revision);
        }
    }
    Ok(keep)
}
pub fn generated_id(name: &str) -> bool {
    let mut parts = name.split('-');
    (0..3).all(|_| {
        parts
            .next()
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
    }) && parts.next().is_none()
}
fn snapshots(root: &Path, entry: &ProjectEntry) -> Result<PathBuf> {
    project_path(root, entry)?;
    Ok(root
        .join(format!("{}.assets", entry.file))
        .join("snapshots"))
}
pub fn unused_snapshots(root: &Path, entry: &ProjectEntry) -> Result<Vec<PathBuf>> {
    let keep = retained(root, entry)?;
    let parent = snapshots(root, entry)?;
    let pending = read_intent(root, entry)?;
    if !guard::exists(&parent)? {
        return Ok(Vec::new());
    }
    guard::checked_directory(&parent)?;
    let mut unused = Vec::new();
    for item in fs::read_dir(&parent)? {
        let item = item?;
        let name = item.file_name().to_string_lossy().into_owned();
        let stem = name.strip_suffix(".pending").unwrap_or(&name);
        if (generated_id(stem) || pending.snapshots.contains(&name)) && !keep.contains(&name) {
            guard::validate_tree(&parent, &item.path())?;
            unused.push(item.path());
        }
    }
    Ok(unused)
}
pub fn prune_locked(root: &Path, entry: &ProjectEntry) -> Result<u64> {
    let mut freed = 0;
    let parent = snapshots(root, entry)?;
    for path in unused_snapshots(root, entry)? {
        let bytes = guard::tree_bytes(&parent, &path)?;
        guard::remove_tree(&parent, &path)?;
        freed += bytes;
    }
    let mut pending = read_intent(root, entry)?;
    pending.snapshots.retain(|name| parent.join(name).exists());
    write_intent(root, entry, &pending)?;
    Ok(freed)
}
pub fn prune_after_save(root: &Path, entry: &ProjectEntry) -> Result<()> {
    prune_locked(root, entry)
        .map(|_| ())
        .map_err(|error| SaveCleanupWarning(format!("{error:#}")).into())
}
#[derive(Debug)]
pub struct SaveCleanupWarning(pub String);
impl std::fmt::Display for SaveCleanupWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Saved successfully; some unused audio could not be cleaned up. Retry in Storage: {}",
            self.0
        )
    }
}
impl std::error::Error for SaveCleanupWarning {}
pub fn prune(entry: &ProjectEntry) -> Result<u64> {
    let root = super::projects_root();
    let _lock = lock_project(&root, entry)?;
    prune_locked(&root, entry)
}
pub fn synchronize_index(root: &Path, entries: &[ProjectEntry]) -> Result<()> {
    let path = root.join(INDEX_FILE);
    let backup = path.with_extension("json.bak");
    for file in [&path, &backup] {
        if guard::exists(file)? {
            guard::managed_child(root, file)?;
        }
    }
    let raw = serde_json::to_vec_pretty(&ProjectIndex {
        projects: entries.to_vec(),
    })?;
    // Recovery must never reintroduce a permanently deleted identity.
    super::atomic_write(&backup, &raw)?;
    super::atomic_write(&path, &raw)
}
pub fn delete(entry: &ProjectEntry, entries: &[ProjectEntry]) -> Result<Vec<ProjectEntry>> {
    let root = super::projects_root();
    let lock = lock_project(&root, entry)?;
    crate::replay::library::migrate(std::slice::from_ref(entry))?;
    delete_locked(&root, entry)?;
    drop(lock);
    let lock_path = project_path(&root, entry)?.with_extension("lock");
    if guard::exists(&lock_path)? {
        guard::remove_file(&root, &lock_path)?;
    }
    let remaining: Vec<_> = entries
        .iter()
        .filter(|e| !e.file.eq_ignore_ascii_case(&entry.file))
        .cloned()
        .collect();
    synchronize_index(&root, &remaining).context(
        "Project files deleted, but the project list could not be updated; retry deletion",
    )?;
    Ok(remaining)
}
fn delete_locked(root: &Path, entry: &ProjectEntry) -> Result<()> {
    let path = project_path(root, entry)?;
    let backup = path.with_extension("json.bak");
    let assets = root.join(format!("{}.assets", entry.file));
    // All boundaries are checked before the first unlink.
    for file in [&path, &backup] {
        if guard::exists(file)? {
            guard::managed_child(root, file)?;
            ensure!(
                fs::metadata(file)?.is_file(),
                "Project configuration is not a file"
            );
        }
    }
    if guard::exists(&assets)? {
        guard::validate_tree(root, &assets)?;
        guard::remove_tree(root, &assets)?;
    }
    for file in [&backup, &path] {
        if guard::exists(file)? {
            guard::remove_file(root, file)?;
        }
    }
    Ok(())
}
pub fn delete_audio(entry: &ProjectEntry) -> Result<()> {
    delete_audio_in(&super::projects_root(), entry)
}
fn delete_audio_in(root: &Path, entry: &ProjectEntry) -> Result<()> {
    let _lock = lock_project(root, entry)?;
    let path = project_path(root, entry)?;
    let snapshots = snapshots(root, entry)?;
    let names = retained(root, entry)?;
    let referenced: Vec<_> = names
        .iter()
        .map(|name| session::safe_child(&snapshots, name))
        .collect::<Result<_>>()?;
    for folder in &referenced {
        if guard::exists(folder)? {
            guard::validate_tree(&snapshots, folder)?;
        }
    }
    let mut configs = Vec::new();
    for file in [path.with_extension("json.bak"), path.clone()] {
        if !guard::exists(&file)? {
            continue;
        }
        guard::managed_child(root, &file)?;
        let mut data: ProjectData = serde_json::from_slice(&fs::read(&file)?)?;
        data.snapshot = None;
        configs.push((file, serde_json::to_vec_pretty(&data)?));
    }
    ensure!(
        configs.iter().any(|(file, _)| file == &path),
        "Project configuration is missing"
    );
    let mut pending = read_intent(root, entry)?;
    for name in names {
        if !pending.snapshots.contains(&name) {
            pending.snapshots.push(name);
        }
    }
    // This bounded record keeps explicitly owned, nonstandard revisions
    // discoverable if deletion fails after the references have been cleared.
    write_intent(root, entry, &pending)?;
    for (file, raw) in configs {
        super::atomic_write(&file, &raw)?;
    }
    prune_after_save(root, entry)
}
pub fn restore_trash(path: &Path) -> Result<ProjectEntry> {
    let root = super::projects_root();
    let trash = root.join("trash");
    guard::validate_tree(&trash, path)?;
    let entry: ProjectEntry = serde_json::from_slice(&fs::read(path.join("entry.json"))?)?;
    let destination = project_path(&root, &entry)?;
    let assets = root.join(format!("{}.assets", entry.file));
    ensure!(
        !guard::exists(&destination)? && !guard::exists(&assets)?,
        "Project identity already exists; restore will not overwrite it"
    );
    let _lock = lock_project(&root, &entry)?;
    let source = path.join(&entry.file);
    ensure!(
        guard::exists(&source)?,
        "The deleted project has no configuration to restore"
    );
    fs::rename(&source, &destination)?;
    if guard::exists(&path.join("assets"))? {
        if let Err(error) = fs::rename(path.join("assets"), &assets) {
            let _ = fs::rename(&destination, &source);
            return Err(error.into());
        }
    }
    let mut entries = super::load_index();
    if !entries
        .iter()
        .any(|e| e.file.eq_ignore_ascii_case(&entry.file))
    {
        entries.push(entry.clone());
    }
    if let Err(error) = super::save_index(&entries) {
        if assets.exists() {
            let _ = fs::rename(&assets, path.join("assets"));
        }
        let _ = fs::rename(&destination, &source);
        return Err(error).context(
            "Restore could not update the project list; retry after checking storage access",
        );
    }
    guard::remove_tree(&trash, path)
        .context("Project restored, but its empty recycle-bin entry could not be removed")?;
    Ok(entry)
}
pub fn delete_trash(path: &Path) -> Result<()> {
    delete_trash_in(&super::projects_root(), path, &super::load_index())
}
fn delete_trash_in(root: &Path, path: &Path, entries: &[ProjectEntry]) -> Result<()> {
    let trash = root.join("trash");
    guard::validate_tree(&trash, path)?;
    let entry = fs::read(path.join("entry.json"))
        .ok()
        .and_then(|raw| serde_json::from_slice::<ProjectEntry>(&raw).ok());
    let orphan = entry
        .as_ref()
        .filter(|entry| {
            !entries
                .iter()
                .any(|e| e.file.eq_ignore_ascii_case(&entry.file))
        })
        .and_then(|entry| project_path(&root, entry).ok().map(|p| (entry, p)));
    let orphan = orphan.filter(|(_, path)| !path.exists());
    let lock = orphan
        .as_ref()
        .map(|(entry, _)| lock_project(&root, entry))
        .transpose()?;
    if let Some((_, path)) = &orphan {
        let backup = path.with_extension("json.bak");
        if guard::exists(&backup)? {
            guard::managed_child(&root, &backup)?;
        }
    }
    if let Some((_, path)) = &orphan {
        let backup = path.with_extension("json.bak");
        if guard::exists(&backup)? {
            guard::remove_file(&root, &backup).context(
                "Unused configuration backup could not be removed; recycle-bin files remain",
            )?;
        }
    }
    guard::remove_tree(&trash, path)?;
    if let Some((_, path)) = orphan {
        drop(lock);
        let lock_path = path.with_extension("lock");
        if guard::exists(&lock_path)? {
            guard::remove_file(&root, &lock_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, ProjectEntry, ProjectData) {
        let root = PathBuf::from("var").join(format!("storage-project-{}", session::id()));
        fs::create_dir_all(&root).unwrap();
        let entry = ProjectEntry {
            name: "Isolated project".into(),
            file: "fixture.json".into(),
        };
        let data = crate::project::data_from_config(&crate::config::AppConfig::new(120, 0, 5));
        (root, entry, data)
    }
    fn clean(root: &Path) {
        let parent = root.parent().unwrap();
        guard::remove_tree(parent, root).unwrap();
    }
    #[test]
    fn snapshots_retain_current_and_backup_then_config_save_releases_old_audio() {
        let (root, entry, data) = fixture();
        let mut audio = crate::engine::core::AudioSnapshot::empty(48000);
        audio.tracks[0].write(
            0,
            [0.125, -0.25],
            &mut crate::engine::loop_audio::OfflinePages,
        );
        let first = session::save_snapshot_in(&root, &entry, &audio, data.clone()).unwrap();
        let second = session::save_snapshot_in(&root, &entry, &audio, data.clone()).unwrap();
        let third = session::save_snapshot_in(&root, &entry, &audio, data.clone()).unwrap();
        let snapshots = snapshots(&root, &entry).unwrap();
        assert!(!snapshots.join(first).exists());
        assert!(snapshots.join(&second).exists());
        assert!(snapshots.join(&third).exists());
        let mut current: ProjectData =
            serde_json::from_slice(&fs::read(project_path(&root, &entry).unwrap()).unwrap())
                .unwrap();
        assert_eq!(current.snapshot.as_deref(), Some(third.as_str()));
        current.track_levels[0] = 0.37;
        let lock = lock_project(&root, &entry).unwrap();
        write_locked(&root, &entry, &current).unwrap();
        prune_after_save(&root, &entry).unwrap();
        drop(lock);
        assert!(!snapshots.join(second).exists());
        assert!(snapshots.join(third).exists());
        assert_eq!(fs::read_dir(snapshots).unwrap().count(), 1);
        clean(&root);
    }
    #[test]
    fn invalid_recovery_pointer_blocks_cleanup_without_losing_saved_config() {
        let (root, entry, data) = fixture();
        write_locked(&root, &entry, &data).unwrap();
        let folder = snapshots(&root, &entry).unwrap().join(session::id());
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("audio.wav"), b"retained").unwrap();
        fs::write(
            project_path(&root, &entry)
                .unwrap()
                .with_extension("json.bak"),
            b"broken",
        )
        .unwrap();
        let error = prune_after_save(&root, &entry).unwrap_err();
        assert!(error.is::<SaveCleanupWarning>());
        assert!(folder.exists());
        assert!(
            serde_json::from_slice::<ProjectData>(
                &fs::read(project_path(&root, &entry).unwrap()).unwrap()
            )
            .is_ok()
        );
        clean(&root);
    }
    #[test]
    fn permanent_project_delete_removes_audio_backups_and_index_recovery_identity() {
        let (root, entry, data) = fixture();
        write_locked(&root, &entry, &data).unwrap();
        write_locked(&root, &entry, &data).unwrap();
        let assets = root.join(format!("{}.assets", entry.file));
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("input.wav"), b"pcm").unwrap();
        synchronize_index(&root, std::slice::from_ref(&entry)).unwrap();
        delete_locked(&root, &entry).unwrap();
        synchronize_index(&root, &[]).unwrap();
        assert!(!assets.exists());
        assert!(!project_path(&root, &entry).unwrap().exists());
        assert!(
            !project_path(&root, &entry)
                .unwrap()
                .with_extension("json.bak")
                .exists()
        );
        for name in [INDEX_FILE, "projects_index.json.bak"] {
            assert!(
                serde_json::from_slice::<ProjectIndex>(&fs::read(root.join(name)).unwrap())
                    .unwrap()
                    .projects
                    .is_empty()
            );
        }
        for file in [INDEX_FILE.to_owned(), INDEX_FILE.to_ascii_uppercase()] {
            assert!(
                project_path(
                    &root,
                    &ProjectEntry {
                        name: String::new(),
                        file
                    }
                )
                .is_err()
            );
        }
        assert!(
            project_path(
                &root,
                &ProjectEntry {
                    name: String::new(),
                    file: "../outside.json".into()
                }
            )
            .is_err()
        );
        clean(&root);
    }
    #[test]
    fn locked_project_and_unrecognized_snapshot_directories_are_preserved() {
        let (root, entry, data) = fixture();
        write_locked(&root, &entry, &data).unwrap();
        let lock = lock_project(&root, &entry).unwrap();
        assert!(lock_project(&root, &entry).is_err());
        drop(lock);
        let snapshots = snapshots(&root, &entry).unwrap();
        fs::create_dir_all(snapshots.join("personal-folder")).unwrap();
        let pending = snapshots.join(format!("{}.pending", session::id()));
        fs::create_dir_all(&pending).unwrap();
        fs::write(pending.join("audio.wav"), b"partial").unwrap();
        assert_eq!(prune_locked(&root, &entry).unwrap(), 7);
        assert!(!pending.exists());
        assert!(snapshots.join("personal-folder").exists());
        clean(&root);
    }
    #[test]
    fn delete_saved_audio_removes_referenced_named_revision_but_preserves_unknown_folder() {
        let (root, entry, mut data) = fixture();
        data.snapshot = Some("imported-audio".into());
        write_locked(&root, &entry, &data).unwrap();
        write_locked(&root, &entry, &data).unwrap();
        let snapshots = snapshots(&root, &entry).unwrap();
        for name in ["imported-audio", "personal-folder"] {
            fs::create_dir_all(snapshots.join(name)).unwrap();
            fs::write(snapshots.join(name).join("loop.wav"), b"audio").unwrap();
        }
        delete_audio_in(&root, &entry).unwrap();
        assert!(!snapshots.join("imported-audio").exists());
        assert!(snapshots.join("personal-folder").exists());
        for path in [
            project_path(&root, &entry).unwrap(),
            project_path(&root, &entry)
                .unwrap()
                .with_extension("json.bak"),
        ] {
            assert!(
                serde_json::from_slice::<ProjectData>(&fs::read(path).unwrap())
                    .unwrap()
                    .snapshot
                    .is_none()
            );
        }
        clean(&root);
    }
    #[test]
    fn failed_project_commit_removes_its_new_snapshot_bundle() {
        let (root, entry, data) = fixture();
        let path = project_path(&root, &entry).unwrap();
        fs::create_dir(&path).unwrap();
        let error = session::save_snapshot_in(
            &root,
            &entry,
            &crate::engine::core::AudioSnapshot::empty(48000),
            data,
        )
        .unwrap_err();
        assert!(!error.is::<SaveCleanupWarning>());
        assert!(path.is_dir());
        assert_eq!(
            fs::read_dir(snapshots(&root, &entry).unwrap())
                .unwrap()
                .count(),
            0
        );
        clean(&root);
    }
    #[test]
    #[cfg(windows)]
    fn named_audio_cleanup_retries_after_file_unlock_and_keeps_distinct_backup_config() {
        use std::os::windows::fs::OpenOptionsExt;
        let (root, entry, mut data) = fixture();
        data.snapshot = Some("named-import".into());
        data.track_levels[0] = 0.2;
        write_locked(&root, &entry, &data).unwrap();
        data.track_levels[0] = 0.8;
        write_locked(&root, &entry, &data).unwrap();
        let snapshot = snapshots(&root, &entry).unwrap().join("named-import");
        fs::create_dir_all(&snapshot).unwrap();
        let audio = snapshot.join("track.wav");
        fs::write(&audio, b"audio").unwrap();
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(1 | 2)
            .open(&audio)
            .unwrap();
        assert!(
            delete_audio_in(&root, &entry)
                .unwrap_err()
                .is::<SaveCleanupWarning>()
        );
        assert!(snapshot.exists());
        assert_eq!(
            read_intent(&root, &entry).unwrap().snapshots,
            vec!["named-import"]
        );
        let path = project_path(&root, &entry).unwrap();
        let current: ProjectData = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let backup: ProjectData =
            serde_json::from_slice(&fs::read(path.with_extension("json.bak")).unwrap()).unwrap();
        assert!(current.snapshot.is_none() && backup.snapshot.is_none());
        assert_eq!(current.track_levels[0], 0.8);
        assert_eq!(backup.track_levels[0], 0.2);
        assert_eq!(
            unused_snapshots(&root, &entry).unwrap(),
            vec![snapshot.clone()]
        );
        drop(held);
        prune_locked(&root, &entry).unwrap();
        assert!(!snapshot.exists());
        assert!(!intent_path(&root, &entry).unwrap().exists());
        clean(&root);
    }
    #[test]
    fn cleanup_intent_never_removes_still_referenced_or_escaped_audio() {
        let (root, entry, mut data) = fixture();
        data.snapshot = Some("retained".into());
        write_locked(&root, &entry, &data).unwrap();
        let snapshot = snapshots(&root, &entry).unwrap().join("retained");
        fs::create_dir_all(&snapshot).unwrap();
        write_intent(
            &root,
            &entry,
            &CleanupIntent {
                snapshots: vec!["retained".into()],
            },
        )
        .unwrap();
        assert!(unused_snapshots(&root, &entry).unwrap().is_empty());
        prune_locked(&root, &entry).unwrap();
        assert!(snapshot.exists());
        super::super::atomic_write(
            &intent_path(&root, &entry).unwrap(),
            br#"{"snapshots":["../outside"]}"#,
        )
        .unwrap();
        assert!(prune_locked(&root, &entry).is_err());
        assert!(snapshot.exists());
        clean(&root);
    }
    #[test]
    fn malformed_trash_identity_can_be_deleted_without_touching_project_index() {
        let (root, _, _) = fixture();
        let trash = root.join("trash/invalid");
        fs::create_dir_all(&trash).unwrap();
        let entry = ProjectEntry {
            name: "Invalid".into(),
            file: INDEX_FILE.into(),
        };
        fs::write(
            trash.join("entry.json"),
            serde_json::to_vec(&entry).unwrap(),
        )
        .unwrap();
        fs::write(root.join("projects_index.json.bak"), b"preserve index").unwrap();
        delete_trash_in(&root, &trash, &[]).unwrap();
        assert!(!trash.exists());
        assert_eq!(
            fs::read(root.join("projects_index.json.bak")).unwrap(),
            b"preserve index"
        );
        clean(&root);
    }
    #[test]
    #[cfg(windows)]
    fn occupied_orphan_backup_preserves_trash_for_retry() {
        use std::os::windows::fs::OpenOptionsExt;
        let (root, entry, data) = fixture();
        let trash = root.join("trash/old-project");
        fs::create_dir_all(&trash).unwrap();
        fs::write(
            trash.join("entry.json"),
            serde_json::to_vec(&entry).unwrap(),
        )
        .unwrap();
        fs::write(trash.join(&entry.file), serde_json::to_vec(&data).unwrap()).unwrap();
        let backup = project_path(&root, &entry)
            .unwrap()
            .with_extension("json.bak");
        fs::write(&backup, b"orphan backup").unwrap();
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(1 | 2)
            .open(&backup)
            .unwrap();
        assert!(delete_trash_in(&root, &trash, &[]).is_err());
        assert!(trash.join("entry.json").exists());
        assert!(backup.exists());
        drop(held);
        delete_trash_in(&root, &trash, &[]).unwrap();
        assert!(!trash.exists() && !backup.exists());
        clean(&root);
    }
}
