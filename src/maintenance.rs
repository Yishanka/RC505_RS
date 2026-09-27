//! Installer/verification CLI. Executes before any GUI or audio device opens.
use anyhow::{Context, Result, ensure};
use std::{fs, path::Path};
pub fn cli() -> Option<Result<()>> {
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("RC505 RS {}", env!("CARGO_PKG_VERSION"));
        return Some(Ok(()));
    }
    if let Some(source) = args.iter().find_map(|a| a.strip_prefix("--migrate-data=")) {
        return Some((|| {
            let destination =
                crate::app_support::paths::appdata_root().context("No data directory")?;
            let count = migrate(Path::new(source), &destination)?;
            println!("Imported {count} files into {}", destination.display());
            Ok(())
        })());
    }
    if args.iter().any(|a| a == "--verify-data") {
        return Some((|| {
            let entries = crate::project::load_index();
            let mut report = Vec::new();
            for entry in entries {
                let path = crate::session::safe_child(
                    &crate::app_support::paths::projects_dir(),
                    &entry.file,
                )?;
                let data = if path.exists() {
                    crate::project::load_project(&entry)?
                } else {
                    None
                };
                let snapshot = data
                    .as_ref()
                    .and_then(|d| d.snapshot.as_ref())
                    .map(|r| crate::session::load_snapshot(&entry, r))
                    .transpose()?;
                report.push(serde_json::json!({"project":entry.name,"file":entry.file,"has_config":data.is_some(),"audio_frames":snapshot.map(|s|s.tracks.map(|t|t.len))}));
            }
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        })());
    }
    if args.iter().any(|a| a == "--check-update") {
        return Some((|| {
            println!(
                "{}",
                serde_json::to_string_pretty(&crate::updater::check()?)?
            );
            Ok(())
        })());
    }
    if args.iter().any(|a| a == "--download-update") {
        return Some((|| {
            println!("{}", crate::updater::download()?.display());
            Ok(())
        })());
    }
    if let Some(installer) = args
        .iter()
        .find_map(|a| a.strip_prefix("--install-update="))
    {
        return Some(crate::updater::install_after_exit(Path::new(installer)));
    }
    None
}
pub fn migrate(source: &Path, destination: &Path) -> Result<usize> {
    let source = source.canonicalize()?;
    fs::create_dir_all(destination)?;
    let destination = destination.canonicalize()?;
    ensure!(
        source != destination && !destination.starts_with(&source),
        "Import destination must be outside the source folder"
    );
    let mut count = 0;
    for name in ["projects", "launcher_config.json"] {
        let path = source.join(name);
        if path.exists() {
            copy_verified(&path, &destination.join(name), &mut count)?;
        }
    }
    ensure!(
        count > 0 || destination.join("projects").exists(),
        "No RC505 RS data found at source"
    );
    let index = fs::read(destination.join("projects/projects_index.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
    let missing: Vec<String> = index
        .as_ref()
        .and_then(|v| v["projects"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v["file"].as_str())
        .filter(|name| !destination.join("projects").join(name).exists())
        .map(str::to_owned)
        .collect();
    crate::project::atomic_write(
        &destination.join("migration.json"),
        &serde_json::to_vec_pretty(
            &serde_json::json!({"source":source,"files_copied":count,"time":chrono::Utc::now().to_rfc3339(),"original_data_preserved":true,"missing_project_files":missing}),
        )?,
    )?;
    Ok(count)
}
fn copy_verified(source: &Path, destination: &Path, count: &mut usize) -> Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "Import does not follow links: {}",
        source.display()
    );
    if metadata.is_dir() {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_verified(&entry.path(), &destination.join(entry.file_name()), count)?;
        }
    } else {
        if source.extension().is_some_and(|ext| ext == "lock") {
            return Ok(());
        }
        if destination.exists() {
            ensure!(
                crate::session::checksum(source)? == crate::session::checksum(destination)?,
                "Import would replace existing data: {}",
                destination.display()
            );
            return Ok(());
        }
        fs::copy(source, destination)?;
        ensure!(
            crate::session::checksum(source)? == crate::session::checksum(destination)?,
            "Import verification failed"
        );
        *count += 1;
    }
    Ok(())
}
