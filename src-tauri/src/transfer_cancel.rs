//! Shared cancel flag for backup, Nissan scan, and guided flash.
//! A cancelled transfer stops before the next step and is not a success.
//!
//! The flag sits behind a reentrant spin lock so a unit test can own it for
//! the whole test. Other tests block in `stop` instead of observing a flag
//! that belongs to a different test.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

static CANCEL: AtomicBool = AtomicBool::new(false);
static LOCKED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static DEPTH: Cell<u32> = Cell::new(0);
}

struct FlagGuard;

impl FlagGuard {
    fn lock() -> Self {
        let depth = DEPTH.get();
        if depth == 0 {
            while LOCKED
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                std::thread::yield_now();
            }
        }
        DEPTH.set(depth + 1);
        FlagGuard
    }
}

impl Drop for FlagGuard {
    fn drop(&mut self) {
        let depth = DEPTH.get().saturating_sub(1);
        DEPTH.set(depth);
        if depth == 0 {
            LOCKED.store(false, Ordering::SeqCst);
        }
    }
}

fn with_flag<T>(f: impl FnOnce() -> T) -> T {
    let _g = FlagGuard::lock();
    f()
}

pub fn cancel_message(stage: &str) -> String {
    format!("Cancelled during {stage}. Stopped before the next step. This is not a success.")
}

pub fn request() {
    with_flag(|| CANCEL.store(true, Ordering::SeqCst));
}

pub fn clear() {
    with_flag(|| CANCEL.store(false, Ordering::SeqCst));
}

pub fn requested() -> bool {
    with_flag(|| CANCEL.load(Ordering::SeqCst))
}

/// Error when the user has asked the running transfer to stop.
pub fn stop(stage: &str) -> Result<(), String> {
    with_flag(|| {
        if CANCEL.load(Ordering::SeqCst) {
            Err(cancel_message(stage))
        } else {
            Ok(())
        }
    })
}

/// Hold the flag lock for the rest of a test so parallel tests cannot observe it.
#[cfg(test)]
pub(crate) fn hold_for_test() -> impl Drop {
    FlagGuard::lock()
}

#[tauri::command]
pub fn cancel_transfer() -> Result<String, String> {
    request();
    Ok("Cancel requested. The running backup, scan, or flash stops before the next step and is not reported as success.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            clear();
        }
    }

    #[test]
    fn cancel_stops_and_clear_lets_the_next_step_run() {
        let _hold = hold_for_test();
        let _reset = Reset;
        clear();
        assert!(stop("backup").is_ok());
        request();
        let err = stop("guided flash").unwrap_err();
        assert!(err.contains("Cancelled during guided flash"), "{err}");
        assert!(err.contains("not a success"), "{err}");
        clear();
        assert!(stop("Nissan scan").is_ok());
    }
}
