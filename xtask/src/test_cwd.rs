//! Shared test-process CWD gate. Included only by test targets.

/// Process-wide fair reader-writer gate serialising tests that mutate the process
/// working directory. The xtask test suite spawns subprocesses (rustc, cargo,
/// git) that inherit the current cwd; meanwhile other tests (`with_temp_cwd`,
/// `with_repo_cwd`) call `std::env::set_current_dir` and delete the temporary
/// directory on teardown. Without serialisation, a subprocess spawn can land
/// in the tiny window between `set_current_dir(temp)` and
/// `remove_dir_all(temp)`, producing "Could not locate working directory" or
/// exit-status-1 failures in unrelated `run_output*` / `capture_output*`
/// tests.
///
/// Readers still run in parallel, while a waiting writer blocks new readers.
/// The standard library `RwLock` admitted enough new readers under Windows
/// parallel test load to starve the dispatch test's writer indefinitely (see
/// #2875), so this small gate makes writer admission explicit without
/// serialising the entire spawning-test lane. See issues #2044 and #2124.
#[cfg(test)]
struct CwdLockState {
    active_readers: usize,
    active_writer: bool,
    waiting_writers: usize,
}

#[cfg(test)]
pub(crate) struct CwdLock {
    state: std::sync::Mutex<CwdLockState>,
    available: std::sync::Condvar,
}

#[cfg(test)]
impl CwdLock {
    fn new() -> Self {
        Self {
            state: std::sync::Mutex::new(CwdLockState {
                active_readers: 0,
                active_writer: false,
                waiting_writers: 0,
            }),
            available: std::sync::Condvar::new(),
        }
    }

    /// Acquire a shared reader slot.
    ///
    /// This gate is not reentrant: callers must not acquire another reader
    /// while holding a `CwdReadGuard`, because a queued writer would block the
    /// nested acquisition while the existing guard blocks that writer.
    fn read(&self) -> CwdReadGuard<'_> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while state.active_writer || state.waiting_writers > 0 {
            state = self
                .available
                .wait(state)
                .unwrap_or_else(|poison| poison.into_inner());
        }
        state.active_readers += 1;
        CwdReadGuard { lock: self }
    }

    /// Acquire the exclusive writer slot.
    ///
    /// This gate is not reentrant: callers must not acquire a writer while
    /// holding either a `CwdReadGuard` or a `CwdWriteGuard`.
    fn write(&self) -> CwdWriteGuard<'_> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.waiting_writers += 1;
        while state.active_writer || state.active_readers > 0 {
            state = self
                .available
                .wait(state)
                .unwrap_or_else(|poison| poison.into_inner());
        }
        state.waiting_writers = state.waiting_writers.saturating_sub(1);
        state.active_writer = true;
        CwdWriteGuard { lock: self }
    }
}

#[cfg(test)]
pub(crate) struct CwdReadGuard<'a> {
    lock: &'a CwdLock,
}

#[cfg(test)]
impl Drop for CwdReadGuard<'_> {
    fn drop(&mut self) {
        let active_readers = {
            let mut state = self
                .lock
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            state.active_readers = state.active_readers.saturating_sub(1);
            state.active_readers
        };
        if active_readers == 0 {
            self.lock.available.notify_all();
        }
    }
}

#[cfg(test)]
pub(crate) struct CwdWriteGuard<'a> {
    lock: &'a CwdLock,
}

#[cfg(test)]
impl Drop for CwdWriteGuard<'_> {
    fn drop(&mut self) {
        {
            let mut state = self
                .lock
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            state.active_writer = false;
        }
        self.lock.available.notify_all();
    }
}

#[cfg(test)]
pub(crate) static CWD_LOCK: std::sync::OnceLock<CwdLock> = std::sync::OnceLock::new();

/// Acquire the exclusive write guard for cwd-manipulating tests (those that
/// call `set_current_dir` or delete a temp cwd). Excludes every other cwd-
/// sensitive test — both writers and readers.
#[cfg(test)]
pub(crate) fn acquire_test_cwd_write_guard() -> CwdWriteGuard<'static> {
    CWD_LOCK.get_or_init(CwdLock::new).write()
}

/// Acquire the shared read guard for subprocess-spawning tests (those that
/// inherit the process cwd but do not change it). Multiple spawning tests can
/// hold the read guard concurrently; they are only excluded while a
/// cwd-manipulating test holds the write guard.
#[cfg(test)]
pub(crate) fn acquire_test_cwd_read_guard() -> CwdReadGuard<'static> {
    CWD_LOCK.get_or_init(CwdLock::new).read()
}
