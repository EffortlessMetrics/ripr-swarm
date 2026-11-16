<!-- section: Added -->
- `packaging/npm/qualification/` proves a clean npm install-to-use
  TypeScript journey: an isolated loopback registry serves the staged
  launcher and native tarballs, and npm installs them with scripts disabled.
  The journey analyzes a TypeScript change through a project-local script,
  follows the explain continuation offline from a foreign directory, and
  checks that a missing or wrong native package fails with exact recovery
  advice. It emits a typed `package_qualification_receipt` for
  `cargo xtask package-qualification-gate` (#4627).
