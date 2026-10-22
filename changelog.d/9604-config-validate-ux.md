<!-- section: Changed -->
- `ripr config validate` now names the discovered `ripr.toml` path on
  success, so ancestor config discovery cannot silently validate a parent's
  file (#9604 batch B).
- `ripr config validate` no longer fails on a config-less root whose
  language-marker detection refuses it (e.g. Python markers in a binary
  without `lang-python`); it succeeds with the detection note carried in
  the output (#9604 batch B).
