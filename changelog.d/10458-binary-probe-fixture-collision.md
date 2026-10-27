<!-- section: Internal / CI -->
- `lsp_lifecycle` test fixture roots add a per-process sequence number, so
  parallel `ripr_binary_probe` calls can no longer share one capture directory
  and splice two CLI processes' JSON into one `stdout` file on Windows (#10458).
