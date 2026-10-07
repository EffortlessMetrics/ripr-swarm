<!-- section: Fixed -->
- `ripr pilot --root X` without `--out` now writes its packet to
  `X/target/ripr/pilot`, where the cache and `ripr agent status --root X`
  already look, instead of `target/ripr/pilot` under the shell's working
  directory, which overwrote that directory's own packet (#5324). `--root .`
  and an explicit `--out` are unchanged; the terminal output already names
  the packet path. The packet's `next` snapshot and verify commands now bind
  that root too, like `ripr agent packet` (absolute even for `--root .`), and
  gain `next.analysis_outcome_command`; they used to say `--root .` while
  redirecting into the launch directory. A changed seam past the pilot seam
  budget is kept so it can still rank change-first.
