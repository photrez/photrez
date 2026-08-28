// Dev-only allocation accounting for the R1 parity shadow harness.
// Wraps the system allocator with CURRENT/PEAK byte counters so the harness can
// report Rust-side memory separately from JS heap. Negligible overhead.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static CUR: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

pub struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            let cur = CUR.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(cur, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        CUR.fetch_sub(layout.size(), Ordering::Relaxed);
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = System.realloc(ptr, layout, new_size);
        if !p.is_null() {
            if new_size >= layout.size() {
                let cur = CUR.fetch_add(new_size - layout.size(), Ordering::Relaxed)
                    + (new_size - layout.size());
                PEAK.fetch_max(cur, Ordering::Relaxed);
            } else {
                CUR.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        p
    }
}

pub fn current_mb() -> f64 {
    (CUR.load(Ordering::Relaxed) as f64 / 1e6 * 10.0).round() / 10.0
}

pub fn peak_mb() -> f64 {
    (PEAK.load(Ordering::Relaxed) as f64 / 1e6 * 10.0).round() / 10.0
}

pub fn reset_peak() {
    PEAK.store(CUR.load(Ordering::Relaxed), Ordering::Relaxed);
}
