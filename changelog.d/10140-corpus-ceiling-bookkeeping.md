<!-- section: Internal / CI -->
- Verdict-corpus `missed_credit_ceiling` values again equal their blessed per-language counts (rust 143, python 30, perl 23): #10117 blessed RIPR-SPEC-0164/0165 measurement rows without finishing the ceiling bookkeeping, and the equality pin from #9628 then failed deterministically on main.
