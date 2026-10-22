<!-- section: Internal / CI -->
- `verdict-corpus check` now ratchets a per-language `missed_credit_ceiling`: the count of abstained rows whose tests do catch the change (Rust 112, TypeScript 23, Python 31, Perl 15). A regression fails the check; an improvement passes with a note to lower the ceiling (#9619).
