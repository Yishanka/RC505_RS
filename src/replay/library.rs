//! The replay library is independent of projects. Legacy project recordings are
//! moved, never re-rendered, and deleting a project cannot remove its replays.
use super::*;

pub fn root() -> PathBuf {
    crate::app_support::paths::appdata_root()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("replays")
}

pub fn migrate(projects: &[crate::project::ProjectEntry]) -> Result<()> {
    let destination = root();
    crate::storage::ensure_directory(&destination)?;
    for project in projects {
        let assets = session::project_assets(project)?;
        if assets.exists() {
            migrate_from(&assets, &destination)?;
        }
    }
    Ok(())
}

fn migrate_from(assets: &Path, destination: &Path) -> Result<()> {
    let assets = crate::storage::checked_directory(assets)?;
    let destination = crate::storage::checked_directory(destination)?;
    for (old, new) in [("replays", ""), ("replay-trash", "trash")] {
        let old = assets.join(old);
        if !old.exists() {
            continue;
        }
        let old = crate::storage::checked_directory(&old)?;
        ensure!(
            old.starts_with(&assets),
            "Legacy replay directory escaped project assets"
        );
        let new = destination.join(new);
        crate::storage::ensure_directory(&new)?;
        for entry in fs::read_dir(&old)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() || entry.file_type()?.is_symlink() {
                continue;
            }
            if super::info(&entry.path()).is_ok() {
                let source = fs::canonicalize(entry.path())?;
                crate::storage::validate_tree(&old, &source)?;
                ensure!(
                    source.parent() == Some(old.as_path()),
                    "Invalid legacy replay path"
                );
                fs::rename(source, new.join(format!("take-migrated-{}", session::id())))?;
            } else if entry.file_name() == "exports" {
                let source = crate::storage::managed_child(&old, &entry.path())?;
                crate::storage::validate_tree(&old, &source)?;
                let exports = destination.join("exports");
                crate::storage::ensure_directory(&exports)?;
                for file in fs::read_dir(entry.path())? {
                    let file = file?;
                    if file.file_type()?.is_file()
                        && file
                            .path()
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
                    {
                        fs::rename(
                            file.path(),
                            exports.join(format!("legacy-{}.wav", session::id())),
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn list() -> Vec<(PathBuf, String)> {
    let Ok(entries) = fs::read_dir(root()) else {
        return Vec::new();
    };
    let mut items: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            super::info(&entry.path())
                .ok()
                .map(|info| (entry.path(), info.name))
        })
        .collect();
    items.sort_by(|a, b| b.0.cmp(&a.0));
    items
}

fn managed_child(parent: &Path, source: &Path) -> Result<PathBuf> {
    crate::storage::managed_child(parent, source)
}

pub fn generated_recording(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| {
            ["draft-", "take-", "take-migrated-", "take-restored-"]
                .iter()
                .any(|prefix| {
                    name.strip_prefix(prefix)
                        .is_some_and(crate::project::storage::generated_id)
                })
        })
}
pub fn delete(source: &Path) -> Result<()> {
    delete_in(&root(), source)
}
fn delete_in(root: &Path, source: &Path) -> Result<()> {
    let source = managed_child(root, source)?;
    ensure!(
        !matches!(
            source.file_name().and_then(|n| n.to_str()),
            Some("trash" | "exports")
        ),
        "Library storage directories cannot be deleted as recordings"
    );
    ensure!(
        super::info(&source).is_ok() || generated_recording(&source),
        "Only recorded replay folders can be deleted"
    );
    crate::storage::remove_tree(root, &source)
}
pub fn restore(source: &Path) -> Result<()> {
    let root = root();
    let trash = root.join("trash");
    crate::storage::validate_tree(&trash, source)?;
    super::info(source)?;
    fs::rename(
        source,
        root.join(format!("take-restored-{}", session::id())),
    )?;
    Ok(())
}

pub fn exports() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root().join("exports")) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .flatten()
        .filter(|e| {
            e.file_type().is_ok_and(|t| t.is_file())
                && e.path()
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
        })
        .map(|e| e.path())
        .collect();
    paths.sort();
    paths.reverse();
    paths
}

pub fn delete_export(path: &Path) -> Result<()> {
    let path = managed_child(&root().join("exports"), path)?;
    ensure!(
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("wav")),
        "Only exported WAV files can be removed"
    );
    crate::storage::remove_file(&root().join("exports"), &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_detaches_replay_and_exports_then_permanent_delete() {
        let temp = PathBuf::from("var").join(format!("global-replay-{}", session::id()));
        let assets = temp.join("project.assets");
        let replay = assets.join("replays/draft-old");
        let global = temp.join("library");
        fs::create_dir_all(&replay).unwrap();
        fs::create_dir_all(&global).unwrap();
        let metadata = ReplayInfo {
            version: 1,
            renderer: RENDERER_VERSION,
            project_id: "old.json".into(),
            name: "Retained".into(),
            sample_rate: 48000,
            frames: 0,
            input_sha256: String::new(),
            events_sha256: String::new(),
            initial_sha256: String::new(),
        };
        fs::write(
            replay.join("replay.json"),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
        fs::write(replay.join("input.wav"), b"exact dry input").unwrap();
        fs::create_dir_all(assets.join("replays/exports")).unwrap();
        fs::write(assets.join("replays/exports/old.wav"), b"old wav").unwrap();
        migrate_from(&assets, &global).unwrap();
        migrate_from(&assets, &global).unwrap();
        assert!(!replay.exists());
        let moved = fs::read_dir(&global)
            .unwrap()
            .flatten()
            .find(|e| super::super::info(&e.path()).is_ok())
            .unwrap()
            .path();
        assert_eq!(
            fs::read(moved.join("input.wav")).unwrap(),
            b"exact dry input"
        );
        assert!(delete_in(&global, &assets).is_err());
        delete_in(&global, &moved).unwrap();
        assert!(!moved.exists());
        assert!(!global.join("trash").exists());
        assert_eq!(fs::read_dir(global.join("exports")).unwrap().count(), 1);
        let temp = fs::canonicalize(temp).unwrap();
        assert!(temp.starts_with(fs::canonicalize("var").unwrap()));
        fs::remove_dir_all(temp).unwrap();
    }
    #[test]
    fn incomplete_generated_recording_can_be_deleted_but_other_folders_are_protected() {
        let temp = PathBuf::from("var").join(format!("replay-delete-{}", session::id()));
        fs::create_dir_all(&temp).unwrap();
        let draft = temp.join(format!("draft-{}", session::id()));
        fs::create_dir_all(&draft).unwrap();
        fs::write(draft.join("input.wav"), b"unfinished pcm").unwrap();
        let personal = temp.join("my-recordings");
        fs::create_dir_all(&personal).unwrap();
        fs::write(personal.join("keep.wav"), b"keep").unwrap();
        assert!(delete_in(&temp, &personal).is_err());
        delete_in(&temp, &draft).unwrap();
        assert!(!draft.exists());
        assert!(personal.join("keep.wav").exists());
        crate::storage::remove_tree(temp.parent().unwrap(), &temp).unwrap();
    }
}
