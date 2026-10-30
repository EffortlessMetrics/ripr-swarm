<!-- section: Fixed -->
- Evidence for an open file no longer stays paused behind "save the file" after a formatter, `git checkout` or coding agent rewrites it on disk and the editor reloads the buffer. A reloaded buffer that equals the file on disk is now analyzed as saved content (#11153).
