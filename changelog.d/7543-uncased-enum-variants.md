<!-- section: Fixed -->
- Rust: a changed match arm over an enum with single-letter or all-caps
  variants (`Kind::A`, `Method::GET`) now names the missing discriminator,
  such as `_` or `Kind::B`, when no related test selects the arm. Such a
  name may be an associated constant, so ripr reads it as a variant only
  when the owner's file declares the one enum of the scrutinee's type with
  that variant and does not import that type name; otherwise the arm stays
  unknown, as before
  ([#7543](https://github.com/EffortlessMetrics/ripr-swarm/issues/7543)).
