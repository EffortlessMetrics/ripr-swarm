<!-- section: Fixed -->
- `ripr reports index` now scans `--receipts-dir` for receipt JSONs and
  lists each one under `validation_receipts`. The flag used to be echoed
  in `inputs.receipts_dir` but never joined into any spec path, so a
  receipt written in the advertised receipts home stayed invisible and a
  syntactically broken receipt file produced no warning at all. A receipt
  file that does not parse as a JSON object renders as `malformed` with a
  warning instead of passing silently (#7381).
