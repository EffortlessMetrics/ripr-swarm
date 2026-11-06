<!-- section: Internal / CI -->
- Executable-file policy compares literal NUL-framed Git index paths against
  UTF-8 allowlist rows, preserves Unix backslashes, and explicitly rejects
  unrepresentable identities and unmerged stages (#7350).
