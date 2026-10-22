<!-- section: Fixed -->
- A macOS-style case-insensitive root strip now decides rootedness by the POSIX rule instead of the host's `Path::is_absolute`, so the host-explicit path renderer gives the same answer when it runs on Windows (#9660).
