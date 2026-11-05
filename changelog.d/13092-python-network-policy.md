<!-- section: Internal / CI -->
- `cargo xtask check-network-policy` now scans tracked Python files for
  `urllib.request`, `urlopen`, `import socket`, and `http.client`, so Python
  network surfaces require an explicit allowlist entry like other languages
  (#13092).
