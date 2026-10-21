<!-- section: Internal / CI -->
- `cargo xtask verdict-corpus relabel` has an opt-in ignored xtask test that
  replays a two-case zero-dependency toy crate through `relabel_case` and
  fails unless `relabel.json` names exactly the deliberately mislabelled
  case (#6706).
