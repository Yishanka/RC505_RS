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
    fs::create_dir_all(&destination)?;
    for project in projects {
        let assets = session::project_assets(project)?;
        if assets.exists() {
            migrate_from(&assets, &destination)?;
        }
    }
    Ok(())
}

fn migrate_from(assets: &Path, destination: &Path) -> Result<()> {
    let assets = fs::canonicalize(assets)?;
    let destination = fs::canonicalize(destination)?;
    for (old, new) in [("replays", ""), ("replay-trash", "trash")] {
        let old = assets.join(old);
        if !old.exists() {
            continue;
        }
        let old = fs::canonicalize(old)?;
        ensure!(
            old.starts_with(&assets),
            "Legacy replay directory escaped project assets"
        );
        let new = destination.join(new);
        fs::create_dir_all(&new)?;
        for entry in fs::read_dir(&old)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() || entry.file_type()?.is_symlink() {
                continue;
            }
            if super::info(&entry.path()).is_ok() {
                let source = fs::canonicalize(entry.path())?;
                ensure!(
                    source.parent() == Some(old.as_path()),
                    "Invalid legacy replay path"
                );
                fs::rename(source, new.join(format!("take-migrated-{}", session::id())))?;
            } else if entry.file_name() == "exports" {
                let exports = destination.join("exports");
                fs::create_dir_all(&exports)?;
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
    ensure!(
        !fs::symlink_metadata(source)?.file_type().is_symlink(),
        "Linked library items cannot be deleted"
    );
    let parent = fs::canonicalize(parent)?;
    let source = fs::canonicalize(source)?;
    ensure!(
        source.parent() == Some(parent.as_path()),
        "Only managed library items can be deleted"
    );
    Ok(source)
}

pub fn trash(source: &Path) -> Result<()> {
    trash_in(&root(), source)
}
fn trash_in(root: &Path, source: &Path) -> Result<()> {
    let source = managed_child(root, source)?;
    super::info(&source)?;
    let trash = root.join("trash");
    fs::create_dir_all(&trash)?;
    ensure!(
        fs::canonicalize(&trash)?.starts_with(fs::canonicalize(root)?),
        "Invalid library trash path"
    );
    fs::rename(source, trash.join(session::id()))?;
    Ok(())
}

pub fn restore_last() -> Result<bool> {
    restore_in(&root())
}
fn restore_in(root: &Path) -> Result<bool> {
    let trash = root.join("trash");
    if !trash.exists() {
        return Ok(false);
    }
    ensure!(
        fs::canonicalize(&trash)?.starts_with(fs::canonicalize(root)?),
        "Invalid library trash path"
    );
    let mut items: Vec<_> = fs::read_dir(&trash)?
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink())
                && super::info(&e.path()).is_ok()
        })
        .collect();
    items.sort_by_key(|e| e.file_name());
    let Some(item) = items.pop() else {
        return Ok(false);
    };
    let source = managed_child(&trash, &item.path())?;
    fs::rename(
        source,
        root.join(format!("take-restored-{}", session::id())),
    )?;
    Ok(true)
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
    fs::remove_file(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_detaches_replay_and_exports_from_project_then_delete_restore() {
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
        assert!(trash_in(&global, &assets).is_err());
        trash_in(&global, &moved).unwrap();
        assert!(!moved.exists());
        assert!(restore_in(&global).unwrap());
        assert!(!restore_in(&global).unwrap());
        assert_eq!(fs::read_dir(global.join("exports")).unwrap().count(), 1);
        let temp = fs::canonicalize(temp).unwrap();
        assert!(temp.starts_with(fs::canonicalize("var").unwrap()));
        fs::remove_dir_all(temp).unwrap();
    }
}
