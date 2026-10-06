//! Presets own their embedded PCM. Library deletion never creates a second copy.
use super::*;
use crate::storage::linked;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum DeleteSoundResult {
    Deleted,
    UsedBy(Vec<String>),
}
pub type DeleteJob = std::sync::mpsc::Receiver<Result<DeleteSoundResult>>;
fn current_references(current: &AppConfig) -> Vec<crate::config::osc_configs::SavedSampleRef> {
    current
        .input_fx
        .banks
        .iter()
        .flat_map(|b| &b.slots)
        .filter_map(|slot| {
            let Some(crate::config::InputFx::Oscillator(osc)) = &slot.fx else {
                return None;
            };
            if osc.sample_temporary {
                None
            } else {
                osc.sample_ref.clone()
            }
        })
        .collect()
}
pub fn start_delete_sound(name: &str, current: &AppConfig) -> Result<DeleteJob> {
    let path = file(name)?;
    let library = root();
    let projects = crate::app_support::paths::projects_dir();
    let references = current_references(current);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("delete-sound".into())
        .spawn(move || {
            let _ = tx.send(delete_sound_with_refs(
                &library,
                &projects,
                &path,
                &references,
            ));
        })?;
    Ok(rx)
}
pub fn start_delete_clip(name: &str) -> Result<DeleteJob> {
    let path = clip_file(name)?;
    let library = clip_root();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("delete-phrase".into())
        .spawn(move || {
            let _ = tx.send(
                crate::storage::remove_file(&library, &path).map(|_| DeleteSoundResult::Deleted),
            );
        })?;
    Ok(rx)
}
#[cfg(test)]
fn delete_sound_in(
    library: &Path,
    projects: &Path,
    path: &Path,
    current: Option<&AppConfig>,
) -> Result<DeleteSoundResult> {
    delete_sound_with_refs(
        library,
        projects,
        path,
        &current.map(current_references).unwrap_or_default(),
    )
}
fn reference_matches(reference: &serde_json::Value, name: &str, checksum: &str) -> bool {
    reference
        .get("preset")
        .and_then(|v| v.as_str())
        .is_some_and(|v| v.to_lowercase() == name.to_lowercase())
        && reference
            .get("sha256")
            .and_then(|v| v.as_str())
            .is_some_and(|v| v.eq_ignore_ascii_case(checksum))
}
fn external_reference(value: &serde_json::Value, name: &str, checksum: &str) -> bool {
    match value {
        serde_json::Value::Object(object) => {
            (object.get("sample").is_none_or(|v| v.is_null())
                && object.get("sample_temporary").and_then(|v| v.as_bool()) != Some(true)
                && object
                    .get("sample_ref")
                    .is_some_and(|r| reference_matches(r, name, checksum)))
                || object
                    .values()
                    .any(|v| external_reference(v, name, checksum))
        }
        serde_json::Value::Array(array) => {
            array.iter().any(|v| external_reference(v, name, checksum))
        }
        _ => false,
    }
}
fn scan_references(
    root: &Path,
    directory: &Path,
    target: &Path,
    name: &str,
    checksum: &str,
    owners: &mut Vec<String>,
) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    anyhow::ensure!(
        !linked(&fs::symlink_metadata(directory)?),
        "Cannot inspect a linked library directory: {}",
        directory.display()
    );
    anyhow::ensure!(
        directory.strip_prefix(root)?.components().count() <= 32,
        "Library folders are too deeply nested"
    );
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        // These are self-contained replay stores, not project/sample references.
        if matches!(entry.file_name().to_str(), Some("replays" | "replay-trash")) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        anyhow::ensure!(
            !linked(&metadata),
            "Cannot inspect a linked library item: {}",
            path.display()
        );
        if metadata.is_dir() {
            scan_references(root, &path, target, name, checksum, owners)?;
        } else if path != target
            && metadata.is_file()
            && path.file_name().is_some_and(|v| {
                let name = v.to_string_lossy();
                name.ends_with(".json") || name.ends_with(".json.bak")
            })
        {
            anyhow::ensure!(
                metadata.len() <= 64 * 1024 * 1024,
                "Cannot inspect an oversized library item: {}",
                path.display()
            );
            let value: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)
                .with_context(|| format!("Cannot inspect {}", path.display()))?;
            if external_reference(&value, name, checksum) {
                owners.push(
                    path.strip_prefix(root)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                );
            }
        }
    }
    Ok(())
}
fn delete_sound_with_refs(
    library: &Path,
    projects: &Path,
    path: &Path,
    references: &[crate::config::osc_configs::SavedSampleRef],
) -> Result<DeleteSoundResult> {
    let canonical = crate::storage::managed_child(library, path)?;
    let library_root = crate::storage::checked_directory(library)?;
    if crate::storage::exists(projects)? {
        crate::storage::checked_directory(projects)?;
    }
    anyhow::ensure!(
        canonical.parent() == Some(library_root.as_path()) && !linked(&fs::symlink_metadata(path)?),
        "Sound must be a file in the sound library"
    );
    let name = path
        .file_stem()
        .and_then(|v| v.to_str())
        .context("Invalid sound name")?;
    anyhow::ensure!(
        fs::metadata(path)?.len() <= 64 * 1024 * 1024,
        "Sound preset exceeds the size limit"
    );
    let bytes = fs::read(path)?;
    let checksum = format!("{:x}", Sha256::digest(&bytes));
    let mut owners = Vec::new();
    if references.iter().any(|r| {
        r.preset.to_lowercase() == name.to_lowercase() && r.sha256.eq_ignore_ascii_case(&checksum)
    }) {
        owners.push("Current project".into());
    }
    scan_references(projects, projects, path, name, &checksum, &mut owners)?;
    scan_references(library, library, path, name, &checksum, &mut owners)?;
    owners.sort();
    owners.dedup();
    if !owners.is_empty() {
        return Ok(DeleteSoundResult::UsedBy(owners));
    }
    crate::storage::remove_file(library, path)?;
    Ok(DeleteSoundResult::Deleted)
}

#[cfg(test)]
#[path = "library_tests.rs"]
mod tests;
