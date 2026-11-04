use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;

pub(super) const TEXT_BYTES: usize = 65_536;
pub(super) static LIVE_BUFFERS: AtomicIsize = AtomicIsize::new(0);
static SERIAL: Mutex<()> = Mutex::new(());

struct Counted;

fn account(size: usize, delta: isize) {
    if size == TEXT_BYTES + 1 {
        LIVE_BUFFERS.fetch_add(delta, Ordering::SeqCst);
    }
}

unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forward the unchanged allocator contract to System.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            account(layout.size(), 1);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        account(layout.size(), -1);
        // SAFETY: System receives its original pointer and layout.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: forward the original allocation and requested size to System.
        let replaced = unsafe { System.realloc(pointer, layout, size) };
        if !replaced.is_null() {
            account(layout.size(), -1);
            account(size, 1);
        }
        replaced
    }
}

#[global_allocator]
static ALLOCATOR: Counted = Counted;

pub(super) fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(super) fn poison<T>(mutex: &Mutex<T>) {
    let result = std::panic::catch_unwind(|| {
        let _guard = mutex.lock().expect("unpoisoned test fixture");
        panic!("storage-lock failure control");
    });
    assert!(result.is_err());
}

const CHILD_TEST: &str = "OSPREY_GPUI_POISON_CHILD";

pub(crate) fn isolated_test() -> bool {
    let thread = std::thread::current();
    let name = thread.name().expect("named test");
    if std::env::var(CHILD_TEST).ok().as_deref() == Some(name) {
        return false;
    }
    let result = test_child(name);
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("running 1 test"),
        "child selected no regression: {stdout}"
    );
    assert!(
        result.status.success(),
        "isolated regression failed: {stdout}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    true
}

fn test_child(name: &str) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", name, "--nocapture"])
        .env(CHILD_TEST, name)
        .output()
        .expect("launch isolated storage regression")
}

pub(crate) fn with_poisoned_stream<T>(body: impl FnOnce() -> T) -> T {
    poison(&super::STREAM);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
    super::STREAM.clear_poison();
    match result {
        Ok(value) => value,
        Err(problem) => std::panic::resume_unwind(problem),
    }
}
