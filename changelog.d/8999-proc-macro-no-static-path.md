<!-- section: Fixed -->
- A finding in a proc-macro crate (`[lib] proc-macro = true`, or a file
  declaring `proc_macro`, `proc_macro_derive` or `proc_macro_attribute`
  entry points) that would read `no_static_path` is now `static_unknown`
  with a `proc_macro_opaque` stop reason and a note that tests reach the code
  through derive or attribute expansion, so serde_derive-style changes no
  longer show a WARNING-level "no test reaches this" claim. Ordinary library
  crates are unchanged (#8999).
