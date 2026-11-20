<!-- section: Fixed -->
- Editor (LSP): on Linux, files on network mounts (NFS, SMB, sshfs and other FUSE mounts) are always reread when checking whether an editor buffer matches the saved file, so a lagging server clock can no longer make ripr reuse a stale digest (#12170).
