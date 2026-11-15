<!-- section: Docs -->
- The install channels guide now documents the opt-in slim single-language
  source build: keep the required `lang-rust` and add only the wanted
  parsers (for example `--no-default-features --features
  lang-rust,lang-python`). The Rust-only graph (`--features lang-rust`)
  counted 131 distinct crates against 206 for the default graph on
  2026-11-14 (#14745).
