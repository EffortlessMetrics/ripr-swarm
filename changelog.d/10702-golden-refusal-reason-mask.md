<!-- section: Internal / CI -->
- `cargo xtask goldens check` no longer compares the wording after `no repair start offered:` in human fixture goldens, so rewording a repair refusal no longer re-blesses one line in each of hundreds of fixtures. A repair start that flips to a refusal, or the refusal line disappearing, is still golden drift (#10702).
