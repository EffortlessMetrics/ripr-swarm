<!-- section: Internal / CI -->
- The `--quiet` façade test masks the workspace root in both its native and forward-slash spellings, so it no longer fails on Windows, where `ripr` prints the root with `/` (#12134).
