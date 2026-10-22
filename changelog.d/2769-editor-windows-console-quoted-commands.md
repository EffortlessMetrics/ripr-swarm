<!-- section: Fixed -->
- On Windows the editor now copies the agent loop commands the server writes in the Windows console quoting (`--base "HEAD~1"`, a root with spaces). It checked them against the POSIX single-quote form only, so those commands were refused with "No ripr agent loop command is available". Found by the packaged Windows editor rehearsal on main 62778ef0 (#2769).
