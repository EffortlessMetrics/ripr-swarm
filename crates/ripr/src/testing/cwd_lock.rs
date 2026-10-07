//! One lock for unit tests that read or change the process working directory.
//!
//! `cargo test` runs unit tests on parallel threads of one process, so a test
//! that calls `set_current_dir` races every test that resolves a path against
//! the working directory. The one test that changes it and the tests that
//! depend on it opt in to this lock (#6965); other readers of the working
//! directory predate it and do not hold it yet.

use std::sync::{Mutex, MutexGuard};

static CWD_LOCK: Mutex<()> = Mutex::new(());

/// Hold the working-directory lock. A poisoned lock still serializes: the
/// test that panicked already restored or abandoned its directory change.
pub(crate) fn hold_cwd() -> MutexGuard<'static, ()> {
    CWD_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
