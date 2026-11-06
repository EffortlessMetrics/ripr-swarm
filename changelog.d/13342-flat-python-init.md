<!-- section: Fixed -->
- `ripr init` enables the available Python preview for flat projects with root
  `.py` files, so the generated configuration agrees with first-run detection;
  binaries without Python support print the existing feature warning. Root
  Python creation, deletion and rename events also reload LSP defaults through
  root-anchored watchers, while explicit language settings remain authoritative (#13342).
