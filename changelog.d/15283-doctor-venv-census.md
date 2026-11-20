<!-- section: Fixed -->
- `ripr doctor` no longer reports `Unanalyzed languages: PowerShell` for a
  virtualenv's `activate.ps1`: the unanalyzed-language census now skips the
  vendored directory names the Python test census already skips (`.venv`,
  `venv`, `.tox`, `.nox`, `site-packages`, `__pycache__`, tool caches) plus any
  directory holding a `pyvenv.cfg`, and pilot, init and status use the same
  skip when deciding whether a root has inspectable source. The oversized-diff
  refusal now also names `--path`/`--exclude` (for example
  `--exclude '.venv/**'`) (#15283).
