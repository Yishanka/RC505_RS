//! Test-only, thread-local allocation accounting around the renderer.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! {static ENABLED:Cell<bool>=const{Cell::new(false)};static COUNT:Cell<usize>=const{Cell::new(0)};}
struct Allocator;
#[global_allocator]
static GLOBAL: Allocator = Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ENABLED.with(|e| {
            if e.get() {
                COUNT.with(|n| n.set(n.get() + 1));
            }
        });
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        ENABLED.with(|e| {
            if e.get() {
                COUNT.with(|n| n.set(n.get() + 1));
            }
        });
        unsafe { System.dealloc(p, layout) }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ENABLED.with(|e| {
            if e.get() {
                COUNT.with(|n| n.set(n.get() + 1));
            }
        });
        unsafe { System.realloc(p, layout, size) }
    }
}
pub fn count(f: impl FnOnce()) -> usize {
    COUNT.with(|v| v.set(0));
    ENABLED.with(|v| v.set(true));
    f();
    ENABLED.with(|v| v.set(false));
    COUNT.with(Cell::get)
}
