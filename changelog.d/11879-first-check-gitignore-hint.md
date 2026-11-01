<!-- section: Fixed -->
- `ripr check` now prints one stderr line on a first run when git ignore rules do not cover ripr's artifact root, naming the exact `.gitignore` entry (the same verdict `ripr doctor` gives), instead of leaving `?? target/` in `git status` unexplained. (#11879)
