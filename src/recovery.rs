//! Optional crash recovery, independent of explicit project/sound saves.
//! All filesystem methods run on a worker. Only ticket/invalidate are UI-safe.
use crate::{
    engine::core::AudioSnapshot,
    project::{self, ProjectData, ProjectEntry},
    session, storage,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const VERSION: u32 = 1;
const SLOTS: [&str; 2] = ["slot-a", "slot-b"];
const MAX_MANIFEST_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Info {
    pub version: u32,
    pub project_file: String,
    pub project_name: String,
    pub generation: u64,
    /// Time the checkpoint request was created, not a claim of latest audio.
    pub captured_unix_ms: u64,
    pub saved_unix_ms: u64,
    pub frame: u64,
    pub sample_rate: u32,
    epoch: String,
    slot: usize,
    manifest_sha256: String,
}
#[derive(Clone, Debug)]
pub enum WriteOutcome {
    Saved(Info),
    Superseded,
    /// New checkpoint is durable; the previous fixed slot needs a cleanup retry.
    SavedCleanupWarning(Info, String),
}
#[derive(Serialize, Deserialize)]
struct Owner {
    version: u32,
    project_file: String,
    epoch: String,
}
#[derive(Serialize, Deserialize)]
struct SlotOwner {
    version: u32,
    project_file: String,
    slot: usize,
}
struct Shared {
    root: PathBuf,
    entry: ProjectEntry,
    epoch: String,
    generation: AtomicU64,
    begun: AtomicBool,
}
#[derive(Clone)]
pub struct Store(Arc<Shared>);
impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryStore")
            .field("project", &self.0.entry.file)
            .field("root", &self.0.root)
            .finish()
    }
}
#[derive(Clone)]
pub struct Ticket {
    shared: Arc<Shared>,
    generation: u64,
    captured_unix_ms: u64,
}
impl std::fmt::Debug for Ticket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryTicket")
            .field("project", &self.shared.entry.file)
            .field("generation", &self.generation)
            .finish()
    }
}
impl Ticket {
    pub fn generation(&self) -> u64 {
        self.generation
    }
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
impl Store {
    /// Creates an identity only. Does not create folders or inspect user files.
    pub fn new(entry: ProjectEntry) -> Result<Self> {
        Self::in_root(crate::app_support::paths::projects_dir(), entry)
    }
    fn in_root(projects: PathBuf, entry: ProjectEntry) -> Result<Self> {
        project::storage::project_path(&projects, &entry)?;
        Ok(Self(Arc::new(Shared {
            root: projects
                .join(format!("{}.assets", entry.file))
                .join("recovery"),
            entry,
            epoch: session::id(),
            generation: AtomicU64::new(0),
            begun: AtomicBool::new(false),
        })))
    }
    pub fn entry(&self) -> &ProjectEntry {
        &self.0.entry
    }
    pub fn ticket(&self) -> Ticket {
        self.ticket_at(now_ms())
    }
    pub fn ticket_at(&self, captured_unix_ms: u64) -> Ticket {
        Ticket {
            shared: self.0.clone(),
            generation: self.0.generation.fetch_add(1, Ordering::AcqRel) + 1,
            captured_unix_ms,
        }
    }
    /// Cancel pending saves before queueing clear or leaving this project. The
    /// caller waits for clear's successful worker result before completing an
    /// explicit discard; merely invalidating memory is not a durable discard.
    pub fn invalidate(&self) -> Ticket {
        self.ticket()
    }
    fn current(&self, ticket: &Ticket) -> bool {
        Arc::ptr_eq(&self.0, &ticket.shared)
            && ticket.generation == self.0.generation.load(Ordering::Acquire)
    }
    fn lock(&self) -> Result<fs::File> {
        use fs2::FileExt;
        storage::ensure_directory(&self.0.root)?;
        let path = self.0.root.join("recovery.lock");
        if storage::exists(&path)? {
            storage::managed_child(&self.0.root, &path)?;
        }
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        file.lock_exclusive()?;
        Ok(file)
    }
    fn read_json<T: serde::de::DeserializeOwned>(&self, path: &Path, limit: u64) -> Result<T> {
        storage::managed_child(path.parent().context("Missing recovery parent")?, path)?;
        ensure!(
            fs::metadata(path)?.len() <= limit,
            "Recovery metadata is oversized"
        );
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }
    fn read_head(&self) -> Result<Option<Info>> {
        let path = self.0.root.join("head.json");
        if !storage::exists(&path)? {
            return Ok(None);
        }
        let info: Info = self.read_json(&path, 65536)?;
        ensure!(info.version == VERSION, "Unsupported recovery version");
        ensure!(
            info.project_file.eq_ignore_ascii_case(&self.0.entry.file),
            "Recovery belongs to another project"
        );
        ensure!(
            info.slot < SLOTS.len() && (8000..=192000).contains(&info.sample_rate),
            "Invalid recovery timing or slot"
        );
        ensure!(
            info.manifest_sha256.len() == 64
                && info.manifest_sha256.bytes().all(|v| v.is_ascii_hexdigit()),
            "Invalid recovery checksum"
        );
        Ok(Some(info))
    }
    fn owns_epoch(&self) -> Result<bool> {
        let path = self.0.root.join("owner.json");
        if !storage::exists(&path)? {
            return Ok(false);
        }
        let owner: Owner = self.read_json(&path, 65536)?;
        ensure!(
            owner.version == VERSION && owner.project_file.eq_ignore_ascii_case(&self.0.entry.file),
            "Invalid recovery session owner"
        );
        Ok(owner.epoch == self.0.epoch)
    }
    /// Claim this editing session without modifying a previously saved recovery
    /// head. The UI must resolve the returned checkpoint before periodic saves.
    pub fn begin(&self) -> Result<Option<Info>> {
        let _lock = self.lock()?;
        ensure!(
            self.0.begun.load(Ordering::Acquire) || self.0.generation.load(Ordering::Acquire) == 0,
            "Recovery session was cancelled before opening"
        );
        if self.0.begun.load(Ordering::Acquire) {
            ensure!(
                self.owns_epoch()?,
                "Recovery session was replaced by a newer session"
            );
            return self.read_head();
        }
        let owner = Owner {
            version: VERSION,
            project_file: self.0.entry.file.clone(),
            epoch: self.0.epoch.clone(),
        };
        project::atomic_write(
            &self.0.root.join("owner.json"),
            &serde_json::to_vec(&owner)?,
        )?;
        self.0.begun.store(true, Ordering::Release);
        self.read_head()
    }
    pub fn inspect(&self) -> Result<Option<Info>> {
        if !storage::exists(&self.0.root)? {
            return Ok(None);
        }
        let _lock = self.lock()?;
        self.read_head()
    }
    fn slot_path(&self, slot: usize) -> PathBuf {
        self.0.root.join(SLOTS[slot])
    }
    fn validate_slot(&self, slot: usize) -> Result<()> {
        let path = self.slot_path(slot);
        storage::validate_tree(&self.0.root, &path)?;
        let marker = path.join("entry.json");
        let owner: SlotOwner = self.read_json(&marker, 65536)?;
        ensure!(
            owner.version == VERSION
                && owner.slot == slot
                && owner.project_file.eq_ignore_ascii_case(&self.0.entry.file),
            "Recovery slot identity does not match this project"
        );
        Ok(())
    }
    fn remove_slot(&self, slot: usize) -> Result<()> {
        let path = self.slot_path(slot);
        if !storage::exists(&path)? {
            return Ok(());
        }
        self.validate_removal(slot)?;
        storage::remove_tree(&self.0.root, &path)
    }
    fn validate_removal(&self, slot: usize) -> Result<()> {
        let path = self.slot_path(slot);
        storage::validate_tree(&self.0.root, &path)?;
        if fs::read_dir(&path)?.next().is_none() {
            return Ok(());
        }
        if storage::exists(&path.join("entry.json"))? {
            return self.validate_slot(slot);
        }
        // The stage intent is committed before mkdir, so an interrupted atomic
        // entry.json write remains identifiable without adopting unknown files.
        let stage: SlotOwner = self.read_json(&self.0.root.join("staging.json"), 65536)?;
        ensure!(
            stage.version == VERSION
                && stage.slot == slot
                && stage.project_file.eq_ignore_ascii_case(&self.0.entry.file),
            "Incomplete recovery slot has no matching ownership record"
        );
        Ok(())
    }
    fn remove_stage(&self) -> Result<()> {
        let path = self.0.root.join("staging.json");
        if !storage::exists(&path)? {
            return Ok(());
        }
        let stage: SlotOwner = self.read_json(&path, 65536)?;
        ensure!(
            stage.version == VERSION
                && stage.slot < SLOTS.len()
                && stage.project_file.eq_ignore_ascii_case(&self.0.entry.file),
            "Invalid recovery staging identity"
        );
        storage::remove_file(&self.0.root, &path)
    }
    pub fn save(
        &self,
        ticket: &Ticket,
        snapshot: &AudioSnapshot,
        data: &ProjectData,
    ) -> Result<WriteOutcome> {
        if !self.current(ticket) {
            return Ok(WriteOutcome::Superseded);
        }
        let _lock = self.lock()?;
        if !self.current(ticket) || !self.owns_epoch()? {
            return Ok(WriteOutcome::Superseded);
        }
        ensure!(
            (8000..=192000).contains(&snapshot.sample_rate),
            "Invalid recovery sample rate"
        );
        let previous = self.read_head()?;
        let slot = previous.as_ref().map_or(0, |head| 1 - head.slot);
        self.remove_slot(slot)?;
        let path = self.slot_path(slot);
        let marker = SlotOwner {
            version: VERSION,
            project_file: self.0.entry.file.clone(),
            slot,
        };
        project::atomic_write(
            &self.0.root.join("staging.json"),
            &serde_json::to_vec(&marker)?,
        )?;
        storage::ensure_directory(&path)?;
        project::atomic_write(&path.join("entry.json"), &serde_json::to_vec(&marker)?)?;
        let writing = session::write_bundle_cancellable(&path, snapshot, data.clone(), &|| {
            !self.current(ticket)
        });
        if let Err(error) = writing {
            self.remove_slot(slot).context(format!(
                "Checkpoint not saved ({error}); incomplete audio cleanup failed"
            ))?;
            if !self.current(ticket) {
                return Ok(WriteOutcome::Superseded);
            }
            return Err(error);
        }
        let manifest = path.join("manifest.json");
        if fs::metadata(&manifest)?.len() > MAX_MANIFEST_BYTES {
            self.remove_slot(slot)?;
            anyhow::bail!("Recovery configuration exceeds the supported size");
        }
        if !self.current(ticket) {
            self.remove_slot(slot)?;
            return Ok(WriteOutcome::Superseded);
        }
        let info = Info {
            version: VERSION,
            project_file: self.0.entry.file.clone(),
            project_name: self.0.entry.name.clone(),
            generation: ticket.generation,
            captured_unix_ms: ticket.captured_unix_ms,
            saved_unix_ms: now_ms(),
            frame: snapshot.at,
            sample_rate: snapshot.sample_rate,
            epoch: self.0.epoch.clone(),
            slot,
            manifest_sha256: session::checksum(&manifest)?,
        };
        if !self.current(ticket) {
            self.remove_slot(slot)?;
            return Ok(WriteOutcome::Superseded);
        }
        if let Err(error) =
            project::atomic_write(&self.0.root.join("head.json"), &serde_json::to_vec(&info)?)
        {
            self.remove_slot(slot)?;
            return Err(error);
        }
        if !self.current(ticket) {
            if let Some(previous) = &previous {
                project::atomic_write(
                    &self.0.root.join("head.json"),
                    &serde_json::to_vec(previous)?,
                )?;
            } else {
                storage::remove_file(&self.0.root, &self.0.root.join("head.json"))?;
            }
            self.remove_slot(slot)?;
            return Ok(WriteOutcome::Superseded);
        }
        if let Some(previous) = previous {
            if let Err(error) = self.remove_slot(previous.slot) {
                return Ok(WriteOutcome::SavedCleanupWarning(
                    info,
                    format!("{error:#}"),
                ));
            }
        }
        if let Err(error) = self.remove_stage() {
            return Ok(WriteOutcome::SavedCleanupWarning(
                info,
                format!("{error:#}"),
            ));
        }
        Ok(WriteOutcome::Saved(info))
    }
    pub fn load(&self, info: &Info) -> Result<(AudioSnapshot, ProjectData)> {
        let _lock = self.lock()?;
        ensure!(
            self.read_head()?.as_ref() == Some(info),
            "Recovery checkpoint changed; refresh before opening"
        );
        self.validate_slot(info.slot)?;
        let path = self.slot_path(info.slot);
        let manifest = path.join("manifest.json");
        ensure!(
            fs::metadata(&manifest)?.len() <= MAX_MANIFEST_BYTES,
            "Recovery configuration is oversized"
        );
        ensure!(
            session::checksum(&manifest)? == info.manifest_sha256,
            "Recovery configuration checksum mismatch"
        );
        let (snapshot, data) = session::read_bundle(&path)?;
        ensure!(
            snapshot.at == info.frame && snapshot.sample_rate == info.sample_rate,
            "Recovery audio timing does not match its checkpoint"
        );
        Ok((snapshot, data))
    }
    /// Returns true only after the durable pointer is gone and owned slots are
    /// removed. A newer session/request is never cleared by an obsolete task.
    pub fn clear(&self, ticket: &Ticket) -> Result<bool> {
        if !self.current(ticket) {
            return Ok(false);
        }
        let _lock = self.lock()?;
        if !self.current(ticket) || !self.owns_epoch()? {
            return Ok(false);
        }
        // Preflight both owned slots before publishing the discard. Link or
        // identity failures leave the existing recovery available to inspect.
        for slot in 0..SLOTS.len() {
            if storage::exists(&self.slot_path(slot))? {
                self.validate_removal(slot)?;
            }
        }
        if !self.current(ticket) {
            return Ok(false);
        }
        let head = self.0.root.join("head.json");
        if storage::exists(&head)? {
            if let Ok(info) = self.read_json::<Info>(&head, 65536) {
                ensure!(
                    info.project_file.eq_ignore_ascii_case(&self.0.entry.file),
                    "Recovery belongs to another project"
                );
            }
            storage::remove_file(&self.0.root, &head)?;
        }
        for slot in 0..SLOTS.len() {
            self.remove_slot(slot)
                .context("Recovery discarded, but its audio cleanup needs retrying")?;
        }
        self.remove_stage()
            .context("Recovery discarded, but staging metadata cleanup needs retrying")?;
        Ok(self
            .0
            .generation
            .compare_exchange(
                ticket.generation,
                ticket.generation + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok())
    }
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
