//! Bytes of the executing image for repair-record provenance.

pub(super) fn read() -> std::io::Result<Vec<u8>> {
    // On Linux, current_exe() resolves this link to a pathname. A build or
    // upgrade can unlink that pathname while this process keeps executing the
    // old inode. Open the kernel link itself: even after replacement it names
    // the running image, never the new file at the old pathname (#10586).
    #[cfg(target_os = "linux")]
    let path = std::path::PathBuf::from("/proc/self/exe");
    #[cfg(not(target_os = "linux"))]
    let path = std::env::current_exe()?;
    std::fs::read(path)
}
