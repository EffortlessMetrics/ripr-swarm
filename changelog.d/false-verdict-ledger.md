<!-- section: Internal / CI -->
- `verdict-corpus check` now fails on a false verdict the corpus's `[[known_false]]` ledger does not list, and on a listed row that is no longer false, so a bless cannot absorb a new false verdict and a fixed row cannot return unnoticed. Added `docs/roadmap/false-verdicts.md` with each remaining row's cause, fix route and owner (#9430).
