<!-- section: Fixed -->
- `ripr doctor` no longer reports `Unanalyzed languages: PowerShell` for a
  virtualenv's `activate.ps1`: the unanalyzed-language census now skips
  `.venv`, `venv`, `.tox`, `.nox`, `site-packages`, `__pycache__`, tool caches
  and any directory holding a `pyvenv.cfg`, the same vendored directories the
  Python test census already skips. The oversized-diff refusal now also
  names `--path`/`--exclude` (for example `--exclude '.venv/**'`) (#15283).
