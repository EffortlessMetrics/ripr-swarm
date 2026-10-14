<!-- section: Fixed -->
- `[languages.python] handwritten_files` and `[languages.typescript]
  handwritten_files` now admit exact repository-relative paths past the
  preview adapters' built-in excluded-directory conventions (`build/`,
  `dist/`, `env/`, `out/`, virtualenvs, caches and friends), mirroring the
  Rust escape hatch: a declared hand-written source is walked, read and
  counted instead of dropped with no override. The
  `language_scope_unsupported` recovery for skipped Python/TypeScript files
  now names this opt-in instead of advising a bare "retry" that cannot
  change the outcome (#7386).
