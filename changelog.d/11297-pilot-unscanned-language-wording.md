<!-- section: Fixed -->
- `ripr pilot` on a Python-only (or other non-Rust) repository no longer prints a `Python preview` block with `status: no_python_findings` and `findings: 0` for Python files it lists under "Not scanned by pilot", and omits the "pilot found no gaps in the Rust code it checked" sentence when the repository has no Rust source (#11297).
