//! Bounded page-shared audio history. Storage is prepared off the audio thread.
use super::loop_audio::{LoopAudio, PageAllocator};
pub const HISTORY_DEPTH: usize = 8;
const HISTORY_PAGE_BUDGET: usize = 2048; // 128 MiB of referenced pages; always keep the latest step.
pub struct AudioStack {
    pub slots: Vec<LoopAudio>,
    pub len: usize,
}
impl AudioStack {
    pub fn new(sr: u32) -> Self {
        Self {
            slots: (0..HISTORY_DEPTH).map(|_| LoopAudio::new(sr)).collect(),
            len: 0,
        }
    }
    pub fn clear(&mut self, pool: &mut impl PageAllocator) {
        for audio in &mut self.slots {
            audio.clear(pool);
        }
        self.len = 0;
    }
    fn discard_oldest(&mut self, pool: &mut impl PageAllocator) {
        self.slots[0].clear(pool);
        self.slots[..self.len].rotate_left(1);
        self.len -= 1;
    }
    pub fn push(&mut self, audio: &LoopAudio, pool: &mut impl PageAllocator) {
        if self.len == HISTORY_DEPTH {
            self.discard_oldest(pool);
        }
        audio.share_into(&mut self.slots[self.len], pool);
        self.len += 1;
        while self.len > 1
            && self.slots[..self.len]
                .iter()
                .map(|s| s.pages.len())
                .sum::<usize>()
                > HISTORY_PAGE_BUDGET
        {
            self.discard_oldest(pool);
        }
    }
    pub fn pop_into(&mut self, audio: &mut LoopAudio, pool: &mut impl PageAllocator) {
        if self.len == 0 {
            return;
        }
        self.len -= 1;
        std::mem::swap(audio, &mut self.slots[self.len]);
        self.slots[self.len].clear(pool);
    }
    pub fn copy_into(&self, to: &mut Self, pool: &mut impl PageAllocator) {
        to.clear(pool);
        for audio in &self.slots[..self.len] {
            to.push(audio, pool);
        }
    }
}
pub struct AudioHistory {
    pub undo: AudioStack,
    pub redo: AudioStack,
}
impl AudioHistory {
    pub fn new(sr: u32) -> Self {
        Self {
            undo: AudioStack::new(sr),
            redo: AudioStack::new(sr),
        }
    }
    pub fn checkpoint(&mut self, audio: &LoopAudio, pool: &mut impl PageAllocator) {
        self.redo.clear(pool);
        self.undo.push(audio, pool);
    }
    pub fn undo(&mut self, audio: &mut LoopAudio, pool: &mut impl PageAllocator) {
        if self.undo.len > 0 {
            self.redo.push(audio, pool);
            self.undo.pop_into(audio, pool);
        }
    }
    pub fn redo(&mut self, audio: &mut LoopAudio, pool: &mut impl PageAllocator) {
        if self.redo.len > 0 {
            self.undo.push(audio, pool);
            self.redo.pop_into(audio, pool);
        }
    }
    pub fn copy_into(&self, to: &mut Self, pool: &mut impl PageAllocator) {
        self.undo.copy_into(&mut to.undo, pool);
        self.redo.copy_into(&mut to.redo, pool);
    }
    pub fn clear(&mut self, pool: &mut impl PageAllocator) {
        self.undo.clear(pool);
        self.redo.clear(pool);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::loop_audio::{OfflinePages, PAGE_FRAMES, Page};
    #[test]
    fn bounded_steps_redo_and_branching_preserve_audio() {
        let mut audio = LoopAudio::from_frames(8000, &[[0.0; 2]]).unwrap();
        let mut history = AudioHistory::new(8000);
        for value in 1..=12 {
            history.checkpoint(&audio, &mut OfflinePages);
            audio.write(0, [value as f32; 2], &mut OfflinePages);
        }
        assert_eq!(history.undo.len, HISTORY_DEPTH);
        for expected in (4..=11).rev() {
            history.undo(&mut audio, &mut OfflinePages);
            assert_eq!(audio.read(0), [expected as f32; 2]);
        }
        for expected in 5..=12 {
            history.redo(&mut audio, &mut OfflinePages);
            assert_eq!(audio.read(0), [expected as f32; 2]);
        }
        history.undo(&mut audio, &mut OfflinePages);
        history.checkpoint(&audio, &mut OfflinePages);
        audio.write(0, [99.0; 2], &mut OfflinePages);
        assert_eq!(history.redo.len, 0);
        history.undo(&mut audio, &mut OfflinePages);
        assert_eq!(audio.read(0), [11.0; 2]);
    }
    #[test]
    fn history_traversal_does_not_allocate_or_free() {
        struct Pool {
            free: Vec<Page>,
            retired: Vec<Page>,
        }
        impl PageAllocator for Pool {
            fn acquire(&mut self) -> Option<Page> {
                self.free.pop()
            }
            fn retire(&mut self, page: Page) {
                self.retired.push(page);
            }
        }
        let mut pool = Pool {
            free: (0..12)
                .map(|_| std::sync::Arc::new([[0.0; 2]; PAGE_FRAMES]))
                .collect(),
            retired: Vec::with_capacity(512),
        };
        let mut audio = LoopAudio::new(8000);
        let mut h = AudioHistory::new(8000);
        audio.write(0, [0.0; 2], &mut pool);
        let count = crate::test_alloc::count(|| {
            for i in 0..8 {
                h.checkpoint(&audio, &mut pool);
                assert!(audio.write(0, [i as f32; 2], &mut pool));
            }
            for _ in 0..8 {
                h.undo(&mut audio, &mut pool);
            }
            for _ in 0..8 {
                h.redo(&mut audio, &mut pool);
            }
        });
        assert_eq!(count, 0);
    }
}
