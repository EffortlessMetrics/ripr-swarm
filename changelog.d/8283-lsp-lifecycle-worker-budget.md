<!-- section: Internal / CI -->
- LSP lifecycle test servers use two Tokio scheduler workers each, preserving
  multi-thread protocol checks while bounding inherited worker pools (#8283).
