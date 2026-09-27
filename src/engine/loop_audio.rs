//! Page sharing makes snapshots and one-level undo independent of loop size.
//! The audio thread obtains replacement pages from a bounded, prepared pool.
use std::sync::Arc;

pub type Frame = [f32; 2];
pub const PAGE_FRAMES: usize = 8192;
pub const MAX_LOOP_SECONDS: usize = 300;
pub type Page = Arc<[Frame; PAGE_FRAMES]>;

pub trait PageAllocator {
    fn acquire(&mut self) -> Option<Page>;
    /// Must not deallocate on the audio thread.
    fn retire(&mut self, page: Page);
}

pub struct OfflinePages;
impl PageAllocator for OfflinePages {
    fn acquire(&mut self) -> Option<Page> {
        Some(Arc::new([[0.0; 2]; PAGE_FRAMES]))
    }
    fn retire(&mut self, _page: Page) {}
}

pub struct LoopAudio {
    pub pages: Vec<Page>,
    pub len: usize,
    capacity: usize,
}

impl LoopAudio {
    pub fn new(sample_rate: u32) -> Self {
        let capacity = sample_rate as usize * MAX_LOOP_SECONDS;
        Self {
            pages: Vec::with_capacity(capacity.div_ceil(PAGE_FRAMES)),
            len: 0,
            capacity,
        }
    }
    pub fn read(&self, frame: usize) -> Frame {
        if frame >= self.len {
            return [0.0; 2];
        }
        self.pages[frame / PAGE_FRAMES][frame % PAGE_FRAMES]
    }
    pub fn write(&mut self, frame: usize, value: Frame, pool: &mut impl PageAllocator) -> bool {
        if frame > self.len || frame >= self.capacity {
            return false;
        }
        let index = frame / PAGE_FRAMES;
        if index == self.pages.len() {
            let Some(page) = pool.acquire() else {
                return false;
            };
            self.pages.push(page);
        }
        if Arc::get_mut(&mut self.pages[index]).is_none() {
            let Some(mut replacement) = pool.acquire() else {
                return false;
            };
            let Some(destination) = Arc::get_mut(&mut replacement) else {
                pool.retire(replacement);
                return false;
            };
            destination.copy_from_slice(self.pages[index].as_ref());
            pool.retire(std::mem::replace(&mut self.pages[index], replacement));
        }
        Arc::get_mut(&mut self.pages[index]).unwrap()[frame % PAGE_FRAMES] = value;
        self.len = self.len.max(frame + 1);
        true
    }
    pub fn clear(&mut self, pool: &mut impl PageAllocator) {
        for page in self.pages.drain(..) {
            pool.retire(page);
        }
        self.len = 0;
    }
    pub fn share_into(&self, destination: &mut Self, pool: &mut impl PageAllocator) {
        destination.clear(pool);
        destination.pages.extend(self.pages.iter().cloned());
        destination.len = self.len;
    }
    pub fn from_frames(sample_rate: u32, frames: &[Frame]) -> anyhow::Result<Self> {
        anyhow::ensure!(
            frames.len() <= sample_rate as usize * MAX_LOOP_SECONDS,
            "Loop exceeds five minutes"
        );
        let mut audio = Self::new(sample_rate);
        for (index, frame) in frames.iter().enumerate() {
            anyhow::ensure!(
                frame.iter().all(|v| v.is_finite()),
                "Non-finite audio sample"
            );
            audio.write(index, *frame, &mut OfflinePages);
        }
        Ok(audio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_is_immutable_and_undo_shares_pages() {
        let mut source = LoopAudio::new(100);
        let mut saved = LoopAudio::new(100);
        for i in 0..(PAGE_FRAMES * 2 + 3) {
            assert!(source.write(i, [i as f32, -(i as f32)], &mut OfflinePages));
        }
        source.share_into(&mut saved, &mut OfflinePages);
        assert!(Arc::ptr_eq(&source.pages[1], &saved.pages[1]));
        source.write(PAGE_FRAMES + 176, [9.0, 8.0], &mut OfflinePages);
        assert_eq!(
            saved.read(PAGE_FRAMES + 176),
            [(PAGE_FRAMES + 176) as f32, -((PAGE_FRAMES + 176) as f32)]
        );
        assert_eq!(source.read(PAGE_FRAMES + 176), [9.0, 8.0]);
        assert!(!Arc::ptr_eq(&source.pages[1], &saved.pages[1]));
    }
}
