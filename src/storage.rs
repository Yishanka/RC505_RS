//! Filesystem boundaries for explicit storage operations. No link traversal.
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub fn linked(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}
fn reject_links(path: &Path) -> Result<()> {
    ensure!(
        !path
            .components()
            .any(|part| matches!(part, Component::ParentDir)),
        "Parent traversal is not allowed in storage paths"
    );
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => ensure!(
                !linked(&metadata),
                "Linked storage paths cannot be changed: {}",
                ancestor.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Cannot inspect {}", ancestor.display()));
            }
        }
    }
    Ok(())
}
pub fn checked_directory(path: &Path) -> Result<PathBuf> {
    reject_links(path)?;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(metadata.is_dir(), "Storage root is not a directory");
    Ok(fs::canonicalize(path)?)
}
pub fn ensure_directory(path: &Path) -> Result<PathBuf> {
    reject_links(path)?;
    fs::create_dir_all(path)?;
    checked_directory(path)
}
pub fn managed_child(parent: &Path, path: &Path) -> Result<PathBuf> {
    let parent = checked_directory(parent)?;
    reject_links(path)?;
    let path = fs::canonicalize(path)?;
    ensure!(
        path.parent() == Some(parent.as_path()),
        "Only direct items in the selected storage folder can be changed"
    );
    Ok(path)
}
pub fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
/// Validate the complete tree before mutating it. Return files first and
/// directories in postorder, with root metadata removed last for retryability.
fn tree(parent: &Path, path: &Path) -> Result<(PathBuf, Vec<PathBuf>, Vec<PathBuf>, u64)> {
    let root = managed_child(parent, path)?;
    ensure!(
        fs::metadata(&root)?.is_dir(),
        "Storage item is not a directory"
    );
    let mut stack = vec![root.clone()];
    let mut directories = Vec::new();
    let mut files = Vec::new();
    let mut bytes = 0u64;
    while let Some(directory) = stack.pop() {
        ensure!(
            directory.starts_with(&root),
            "Storage tree escaped its root"
        );
        directories.push(directory.clone());
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                !linked(&metadata),
                "Linked items cannot be deleted: {}",
                path.display()
            );
            ensure!(
                fs::canonicalize(&path)?.parent() == Some(directory.as_path()),
                "Storage item escaped its directory"
            );
            if metadata.is_dir() {
                stack.push(path);
            } else if metadata.is_file() {
                bytes = bytes.saturating_add(metadata.len());
                files.push(path);
            } else {
                anyhow::bail!("Unsupported storage item: {}", path.display());
            }
        }
    }
    files.sort_by_key(|path| {
        matches!(
            path.file_name().and_then(|n| n.to_str()),
            Some("replay.json" | "entry.json")
        )
    });
    directories.reverse();
    Ok((root, files, directories, bytes))
}
pub fn tree_bytes(parent: &Path, path: &Path) -> Result<u64> {
    Ok(tree(parent, path)?.3)
}
pub fn validate_tree(parent: &Path, path: &Path) -> Result<()> {
    tree(parent, path).map(|_| ())
}
pub fn remove_file(parent: &Path, path: &Path) -> Result<()> {
    let path = managed_child(parent, path)?;
    ensure!(
        fs::symlink_metadata(&path)?.is_file(),
        "Storage item is not a file"
    );
    fs::remove_file(&path).with_context(|| format!("File could not be deleted: {}", path.display()))
}
pub fn remove_tree(parent: &Path, path: &Path) -> Result<()> {
    let (root, files, directories, _) = tree(parent, path)?;
    // Metadata is last, so an interrupted removal remains identifiable and can
    // be retried. Each unlink also rechecks links introduced after preflight.
    let mut by_directory = std::collections::HashMap::<PathBuf, Vec<PathBuf>>::new();
    for file in files {
        by_directory
            .entry(file.parent().context("Missing file parent")?.to_owned())
            .or_default()
            .push(file);
    }
    for directory in directories {
        if let Some(files) = by_directory.remove(&directory) {
            for file in files {
                remove_file(&directory, &file)?;
            }
        }
        reject_links(&directory)?;
        ensure!(
            fs::canonicalize(&directory)?.starts_with(&root),
            "Storage directory escaped its root"
        );
        fs::remove_dir(&directory)
            .with_context(|| format!("Directory could not be deleted: {}", directory.display()))?;
    }
    Ok(())
}

#[derive(Clone)]
pub enum ItemKind {
    ProjectTrash,
    ReplayTrash,
    IncompleteReplay,
}
#[derive(Clone)]
pub struct Item {
    pub path: PathBuf,
    pub name: String,
    pub bytes: u64,
    pub kind: ItemKind,
    pub restorable: bool,
}
pub struct ProjectUsage {
    pub entry: crate::project::ProjectEntry,
    pub bytes: u64,
    pub unused: u64,
}
#[derive(Default)]
pub struct Inventory {
    pub projects: Vec<ProjectUsage>,
    pub items: Vec<Item>,
    pub errors: Vec<String>,
}
pub fn inventory(entries: &[crate::project::ProjectEntry]) -> Inventory {
    let mut result = Inventory::default();
    let projects = crate::app_support::paths::projects_dir();
    let replays = crate::replay::library::root();
    for entry in entries {
        let stats = (|| -> Result<ProjectUsage> {
            let _lock = crate::project::storage::lock_project(&projects, entry)?;
            let assets = crate::session::project_assets(entry)?;
            let snapshots = assets.join("snapshots");
            let bytes = if exists(&snapshots)? {
                tree_bytes(&assets, &snapshots)?
            } else {
                0
            };
            let mut unused = 0;
            for path in crate::project::storage::unused_snapshots(&projects, entry)? {
                unused += tree_bytes(&snapshots, &path)?;
            }
            Ok(ProjectUsage {
                entry: entry.clone(),
                bytes,
                unused,
            })
        })();
        match stats {
            Ok(stats) => result.projects.push(stats),
            Err(error) => result.errors.push(format!("{}: {error}", entry.name)),
        }
    }
    for (parent, kind) in [
        (projects.join("trash"), ItemKind::ProjectTrash),
        (replays.join("trash"), ItemKind::ReplayTrash),
        (replays, ItemKind::IncompleteReplay),
    ] {
        if !parent.exists() {
            continue;
        }
        let scan = (|| -> Result<()> {
            checked_directory(&parent)?;
            for item in fs::read_dir(&parent)? {
                let item = item?;
                let path = item.path();
                let metadata = fs::symlink_metadata(&path)?;
                if !metadata.is_dir() || linked(&metadata) {
                    continue;
                }
                let name = item.file_name().to_string_lossy().into_owned();
                let (name, restorable) = match kind {
                    ItemKind::IncompleteReplay => {
                        if !crate::replay::library::generated_recording(&path)
                            || crate::replay::info(&path).is_ok()
                        {
                            continue;
                        }
                        (name, false)
                    }
                    ItemKind::ReplayTrash => {
                        let replay = crate::replay::info(&path).ok();
                        (
                            replay.as_ref().map_or(name, |i| i.name.clone()),
                            replay.is_some(),
                        )
                    }
                    ItemKind::ProjectTrash => {
                        let entry = fs::read(path.join("entry.json")).ok().and_then(|raw| {
                            serde_json::from_slice::<crate::project::ProjectEntry>(&raw).ok()
                        });
                        let restorable = entry.as_ref().is_some_and(|e| {
                            crate::session::safe_child(&path, &e.file).is_ok_and(|p| p.is_file())
                        });
                        (entry.map_or(name, |e| e.name), restorable)
                    }
                };
                match tree_bytes(&parent, &path) {
                    Ok(bytes) => result.items.push(Item {
                        path,
                        name,
                        bytes,
                        kind: kind.clone(),
                        restorable,
                    }),
                    Err(error) => result.errors.push(error.to_string()),
                }
            }
            Ok(())
        })();
        if let Err(error) = scan {
            result.errors.push(error.to_string());
        }
    }
    result
}
pub fn delete_item(item: &Item) -> Result<()> {
    if matches!(item.kind, ItemKind::ProjectTrash) {
        return crate::project::storage::delete_trash(&item.path);
    }
    let parent = match item.kind {
        ItemKind::ProjectTrash => crate::app_support::paths::projects_dir().join("trash"),
        ItemKind::ReplayTrash => crate::replay::library::root().join("trash"),
        ItemKind::IncompleteReplay => crate::replay::library::root(),
    };
    if matches!(item.kind, ItemKind::IncompleteReplay) {
        ensure!(
            crate::replay::library::generated_recording(&item.path)
                && crate::replay::info(&item.path).is_err(),
            "Recording changed; refresh the storage list"
        );
    }
    remove_tree(&parent, &item.path)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> PathBuf {
        let root = PathBuf::from("var").join(format!("storage-boundary-{}", crate::session::id()));
        fs::create_dir_all(&root).unwrap();
        root
    }
    #[test]
    fn delete_frees_nested_files_and_refuses_parent_or_sibling_paths() {
        let root = fixture();
        let managed = root.join("managed");
        let outside = root.join("outside");
        fs::create_dir_all(managed.join("take/audio")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep.wav"), b"keep").unwrap();
        fs::write(managed.join("take/audio/input.wav"), b"recording").unwrap();
        fs::write(managed.join("take/replay.json"), b"meta").unwrap();
        assert_eq!(tree_bytes(&managed, &managed.join("take")).unwrap(), 13);
        assert!(remove_tree(&managed, &outside).is_err());
        assert!(remove_tree(&managed, &managed).is_err());
        assert!(remove_tree(&managed, &managed.join("../outside")).is_err());
        remove_tree(&managed, &managed.join("take")).unwrap();
        assert!(!managed.join("take").exists());
        assert!(outside.join("keep.wav").exists());
        remove_tree(root.parent().unwrap(), &root).unwrap();
    }
    #[test]
    #[cfg(windows)]
    fn junction_preflight_rejects_entire_tree_before_deletion_or_directory_creation() {
        use std::os::windows::process::CommandExt;
        let root = fixture();
        let managed = root.join("managed");
        let outside = root.join("outside");
        fs::create_dir_all(managed.join("take")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(managed.join("take/replay.json"), b"keep metadata").unwrap();
        fs::write(outside.join("keep.wav"), b"keep").unwrap();
        let link = fs::canonicalize(&managed).unwrap().join("take/link");
        let destination = fs::canonicalize(&outside).unwrap();
        let status = std::process::Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&destination)
            .creation_flags(0x08000000)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        assert!(remove_tree(&managed, &managed.join("take")).is_err());
        assert!(managed.join("take/replay.json").exists());
        assert!(outside.join("keep.wav").exists());
        assert!(ensure_directory(&link.join("must-not-create")).is_err());
        assert!(!outside.join("must-not-create").exists());
        // Remove only the known junction itself, never its destination.
        fs::remove_dir(&link).unwrap();
        remove_tree(root.parent().unwrap(), &root).unwrap();
    }
    #[test]
    #[cfg(windows)]
    fn interrupted_delete_reports_failure_keeps_metadata_and_can_be_retried() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = fixture();
        let item = root.join("take");
        fs::create_dir_all(item.join("initial")).unwrap();
        fs::write(item.join("replay.json"), b"metadata").unwrap();
        let audio = item.join("initial/track.wav");
        fs::write(&audio, b"audio").unwrap();
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(1 | 2)
            .open(&audio)
            .unwrap();
        assert!(remove_tree(&root, &item).is_err());
        assert!(item.join("replay.json").exists());
        assert!(audio.exists());
        drop(held);
        remove_tree(&root, &item).unwrap();
        assert!(!item.exists());
        remove_tree(root.parent().unwrap(), &root).unwrap();
    }
}
