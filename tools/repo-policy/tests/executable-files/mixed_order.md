# check-executable-files

Status: fail

## Why This Matters

Checked-in executable scripts make automation drift away from the Rust-first xtask surface.

## Violations

```text
checked-in executable file is not allowlisted: unregistered
  preferred: use cargo xtask instead of executable scripts
```

```text
a-missing allowlist entry is stale: path is not in git ls-files --stage; remove the entry
```

```text
demoted allowlist entry is stale: mode is 100644, expected 100755; remove the entry
```

```text
z-missing allowlist entry is stale: path is not in git ls-files --stage; remove the entry
```

## Fix Kind

```text
policy_exception_required
```

## Recommended Fixes

1. Remove the executable bit from ordinary files.
2. Move script behavior into xtask.
3. If an executable file is truly required, add a reviewed allowlist entry.
4. If an allowlist path is missing or no longer mode 100755, remove the entry.

## Exception Template

```text
policy/executable_allowlist.txt entry:
path/to/file
```

## Rerun

```bash
cargo xtask check-executable-files
```
