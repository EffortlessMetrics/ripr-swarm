<!-- section: Internal / CI -->
- `check-file-policy` now prepares the `xtask` and `ripr` test targets in separate cargo builds instead of one `-p ripr -p xtask` build. On Windows the combined build made Cargo recompile the `xtask` binary (about 95 s) each time it alternated with an `xtask`-only build, which was most of the 218 s a warm run took (#12245).
