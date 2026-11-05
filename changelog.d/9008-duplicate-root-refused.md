<!-- section: Fixed -->
- Refuse a conflicting repeated `--root` (`--root a --root b`) with a usage error naming both values instead of silently analyzing the last root; an identical repeat stays absorbed so scripted double-root invocations keep working (#9008).
