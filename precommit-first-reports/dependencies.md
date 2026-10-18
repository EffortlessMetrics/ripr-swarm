# check-dependencies

Status: fail

## Why This Matters

Dependency manager surfaces change build and supply-chain behavior, so they need an explicit owner and reason.

## Violations

```text
dependency surface is not allowlisted: fixtures/macro_site_scoped_rest_limit/input/Cargo.lock
  preferred: keep dependency managers scoped to approved Cargo, VS Code, or fixture surfaces
```

## Fix Kind

```text
policy_exception_required
```

## Recommended Fixes

1. Keep dependency files inside approved Cargo, VS Code, or fixture surfaces.
2. Explain new dependency surfaces in the PR.
3. Add an allowlist entry only when the surface is intentional.

## Exception Template

```text
policy/dependency_allowlist.txt entry:
glob|kind|owner|reason
```

## Rerun

```bash
cargo xtask check-dependencies
```
