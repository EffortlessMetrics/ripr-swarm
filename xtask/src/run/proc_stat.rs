/// POSIX ESRCH ("No such process"). Reading `/proc/<pid>/stat` after the
/// task has exited between `readdir` and the read fails with this code, not
/// with `ErrorKind::NotFound` (ENOENT).
pub(super) const PROC_STAT_ESRCH: i32 = 3;

/// A vanished `/proc/<pid>/stat` is gone. Permission and other failures stay
/// unreadable so the scan remains fail-closed.
pub(super) fn proc_stat_error_is_gone(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::NotFound || err.raw_os_error() == Some(PROC_STAT_ESRCH)
}
