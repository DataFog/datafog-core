//! Observe actual native allocations around the public exporter. No shipping allocator seam.
use datafog_core::{
    TextRange,
    workspace::{ErrorCode, Workspace, WorkspaceError, WorkspaceFinding},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

#[derive(Clone, Copy)]
struct Slot {
    pointer: usize,
    capacity: usize,
}
impl Slot {
    const EMPTY: Self = Self {
        pointer: 0,
        capacity: 0,
    };
}
#[derive(Clone, Copy)]
struct Watch {
    active: bool,
    // The writer has one backing, or old + replacement during secure growth.
    slots: [Slot; 2],
    private_start: usize,
    private_end: usize,
    fail_after_private_allocation: bool,
    fail_next: bool,
    injected: bool,
    overflow: bool,
    private_backings: usize,
    private_wiped: bool,
    private_reallocated: bool,
}
impl Watch {
    const EMPTY: Self = Self {
        active: false,
        slots: [Slot::EMPTY; 2],
        private_start: 0,
        private_end: 0,
        fail_after_private_allocation: false,
        fail_next: false,
        injected: false,
        overflow: false,
        private_backings: 0,
        private_wiped: true,
        private_reallocated: false,
    };
}
thread_local! {static WATCH:Cell<Watch>=const{Cell::new(Watch::EMPTY)};}
struct ObservingAllocator;
fn should_fail() -> bool {
    WATCH
        .try_with(|cell| {
            let mut watch = cell.get();
            if watch.active && watch.fail_next {
                watch.fail_next = false;
                watch.injected = true;
                cell.set(watch);
                true
            } else {
                false
            }
        })
        .unwrap_or(false)
}
fn allocated(pointer: *mut u8, capacity: usize) {
    if pointer.is_null() {
        return;
    }
    let _ = WATCH.try_with(|cell| {
        let mut watch = cell.get();
        if watch.active {
            if let Some(slot) = watch.slots.iter_mut().find(|slot| slot.pointer == 0) {
                *slot = Slot {
                    pointer: pointer as usize,
                    capacity,
                };
            } else {
                watch.overflow = true;
            }
            if watch.fail_after_private_allocation
                && capacity >= watch.private_end
                && !watch.injected
            {
                watch.fail_next = true;
            }
            cell.set(watch);
        }
    });
}
fn inspect_and_remove(pointer: *mut u8, reallocated: bool) {
    let _ = WATCH.try_with(|cell| {
        let mut watch = cell.get();
        if watch.active {
            if let Some(index) = watch
                .slots
                .iter()
                .position(|slot| slot.pointer == pointer as usize)
            {
                let slot = watch.slots[index];
                watch.slots[index] = Slot::EMPTY;
                if slot.capacity >= watch.private_end {
                    // Watched allocations/tails are initialized by this allocator.
                    // The fixture forces later growth after the full display was
                    // written; observing zeroes before free proves its cleanup.
                    let value = unsafe {
                        std::slice::from_raw_parts(
                            pointer.add(watch.private_start),
                            watch.private_end - watch.private_start,
                        )
                    };
                    watch.private_backings += 1;
                    watch.private_wiped &= value.iter().all(|byte| *byte == 0);
                    watch.private_reallocated |= reallocated;
                }
                cell.set(watch);
            }
        }
    });
}
fn active() -> bool {
    WATCH.try_with(|cell| cell.get().active).unwrap_or(false)
}
fn tracked(pointer: *mut u8) -> bool {
    WATCH
        .try_with(|cell| {
            let watch = cell.get();
            watch.active
                && watch
                    .slots
                    .iter()
                    .any(|slot| slot.pointer == pointer as usize)
        })
        .unwrap_or(false)
}
unsafe impl GlobalAlloc for ObservingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if should_fail() {
            return std::ptr::null_mut();
        }
        // Initialize watched storage so the observer remains memory-safe even
        // if later serializer changes alter which capacity has been written.
        let pointer = if active() {
            unsafe { System.alloc_zeroed(layout) }
        } else {
            unsafe { System.alloc(layout) }
        };
        allocated(pointer, layout.size());
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        inspect_and_remove(pointer, false);
        unsafe { System.dealloc(pointer, layout) };
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if should_fail() {
            return std::ptr::null_mut();
        }
        let observed = tracked(pointer);
        inspect_and_remove(pointer, true);
        let next = unsafe { System.realloc(pointer, layout, size) };
        if observed {
            if next.is_null() {
                // realloc failure preserves the old initialized backing.
                allocated(pointer, layout.size());
            } else {
                if size > layout.size() {
                    unsafe {
                        std::ptr::write_bytes(next.add(layout.size()), 0, size - layout.size())
                    };
                }
                allocated(next, size);
            }
        }
        next
    }
}
#[global_allocator]
static ALLOCATOR: ObservingAllocator = ObservingAllocator;

fn workspace() -> (Workspace, Vec<u8>, usize, usize) {
    let private = format!("Synthetic private display {}", "Z".repeat(8192));
    let mut workspace = Workspace::create([1; 16], [2; 16]).unwrap();
    let mut plan = workspace
        .prepare(
            &private,
            &[WorkspaceFinding::new(
                "PERSON",
                &private,
                TextRange {
                    start: 0,
                    end: private.len(),
                },
            )],
            [3; 16],
        )
        .unwrap();
    let parent = plan.expected_head().unwrap();
    let saved = plan.candidate_head().unwrap();
    workspace.activate_saved(&mut plan, parent, saved).unwrap();
    let original = workspace.export_bundle().unwrap();
    let start = original
        .windows(private.len())
        .position(|window| window == private.as_bytes())
        .unwrap();
    let end = start + private.len();
    (workspace, original, start, end)
}
fn export(
    workspace: &Workspace,
    start: usize,
    end: usize,
    fail: bool,
) -> (Result<Vec<u8>, WorkspaceError>, Watch) {
    WATCH.with(|cell| {
        cell.set(Watch {
            active: true,
            private_start: start,
            private_end: end,
            fail_after_private_allocation: fail,
            ..Watch::EMPTY
        })
    });
    let result = workspace.export_bundle();
    let watch = WATCH.with(|cell| {
        let watch = cell.get();
        cell.set(Watch::EMPTY);
        watch
    });
    (result, watch)
}
#[test]
fn exported_snapshot_growth_wipes_private_old_backings_before_freeing_them() {
    let (workspace, mut original, start, end) = workspace();
    let (result, watch) = export(&workspace, start, end, false);
    let mut exported = result.unwrap();
    assert!(
        !watch.overflow,
        "unexpected allocations invalidate the isolated writer observation"
    );
    assert!(
        watch.private_backings > 0,
        "fixture must exercise growth after a complete private display"
    );
    assert!(
        !watch.private_reallocated && watch.private_wiped,
        "private snapshot backing reached allocator growth/free without wiping"
    );
    assert_eq!(
        exported, original,
        "successful public Vec transfer stays intact and caller-owned"
    );
    exported.fill(0);
    original.fill(0);
}
#[test]
fn fallible_export_allocation_failure_wipes_serialized_private_prefix_without_mutating_state() {
    let (workspace, mut original, start, end) = workspace();
    let (result, watch) = export(&workspace, start, end, true);
    assert!(!watch.overflow);
    assert!(
        watch.injected,
        "fixture must fail an actual allocation after the private display was written"
    );
    assert_eq!(result.err().unwrap().code, ErrorCode::ResourceLimit);
    assert!(
        watch.private_backings > 0 && watch.private_wiped,
        "failed encoding dropped a private prefix without wiping"
    );
    let mut after = workspace.export_bundle().unwrap();
    assert_eq!(
        after, original,
        "failed serialization changes no committed bytes"
    );
    after.fill(0);
    original.fill(0);
}
