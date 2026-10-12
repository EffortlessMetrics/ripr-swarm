<!-- section: Fixed -->
- On the lexical fallback path, a never-true `#[cfg(..)]` longer than 32
  lines, or separated from the function by a comment, is no longer missed
  and credited as grip. A bounded balanced scan joins the gate and skips
  comments; an unclosed `#[` still fails open. ([#7043](https://github.com/EffortlessMetrics/ripr-swarm/issues/7043))
