<!-- section: Fixed -->
- `ripr doctor` no longer reports a healthy setup and recommends `ripr check` where that command cannot pick a base, such as a depth-1 CI checkout or a repository whose branch is neither `main` nor `master`. It now shows the same reason and repair `ripr check` prints, and recommends `ripr check --base REF` for the diagnosed root (#14104).
