<!-- section: Fixed -->
- `ripr doctor` and `ripr pilot` no longer let a first run in a non-Cargo
  project end with un-ignored ripr state and no warning. On the first
  `doctor` run in a project without a `Cargo.toml`, when nothing exists
  under `target/ripr` yet and git positively reports the path as not
  covered by ignore rules, doctor prints a pre-emptive advisory naming the
  exact `.gitignore` entry (`/target/ripr/`); `ripr pilot` prints a one-line
  stderr note with the same entry after writing its default packet
  location. Cargo projects are unchanged: the `target/` convention already
  covers the artifacts, and an established warning still names existing
  un-ignored residue (#9858).
