<!-- section: Fixed -->
- `pr-ledger record --out-jsonl` and `policy history --out-jsonl` now refuse an
  absolute spelling of, or a symlink to, `--out`, `--out-md` or an evidence
  input, matching `--out` and `--out-md`; `--history` equal to `--out-jsonl`
  still appends (#14352).
